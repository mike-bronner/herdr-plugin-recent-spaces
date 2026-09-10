use std::panic::{catch_unwind, AssertUnwindSafe};

use crate::claim::Claim;

pub const POLL_SECONDS: f64 = 2.0;

pub const GRACE_SECONDS: f64 = 30.0;

pub fn watch<P, C, S>(claim: &dyn Claim, token: &str, mut poll: P, clock: C, sleep: S)
where
    P: FnMut(f64) -> Result<(), String>,
    C: Fn() -> f64,
    S: Fn(f64),
{
    if claim.write(token).is_err() {
        return;
    }

    let mut unreachable_since: Option<f64> = None;
    loop {
        if claim.read().as_deref() != Some(token) {
            return;
        }
        let polled = catch_unwind(AssertUnwindSafe(|| poll(clock())));
        match polled {
            Ok(Ok(())) => unreachable_since = None,
            Ok(Err(_)) | Err(_) => {
                let now = clock();
                match unreachable_since {
                    None => unreachable_since = Some(now),
                    Some(since) if now - since >= GRACE_SECONDS => return,
                    Some(_) => {}
                }
            }
        }
        sleep(POLL_SECONDS);
    }
}
