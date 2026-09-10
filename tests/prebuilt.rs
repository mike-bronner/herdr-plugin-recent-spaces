mod support;

use std::path::Path;

use support::*;

const TOOLS_WITHOUT_A_CHECKSUMMER: [&str; 14] = [
    "sh", "sed", "head", "awk", "tr", "grep", "basename", "dirname", "uname", "mkdir", "rm",
    "chmod", "mv", "curl",
];

fn no_toolchain(root: &Path) {
    executable(&root.join("bin/find-cargo"), "#!/bin/sh\nexit 1\n");
}

fn asset_name_for_host() -> String {
    let out = std::process::Command::new("/bin/sh")
        .arg(manifest_dir().join("bin/asset-name"))
        .env_clear()
        .env("PATH", LAUNCHD_PATH)
        .output()
        .expect("cannot run bin/asset-name");
    assert!(out.status.success(), "bin/asset-name refused this host");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn asset_path(version: &str, asset: &str) -> String {
    format!("/owner/repo/releases/download/v{}/{}", version, asset)
}

fn logging_cargo(dir: &TempDir, log: &Path) -> String {
    let bin = dir.dir("cargo-bin");
    executable(
        &bin.join("cargo"),
        &format!(
            "#!/bin/sh\nprintf 'COMPILED\\n' > '{}'\n",
            log.to_string_lossy()
        ),
    );
    format!("{}:{}", bin.to_string_lossy(), LAUNCHD_PATH)
}

fn failing_cargo(dir: &TempDir) -> String {
    let bin = dir.dir("cargo-bin");
    executable(
        &bin.join("cargo"),
        "#!/bin/sh\necho 'error[E0001]' >&2\nexit 1\n",
    );
    format!("{}:{}", bin.to_string_lossy(), LAUNCHD_PATH)
}

#[test]
fn the_asset_name_for_this_host_names_the_platform_the_tests_were_built_for() {
    let expected = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => "watch-aarch64-apple-darwin",
        ("macos", "x86_64") => "watch-x86_64-apple-darwin",
        ("linux", "aarch64") => "watch-aarch64-unknown-linux-musl",
        ("linux", "x86_64") => "watch-x86_64-unknown-linux-musl",
        (os, arch) => panic!("this test knows no name for {} {}", os, arch),
    };
    assert_eq!(asset_name_for_host(), expected);
}

#[test]
fn an_unrecognised_host_gets_no_asset_name_rather_than_a_guess() {
    let dir = TempDir::new();
    let bin = dir.dir("stub");
    executable(
        &bin.join("uname"),
        "#!/bin/sh\ncase \"$1\" in -s) echo Plan9 ;; -m) echo ppc64 ;; esac\n",
    );
    let out = std::process::Command::new("/bin/sh")
        .arg(manifest_dir().join("bin/asset-name"))
        .env_clear()
        .env(
            "PATH",
            format!("{}:{}", bin.to_string_lossy(), LAUNCHD_PATH),
        )
        .output()
        .expect("cannot run bin/asset-name");
    assert_ne!(
        out.status.code(),
        Some(0),
        "an unknown host must not be named"
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "",
        "a guessed name would download a binary built for another platform"
    );
}

#[test]
fn a_target_triple_is_named_exactly_as_the_host_form_is() {
    for triple in [
        "aarch64-apple-darwin",
        "x86_64-apple-darwin",
        "aarch64-unknown-linux-musl",
        "x86_64-unknown-linux-musl",
    ] {
        let out = std::process::Command::new("/bin/sh")
            .arg(manifest_dir().join("bin/asset-name"))
            .arg(triple)
            .env_clear()
            .env("PATH", LAUNCHD_PATH)
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            format!("watch-{}", triple)
        );
    }
}

