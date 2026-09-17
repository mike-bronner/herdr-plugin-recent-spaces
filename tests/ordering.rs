mod support;

use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use herdr_plugin_kit::api::client::CallError;
use recent_spaces::claim::{claim_path, Claim, FileClaim, CLAIM_PREFIX};
use recent_spaces::order::{
    alphabetical, hold_alphabetical, move_block, order_path, read, toggle, write, Mode, ORDER_FILE,
};
use recent_spaces::promote::{tick, Dwell};
use serde_json::json;
use support::*;

fn ids(listed: &[&str], pin: &str) -> Vec<String> {
    let rows: Vec<_> = listed
        .iter()
        .enumerate()
        .map(|(at, label)| open(label, &format!("w{}", at + 1), false))
        .collect();
    alphabetical(&rows, pin)
}

fn settled(id: &str, since: f64) -> Dwell {
    let mut dwell = Dwell::new();
    dwell.observe(Some(id), since);
    dwell
}

fn unsorted() -> Vec<serde_json::Value> {
    vec![
        listed("delta", "w2", false),
        listed("~", "w1", false),
        listed("Alpha", "w3", true),
        listed("charlie", "w4", false),
    ]
}

fn sorted_ids() -> Vec<String> {
    ["w1", "w3", "w4", "w2"]
        .iter()
        .map(|id| id.to_string())
        .collect()
}

#[test]
fn the_pin_leads_and_everything_else_follows_it_in_label_order() {
    assert_eq!(
        ids(&["delta", "~", "alpha", "charlie"], "~"),
        ["w2", "w3", "w4", "w1"].map(String::from)
    );
}

#[test]
fn the_comparison_ignores_case_rather_than_scattering_the_list() {
    assert_eq!(
        ids(&["banana", "Apple", "cherry", "Blueberry"], "nothing"),
        ["w2", "w1", "w4", "w3"].map(String::from),
        "case-sensitive order would put both capitals above every lowercase label"
    );
}

#[test]
fn a_pin_label_no_workspace_carries_sorts_every_row_and_holds_none() {
    assert_eq!(
        ids(&["delta", "alpha", "charlie"], "~"),
        ["w2", "w3", "w1"].map(String::from)
    );
}

#[test]
fn only_the_first_row_carrying_the_pin_label_is_pinned() {
    assert_eq!(
        ids(&["alpha", "~", "~"], "~"),
        ["w2", "w1", "w3"].map(String::from),
        "hold_pin holds one workspace, so a second row with the same label is an \
         ordinary one and sorts with the rest"
    );
}

#[test]
fn rows_whose_labels_differ_only_in_case_keep_the_order_they_were_listed_in() {
    assert_eq!(
        ids(&["same", "Same"], "nothing"),
        ["w1", "w2"].map(String::from)
    );
    assert_eq!(
        ids(&["Same", "same"], "nothing"),
        ["w1", "w2"].map(String::from),
        "a stable sort leaves an unbreakable tie where it found it, so the sidebar \
         does not shuffle those two on every poll"
    );
}

#[test]
fn an_empty_sidebar_orders_nothing() {
    assert_eq!(alphabetical(&[], "~"), Vec::<String>::new());
}

#[test]
fn a_state_dir_with_no_order_file_reads_as_the_order_this_plugin_has_always_kept() {
    let state = TempDir::new();
    assert_eq!(read(state.path()), Mode::Recency);
}

#[test]
fn a_written_mode_is_what_the_next_reader_gets() {
    let state = TempDir::new();
    for mode in [Mode::Alphabetical, Mode::Recency, Mode::Alphabetical] {
        write(state.path(), mode).expect("the mode could not be written");
        assert_eq!(read(state.path()), mode);
    }
}

#[test]
fn a_state_dir_that_does_not_exist_yet_is_made_rather_than_refused() {
    let parent = TempDir::new();
    let state = parent.join("not-here-yet");
    write(&state, Mode::Alphabetical).expect("the mode could not be written");
    assert_eq!(read(&state), Mode::Alphabetical);
}

#[test]
fn a_file_that_is_not_json_reads_as_recency_rather_than_stopping_the_watcher() {
    let state = TempDir::new();
    std::fs::write(order_path(state.path()), "alphabetical\n").unwrap();
    assert_eq!(read(state.path()), Mode::Recency);
}

