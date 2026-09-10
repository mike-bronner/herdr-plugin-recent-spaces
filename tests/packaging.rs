mod support;

use std::path::{Path, PathBuf};
use std::process::Command;

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
fn it_starts_exactly_one_watcher() {
    let parsed = manifest();
    let startup = parsed["startup"].as_array().unwrap();
    assert_eq!(
        startup.len(),
        1,
        "two watchers would fight over the sidebar order and hold two of the 32 slots"
    );
    assert_eq!(argv(&startup[0]), vec!["sh", "bin/watch"]);
}

#[test]
fn the_watcher_is_dispatched_through_the_shim_and_not_a_build_artifact() {
    let parsed = manifest();
    let command = argv(&parsed["startup"].as_array().unwrap()[0]);
    assert!(
        !command.iter().any(|a| a.contains("target/")),
        "a build-artifact path breaks the moment the profile changes"
    );
}

#[test]
fn the_manifest_declares_a_build_step_so_a_github_install_shows_one() {
    let parsed = manifest();
    let build = parsed["build"].as_array().unwrap();
    assert_eq!(build.len(), 1);
    assert_eq!(argv(&build[0]), vec!["sh", "bin/build"]);
}

#[test]
fn no_event_hook_is_subscribed() {
    assert!(
        !manifest().contains_key("events"),
        "Herdr 0.9.0 gates dispatch on the origin of a change, so no event kind sees UI focus"
    );
}

#[test]
fn nothing_in_the_manifest_dispatches_an_interpreter() {
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
fn the_shim_named_in_the_manifest_is_there_and_runnable() {
    let parsed = manifest();
    let command = argv(&parsed["startup"].as_array().unwrap()[0]);
    let path = manifest_dir().join(&command[1]);
    assert!(path.is_file(), "{}", path.display());
    let mode =
        std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(&path).unwrap().permissions());
    assert_eq!(mode & 0o111, 0o111, "{} is not executable", path.display());
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
        "api.rs",
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

fn fake_root(dir: &TempDir, real_build: bool) -> PathBuf {
    let root = dir.dir("plugin");
    std::fs::create_dir_all(root.join("bin")).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("target/release")).unwrap();
    std::fs::copy(manifest_dir().join("bin/watch"), root.join("bin/watch")).unwrap();
    if real_build {
        std::fs::copy(manifest_dir().join("bin/build"), root.join("bin/build")).unwrap();
    }
    std::fs::write(root.join("Cargo.toml"), "").unwrap();
    std::fs::write(root.join("Cargo.lock"), "").unwrap();
    std::fs::write(root.join("src/main.rs"), "").unwrap();
    root
}

fn executable(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, body).unwrap();
    let mut perms = std::fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).unwrap();
}

struct Shim {
    status: i32,
    stdout: String,
    stderr: String,
}

fn run_script(root: &Path, script: &str, args: &[&str], extra: &[(&str, &str)]) -> Shim {
    let mut command = Command::new("/bin/sh");
    command
        .arg(root.join(script))
        .args(args)
        .env_clear()
        .env("PATH", LAUNCHD_PATH)
        .env("HOME", "/private/tmp")
        .env("HERDR_PLUGIN_ROOT", root);
    for (key, value) in extra {
        command.env(key, value);
    }
    let out = command.output().expect("cannot run the script");
    Shim {
        status: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
    }
}

fn run_shim(root: &Path, extra: &[(&str, &str)]) -> Shim {
    run_script(root, "bin/watch", &[], extra)
}

fn run_build(root: &Path, extra: &[(&str, &str)]) -> Shim {
    run_script(root, "bin/build", &[], extra)
}

fn current(root: &Path) {
    for named in ["src/main.rs", "src", "Cargo.toml", "Cargo.lock"] {
        set_mtime(&root.join(named), 1000);
    }
    set_mtime(&root.join("target/release/watch"), 2000);
}

#[test]
fn the_shim_runs_the_binary_and_builds_nothing_when_it_is_current() {
    let dir = TempDir::new();
    let root = fake_root(&dir, false);
    executable(&root.join("bin/build"), "#!/bin/sh\necho BUILT >&2\n");
    executable(&root.join("target/release/watch"), "#!/bin/sh\necho RAN\n");
    current(&root);

    let run = run_shim(&root, &[]);
    assert_eq!(run.status, 0, "{}", run.stderr);
    assert_eq!(run.stdout, "RAN\n");
    assert!(!run.stderr.contains("BUILT"), "{}", run.stderr);
}

