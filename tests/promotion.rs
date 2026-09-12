mod support;

use herdr_plugin_kit::api::client::{CallError, Client, Socket};
use recent_spaces::promote::{hold_pin, tick, Dwell};
use support::*;

fn refusal_code(error: CallError) -> Option<String> {
    match error {
        CallError::Server(body) => Some(body.code),
        other => panic!("the stub refused the call, and this reads as {:?}", other),
    }
}

fn settled(id: &str, since: f64) -> Dwell {
    let mut dwell = Dwell::new();
    dwell.observe(Some(id), since);
    dwell
}

fn poll(stub: &Stub, dwell: &mut Dwell, now: f64) -> Vec<(String, u64)> {
    tick(&stub.client(), &settings(10.0, "~"), dwell, now).expect("the poll failed");
    stub.moves()
}

#[test]
fn a_pin_already_first_is_left_alone() {
    let stub = Stub::start(Script::default());
    let rows = vec![open("~", "w1", false), open("a", "w2", false)];
    let pin = hold_pin(&stub.client(), &rows, "~").unwrap();
    assert_eq!(pin.as_deref(), Some("w1"));
    assert_eq!(stub.moves(), Vec::new());
}

#[test]
fn a_pin_elsewhere_is_moved_to_the_top() {
    let stub = Stub::start(Script::default());
    let rows = vec![open("a", "w2", false), open("~", "w1", false)];
    let pin = hold_pin(&stub.client(), &rows, "~").unwrap();
    assert_eq!(pin.as_deref(), Some("w1"));
    assert_eq!(stub.moves(), vec![("w1".to_string(), 0)]);
}

#[test]
fn no_pin_open_moves_nothing() {
    let stub = Stub::start(Script::default());
    let rows = vec![open("a", "w2", false)];
    assert_eq!(hold_pin(&stub.client(), &rows, "~").unwrap(), None);
    assert_eq!(stub.moves(), Vec::new());
}

#[test]
fn a_refused_pin_move_is_reported_rather_than_swallowed() {
    let stub = Stub::start(Script::default().failing("workspace.move", "workspace_not_found"));
    let rows = vec![open("a", "w2", false), open("~", "w1", false)];
    let failed = hold_pin(&stub.client(), &rows, "~");
    assert_eq!(
        failed.err().and_then(refusal_code).as_deref(),
        Some("workspace_not_found")
    );
}

#[test]
fn a_different_workspace_restarts_the_clock() {
    let mut dwell = settled("w2", 100.0);
    dwell.promoted = true;
    dwell.observe(Some("w3"), 140.0);
    assert_eq!(dwell.workspace_id.as_deref(), Some("w3"));
    assert_eq!(dwell.since, 140.0);
    assert!(!dwell.promoted);
}

#[test]
fn the_same_workspace_leaves_since_and_promoted_alone() {
    let mut dwell = settled("w2", 100.0);
    dwell.promoted = true;
    dwell.observe(Some("w2"), 140.0);
    assert_eq!(dwell.since, 100.0);
    assert!(dwell.promoted);
}

#[test]
fn losing_focus_entirely_restarts_the_clock() {
    let mut dwell = settled("w2", 100.0);
    dwell.observe(None, 140.0);
    assert_eq!(dwell.workspace_id, None);
    assert_eq!(dwell.since, 140.0);
}

#[test]
fn a_workspace_dwelled_in_long_enough_is_promoted_below_the_pin() {
    let stub = Stub::start(Script::default().open(vec![
        listed("~", "w1", false),
        listed("a", "w2", false),
        listed("b", "w3", true),
    ]));
    let mut dwell = settled("w3", 100.0);
    assert_eq!(poll(&stub, &mut dwell, 110.0), vec![("w3".to_string(), 1)]);
    assert!(dwell.promoted);
}

#[test]
fn short_of_the_dwell_nothing_moves() {
    let stub = Stub::start(Script::default().open(vec![
        listed("~", "w1", false),
        listed("a", "w2", false),
        listed("b", "w3", true),
    ]));
    let mut dwell = settled("w3", 100.0);
    assert_eq!(poll(&stub, &mut dwell, 109.9), Vec::new());
    assert!(!dwell.promoted);
}

#[test]
fn a_focus_change_seen_by_this_poll_restarts_the_clock() {
    let stub = Stub::start(Script::default().open(vec![
        listed("~", "w1", false),
        listed("a", "w2", false),
        listed("b", "w3", true),
    ]));
    let mut dwell = settled("w2", 100.0);
    assert_eq!(poll(&stub, &mut dwell, 110.0), Vec::new());
    assert_eq!(dwell.workspace_id.as_deref(), Some("w3"));
    assert_eq!(dwell.since, 110.0);
}