#[test]
fn a_file_naming_an_order_this_plugin_does_not_have_reads_as_recency() {
    let state = TempDir::new();
    std::fs::write(
        order_path(state.path()),
        json!({"order": "by-colour"}).to_string(),
    )
    .unwrap();
    assert_eq!(read(state.path()), Mode::Recency);
}

#[test]
fn a_file_carrying_no_order_at_all_reads_as_recency() {
    let state = TempDir::new();
    for body in [
        json!({}),
        json!({"order": 3}),
        json!({"mode": "alphabetical"}),
    ] {
        std::fs::write(order_path(state.path()), body.to_string()).unwrap();
        assert_eq!(read(state.path()), Mode::Recency, "{}", body);
    }
}

#[test]
fn toggling_flips_the_order_and_answers_with_the_one_now_in_force() {
    let state = TempDir::new();
    assert_eq!(toggle(state.path()).unwrap(), Mode::Alphabetical);
    assert_eq!(read(state.path()), Mode::Alphabetical);
    assert_eq!(toggle(state.path()).unwrap(), Mode::Recency);
    assert_eq!(read(state.path()), Mode::Recency);
}

#[test]
fn toggling_a_file_nobody_can_read_repairs_it_rather_than_leaving_it() {
    let state = TempDir::new();
    std::fs::write(order_path(state.path()), "{not json").unwrap();
    assert_eq!(
        toggle(state.path()).unwrap(),
        Mode::Alphabetical,
        "an unreadable file reads as recency, so the flip off it is alphabetical"
    );
    assert_eq!(read(state.path()), Mode::Alphabetical);
}

#[test]
fn a_state_dir_that_cannot_be_written_is_reported_rather_than_swallowed() {
    let parent = TempDir::new();
    let state = parent.dir("locked");
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o555)).unwrap();
    let refused = write(&state, Mode::Alphabetical);
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(
        refused.is_err(),
        "a toggle that silently changed nothing would leave the sidebar in the order \
         the user just asked it to leave: {:?}",
        refused
    );
}

#[test]
fn the_order_file_sits_beside_the_claim_rather_than_in_place_of_it() {
    let state = TempDir::new();
    let socket = Path::new("/private/tmp/recent-spaces-ordering.sock");
    let claim = FileClaim::new(state.path(), socket);
    claim.write("token").unwrap();
    write(state.path(), Mode::Alphabetical).unwrap();

    assert_ne!(order_path(state.path()), claim_path(state.path(), socket));
    assert!(
        !ORDER_FILE.starts_with(CLAIM_PREFIX),
        "the claim is named after whichever socket it was written for, so a name \
         inside that prefix is one some future socket can hash into: {}",
        ORDER_FILE
    );
    assert_eq!(claim.read().as_deref(), Some("token"));
    assert_eq!(read(state.path()), Mode::Alphabetical);
}

#[test]
fn a_sidebar_out_of_order_is_set_with_one_call_carrying_every_row_in_order() {
    let stub = Stub::start(Script::default().open(unsorted()));
    let rows = workspace_rows(&unsorted());
    hold_alphabetical(&stub.client(), &rows, "~").expect("the sort failed");

    assert_eq!(stub.blocks(), vec![sorted_ids()]);
    assert_eq!(
        stub.moves(),
        Vec::new(),
        "one move_block replaces the per-workspace moves rather than joining them"
    );
}

#[test]
fn a_sidebar_already_in_order_is_left_alone() {
    let stub = Stub::start(Script::default());
    let rows = workspace_rows(&[
        listed("~", "w1", false),
        listed("Alpha", "w3", false),
        listed("charlie", "w4", false),
        listed("delta", "w2", false),
    ]);
    hold_alphabetical(&stub.client(), &rows, "~").expect("the sort failed");
    assert_eq!(
        stub.blocks(),
        Vec::<Vec<String>>::new(),
        "re-asserting an order it already has costs a call every two seconds"
    );
}

#[test]
fn an_empty_sidebar_asks_for_no_block_at_all() {
    let stub = Stub::start(Script::default());
    hold_alphabetical(&stub.client(), &[], "~").expect("the sort failed");
    assert_eq!(
        stub.blocks(),
        Vec::<Vec<String>>::new(),
        "Herdr refuses a block with no workspace_ids, measured 0.9.1 on 2026-09-17"
    );
}

