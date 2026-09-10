mod support;

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use recent_spaces::claim::Claim;
use recent_spaces::retire::{watch, GRACE_SECONDS, POLL_SECONDS};
use support::*;

#[derive(Clone)]
enum Outcome {
    Ok,
    Fail,
    Panic,
}

struct Finishing {
    token: String,
    done: Rc<Cell<bool>>,
    writes: RefCell<Vec<String>>,
}

impl Claim for Finishing {
    fn write(&self, token: &str) -> Result<(), String> {
        self.writes.borrow_mut().push(token.to_string());
        Ok(())
    }

    fn read(&self) -> Option<String> {
        match self.done.get() {
            true => Some("a-newer-watcher".to_string()),
            false => Some(self.token.clone()),
        }
    }
}

struct Run {
    polled: Vec<f64>,
    slept: Vec<f64>,
    written: Vec<String>,
}

fn run_watch(outcomes: Vec<Outcome>, step: f64) -> Run {
    let done = Rc::new(Cell::new(false));
    let claim = Finishing {
        token: "tok".to_string(),
        done: Rc::clone(&done),
        writes: RefCell::new(Vec::new()),
    };
    let polled: RefCell<Vec<f64>> = RefCell::new(Vec::new());
    let slept: RefCell<Vec<f64>> = RefCell::new(Vec::new());
    let reads = Cell::new(0.0);

    watch(
        &claim,
        "tok",
        |now| {
            polled.borrow_mut().push(now);
            let at = polled.borrow().len() - 1;
            match outcomes.get(at) {
                None => {
                    done.set(true);
                    Ok(())
                }
                Some(Outcome::Ok) => Ok(()),
                Some(Outcome::Fail) => Err("cannot reach the socket".to_string()),
                Some(Outcome::Panic) => panic!("a bug inside the poll"),
            }
        },
        || {
            let n = reads.get();
            reads.set(n + 1.0);
            step * n
        },
        |seconds| slept.borrow_mut().push(seconds),
    );

    Run {
        polled: polled.into_inner(),
        slept: slept.into_inner(),
        written: claim.writes.into_inner(),
    }
}

fn never_polls(claim: &dyn Claim) -> bool {
    let polled = Cell::new(false);
    watch(
        claim,
        "tok",
        |_| {
            polled.set(true);
            Ok(())
        },
        || 0.0,
        |_| panic!("the loop slept instead of retiring"),
    );
    !polled.get()
}

#[test]
fn it_retires_when_a_newer_watcher_claims_the_socket() {
    assert!(never_polls(&StubClaim::held_by("a-newer-watcher")));
}

#[test]
fn it_retires_when_the_claim_file_has_gone() {
    assert!(never_polls(&StubClaim::missing()));
}

#[test]
fn it_never_starts_without_a_claim_of_its_own() {
    assert!(never_polls(&StubClaim::unwritable_but_answering("tok")));
}

#[test]
fn it_claims_the_socket_before_it_polls_at_all() {
    let run = run_watch(vec![Outcome::Ok], 10.0);
    assert_eq!(run.written, vec!["tok".to_string()]);
}

#[test]
fn a_single_failing_poll_does_not_end_the_loop() {
    let run = run_watch(vec![Outcome::Fail, Outcome::Ok], 10.0);
    assert_eq!(run.polled.len(), 3, "{:?}", run.polled);
}

#[test]
fn it_exits_once_the_socket_has_been_gone_for_the_grace_period() {
    assert_eq!(GRACE_SECONDS, 30.0);
    let run = run_watch(vec![Outcome::Fail; 10], 10.0);
    assert_eq!(run.polled, vec![0.0, 20.0, 40.0]);
}

#[test]
fn one_good_poll_resets_the_grace_period() {
    let mut outcomes = Vec::new();
    for _ in 0..10 {
        outcomes.push(Outcome::Fail);
        outcomes.push(Outcome::Ok);
    }
    let run = run_watch(outcomes, 10.0);
    assert_eq!(run.polled.len(), 21, "{:?}", run.polled);
}

#[test]
fn a_panicking_poll_does_not_end_the_loop() {
    let run = run_watch(vec![Outcome::Panic, Outcome::Ok], 10.0);
    assert_eq!(run.polled.len(), 3, "{:?}", run.polled);
}

#[test]
fn a_poll_that_panics_every_time_retires_on_the_grace_period() {
    let run = run_watch(vec![Outcome::Panic; 10], 10.0);
    assert_eq!(run.polled, vec![0.0, 20.0, 40.0]);
}

#[test]
fn the_loop_waits_the_poll_interval_between_polls() {
    let run = run_watch(vec![Outcome::Ok, Outcome::Ok], 10.0);
    assert_eq!(run.slept, vec![POLL_SECONDS; 3]);
}

#[test]
fn the_poll_interval_is_slow_enough_not_to_spin() {
    let run = run_watch(vec![Outcome::Ok], 10.0);
    assert!(
        run.slept.iter().all(|waited| *waited >= 1.0),
        "it runs for the whole life of a session: {:?}",
        run.slept
    );
}