#[test]
fn a_promoted_workspace_is_not_promoted_again() {
    let stub = Stub::start(Script::default().open(vec![
        listed("~", "w1", false),
        listed("b", "w3", true),
        listed("a", "w2", false),
    ]));
    let mut dwell = settled("w3", 100.0);
    dwell.promoted = true;
    assert_eq!(poll(&stub, &mut dwell, 200.0), Vec::new());
}

#[test]
fn a_workspace_already_in_place_settles_without_moving() {
    let stub = Stub::start(Script::default().open(vec![
        listed("~", "w1", false),
        listed("b", "w3", true),
        listed("a", "w2", false),
    ]));
    let mut dwell = settled("w3", 100.0);
    assert_eq!(poll(&stub, &mut dwell, 110.0), Vec::new());
    assert!(dwell.promoted);
}

#[test]
fn the_pin_itself_is_never_promoted() {
    let stub = Stub::start(
        Script::default().open(vec![listed("~", "w1", true), listed("a", "w2", false)]),
    );
    let mut dwell = settled("w1", 100.0);
    assert_eq!(poll(&stub, &mut dwell, 200.0), Vec::new());
}

#[test]
fn no_focused_workspace_promotes_nothing() {
    let stub = Stub::start(
        Script::default().open(vec![listed("~", "w1", false), listed("a", "w2", false)]),
    );
    let mut dwell = Dwell::new();
    assert_eq!(poll(&stub, &mut dwell, 200.0), Vec::new());
}

#[test]
fn the_pin_is_held_even_when_nothing_is_promoted() {
    let stub = Stub::start(
        Script::default().open(vec![listed("a", "w2", false), listed("~", "w1", true)]),
    );
    let mut dwell = settled("w1", 100.0);
    assert_eq!(poll(&stub, &mut dwell, 200.0), vec![("w1".to_string(), 0)]);
}

#[test]
fn an_empty_workspace_list_does_nothing_and_leaves_the_clock_alone() {
    let stub = Stub::start(Script::default());
    let mut dwell = settled("w3", 100.0);
    assert_eq!(poll(&stub, &mut dwell, 200.0), Vec::new());
    assert_eq!(dwell.workspace_id.as_deref(), Some("w3"));
    assert_eq!(dwell.since, 100.0);
}

#[test]
fn a_pin_label_no_workspace_carries_holds_nothing_and_still_promotes() {
    let stub = Stub::start(
        Script::default().open(vec![listed("b", "w3", true), listed("a", "w2", false)]),
    );
    let mut dwell = settled("w3", 100.0);
    tick(
        &stub.client(),
        &settings(10.0, "nothing"),
        &mut dwell,
        110.0,
    )
    .unwrap();
    assert_eq!(
        stub.moves(),
        vec![("w3".to_string(), 1)],
        "index 0 is left to whatever Herdr put there"
    );
}

#[test]
fn a_dwell_read_from_config_is_what_the_clock_waits_for() {
    let stub = Stub::start(Script::default().open(vec![
        listed("~", "w1", false),
        listed("a", "w2", false),
        listed("b", "w3", true),
    ]));
    let mut dwell = settled("w3", 100.0);
    tick(&stub.client(), &settings(30.0, "~"), &mut dwell, 110.0).unwrap();
    assert_eq!(stub.moves(), Vec::new(), "ten seconds is short of thirty");
    tick(&stub.client(), &settings(30.0, "~"), &mut dwell, 130.0).unwrap();
    assert_eq!(stub.moves(), vec![("w3".to_string(), 1)]);
}

#[test]
fn an_unreachable_socket_is_reported_so_the_grace_clock_can_start() {
    let client = Client::new(
        Socket::at("/private/tmp/recent-spaces-no-such.sock"),
        "test",
    );
    let mut dwell = Dwell::new();
    let failed = tick(&client, &settings(10.0, "~"), &mut dwell, 1.0);
    assert!(
        matches!(failed, Err(CallError::Connect { .. })),
        "a socket that is not there is a connect failure rather than a refusal, \
         because the two want different answers: {:?}",
        failed
    );
}

#[test]
fn a_refused_workspace_list_is_reported_rather_than_read_as_no_workspaces() {
    let stub = Stub::start(Script::default().failing("workspace.list", "internal_error"));
    let mut dwell = settled("w3", 100.0);
    assert!(tick(&stub.client(), &settings(10.0, "~"), &mut dwell, 200.0).is_err());
}