#[test]
fn a_refused_block_is_reported_rather_than_swallowed() {
    let stub = Stub::start(
        Script::default()
            .open(unsorted())
            .failing("workspace.move_block", "workspace_not_found"),
    );
    let rows = workspace_rows(&unsorted());
    let failed = hold_alphabetical(&stub.client(), &rows, "~");
    assert!(
        matches!(failed, Err(CallError::Server(_))),
        "a workspace closed between the list and the block is a refusal the grace \
         clock has to see: {:?}",
        failed
    );
}

#[test]
fn a_block_answered_with_anything_but_the_sidebar_is_refused() {
    let stub =
        Stub::start(Script::default().answering("workspace.move_block", json!({"type": "ok"})));
    let refused = move_block(&stub.client(), vec!["w1".to_string()]);
    assert!(
        matches!(refused, Err(CallError::Protocol(_))),
        "`ok` is a shape Herdr can send and is not the sidebar this call names, and \
         the answer is discarded, so nothing else would notice: {:?}",
        refused
    );
}

#[test]
fn in_alphabetical_mode_a_poll_orders_the_whole_sidebar_by_label() {
    let stub = Stub::start(Script::default().open(unsorted()));
    let state = TempDir::new();
    write(state.path(), Mode::Alphabetical).unwrap();
    let mut dwell = settled("w3", 100.0);

    tick(
        &stub.client(),
        &settings(10.0, "~"),
        state.path(),
        &mut dwell,
        200.0,
    )
    .expect("the poll failed");

    assert_eq!(stub.blocks(), vec![sorted_ids()]);
}

#[test]
fn in_alphabetical_mode_a_workspace_past_its_dwell_is_not_promoted() {
    let stub = Stub::start(Script::default().open(unsorted()));
    let state = TempDir::new();
    write(state.path(), Mode::Alphabetical).unwrap();
    let mut dwell = settled("w3", 100.0);

    tick(
        &stub.client(),
        &settings(10.0, "~"),
        state.path(),
        &mut dwell,
        200.0,
    )
    .unwrap();

    assert_eq!(
        stub.moves(),
        Vec::new(),
        "alphabetical owns the position, so a promotion into index 1 would fight it"
    );
    assert!(
        !dwell.promoted,
        "the clock is inert in this mode rather than quietly counting"
    );
}

#[test]
fn in_alphabetical_mode_the_pin_is_held_by_that_same_call_rather_than_its_own() {
    let stub = Stub::start(Script::default().open(unsorted()));
    let state = TempDir::new();
    write(state.path(), Mode::Alphabetical).unwrap();
    let mut dwell = Dwell::new();

    tick(
        &stub.client(),
        &settings(10.0, "~"),
        state.path(),
        &mut dwell,
        200.0,
    )
    .unwrap();

    assert_eq!(
        stub.methods()
            .iter()
            .filter(|m| m.starts_with("workspace.move"))
            .count(),
        1
    );
    assert_eq!(
        stub.blocks()[0][0],
        "w1",
        "the pin keeps index 0 in both orders"
    );
}

#[test]
fn leaving_alphabetical_mode_starts_the_dwell_clock_again() {
    let stub = Stub::start(Script::default().open(unsorted()));
    let state = TempDir::new();
    let mut dwell = settled("w3", 100.0);

    let poll = |dwell: &mut Dwell, now: f64| {
        tick(
            &stub.client(),
            &settings(10.0, "~"),
            state.path(),
            dwell,
            now,
        )
        .expect("the poll failed")
    };

    poll(&mut dwell, 110.0);
    assert!(
        dwell.promoted,
        "recency promotes w3 once the dwell is spent"
    );

    write(state.path(), Mode::Alphabetical).unwrap();
    poll(&mut dwell, 120.0);

    write(state.path(), Mode::Recency).unwrap();
    poll(&mut dwell, 130.0);
    assert_eq!(
        dwell.since, 130.0,
        "coming back with the clock still spent would leave the sidebar alphabetical \
         until focus happened to move"
    );
    assert!(!dwell.promoted);
}