#[test]
fn the_shim_rebuilds_when_a_source_file_is_newer_than_the_binary() {
    let dir = TempDir::new();
    let root = fake_root(&dir, false);
    executable(&root.join("bin/build"), "#!/bin/sh\necho BUILT >&2\n");
    executable(&root.join("target/release/watch"), "#!/bin/sh\necho RAN\n");
    current(&root);
    set_mtime(&root.join("src/main.rs"), 3000);

    let run = run_shim(&root, &[]);
    assert_eq!(run.status, 0, "{}", run.stderr);
    assert!(run.stderr.contains("BUILT"), "{}", run.stderr);
    assert_eq!(run.stdout, "RAN\n", "it still runs afterwards");
}

#[test]
fn the_shim_rebuilds_when_the_manifest_or_the_lockfile_moves() {
    for changed in ["Cargo.toml", "Cargo.lock"] {
        let dir = TempDir::new();
        let root = fake_root(&dir, false);
        executable(&root.join("bin/build"), "#!/bin/sh\necho BUILT >&2\n");
        executable(&root.join("target/release/watch"), "#!/bin/sh\n");
        current(&root);
        set_mtime(&root.join(changed), 3000);
        assert!(run_shim(&root, &[]).stderr.contains("BUILT"), "{}", changed);
    }
}

#[test]
fn the_shim_builds_when_there_is_no_binary_at_all() {
    let dir = TempDir::new();
    let root = fake_root(&dir, false);
    executable(
        &root.join("bin/build"),
        "#!/bin/sh\nprintf '#!/bin/sh\\necho RAN\\n' > \"$HERDR_PLUGIN_ROOT/target/release/watch\"\nchmod +x \"$HERDR_PLUGIN_ROOT/target/release/watch\"\n",
    );
    let run = run_shim(&root, &[]);
    assert_eq!(run.status, 0, "{}", run.stderr);
    assert_eq!(run.stdout, "RAN\n");
}

#[test]
fn a_failed_build_runs_the_binary_already_there_and_says_it_may_be_stale() {
    let dir = TempDir::new();
    let root = fake_root(&dir, false);
    executable(&root.join("bin/build"), "#!/bin/sh\nexit 1\n");
    executable(&root.join("target/release/watch"), "#!/bin/sh\necho RAN\n");
    current(&root);
    set_mtime(&root.join("src/main.rs"), 3000);

    let run = run_shim(&root, &[]);
    assert_eq!(run.status, 0, "{}", run.stderr);
    assert_eq!(run.stdout, "RAN\n");
    assert!(run.stderr.contains("may be stale"), "{}", run.stderr);
}

#[test]
fn a_failed_build_with_no_binary_at_all_refuses_and_names_the_path() {
    let dir = TempDir::new();
    let root = fake_root(&dir, false);
    executable(&root.join("bin/build"), "#!/bin/sh\nexit 1\n");
    let run = run_shim(&root, &[]);
    assert_eq!(run.status, 1);
    assert!(
        run.stderr.contains("target/release/watch"),
        "{}",
        run.stderr
    );
}

#[test]
fn the_shim_reports_the_version_without_building_anything() {
    let dir = TempDir::new();
    let root = fake_root(&dir, false);
    executable(&root.join("bin/build"), "#!/bin/sh\necho BUILT >&2\n");
    executable(
        &root.join("target/release/watch"),
        "#!/bin/sh\necho \"watch 9.9.9\"\n",
    );
    set_mtime(&root.join("target/release/watch"), 1000);
    set_mtime(&root.join("src/main.rs"), 3000);

    let run = run_script(&root, "bin/watch", &["--version"], &[]);
    assert_eq!(run.status, 0, "{}", run.stderr);
    assert_eq!(run.stdout, "watch 9.9.9\n");
    assert!(
        !run.stderr.contains("BUILT"),
        "a rebuild would hide the staleness the version exists to show: {}",
        run.stderr
    );
}

