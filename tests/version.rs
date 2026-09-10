mod support;

use std::path::{Path, PathBuf};
use std::process::Command;

use recent_spaces::version::{self, Manifest, BUILT, COMMIT, CRATE_VERSION, UNKNOWN_COMMIT};
use support::*;

const BINARY: &str = env!("CARGO_BIN_EXE_watch");

struct Run {
    status: i32,
    stdout: String,
    stderr: String,
}

fn bounded(command: &mut Command) -> Run {
    let mut child = command
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("cannot run the watcher");

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if child
            .try_wait()
            .expect("cannot check the watcher")
            .is_some()
        {
            break;
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            panic!("the version query never exited, so it is polling instead of reporting");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    let out = child
        .wait_with_output()
        .expect("cannot read what the watcher said");
    Run {
        status: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
    }
}

fn ask(binary: &Path, extra: &[(&str, &str)]) -> Run {
    let mut command = Command::new(binary);
    command
        .arg("--version")
        .env_clear()
        .env("PATH", LAUNCHD_PATH)
        .env("HOME", "/private/tmp");
    for (key, value) in extra {
        command.env(key, value);
    }
    bounded(&mut command)
}

fn run_version(extra: &[(&str, &str)]) -> Run {
    ask(Path::new(BINARY), extra)
}

fn looks_like_a_timestamp(text: &str) -> bool {
    let shape = "0000-00-00T00:00:00Z";
    if text.len() != shape.len() {
        return false;
    }
    text.chars()
        .zip(shape.chars())
        .all(|(got, want)| match want {
            '0' => got.is_ascii_digit(),
            other => got == other,
        })
}

fn header_of(report: &str) -> String {
    report.lines().next().unwrap().to_string()
}

fn manifest_of(report: &str) -> String {
    report.lines().nth(1).unwrap().to_string()
}

fn in_parentheses(header: &str) -> String {
    let opened = header.find('(').expect(header) + 1;
    let closed = header.rfind(')').expect(header);
    header[opened..closed].to_string()
}

fn git_out(root: &Path, args: &[&str]) -> Option<String> {
    let out = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .ok()?;
    match out.status.success() {
        true => Some(String::from_utf8_lossy(&out.stdout).trim().to_string()),
        false => None,
    }
}

fn manifest_version() -> String {
    read_repo_file("herdr-plugin.toml")
        .parse::<toml::Table>()
        .unwrap()["version"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn the_header_names_the_binary_the_version_the_commit_and_the_build_time() {
    let run = run_version(&[]);
    assert_eq!(run.status, 0, "{}", run.stderr);
    assert_eq!(run.stderr, "");

    let header = header_of(&run.stdout);
    assert_eq!(
        header,
        format!("watch {} ({}, built {})", CRATE_VERSION, COMMIT, BUILT)
    );
    let inside = in_parentheses(&header);
    let (commit, built) = inside.split_once(", built ").expect(&inside);
    assert_eq!(commit, COMMIT);
    assert!(looks_like_a_timestamp(built), "{}", built);
}

#[test]
fn the_build_time_is_an_instant_in_utc() {
    assert!(looks_like_a_timestamp(BUILT), "{}", BUILT);
}

#[test]
fn the_timestamp_shape_check_rejects_what_it_should() {
    assert!(looks_like_a_timestamp("2026-09-10T17:19:22Z"));
    assert!(!looks_like_a_timestamp("2026-09-10 17:19:22"));
    assert!(!looks_like_a_timestamp("2026-09-10T17:19Z"));
    assert!(!looks_like_a_timestamp("unknown"));
    assert!(!looks_like_a_timestamp("20x6-09-10T17:19:22Z"));
}

#[test]
fn the_manifest_line_says_which_version_came_from_which_file() {
    let run = run_version(&[("HERDR_PLUGIN_ROOT", manifest_dir().to_str().unwrap())]);
    assert_eq!(
        manifest_of(&run.stdout),
        format!(
            "manifest {} at {}",
            manifest_version(),
            manifest_dir().join("herdr-plugin.toml").display()
        )
    );
}

#[test]
fn the_binary_finds_its_own_checkout_with_no_herdr_anywhere_in_the_environment() {
    let run = run_version(&[]);
    assert_eq!(run.status, 0, "{}", run.stderr);
    assert!(
        manifest_of(&run.stdout).ends_with("herdr-plugin.toml"),
        "{}",
        run.stdout
    );
}

#[test]
fn two_agreeing_versions_are_reported_without_a_verdict() {
    let run = run_version(&[]);
    assert_eq!(CRATE_VERSION, manifest_version(), "a test enforces this");
    assert_eq!(run.stdout.lines().count(), 2, "{}", run.stdout);
    assert!(!run.stdout.contains("STALE"), "{}", run.stdout);
}

#[test]
fn a_manifest_it_cannot_read_is_named_rather_than_guessed_at() {
    let dir = TempDir::new();
    let report = version::report(
        "watch",
        &Manifest::Unreadable(dir.join("herdr-plugin.toml")),
    );
    assert_eq!(
        manifest_of(&report),
        format!(
            "manifest unreadable at {}",
            dir.join("herdr-plugin.toml").display()
        )
    );
    assert!(!report.contains("STALE"), "{}", report);
}

#[test]
fn with_no_plugin_root_the_manifest_line_says_how_to_point_at_one() {
    let report = version::report("watch", &Manifest::NoRoot);
    assert_eq!(
        manifest_of(&report),
        "manifest not found: set HERDR_PLUGIN_ROOT to the plugin checkout to read it"
    );
    assert_eq!(report.lines().count(), 2, "{}", report);
}

#[test]
fn the_stale_verdict_names_both_versions_and_the_fix() {
    let manifest = Manifest::Found {
        version: "9.9.9".to_string(),
        path: PathBuf::from("/p/herdr-plugin.toml"),
    };
    let report = version::report("watch", &manifest);
    assert_eq!(report.lines().count(), 3, "{}", report);
    assert_eq!(
        report.lines().nth(2).unwrap(),
        format!(
            "STALE: this binary is {} but the manifest is 9.9.9. Rebuild it with `cargo build --release`.",
            CRATE_VERSION
        )
    );
}

#[test]
fn reading_a_manifest_tells_the_failures_apart() {
    let dir = TempDir::new();
    assert_eq!(version::read_manifest(None), Manifest::NoRoot);
    assert_eq!(
        version::read_manifest(Some(dir.path())),
        Manifest::Unreadable(dir.join("herdr-plugin.toml"))
    );

    dir.write("herdr-plugin.toml", "version = \"unterminated\n");
    assert_eq!(
        version::read_manifest(Some(dir.path())),
        Manifest::Unparsed(dir.join("herdr-plugin.toml"))
    );

    dir.write("herdr-plugin.toml", "id = \"x\"\n");
    assert_eq!(
        version::read_manifest(Some(dir.path())),
        Manifest::Unparsed(dir.join("herdr-plugin.toml")),
        "a manifest with no version cannot be compared against the binary"
    );

    dir.write("herdr-plugin.toml", "version = \"1.2.3\"\n");
    assert_eq!(
        version::read_manifest(Some(dir.path())),
        Manifest::Found {
            version: "1.2.3".to_string(),
            path: dir.join("herdr-plugin.toml")
        }
    );
}

#[test]
fn the_commit_carries_the_state_of_the_tree_it_was_built_from() {
    let head = git_out(&manifest_dir(), &["rev-parse", "--short", "HEAD"]);
    let changes = git_out(&manifest_dir(), &["status", "--porcelain"]);
    match (head, changes) {
        (Some(head), Some(changes)) if changes.is_empty() => assert_eq!(COMMIT, head),
        (Some(head), Some(_)) => assert_eq!(COMMIT, format!("{}-dirty", head)),
        (Some(head), None) => assert_eq!(COMMIT, format!("{}-unverified", head)),
        (None, _) => assert_eq!(COMMIT, UNKNOWN_COMMIT),
    }
}

#[test]
fn asking_for_the_version_never_reaches_the_socket() {
    let stub = Stub::start(Script::default().open(vec![listed("~", "w1", true)]));
    let state = TempDir::new();
    let run = run_version(&[
        ("HERDR_SOCKET_PATH", stub.socket().to_str().unwrap()),
        ("HERDR_PLUGIN_STATE_DIR", state.path().to_str().unwrap()),
    ]);
    assert_eq!(run.status, 0, "{}", run.stderr);
    assert_eq!(stub.requests().len(), 0, "{:?}", stub.methods());
    assert_eq!(
        std::fs::read_dir(state.path()).unwrap().count(),
        0,
        "it claimed the socket instead of reporting and leaving"
    );
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap().flatten() {
        let target = to.join(entry.file_name());
        match entry.file_type().unwrap().is_dir() {
            true => copy_tree(&entry.path(), &target),
            false => {
                std::fs::copy(entry.path(), &target).unwrap();
            }
        }
    }
}

fn crate_copy(dir: &TempDir) -> PathBuf {
    let root = dir.dir("crate");
    for name in ["Cargo.toml", "Cargo.lock", "build.rs", "herdr-plugin.toml"] {
        std::fs::copy(manifest_dir().join(name), root.join(name)).unwrap();
    }
    copy_tree(&manifest_dir().join("src"), &root.join("src"));
    assert!(!root.join(".git").exists());
    root
}

fn build(root: &Path, target: &Path) -> PathBuf {
    let built = Command::new(env!("CARGO"))
        .args(["build", "--release", "--manifest-path"])
        .arg(root.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", target)
        .current_dir(root)
        .output()
        .expect("cannot run cargo");
    assert!(
        built.status.success(),
        "the build failed: {}",
        String::from_utf8_lossy(&built.stderr)
    );
    target.join("release/watch")
}

fn build_with(root: &Path, target: &Path, extra: &[(&str, &str)]) -> PathBuf {
    let mut command = Command::new(env!("CARGO"));
    command
        .args(["build", "--release", "--manifest-path"])
        .arg(root.join("Cargo.toml"))
        .env("CARGO_TARGET_DIR", target)
        .current_dir(root);
    for (key, value) in extra {
        command.env(key, value);
    }
    let built = command.output().expect("cannot run cargo");
    assert!(
        built.status.success(),
        "the build failed: {}",
        String::from_utf8_lossy(&built.stderr)
    );
    target.join("release/watch")
}

#[test]
fn a_tree_whose_state_cannot_be_checked_is_not_reported_as_clean() {
    let dir = TempDir::new();
    let root = crate_copy(&dir);
    let target = dir.join("target");
    let fake = dir.dir("bin");
    std::fs::write(
        fake.join("git"),
        "#!/bin/sh\ncase \"$1\" in\n  rev-parse) echo deadbee ;;\n  *) exit 1 ;;\nesac\n",
    )
    .unwrap();
    let mut mode = std::fs::metadata(fake.join("git")).unwrap().permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut mode, 0o755);
    std::fs::set_permissions(fake.join("git"), mode).unwrap();

    let path = format!(
        "{}:{}",
        fake.to_string_lossy(),
        std::env::var("PATH").unwrap_or_else(|_| LAUNCHD_PATH.to_string())
    );
    let binary = build_with(&root, &target, &[("PATH", &path)]);
    let report = ask(&binary, &[("HERDR_PLUGIN_ROOT", root.to_str().unwrap())]).stdout;

    let inside = in_parentheses(&header_of(&report));
    assert!(
        inside.starts_with("deadbee-unverified"),
        "a status check that could not run must not read as a clean tree: {}",
        inside
    );
}

fn build_and_ask(root: &Path, target: &Path) -> String {
    let binary = build(root, target);
    ask(&binary, &[("HERDR_PLUGIN_ROOT", root.to_str().unwrap())]).stdout
}

fn past_the_next_second() {
    std::thread::sleep(std::time::Duration::from_millis(1100));
}

fn git_in(root: &Path, args: &[&str]) {
    let out = Command::new("git")
        .args(["-c", "user.email=t@example.com", "-c", "user.name=t"])
        .args(args)
        .current_dir(root)
        .output()
        .expect("cannot run git");
    assert!(
        out.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn a_checkout_with_no_git_builds_and_reports_an_unknown_commit() {
    let dir = TempDir::new();
    let root = crate_copy(&dir);
    let target = dir.join("target");
    let first = build_and_ask(&root, &target);

    let inside = in_parentheses(&header_of(&first));
    let (commit, built) = inside.split_once(", built ").expect(&inside);
    assert_eq!(commit, UNKNOWN_COMMIT, "{}", first);
    assert!(looks_like_a_timestamp(built), "{}", built);
    assert_eq!(first.lines().count(), 2, "{}", first);

    past_the_next_second();
    let again = build_and_ask(&root, &target);
    assert_eq!(
        first, again,
        "a second build with nothing changed rebuilt anyway, so the stamp moved"
    );

    past_the_next_second();
    std::fs::write(
        root.join("src/version.rs"),
        read_repo_file("src/version.rs"),
    )
    .unwrap();
    let edited = build_and_ask(&root, &target);
    assert_ne!(
        header_of(&first),
        header_of(&edited),
        "a source edit left the build stamp behind the binary beside it"
    );
}

#[test]
fn a_manifest_that_disagrees_with_the_binary_is_diagnosed_at_run_time() {
    let dir = TempDir::new();
    let root = crate_copy(&dir);
    let target = dir.join("target");
    let binary = build(&root, &target);

    let agreed = ask(&binary, &[("HERDR_PLUGIN_ROOT", root.to_str().unwrap())]);
    assert_eq!(agreed.stdout.lines().count(), 2, "{}", agreed.stdout);

    std::fs::write(root.join("herdr-plugin.toml"), "version = \"9.9.9\"\n").unwrap();
    let stale = ask(&binary, &[("HERDR_PLUGIN_ROOT", root.to_str().unwrap())]);
    assert_eq!(stale.status, 0, "{}", stale.stderr);
    assert_eq!(
        stale.stdout.lines().nth(2).unwrap(),
        format!(
            "STALE: this binary is {} but the manifest is 9.9.9. Rebuild it with `cargo build --release`.",
            CRATE_VERSION
        ),
        "the same binary must notice without being rebuilt"
    );

    std::fs::remove_file(root.join("herdr-plugin.toml")).unwrap();
    let gone = ask(&binary, &[("HERDR_PLUGIN_ROOT", root.to_str().unwrap())]);
    assert_eq!(gone.status, 0, "{}", gone.stderr);
    assert!(
        manifest_of(&gone.stdout).starts_with("manifest unreadable at "),
        "{}",
        gone.stdout
    );

    std::fs::write(root.join("herdr-plugin.toml"), "version = \"unterminated\n").unwrap();
    let broken = ask(&binary, &[("HERDR_PLUGIN_ROOT", root.to_str().unwrap())]);
    assert_eq!(broken.status, 0, "{}", broken.stderr);
    assert!(
        manifest_of(&broken.stdout).starts_with("manifest unparsed at "),
        "{}",
        broken.stdout
    );

    let stranded = dir.dir("a/b/c");
    std::fs::copy(&binary, stranded.join("watch")).unwrap();
    let lost = ask(&stranded.join("watch"), &[]);
    assert_eq!(lost.status, 0, "{}", lost.stderr);
    assert_eq!(
        manifest_of(&lost.stdout),
        "manifest not found: set HERDR_PLUGIN_ROOT to the plugin checkout to read it"
    );
}

#[test]
fn the_commit_follows_the_tree_it_was_built_from() {
    let dir = TempDir::new();
    let root = crate_copy(&dir);
    let target = dir.join("target");
    git_in(&root, &["init", "--quiet"]);
    git_in(&root, &["add", "."]);
    git_in(&root, &["commit", "--quiet", "-m", "first"]);

    let clean = in_parentheses(&header_of(&build_and_ask(&root, &target)));
    let head = git_out(&root, &["rev-parse", "--short", "HEAD"]).unwrap();
    assert!(clean.starts_with(&head), "{}", clean);
    assert!(!clean.contains("-dirty"), "{}", clean);
    assert!(!clean.contains("-unverified"), "{}", clean);

    std::fs::write(
        root.join("herdr-plugin.toml"),
        "version = \"0.5.0\"\nid = \"x\"\n",
    )
    .unwrap();
    git_in(&root, &["add", "herdr-plugin.toml"]);
    let dirty = in_parentheses(&header_of(&build_and_ask(&root, &target)));
    assert!(
        dirty.starts_with(&format!("{}-dirty", head)),
        "a staged change is uncommitted, so the hash alone would misreport it: {}",
        dirty
    );

    git_in(&root, &["commit", "--quiet", "-m", "second"]);
    let moved = in_parentheses(&header_of(&build_and_ask(&root, &target)));
    let second = git_out(&root, &["rev-parse", "--short", "HEAD"]).unwrap();
    assert!(moved.starts_with(&second), "{}", moved);
    assert_ne!(head, second);

    git_in(
        &root,
        &["commit", "--quiet", "--allow-empty", "-m", "third"],
    );
    let empty = in_parentheses(&header_of(&build_and_ask(&root, &target)));
    let third = git_out(&root, &["rev-parse", "--short", "HEAD"]).unwrap();
    assert!(
        empty.starts_with(&third),
        "a commit that edits no file left the binary reporting the one before it: {}",
        empty
    );
}