#[test]
fn the_workflow_builds_every_platform_the_shim_knows_how_to_ask_for() {
    let workflow = read_repo_file(".github/workflows/release.yml");
    let built: Vec<String> = workflow
        .lines()
        .filter_map(|line| line.trim().strip_prefix("target: "))
        .map(|target| target.trim().to_string())
        .collect();

    let script = read_repo_file("bin/asset-name");
    let asked: Vec<String> = script
        .lines()
        .filter_map(|line| {
            let (_, rest) = line.split_once(") triple=\"")?;
            Some(rest.split('"').next()?.to_string())
        })
        .collect();

    assert!(!built.is_empty(), "no targets found in the workflow");
    assert!(!asked.is_empty(), "no triples found in bin/asset-name");
    let mut built_sorted = built.clone();
    let mut asked_sorted = asked.clone();
    built_sorted.sort();
    asked_sorted.sort();
    assert_eq!(
        built_sorted, asked_sorted,
        "a platform the shim asks for and the workflow never builds is a 404 on \
         somebody's machine, and one the workflow builds and the shim never asks \
         for is an asset nobody downloads"
    );
}

#[test]
fn the_workflow_names_the_asset_with_the_shared_script_and_never_by_hand() {
    let workflow = read_repo_file(".github/workflows/release.yml");
    assert!(
        workflow.contains("sh bin/asset-name \"$TARGET\""),
        "the workflow must ask the same script the shim asks"
    );
    assert!(
        !workflow.contains("watch-"),
        "an asset name spelled out here is exactly how the two sides come to disagree"
    );
}

#[test]
fn install_downloads_the_published_binary_instead_of_compiling() {
    let dir = TempDir::new();
    let root = fake_root(&dir, true);
    let assets = Assets::start();
    declare(&root, "9.9.9", assets.base());
    let asset = asset_name_for_host();
    let body = b"#!/bin/sh\necho PUBLISHED\n";
    assets.publish(&asset_path("9.9.9", &asset), body);
    assets.publish(
        &format!("{}.sha256", asset_path("9.9.9", &asset)),
        format!("{}  {}\n", sha256_of(body), asset).as_bytes(),
    );
    let log = dir.join("cargo-log");
    let path = logging_cargo(&dir, &log);

    let run = run_build_args(&root, &["--prefer-download"], &[("PATH", &path)]);

    assert_eq!(run.status, 0, "{}", run.stderr);
    assert!(
        !log.exists(),
        "a published binary was there, so nothing should have been compiled"
    );
    let installed = root.join("target/release/watch");
    assert_eq!(std::fs::read(&installed).unwrap(), body.to_vec());
    assert_eq!(mode_of(&installed) & 0o111, 0o111, "it must be runnable");
}

#[test]
fn the_marker_records_the_version_the_binary_was_downloaded_for() {
    let dir = TempDir::new();
    let root = fake_root(&dir, true);
    let assets = Assets::start();
    declare(&root, "9.9.9", assets.base());
    let asset = asset_name_for_host();
    let body = b"published\n";
    assets.publish(&asset_path("9.9.9", &asset), body);
    assets.publish(
        &format!("{}.sha256", asset_path("9.9.9", &asset)),
        format!("{}  {}\n", sha256_of(body), asset).as_bytes(),
    );

    let run = run_build_args(&root, &["--prefer-download"], &[("PATH", LAUNCHD_PATH)]);
    assert_eq!(run.status, 0, "{}", run.stderr);

    let marker = std::fs::read_to_string(root.join("target/release/watch.download")).unwrap();
    assert!(
        marker.contains("version=9.9.9"),
        "the shim reads this line to tell a fresh release from new source: {}",
        marker
    );
    assert!(marker.contains(&format!("asset={}", asset)), "{}", marker);
    assert!(marker.contains(&sha256_of(body)), "{}", marker);
}

#[test]
fn the_version_in_the_download_url_is_the_plugin_version_and_not_the_herdr_floor() {
    let dir = TempDir::new();
    let root = fake_root(&dir, true);
    let assets = Assets::start();
    declare(&root, "9.9.9", assets.base());

    run_build_args(&root, &["--prefer-download"], &[("PATH", LAUNCHD_PATH)]);

    let asked = assets.asked();
    assert!(
        asked.iter().any(|path| path.contains("/v9.9.9/")),
        "the release is keyed on the declared version: {:?}",
        asked
    );
    assert!(
        !asked.iter().any(|path| path.contains("/v0.9.0/")),
        "min_herdr_version is the Herdr floor, not a release of this plugin: {:?}",
        asked
    );
}