#[test]
fn asking_the_shim_for_the_version_with_no_binary_refuses_without_building() {
    let dir = TempDir::new();
    let root = fake_root(&dir, false);
    executable(&root.join("bin/build"), "#!/bin/sh\necho BUILT >&2\n");

    let run = run_script(&root, "bin/watch", &["--version"], &[]);
    assert_eq!(run.status, 1);
    assert_eq!(run.stdout, "");
    assert!(
        run.stderr.contains("target/release/watch"),
        "{}",
        run.stderr
    );
    assert!(!run.stderr.contains("BUILT"), "{}", run.stderr);
}

#[test]
fn the_shim_builds_as_usual_for_every_other_argument() {
    let dir = TempDir::new();
    let root = fake_root(&dir, false);
    executable(&root.join("bin/build"), "#!/bin/sh\necho BUILT >&2\n");
    executable(&root.join("target/release/watch"), "#!/bin/sh\necho RAN\n");
    set_mtime(&root.join("target/release/watch"), 1000);
    set_mtime(&root.join("src/main.rs"), 3000);

    let run = run_script(&root, "bin/watch", &["--version-ish"], &[]);
    assert!(run.stderr.contains("BUILT"), "{}", run.stderr);
}

#[test]
fn the_shim_hands_its_arguments_to_the_binary() {
    let dir = TempDir::new();
    let root = fake_root(&dir, false);
    executable(&root.join("bin/build"), "#!/bin/sh\n");
    executable(
        &root.join("target/release/watch"),
        "#!/bin/sh\nprintf '%s\\n' \"$@\"\n",
    );
    current(&root);

    let run = run_script(&root, "bin/watch", &["--one", "two words"], &[]);
    assert_eq!(run.stdout, "--one\ntwo words\n");
}

fn fake_cargo(dir: &TempDir, rel: &str, log: &Path) -> PathBuf {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    executable(
        &path,
        &format!(
            "#!/bin/sh\nprintf '%s\\n%s\\n' \"$0\" \"$PATH\" > '{}'\n",
            log.to_string_lossy()
        ),
    );
    path
}

#[test]
fn the_build_script_prepends_the_cargo_directory_to_the_path_it_builds_under() {
    let dir = TempDir::new();
    let root = fake_root(&dir, true);
    let log = dir.join("cargo-log");
    let cargo = fake_cargo(&dir, "rustup/bin/cargo", &log);
    let with_cargo = format!(
        "{}:{}",
        cargo.parent().unwrap().to_string_lossy(),
        LAUNCHD_PATH
    );

    let run = run_build(&root, &[("PATH", &with_cargo)]);
    assert_eq!(run.status, 0, "{}", run.stderr);
    let recorded = std::fs::read_to_string(&log).unwrap();
    let mut lines = recorded.lines();
    assert_eq!(lines.next().unwrap(), cargo.to_string_lossy());
    let path = lines.next().unwrap();
    assert!(
        path.starts_with(&cargo.parent().unwrap().to_string_lossy().to_string()),
        "cargo execs rustc from its own directory, so that directory must lead the PATH: {}",
        path
    );
}

#[test]
fn the_build_script_finds_cargo_off_the_path_when_the_launchd_path_hides_it() {
    let dir = TempDir::new();
    let root = fake_root(&dir, true);
    let log = dir.join("cargo-log");
    let cargo = fake_cargo(&dir, "elsewhere/cargo", &log);

    let run = run_build(&root, &[("CARGO", cargo.to_str().unwrap())]);
    assert_eq!(run.status, 0, "{}", run.stderr);
    let recorded = std::fs::read_to_string(&log).unwrap();
    assert!(
        recorded.starts_with(&cargo.to_string_lossy().to_string()),
        "{}",
        recorded
    );
}

#[test]
fn the_build_script_prefers_cargo_on_the_path_over_the_named_fallbacks() {
    let dir = TempDir::new();
    let root = fake_root(&dir, true);
    let wanted = dir.join("wanted-log");
    let ignored = dir.join("ignored-log");
    let on_path = fake_cargo(&dir, "onpath/cargo", &wanted);
    let fallback = fake_cargo(&dir, "fallback/cargo", &ignored);
    let with_cargo = format!(
        "{}:{}",
        on_path.parent().unwrap().to_string_lossy(),
        LAUNCHD_PATH
    );

    run_build(
        &root,
        &[("PATH", &with_cargo), ("CARGO", fallback.to_str().unwrap())],
    );
    assert!(wanted.exists(), "the cargo on the PATH must win");
    assert!(!ignored.exists());
}

