use crate::api::{self, CallError, Client, Workspace};
use crate::config::Settings;

pub const PROMOTE_INDEX: usize = 1;

pub const PIN_INDEX: usize = 0;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Dwell {
    pub workspace_id: Option<String>,
    pub since: f64,
    pub promoted: bool,
}

impl Dwell {
    pub fn new() -> Dwell {
        Dwell::default()
    }

    pub fn observe(&mut self, workspace_id: Option<&str>, now: f64) {
        if workspace_id != self.workspace_id.as_deref() {
            self.workspace_id = workspace_id.map(|id| id.to_string());
            self.since = now;
            self.promoted = false;
        }
    }
}

pub fn hold_pin(
    client: &Client,
    listed: &[Workspace],
    pin_label: &str,
) -> Result<Option<String>, CallError> {
    let pin = listed
        .iter()
        .find(|w| w.label == pin_label)
        .map(|w| w.workspace_id.clone());
    if let Some(id) = &pin {
        let first = listed.first().map(|w| w.workspace_id.as_str());
        if first != Some(id.as_str()) {
            api::workspace_move(client, id, PIN_INDEX)?;
        }
    }
    Ok(pin)
}

pub fn tick(
    client: &Client,
    settings: &Settings,
    dwell: &mut Dwell,
    now: f64,
) -> Result<(), CallError> {
    let listed = api::workspaces(client)?;
    if listed.is_empty() {
        return Ok(());
    }
    let pin = hold_pin(client, &listed, &settings.pin)?;

    let focused = listed
        .iter()
        .find(|w| w.focused)
        .map(|w| w.workspace_id.clone());
    dwell.observe(focused.as_deref(), now);

    let Some(current) = focused else {
        return Ok(());
    };
    if Some(&current) == pin.as_ref() || dwell.promoted {
        return Ok(());
    }
    if now - dwell.since < settings.dwell {
        return Ok(());
    }

    dwell.promoted = true;
    let at = listed.iter().position(|w| w.workspace_id == current);
    if at != Some(PROMOTE_INDEX) {
        api::workspace_move(client, &current, PROMOTE_INDEX)?;
    }
    Ok(())
}