#[test]
fn a_checksum_mismatch_refuses_the_download_and_compiles_instead() {
    let dir = TempDir::new();
    let root = fake_root(&dir, true);
    let assets = Assets::start();
    declare(&root, "9.9.9", assets.base());
    let asset = asset_name_for_host();
    assets.publish(&asset_path("9.9.9", &asset), b"tampered\n");
    assets.publish(
        &format!("{}.sha256", asset_path("9.9.9", &asset)),
        format!("{}  {}\n", sha256_of(b"what was published\n"), asset).as_bytes(),
    );
    let log = dir.join("cargo-log");
    let path = logging_cargo(&dir, &log);

    let run = run_build_args(&root, &["--prefer-download"], &[("PATH", &path)]);

    assert_eq!(run.status, 0, "{}", run.stderr);
    assert!(log.exists(), "it must fall through to compiling");
    assert!(
        !root.join("target/release/watch").exists(),
        "an unverified binary must never reach the path the shim execs"
    );
    assert!(
        !root.join("target/release/watch.download").exists(),
        "a refused download must leave no marker"
    );
    assert!(run.stderr.contains("does not match"), "{}", run.stderr);
}

#[test]
fn the_parts_of_a_refused_download_are_not_left_behind() {
    let dir = TempDir::new();
    let root = fake_root(&dir, true);
    let assets = Assets::start();
    declare(&root, "9.9.9", assets.base());
    let asset = asset_name_for_host();
    assets.publish(&asset_path("9.9.9", &asset), b"tampered\n");
    assets.publish(
        &format!("{}.sha256", asset_path("9.9.9", &asset)),
        format!("{}  {}\n", sha256_of(b"other\n"), asset).as_bytes(),
    );

    run_build_args(&root, &["--prefer-download"], &[("PATH", LAUNCHD_PATH)]);

    for leftover in ["watch.part", "watch.part.sha256"] {
        assert!(
            !root.join("target/release").join(leftover).exists(),
            "{} was left behind",
            leftover
        );
    }
}

#[test]
fn a_missing_checksum_refuses_the_download_and_compiles_instead() {
    let dir = TempDir::new();
    let root = fake_root(&dir, true);
    let assets = Assets::start();
    declare(&root, "9.9.9", assets.base());
    let asset = asset_name_for_host();
    assets.publish(&asset_path("9.9.9", &asset), b"unverifiable\n");
    let log = dir.join("cargo-log");
    let path = logging_cargo(&dir, &log);

    let run = run_build_args(&root, &["--prefer-download"], &[("PATH", &path)]);

    assert_eq!(run.status, 0, "{}", run.stderr);
    assert!(log.exists(), "it must fall through to compiling");
    assert!(
        !root.join("target/release/watch").exists(),
        "a binary with no published checksum must not be installed"
    );
    assert!(
        run.stderr.contains("no checksum published"),
        "{}",
        run.stderr
    );
}

#[test]
fn a_checksum_that_is_not_a_digest_refuses_the_download_and_compiles_instead() {
    let dir = TempDir::new();
    let root = fake_root(&dir, true);
    let assets = Assets::start();
    declare(&root, "9.9.9", assets.base());
    let asset = asset_name_for_host();
    assets.publish(&asset_path("9.9.9", &asset), b"unverifiable\n");
    assets.publish(
        &format!("{}.sha256", asset_path("9.9.9", &asset)),
        b"<!doctype html><title>404</title>",
    );
    let log = dir.join("cargo-log");
    let path = logging_cargo(&dir, &log);

    let run = run_build_args(&root, &["--prefer-download"], &[("PATH", &path)]);

    assert_eq!(run.status, 0, "{}", run.stderr);
    assert!(log.exists(), "it must fall through to compiling");
    assert!(!root.join("target/release/watch").exists());
    assert!(run.stderr.contains("not a sha256 digest"), "{}", run.stderr);
}

