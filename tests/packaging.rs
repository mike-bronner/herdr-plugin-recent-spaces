mod support;

use std::path::PathBuf;

use support::*;

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
    for section in ["build", "startup"] {
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
        "promote.rs",
        "retire.rs",
        "version.rs",
        "mod.rs",
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
        .filter_map(|line| line.trim().strip_prefix("uses: "))
        .map(str::to_string)
        .collect()
}

fn pinned_ref(reference: &str) -> String {
    reference
        .rsplit_once('@')
        .unwrap_or_else(|| panic!("{} names no ref, so it tracks a moving branch", reference))
        .1
        .to_string()
}

#[test]
fn ci_calls_the_kit_and_keeps_no_gate_of_its_own() {
    assert_eq!(
        called_workflows(CI_WORKFLOW),
        vec![format!("{}/.github/workflows/plugin-ci.yml@0.2.0", KIT)],
        "a second copy of the gates here would be one more thing to keep in step \
         with the kit, and it could not be tested from this repository"
    );
    assert!(
        !read_repo_file(CI_WORKFLOW).contains("run: cargo"),
        "a cargo command left behind would run beside the kit's own and could \
         disagree with it"
    );
}

#[test]
fn ci_runs_this_plugins_suite_on_macos_because_it_cannot_run_anywhere_else() {
    let text = read_repo_file(CI_WORKFLOW);
    assert!(
        text.contains("test_os: macos-latest"),
        "the suite writes under /private/tmp, which does not exist on Linux, and \
         the kit's default runner is ubuntu-latest: {}",
        text
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
fn the_release_caller_grants_the_permission_the_called_workflow_cannot_raise() {
    assert_eq!(
        called_workflows(RELEASE_WORKFLOW),
        vec![format!(
            "{}/.github/workflows/plugin-release.yml@0.2.0",
            KIT
        )],
        "the kit is the one place that publishes, and it is what refuses the wrong \
         tag form"
    );
    let text = read_repo_file(RELEASE_WORKFLOW);
    assert!(
        text.contains("\n    permissions:\n      contents: write\n"),
        "the job's own grant is what a called workflow runs on, and it cannot \
         raise its own, so without this the release fails before it starts: {}",
        text
    );
    assert!(
        text.contains("\npermissions:\n  contents: write\n"),
        "the kit's documented caller grants it at the workflow too, so a job \
         added later inherits it rather than silently publishing nothing: {}",
        text
    );
}

#[test]
fn both_callers_and_the_crate_pin_the_same_kit() {
    let crate_pin = read_repo_file("Cargo.toml")
        .lines()
        .filter_map(|line| line.split_once("tag = \""))
        .filter_map(|(_, rest)| rest.split('"').next().map(str::to_string))
        .collect::<Vec<String>>();
    assert!(!crate_pin.is_empty(), "Cargo.toml pins no kit tag");

    let mut every: Vec<String> = crate_pin;
    for workflow in [CI_WORKFLOW, RELEASE_WORKFLOW] {
        for reference in called_workflows(workflow) {
            assert!(
                reference.starts_with(KIT),
                "{} calls something that is not the kit: {}",
                workflow,
                reference
            );
            every.push(pinned_ref(&reference));
        }
    }
    every.sort();
    every.dedup();
    assert_eq!(
        every.len(),
        1,
        "the workflows check this plugin against the kit they name and the code \
         compiles against the kit Cargo.toml names, so two pins that disagree \
         check one tree against another kit: {:?}",
        every
    );
}