#[test]
fn the_order_a_poll_keeps_is_the_one_the_file_named_when_it_ran() {
    let stub = Stub::start(Script::default().open(unsorted()));
    let state = TempDir::new();
    let mut dwell = settled("w3", 100.0);

    tick(
        &stub.client(),
        &settings(10.0, "~"),
        state.path(),
        &mut dwell,
        110.0,
    )
    .unwrap();
    assert_eq!(
        stub.moves(),
        vec![("w1".to_string(), 0), ("w3".to_string(), 1)],
        "recency holds the pin and promotes in two calls of its own"
    );
    assert_eq!(stub.blocks(), Vec::<Vec<String>>::new());

    write(state.path(), Mode::Alphabetical).unwrap();
    tick(
        &stub.client(),
        &settings(10.0, "~"),
        state.path(),
        &mut dwell,
        120.0,
    )
    .unwrap();
    assert_eq!(
        stub.blocks(),
        vec![sorted_ids()],
        "the watcher re-reads the file every poll, which is the only way the action \
         reaches it"
    );
}

#[test]
fn the_toggle_says_which_order_it_switched_to_and_leaves_it_behind() {
    let state = TempDir::new();
    let env = [("HERDR_PLUGIN_STATE_DIR", state.path().to_str().unwrap())];

    let run = invoke(Path::new(BINARY), &["--toggle-order"], &env);
    assert_eq!(run.status, 0, "{}", run.stderr);
    assert_eq!(
        run.stderr,
        "recent-spaces: the sidebar is now in alphabetical order, from the next poll\n"
    );
    assert_eq!(read(state.path()), Mode::Alphabetical);

    let back = invoke(Path::new(BINARY), &["--toggle-order"], &env);
    assert_eq!(
        back.stderr,
        "recent-spaces: the sidebar is now in recency order, from the next poll\n"
    );
    assert_eq!(read(state.path()), Mode::Recency);
}

#[test]
fn the_toggle_reaches_no_socket() {
    let stub = Stub::start(Script::default().open(unsorted()));
    let state = TempDir::new();
    let run = invoke(
        Path::new(BINARY),
        &["--toggle-order"],
        &[
            ("HERDR_SOCKET_PATH", stub.socket().to_str().unwrap()),
            ("HERDR_PLUGIN_STATE_DIR", state.path().to_str().unwrap()),
        ],
    );

    assert_eq!(run.status, 0, "{}", run.stderr);
    assert_eq!(
        stub.requests().len(),
        0,
        "the action writes a file and exits; the watcher is what talks to Herdr: {:?}",
        stub.methods()
    );
}

#[test]
fn a_toggle_that_cannot_write_fails_rather_than_reporting_a_switch() {
    let parent = TempDir::new();
    let state = parent.dir("locked");
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o555)).unwrap();
    let run = invoke(
        Path::new(BINARY),
        &["--toggle-order"],
        &[("HERDR_PLUGIN_STATE_DIR", state.to_str().unwrap())],
    );
    std::fs::set_permissions(&state, std::fs::Permissions::from_mode(0o755)).unwrap();

    assert_eq!(
        run.status, 1,
        "herdr plugin log reads the exit code, and a zero there says the order changed"
    );
    assert!(run.stderr.contains("cannot write"), "{}", run.stderr);
}

#[test]
fn a_running_watcher_takes_up_the_order_the_action_wrote() {
    let stub = Stub::start(Script::default().open(unsorted()));
    let state = TempDir::new();
    let mut watcher = Running::start(stub.socket(), state.path());
    watch_until(
        &mut watcher,
        || stub.methods().contains(&"workspace.list".to_string()),
        "workspace.list",
    );
    assert_eq!(
        stub.blocks(),
        Vec::<Vec<String>>::new(),
        "it starts in the order it has always kept"
    );

    let toggled = invoke(
        Path::new(BINARY),
        &["--toggle-order"],
        &[("HERDR_PLUGIN_STATE_DIR", state.path().to_str().unwrap())],
    );
    assert_eq!(toggled.status, 0, "{}", toggled.stderr);

    watch_until(
        &mut watcher,
        || !stub.blocks().is_empty(),
        "the sidebar being ordered by label",
    );
    assert_eq!(stub.blocks()[0], sorted_ids());
    watcher.stop();
}