#[test]
fn a_download_is_refused_when_nothing_on_the_machine_can_verify_it() {
    let dir = TempDir::new();
    let root = fake_root(&dir, true);
    let assets = Assets::start();
    declare(&root, "9.9.9", assets.base());
    let asset = asset_name_for_host();
    let body = b"perfectly good\n";
    assets.publish(&asset_path("9.9.9", &asset), body);
    assets.publish(
        &format!("{}.sha256", asset_path("9.9.9", &asset)),
        format!("{}  {}\n", sha256_of(body), asset).as_bytes(),
    );

    let stripped = only_these_tools(&dir, "no-checksummer", &TOOLS_WITHOUT_A_CHECKSUMMER);
    let run = run_build_args(&root, &["--prefer-download"], &[("PATH", &stripped)]);

    assert!(
        !root.join("target/release/watch").exists(),
        "verification is the gate on the move, so it cannot be skipped when no \
         checksum tool exists"
    );
    assert!(
        run.stderr.contains("could not be verified"),
        "{}",
        run.stderr
    );

    let mut allowed_tools = TOOLS_WITHOUT_A_CHECKSUMMER.to_vec();
    allowed_tools.push("shasum");
    let with_checksummer = only_these_tools(&dir, "with-checksummer", &allowed_tools);
    let allowed = run_build_args(
        &root,
        &["--prefer-download"],
        &[("PATH", &with_checksummer)],
    );
    assert_eq!(allowed.status, 0, "{}", allowed.stderr);
    assert_eq!(
        std::fs::read(root.join("target/release/watch")).unwrap(),
        body.to_vec(),
        "the same download succeeds once a checksum tool is there, so the refusal \
         above was the missing tool and not the harness"
    );
}

#[test]
fn a_missing_asset_compiles_instead() {
    let dir = TempDir::new();
    let root = fake_root(&dir, true);
    let assets = Assets::start();
    declare(&root, "9.9.9", assets.base());
    let log = dir.join("cargo-log");
    let path = logging_cargo(&dir, &log);

    let run = run_build_args(&root, &["--prefer-download"], &[("PATH", &path)]);

    assert_eq!(run.status, 0, "{}", run.stderr);
    assert!(log.exists(), "an absent asset must not abort the install");
    assert!(
        run.stderr.contains("nothing published at"),
        "{}",
        run.stderr
    );
}

#[test]
fn an_unreachable_network_compiles_instead() {
    let dir = TempDir::new();
    let root = fake_root(&dir, true);
    let closed = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        format!("http://127.0.0.1:{}/owner/repo", port)
    };
    declare(&root, "9.9.9", &closed);
    let log = dir.join("cargo-log");
    let path = logging_cargo(&dir, &log);

    let run = run_build_args(&root, &["--prefer-download"], &[("PATH", &path)]);

    assert_eq!(
        run.status, 0,
        "Herdr aborts an install whose build step fails, so no network must not \
         cost somebody the plugin: {}",
        run.stderr
    );
    assert!(log.exists(), "it must fall through to compiling");
}

#[test]
fn the_shim_asking_for_a_rebuild_compiles_and_never_downloads() {
    let dir = TempDir::new();
    let root = fake_root(&dir, true);
    let assets = Assets::start();
    declare(&root, "9.9.9", assets.base());
    let asset = asset_name_for_host();
    let body = b"the release, which is not this source\n";
    assets.publish(&asset_path("9.9.9", &asset), body);
    assets.publish(
        &format!("{}.sha256", asset_path("9.9.9", &asset)),
        format!("{}  {}\n", sha256_of(body), asset).as_bytes(),
    );
    let log = dir.join("cargo-log");
    let path = logging_cargo(&dir, &log);

    let run = run_build(&root, &[("PATH", &path)]);

    assert_eq!(run.status, 0, "{}", run.stderr);
    assert!(log.exists(), "a rebuild request means compile");
    assert!(
        assets.asked().is_empty(),
        "a download cannot answer a source change, so it must not even be tried: {:?}",
        assets.asked()
    );
}

#[test]
fn a_machine_with_no_toolchain_downloads_when_the_shim_asks_for_a_rebuild() {
    let dir = TempDir::new();
    let root = fake_root(&dir, true);
    let assets = Assets::start();
    declare(&root, "9.9.9", assets.base());
    no_toolchain(&root);
    let asset = asset_name_for_host();
    let body = b"#!/bin/sh\necho PUBLISHED\n";
    assets.publish(&asset_path("9.9.9", &asset), body);
    assets.publish(
        &format!("{}.sha256", asset_path("9.9.9", &asset)),
        format!("{}  {}\n", sha256_of(body), asset).as_bytes(),
    );

    let run = run_build(&root, &[("PATH", LAUNCHD_PATH)]);

    assert_eq!(run.status, 0, "{}", run.stderr);
    assert_eq!(
        std::fs::read(root.join("target/release/watch")).unwrap(),
        body.to_vec(),
        "with no toolchain the download is the only way, so it must be tried"
    );
}

