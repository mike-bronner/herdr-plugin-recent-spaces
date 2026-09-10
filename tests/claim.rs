mod support;

use std::path::Path;

use recent_spaces::claim::{claim_path, Claim, FileClaim, CLAIM_PREFIX};
use support::*;

fn claim(state_dir: &Path, socket: &str) -> FileClaim {
    FileClaim::new(state_dir, Path::new(socket))
}

#[test]
fn a_written_claim_reads_back() {
    let dir = TempDir::new();
    let held = claim(dir.path(), "/run/a.sock");
    held.write("abc").unwrap();
    assert_eq!(held.read().as_deref(), Some("abc"));
}

#[test]
fn the_last_writer_owns_the_socket() {
    let dir = TempDir::new();
    let held = claim(dir.path(), "/run/a.sock");
    held.write("first").unwrap();
    held.write("second").unwrap();
    assert_eq!(held.read().as_deref(), Some("second"));
}

#[test]
fn a_missing_claim_reads_as_none() {
    let dir = TempDir::new();
    assert_eq!(claim(dir.path(), "/run/a.sock").read(), None);
}

#[test]
fn a_corrupt_claim_reads_as_none() {
    let dir = TempDir::new();
    let held = claim(dir.path(), "/run/a.sock");
    held.write("abc").unwrap();
    std::fs::write(held.path(), "{not json").unwrap();
    assert_eq!(held.read(), None);
}

#[test]
fn a_claim_without_a_token_reads_as_none() {
    let dir = TempDir::new();
    let held = claim(dir.path(), "/run/a.sock");
    held.write("abc").unwrap();
    std::fs::write(held.path(), "{\"pid\": 1}\n").unwrap();
    assert_eq!(held.read(), None);
}

#[test]
fn two_sockets_in_one_state_dir_get_separate_claims() {
    let dir = TempDir::new();
    let a = claim(dir.path(), "/run/a.sock");
    let b = claim(dir.path(), "/run/b.sock");
    assert_ne!(a.path(), b.path());
    a.write("owner-a").unwrap();
    b.write("owner-b").unwrap();
    assert_eq!(a.read().as_deref(), Some("owner-a"));
    assert_eq!(b.read().as_deref(), Some("owner-b"));
}

#[test]
fn the_same_socket_keys_the_same_claim_file() {
    let dir = TempDir::new();
    assert_eq!(
        claim_path(dir.path(), Path::new("/run/a.sock")),
        claim_path(dir.path(), Path::new("/run/a.sock"))
    );
}

#[test]
fn the_claim_is_written_under_the_state_dir_and_names_this_plugin() {
    let dir = TempDir::new();
    let held = claim(dir.path(), "/run/a.sock");
    assert_eq!(held.path().parent(), Some(dir.path()));
    let name = held
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    assert!(name.starts_with(CLAIM_PREFIX), "{}", name);
    assert!(name.ends_with(".json"), "{}", name);
}

#[test]
fn writing_a_claim_makes_the_state_dir_when_herdr_has_not() {
    let dir = TempDir::new();
    let state = dir.join("state/plugins/mikebronner.recent-spaces");
    let held = claim(&state, "/run/a.sock");
    held.write("abc").unwrap();
    assert_eq!(held.read().as_deref(), Some("abc"));
}

#[test]
fn the_claim_names_the_pid_and_the_socket_so_it_can_be_killed_by_hand() {
    let dir = TempDir::new();
    let held = claim(dir.path(), "/run/a.sock");
    held.write("abc").unwrap();
    let body = std::fs::read_to_string(held.path()).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(parsed["pid"].as_u64(), Some(std::process::id() as u64));
    assert_eq!(parsed["socket"].as_str(), Some("/run/a.sock"));
}

#[test]
fn leaving_no_staging_file_behind() {
    let dir = TempDir::new();
    claim(dir.path(), "/run/a.sock").write("abc").unwrap();
    let left: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    assert_eq!(left.len(), 1, "{:?}", left);
    assert!(!left[0].ends_with(".tmp"), "{:?}", left);
}

#[test]
fn an_unwritable_state_dir_is_reported_rather_than_passing_silently() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new();
    let locked = dir.dir("locked");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o500)).unwrap();
    let held = claim(&locked.join("state"), "/run/a.sock");
    let refused = held.write("abc");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
    assert!(
        refused.is_err(),
        "a claim it cannot write must not read as held"
    );
}
