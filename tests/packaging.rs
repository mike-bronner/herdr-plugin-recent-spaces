mod support;

use std::path::PathBuf;

use recent_spaces::version::TOGGLE_FLAG;
use support::*;

const TOGGLE_ACTION: &str = "toggle-order";

fn manifest() -> toml::Table {
    read_repo_file("herdr-plugin.toml")
        .parse::<toml::Table>()
        .expect("herdr-plugin.toml is not valid TOML")
}

fn argv(entry: &toml::Value) -> Vec<String> {
    entry["command"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

fn platforms(entry: &toml::Value) -> Vec<String> {
    entry["platforms"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

fn entries(section: &str) -> Vec<toml::Value> {
    manifest()[section].as_array().unwrap().clone()
}

fn declared_platforms() -> Vec<String> {
    manifest()["platforms"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

fn rust_files() -> Vec<PathBuf> {
    let mut found = vec![manifest_dir().join("build.rs")];
    for dir in ["src", "tests", "tests/support"] {
        let Ok(entries) = std::fs::read_dir(manifest_dir().join(dir)) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "rs").unwrap_or(false) {
                found.push(path);
            }
        }
    }
    found
}

#[test]
fn the_manifest_parses_as_toml() {
    manifest();
}

#[test]
fn a_typo_in_the_manifest_is_really_caught() {
    let broken = format!(
        "{}\nname = \"unterminated\n",
        read_repo_file("herdr-plugin.toml")
    );
    assert!(broken.parse::<toml::Table>().is_err());
}

#[test]
fn the_herdr_floor_stays_where_it_was_set() {
    assert_eq!(
        manifest()["min_herdr_version"].as_str(),
        Some("0.9.0"),
        "the floor is a decision, not a detail"
    );
}

#[test]
fn it_starts_exactly_one_watcher_on_every_platform_it_claims() {
    for platform in declared_platforms() {
        let starting: Vec<Vec<String>> = entries("startup")
            .iter()
            .filter(|entry| platforms(entry).contains(&platform))
            .map(argv)
            .collect();
        assert_eq!(
            starting.len(),
            1,
            "two watchers would fight over the sidebar order and hold two of the 32 \
             slots, and none at all leaves {} with a plugin that never runs: {:?}",
            platform,
            starting
        );
    }
}

#[test]
fn the_watcher_is_dispatched_through_the_launcher_and_not_a_build_artifact() {
    for entry in entries("startup") {
        let command = argv(&entry);
        assert!(
            command.iter().any(|a| a.starts_with("bin/launcher")),
            "the launcher is what rebuilds a stale binary before it execs one: {:?}",
            command
        );
        assert!(
            !command.iter().any(|a| a.contains("target/")),
            "a build-artifact path breaks the moment the profile changes: {:?}",
            command
        );
    }
}

#[test]
fn the_manifest_declares_a_build_step_for_every_platform_it_claims() {
    for platform in declared_platforms() {
        let building: Vec<Vec<String>> = entries("build")
            .iter()
            .filter(|entry| platforms(entry).contains(&platform))
            .map(argv)
            .collect();
        assert_eq!(
            building.len(),
            1,
            "a platform with no build step installs a plugin with no binary: {:?}",
            building
        );
        assert!(
            building[0].iter().any(|a| a == "--install"),
            "the flag names the calling context, and without it the install path \
             asks a socket that is not listening yet: {:?}",
            building[0]
        );
    }
}

#[test]
fn every_platform_the_manifest_claims_is_served_by_a_shim_it_can_run() {
    let interpreters = [("macos", "sh"), ("linux", "sh"), ("windows", "powershell")];
    for platform in declared_platforms() {
        let (_, interpreter) = interpreters
            .iter()
            .find(|(name, _)| *name == platform)
            .unwrap_or_else(|| panic!("no interpreter is known for {}", platform));
        for section in ["build", "startup"] {
            for entry in entries(section) {
                if !platforms(&entry).contains(&platform) {
                    continue;
                }
                assert_eq!(
                    &argv(&entry)[0],
                    interpreter,
                    "`sh` cannot run on Windows at all, so a shim declared for the \
                     wrong platform never executes and the asset built for that \
                     user is unreachable: {} on {}",
                    section,
                    platform
                );
            }
        }
    }
}

#[test]
fn windows_is_claimed_because_the_release_publishes_assets_for_it() {
    assert!(
        declared_platforms().contains(&"windows".to_string()),
        "the release workflow builds six targets, and holding the declaration \
         would publish two assets nobody can install"
    );
}

#[test]
fn the_only_control_this_plugin_has_is_declared_as_an_action() {
    let ours: Vec<toml::Value> = entries("actions")
        .into_iter()
        .filter(|entry| entry["id"].as_str() == Some(TOGGLE_ACTION))
        .collect();
    assert_eq!(
        ours.len(),
        1,
        "a manifest cannot declare a keybinding, so the action is the whole of what \
         this plugin can offer the user to switch orders with"
    );
    assert!(
        argv(&ours[0]).iter().any(|a| a == TOGGLE_FLAG),
        "the manifest and the binary have to name the same flag, or Herdr dispatches \
         an argument the binary refuses: {:?}",
        argv(&ours[0])
    );
}

#[test]
fn no_two_actions_share_an_id_whatever_their_platforms_say() {
    let mut ids: Vec<String> = entries("actions")
        .iter()
        .map(|entry| entry["id"].as_str().unwrap().to_string())
        .collect();
    let declared = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(
        ids.len(),
        declared,
        "measured against Herdr 0.9.1 on 2026-09-17: two [[actions]] entries sharing \
         an id are refused with `duplicate_plugin_action_id` even when their \
         `platforms` lists are disjoint, and the whole manifest then fails to load. \
         Doubling an action per platform is fatal where doubling a build step is not."
    );
}

#[test]
fn the_toggle_is_dispatched_through_the_launcher_and_not_a_build_artifact() {
    for entry in entries("actions") {
        let command = argv(&entry);
        assert!(
            command.iter().any(|a| a.starts_with("bin/launcher")),
            "the launcher is what rebuilds a stale binary before it execs one: {:?}",
            command
        );
        assert!(
            !command.iter().any(|a| a.contains("target/")),
            "a build-artifact path breaks the moment the profile changes: {:?}",
            command
        );
    }
}

#[test]
fn the_readme_names_the_action_the_way_a_keybinding_has_to_name_it() {
    let bindable = format!(
        "{}.{}",
        manifest()["id"].as_str().unwrap(),
        entries("actions")[0]["id"].as_str().unwrap()
    );
    assert!(
        read_repo_file("README.md").contains(&bindable),
        "a user binds `{}` by hand, so a README naming anything else binds nothing",
        bindable
    );
}

#[test]
fn no_event_hook_is_subscribed() {
    assert!(
        !manifest().contains_key("events"),
        "Herdr 0.9.0 gates dispatch on the origin of a change, so no event kind sees UI focus"
    );
}

#[test]
fn no_trace_of_the_python_watcher_is_left_in_the_manifest() {
    let text = read_repo_file("herdr-plugin.toml");
    assert!(!text.contains("python"), "{}", text);
    assert!(!text.contains("watch-focus"), "{}", text);
}

#[test]
fn no_file_in_the_repo_names_a_release_artifact_path() {
    for name in [
        "herdr-plugin.toml",
        "README.md",
        "defaults.toml",
        "docs/herdr-behaviour.md",
    ] {
        assert!(
            !read_repo_file(name).contains("target/release"),
            "{} names a build artifact",
            name
        );
    }
}

#[test]
fn the_manifest_and_the_crate_agree_on_the_version() {
    let parsed = read_repo_file("Cargo.toml").parse::<toml::Table>().unwrap();
    let crate_version = parsed["package"]["version"].as_str().unwrap().to_string();
    assert_eq!(manifest()["version"].as_str(), Some(crate_version.as_str()));
}

#[test]
fn every_shim_named_in_the_manifest_is_there() {
    for section in ["build", "startup", "actions"] {
        for entry in entries(section) {
            let command = argv(&entry);
            let named = command
                .iter()
                .find(|a| a.starts_with("bin/"))
                .unwrap_or_else(|| panic!("{} names no shim: {:?}", section, command));
            let path = manifest_dir().join(named);
            assert!(
                path.is_file(),
                "Herdr dispatches this and finds nothing: {}",
                path.display()
            );
        }
    }
}

#[test]
fn the_shims_sh_dispatches_carry_the_executable_bit_the_kit_gave_them() {
    for shim in ["bin/build", "bin/launcher", "bin/find-cargo"] {
        let path = manifest_dir().join(shim);
        let mode = std::os::unix::fs::PermissionsExt::mode(
            &std::fs::metadata(&path).unwrap().permissions(),
        );
        assert_eq!(mode & 0o111, 0o111, "{} is not executable", path.display());
    }
}

#[test]
fn the_shipped_defaults_keep_the_comments_that_are_their_interface() {
    let text = read_repo_file("defaults.toml");
    let comments = text
        .lines()
        .filter(|l| l.trim_start().starts_with('#'))
        .count();
    assert!(
        comments >= 20,
        "defaults.toml is edited by hand, so its comments are its interface: {} left",
        comments
    );
}

#[test]
fn no_rust_source_file_carries_a_comment() {
    for path in rust_files() {
        let text = std::fs::read_to_string(&path).unwrap();
        for (n, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            assert!(
                !(trimmed.starts_with("//") || trimmed.starts_with("/*")),
                "{}:{} carries a comment: {}",
                path.display(),
                n + 1,
                line
            );
            if let Some(at) = line.find("//") {
                assert!(
                    line[..at].contains('"') || line[..at].contains(':'),
                    "{}:{} carries a trailing comment: {}",
                    path.display(),
                    n + 1,
                    line
                );
            }
        }
    }
}

#[test]
fn every_rust_source_file_is_covered_by_that_guard() {
    let found: Vec<String> = rust_files()
        .iter()
        .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
        .collect();
    for name in [
        "build.rs",
        "main.rs",
        "lib.rs",
        "claim.rs",
        "config.rs",
        "order.rs",
        "promote.rs",
        "retire.rs",
        "version.rs",
        "mod.rs",
        "ordering.rs",
        "promotion.rs",
        "retirement.rs",
        "settings.rs",
        "packaging.rs",
    ] {
        assert!(
            found.contains(&name.to_string()),
            "{} was not scanned",
            name
        );
    }
}

#[test]
fn two_temporary_directories_never_share_a_path() {
    let one = TempDir::new();
    let two = TempDir::new();
    assert_ne!(one.path(), two.path());
    let name = one
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    assert_ne!(
        name,
        format!("recent-spaces-t{}", std::process::id()),
        "a path keyed on the process id alone collides between test binaries"
    );
}

const CI_WORKFLOW: &str = ".github/workflows/ci.yml";

const RELEASE_WORKFLOW: &str = ".github/workflows/release.yml";

const KIT: &str = "mike-bronner/herdr-plugin-kit";

fn called_workflows(workflow: &str) -> Vec<String> {
    read_repo_file(workflow)
        .lines()
        .filter_map(|line| line.strip_prefix("    uses: "))
        .map(str::to_string)
        .collect()
}

fn jobs_section(workflow: &str) -> Vec<String> {
    let text = read_repo_file(workflow);
    let body: Vec<String> = text
        .lines()
        .skip_while(|line| *line != "jobs:")
        .skip(1)
        .map(str::to_string)
        .collect();
    assert!(!body.is_empty(), "{} declares no jobs at all", workflow);
    body
}

fn workflow_job(workflow: &str, job: &str) -> String {
    let opener = format!("  {}:", job);
    let section = jobs_section(workflow);
    let body: Vec<&str> = section
        .iter()
        .map(String::as_str)
        .skip_while(|line| *line != opener)
        .skip(1)
        .take_while(|line| line.trim().is_empty() || line.starts_with("   "))
        .collect();
    assert!(
        !body.is_empty(),
        "{} declares no job named {} below its jobs: key, and a search that \
         started above it would answer with the on: trigger of the same name",
        workflow,
        job
    );
    body.join("\n")
}

fn checkout_refs(workflow: &str) -> Vec<String> {
    read_repo_file(workflow)
        .lines()
        .filter_map(|line| line.trim().strip_prefix("ref: "))
        .map(str::to_string)
        .collect()
}

fn workflow_steps(workflow: &str) -> String {
    jobs_section(workflow)
        .iter()
        .filter(|line| !line.trim_start().starts_with('#'))
        .cloned()
        .collect::<Vec<String>>()
        .join("\n")
}

fn pinned_ref(reference: &str) -> String {
    reference
        .rsplit_once('@')
        .unwrap_or_else(|| panic!("{} names no ref, so it tracks a moving branch", reference))
        .1
        .to_string()
}

#[test]
fn ci_keeps_its_own_gates_rather_than_calling_for_them() {
    let called = called_workflows(CI_WORKFLOW);
    assert!(
        called.is_empty(),
        "plugin-ci.yml was withdrawn in kit 0.4.0 and did not come back, because a \
         called workflow cannot discover which of its own versions a caller pinned: \
         {} calls {:?}",
        CI_WORKFLOW,
        called
    );
}

#[test]
fn the_release_calls_the_kit_rather_than_carrying_a_job_of_its_own() {
    assert_eq!(
        called_workflows(RELEASE_WORKFLOW),
        vec![format!(
            "{}/.github/workflows/plugin-release.yml@0.5.2",
            KIT
        )],
        "the kit is the one place that publishes, and it is what refuses the wrong \
         tag form; a job rewritten here is the duplication three plugins pay for \
         and the copy the kit's own tests cannot cover"
    );
}

#[test]
fn ci_checks_the_kit_out_at_the_pin_this_crate_compiles_against() {
    let text = read_repo_file(CI_WORKFLOW);
    assert!(
        text.contains(&format!("repository: {}", KIT)),
        "the gates live in the kit, so it has to reach the runner: {}",
        CI_WORKFLOW
    );
    assert!(
        text.contains("ref: ${{ steps.kit.outputs.tag }}"),
        "the tag the crate compiles against is the tag the gates come from, and \
         anything else checks this tree against a kit it never uses: {}",
        CI_WORKFLOW
    );
}

#[test]
fn ci_reads_the_kit_pin_from_cargo_rather_than_from_the_toml() {
    let job = workflow_job(CI_WORKFLOW, "kit-gates");
    assert!(
        job.contains("cargo metadata --no-deps --format-version 1"),
        "a regex over Cargo.toml cannot see a workspace-inherited dependency, and \
         cargo answers with no network and no lockfile: {}",
        job
    );
    assert!(
        job.contains("exit 1"),
        "a branch, a commit or a path pin has no version to check against, and the \
         gate has to stop rather than check out nothing: {}",
        job
    );
}

#[test]
fn ci_takes_the_whole_tag_history_because_the_version_gate_reads_it() {
    let job = workflow_job(CI_WORKFLOW, "kit-gates");
    assert!(
        job.contains("fetch-depth: 0"),
        "a shallow clone lets the version gate pass by seeing no releases at all, \
         which reports green rather than reporting nothing: {}",
        job
    );
}

#[test]
fn ci_runs_both_of_the_gates_the_kit_publishes() {
    let job = workflow_job(CI_WORKFLOW, "kit-gates");
    for gate in [
        "python3 kit/templates/sync_bin.py . --check",
        "python3 kit/tools/plugin_gate.py versions .",
    ] {
        assert!(
            job.contains(gate),
            "without it the kit is a suggestion, and a manifest that disagrees with \
             its tag 404s on every install and then compiles in silence: {} is \
             missing {}",
            CI_WORKFLOW,
            gate
        );
    }
}

#[test]
fn ci_runs_this_plugins_suite_on_macos_because_it_cannot_run_anywhere_else() {
    let job = workflow_job(CI_WORKFLOW, "gates");
    assert!(
        job.contains("runs-on: macos-latest"),
        "the suite writes under /private/tmp, which does not exist on Linux: {}",
        job
    );
    assert!(
        job.contains("cargo test --locked"),
        "the job pinned to macOS has to be the one that runs the suite: {}",
        job
    );
}

#[test]
fn ci_compiles_every_target_the_release_publishes() {
    let job = workflow_job(CI_WORKFLOW, "build");
    assert!(
        job.contains("include: ${{ fromJSON(needs.kit-gates.outputs.matrix) }}"),
        "nobody on this project has Windows hardware, so the compiler is the only \
         thing that ever checks those paths, and it has to check them before a \
         release rather than during one: {}",
        job
    );
    assert!(
        job.contains("cargo build --locked --target"),
        "the question this job answers is whether the tree compiles and links for \
         each target: {}",
        job
    );
}

#[test]
fn ci_is_triggered_by_a_push_as_well_as_a_pull_request() {
    let triggers: Vec<String> = read_repo_file(CI_WORKFLOW)
        .lines()
        .map(|line| line.trim().to_string())
        .skip_while(|line| line != "on:")
        .take_while(|line| line != "jobs:")
        .collect();
    for event in ["push:", "pull_request:"] {
        assert!(
            triggers.contains(&event.to_string()),
            "a pull request that cannot compute a merge ref against main has its \
             pull_request workflows skipped entirely, with no run and no error: {:?}",
            triggers
        );
    }
}

#[test]
fn the_release_builds_nothing_publishes_nothing_and_names_nothing_itself() {
    let steps = workflow_steps(RELEASE_WORKFLOW);
    for job_of_its_own in [
        "cargo build",
        "rustup target add",
        "upload-artifact",
        "download-artifact",
        "gh release upload",
        "sha256sum",
        "plugin_gate.py",
        "cargo metadata",
        "strategy:",
        "runs-on:",
        "steps:",
    ] {
        assert!(
            !steps.contains(job_of_its_own),
            "a release job maintained once and consumed by three plugins is the \
             duplication this call exists to end, and a second copy here is the one \
             the kit's own tests cannot cover: {} still runs {}\n{}",
            RELEASE_WORKFLOW,
            job_of_its_own,
            steps
        );
    }
}

#[test]
fn neither_workflow_writes_a_target_triple_or_composes_an_asset_name() {
    for workflow in [CI_WORKFLOW, RELEASE_WORKFLOW] {
        let steps = workflow_steps(workflow);
        for triple in ["-apple-darwin", "-unknown-linux-", "-pc-windows-"] {
            assert!(
                !steps.contains(triple),
                "a second copy of the target table can disagree with the one the \
                 kit's own tests cover: {} writes {}",
                workflow,
                triple
            );
        }
        assert!(
            !steps.contains("cut -c1-12"),
            "an asset name composed here is the producing half of a two-sided \
             agreement written a second time, and the two disagree as a 404 and a \
             silent compile that nobody sees: {}",
            workflow
        );
    }
}

#[test]
fn the_release_caller_grants_the_permission_the_called_workflow_cannot_raise() {
    let text = read_repo_file(RELEASE_WORKFLOW);
    assert!(
        text.contains("\npermissions:\n  contents: write\n"),
        "the kit's documented caller grants it at the workflow too, so a job added \
         later inherits it rather than silently publishing nothing: {}",
        text
    );
    assert!(
        workflow_job(RELEASE_WORKFLOW, "release").contains("contents: write"),
        "a called workflow runs on the caller's permissions and cannot raise its \
         own, so without the job's own grant the run fails before it starts: {}",
        text
    );
}

#[test]
fn every_checkout_ref_in_ci_is_computed_rather_than_written() {
    for reference in checkout_refs(CI_WORKFLOW) {
        assert!(
            reference.starts_with("${{"),
            "a kit version written here is a second pin that can disagree with \
             Cargo.toml, and this tree would then be checked against one kit while \
             compiling against another: {} pins {}",
            CI_WORKFLOW,
            reference
        );
    }
}

#[test]
fn the_release_caller_and_the_crate_pin_one_kit_between_them() {
    let pins = read_repo_file("Cargo.toml")
        .lines()
        .filter(|line| line.contains(KIT))
        .filter_map(|line| line.split_once("tag = \""))
        .filter_map(|(_, rest)| rest.split('"').next().map(str::to_string))
        .collect::<Vec<String>>();
    assert_eq!(
        pins.len(),
        2,
        "the shipped crate and the build-script helper both come from the kit: {:?}",
        pins
    );

    let mut every = pins;
    for reference in called_workflows(RELEASE_WORKFLOW) {
        assert!(
            reference.starts_with(KIT),
            "{} calls something that is not the kit: {}",
            RELEASE_WORKFLOW,
            reference
        );
        every.push(pinned_ref(&reference));
    }
    assert_eq!(
        every.len(),
        3,
        "two crate pins and one caller ref are the three this repository carries, \
         and a missing one is a pin that stopped being checked: {:?}",
        every
    );

    every.sort();
    every.dedup();
    assert_eq!(
        every.len(),
        1,
        "the caller runs one kit's release workflow and the crate compiles against \
         another kit's tools, so two pins that disagree publish assets named by a \
         plugin_gate.py this tree never compiled against: {:?}",
        every
    );
}

#[test]
fn the_kit_is_pinned_by_tag_and_never_by_a_moving_reference() {
    for line in read_repo_file("Cargo.toml")
        .lines()
        .filter(|line| line.contains(KIT))
    {
        assert!(
            line.contains("tag = \""),
            "the kit's release workflow resolves this pin with cargo metadata and \
             refuses a branch, a commit, a bare git source and a path pin by name, \
             so anything else stops the release rather than degrading quietly: {}",
            line
        );
        for moving in ["branch = \"", "rev = \"", "path = \""] {
            assert!(
                !line.contains(moving),
                "a second source key beside the tag is what cargo resolves instead \
                 of it, and the release job has no version left to check against: {}",
                line
            );
        }
    }
}