#[test]
fn a_machine_with_no_toolchain_updates_to_a_new_release_through_the_shim() {
    let dir = TempDir::new();
    let root = fake_root(&dir, true);
    let assets = Assets::start();
    no_toolchain(&root);

    let asset = asset_name_for_host();
    let old = b"#!/bin/sh\necho OLD\n";
    let new = b"#!/bin/sh\necho NEW\n";
    for (version, body) in [("9.9.9", old), ("9.9.10", new)] {
        assets.publish(&asset_path(version, &asset), body);
        assets.publish(
            &format!("{}.sha256", asset_path(version, &asset)),
            format!("{}  {}\n", sha256_of(body), asset).as_bytes(),
        );
    }

    declare(&root, "9.9.9", assets.base());
    let installed = run_build_args(&root, &["--prefer-download"], &[("PATH", LAUNCHD_PATH)]);
    assert_eq!(installed.status, 0, "{}", installed.stderr);
    assert_eq!(run_shim(&root, &[]).stdout, "OLD\n");

    declare(&root, "9.9.10", assets.base());
    let updated = run_shim(&root, &[]);

    assert_eq!(updated.status, 0, "{}", updated.stderr);
    assert_eq!(
        updated.stdout, "NEW\n",
        "pulling a new release must reach the new binary with no cargo anywhere"
    );
    assert!(
        std::fs::read_to_string(root.join("target/release/watch.download"))
            .unwrap()
            .contains("version=9.9.10"),
        "the marker must move with the binary or the next start downloads again"
    );
}

#[test]
fn a_failed_compile_is_never_replaced_by_a_published_binary() {
    let dir = TempDir::new();
    let root = fake_root(&dir, true);
    let assets = Assets::start();
    declare(&root, "9.9.9", assets.base());
    let asset = asset_name_for_host();
    let body = b"the release\n";
    assets.publish(&asset_path("9.9.9", &asset), body);
    assets.publish(
        &format!("{}.sha256", asset_path("9.9.9", &asset)),
        format!("{}  {}\n", sha256_of(body), asset).as_bytes(),
    );
    let path = failing_cargo(&dir);

    let run = run_build(&root, &[("PATH", &path)]);

    assert_eq!(run.status, 1, "a failed build is a failure: {}", run.stderr);
    assert!(
        assets.asked().is_empty(),
        "substituting a release binary would run code the developer did not write \
         and hide the error that stopped theirs: {:?}",
        assets.asked()
    );
    assert!(
        !root.join("target/release/watch").exists(),
        "nothing was installed over the failed build"
    );
    assert!(
        run.stderr.contains("cargo build --release failed"),
        "{}",
        run.stderr
    );
}

#[test]
fn a_successful_compile_clears_a_download_marker() {
    let dir = TempDir::new();
    let root = fake_root(&dir, true);
    declare(&root, "9.9.9", "http://127.0.0.1:1/owner/repo");
    let marker = root.join("target/release/watch.download");
    std::fs::write(&marker, "version=1.0.0\n").unwrap();
    let log = dir.join("cargo-log");
    let path = logging_cargo(&dir, &log);

    let run = run_build(&root, &[("PATH", &path)]);

    assert_eq!(run.status, 0, "{}", run.stderr);
    assert!(
        !marker.exists(),
        "a compiled binary left marked as downloaded would stop rebuilding on a \
         source change, which is the developer loop"
    );
}

#[test]
fn when_both_ways_fail_the_message_names_both_and_how_to_fix_it() {
    let dir = TempDir::new();
    let root = fake_root(&dir, true);
    let closed = {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        format!("http://127.0.0.1:{}/owner/repo", port)
    };
    declare(&root, "9.9.9", &closed);
    no_toolchain(&root);

    let run = run_build_args(&root, &["--prefer-download"], &[("PATH", LAUNCHD_PATH)]);

    assert_eq!(run.status, 1);
    assert!(run.stderr.contains("both ways"), "{}", run.stderr);
    assert!(run.stderr.contains("cargo not found"), "{}", run.stderr);
    assert!(run.stderr.contains("sh.rustup.rs"), "{}", run.stderr);
    assert!(
        run.stderr.contains("cargo build --release"),
        "{}",
        run.stderr
    );
    assert!(
        run.stderr.contains(root.to_str().unwrap()),
        "somebody reading this is stuck and it is all they have: {}",
        run.stderr
    );
}