#[test]
fn the_build_script_searches_for_cargo_in_the_measured_order() {
    let script = read_repo_file("bin/build");
    let order = [
        "${CARGO:-}",
        "${CARGO_HOME:-$HOME/.cargo}/bin/cargo",
        "$HOME/.cargo/bin/cargo",
        "/opt/homebrew/opt/rustup/bin/cargo",
        "/opt/homebrew/bin/cargo",
        "/usr/local/opt/rustup/bin/cargo",
        "/usr/local/bin/cargo",
    ];
    let mut at = script
        .find("command -v cargo")
        .expect("the PATH is searched first");
    for candidate in order {
        let next = script[at..]
            .find(candidate)
            .unwrap_or_else(|| panic!("bin/build never names {}", candidate));
        at += next + candidate.len();
    }
}

#[test]
fn the_build_script_carries_the_message_for_a_machine_with_no_toolchain() {
    let script = read_repo_file("bin/build");
    assert!(script.contains("cargo not found"), "{}", script);
    assert!(script.contains("then run"), "{}", script);
    assert!(script.contains("cargo build --release"), "{}", script);
    assert!(script.contains("$PLUGIN_ROOT"), "{}", script);
}

#[test]
fn the_build_script_says_which_cargo_it_found_and_lets_cargo_print_its_own_output() {
    let dir = TempDir::new();
    let root = fake_root(&dir, true);
    let cargo_dir = dir.dir("bin");
    executable(
        &cargo_dir.join("cargo"),
        "#!/bin/sh\necho 'Compiling recent-spaces'\n",
    );
    let with_cargo = format!("{}:{}", cargo_dir.to_string_lossy(), LAUNCHD_PATH);

    let run = run_build(&root, &[("PATH", &with_cargo)]);
    assert_eq!(run.status, 0, "{}", run.stderr);
    assert!(
        run.stderr.contains("building the watcher with"),
        "{}",
        run.stderr
    );
    assert!(
        run.stderr.contains("Compiling recent-spaces"),
        "cargo's own output is not captured: {}",
        run.stderr
    );
    assert_eq!(run.stdout, "", "a startup command's stdout is not a log");
}

#[test]
fn the_build_script_carries_no_spinner_and_nothing_that_served_one() {
    let script = read_repo_file("bin/build");
    for gone in ["trap", "tput", "stty", "mktemp", "\\033"] {
        assert!(
            !script.contains(gone),
            "{} is left over from the spinner",
            gone
        );
    }
}

#[test]
fn a_failing_cargo_is_reported_rather_than_silently_producing_nothing() {
    let dir = TempDir::new();
    let root = fake_root(&dir, true);
    let cargo_dir = dir.dir("bin");
    executable(&cargo_dir.join("cargo"), "#!/bin/sh\nexit 3\n");
    let with_cargo = format!("{}:{}", cargo_dir.to_string_lossy(), LAUNCHD_PATH);

    let run = run_build(&root, &[("PATH", &with_cargo)]);
    assert_eq!(run.status, 1);
    assert!(
        run.stderr.contains("cargo build --release failed"),
        "{}",
        run.stderr
    );
}

#[test]
fn the_build_script_names_the_manifest_it_is_building() {
    let dir = TempDir::new();
    let root = fake_root(&dir, true);
    let log = dir.join("cargo-args");
    let cargo_dir = dir.dir("bin");
    executable(
        &cargo_dir.join("cargo"),
        &format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\n",
            log.to_string_lossy()
        ),
    );
    let with_cargo = format!("{}:{}", cargo_dir.to_string_lossy(), LAUNCHD_PATH);

    run_build(&root, &[("PATH", &with_cargo)]);
    let args: Vec<String> = std::fs::read_to_string(&log)
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect();
    assert_eq!(
        args,
        vec![
            "build".to_string(),
            "--release".to_string(),
            "--manifest-path".to_string(),
            root.join("Cargo.toml").to_string_lossy().to_string()
        ]
    );
}