fn downloaded(root: &Path, marked_version: &str) {
    executable(&root.join("target/release/watch"), "#!/bin/sh\necho RAN\n");
    std::fs::write(
        root.join("target/release/watch.download"),
        format!("version={}\nasset=watch-test\n", marked_version),
    )
    .unwrap();
    executable(&root.join("bin/build"), "#!/bin/sh\necho BUILT >&2\n");
}

#[test]
fn a_downloaded_binary_is_not_rebuilt_when_the_source_is_newer() {
    let dir = TempDir::new();
    let root = fake_root(&dir, false);
    downloaded(&root, "9.9.9");
    declare(&root, "9.9.9", "http://127.0.0.1:1/owner/repo");
    current(&root);
    set_mtime(&root.join("src/main.rs"), 3000);

    let run = run_shim(&root, &[]);

    assert_eq!(run.status, 0, "{}", run.stderr);
    assert_eq!(run.stdout, "RAN\n");
    assert!(
        !run.stderr.contains("BUILT"),
        "on every commit after a release the source is newer, and a machine with \
         no toolchain must not be asked to rebuild for that: {}",
        run.stderr
    );
}

#[test]
fn a_downloaded_binary_is_rebuilt_when_the_manifest_version_has_moved() {
    let dir = TempDir::new();
    let root = fake_root(&dir, false);
    downloaded(&root, "9.9.9");
    declare(&root, "9.9.10", "http://127.0.0.1:1/owner/repo");
    current(&root);

    let run = run_shim(&root, &[]);

    assert!(
        run.stderr.contains("BUILT"),
        "a new release is the one thing that makes a downloaded binary stale: {}",
        run.stderr
    );
}

#[test]
fn a_downloaded_binary_is_rebuilt_when_the_declared_version_cannot_be_read() {
    for manifest in ["", "id = \"probe.fake\"\nmin_herdr_version = \"0.9.0\"\n"] {
        let dir = TempDir::new();
        let root = fake_root(&dir, false);
        downloaded(&root, "9.9.9");
        std::fs::write(root.join("herdr-plugin.toml"), manifest).unwrap();
        current(&root);

        let run = run_shim(&root, &[]);

        assert!(
            run.stderr.contains("BUILT"),
            "an unknown version fails closed: {:?} gave {}",
            manifest,
            run.stderr
        );
    }
}

#[test]
fn the_shim_compares_against_the_plugin_version_and_not_the_herdr_floor() {
    let dir = TempDir::new();
    let root = fake_root(&dir, false);
    downloaded(&root, "0.9.0");
    declare(&root, "9.9.9", "http://127.0.0.1:1/owner/repo");
    current(&root);

    let run = run_shim(&root, &[]);

    assert!(
        run.stderr.contains("BUILT"),
        "reading min_herdr_version as the plugin version would call this binary \
         current when it is a release behind: {}",
        run.stderr
    );
}

#[test]
fn a_compiled_binary_still_rebuilds_on_a_source_change_when_a_manifest_is_there() {
    let dir = TempDir::new();
    let root = fake_root(&dir, false);
    executable(&root.join("target/release/watch"), "#!/bin/sh\necho RAN\n");
    executable(&root.join("bin/build"), "#!/bin/sh\necho BUILT >&2\n");
    declare(&root, "9.9.9", "http://127.0.0.1:1/owner/repo");
    current(&root);
    set_mtime(&root.join("src/main.rs"), 3000);

    let run = run_shim(&root, &[]);

    assert!(
        run.stderr.contains("BUILT"),
        "no marker means the binary belongs to the source beside it: {}",
        run.stderr
    );
}

#[test]
fn the_shipped_repository_is_an_https_github_url() {
    let parsed = read_repo_file("Cargo.toml").parse::<toml::Table>().unwrap();
    let repository = parsed["package"]["repository"].as_str().unwrap();
    assert!(
        repository.starts_with("https://github.com/"),
        "the download composes its URL from this value, and the checksum beside a \
         binary is only as good as the transport that fetched it: {}",
        repository
    );
}
