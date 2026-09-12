use herdr_plugin_kit::api::client::{CallError, Client};
use herdr_plugin_kit::api::generated::{
    EmptyParams, RequestMethod, WorkspaceInfo, WorkspaceListAnswer, WorkspaceMoveParams,
};

use crate::config::Settings;

pub const PROMOTE_INDEX: u32 = 1;

pub const PIN_INDEX: u32 = 0;

pub fn workspaces(client: &Client) -> Result<Vec<WorkspaceInfo>, CallError> {
    client
        .call::<WorkspaceListAnswer>(RequestMethod::WorkspaceList(EmptyParams(
            serde_json::Map::new(),
        )))
        .map(|answer| answer.workspaces)
}

pub fn workspace_move(
    client: &Client,
    workspace_id: &str,
    insert_index: u32,
) -> Result<(), CallError> {
    client
        .call::<WorkspaceListAnswer>(RequestMethod::WorkspaceMove(WorkspaceMoveParams {
            insert_index,
            workspace_id: workspace_id.to_string(),
        }))
        .map(|_| ())
}

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
    listed: &[WorkspaceInfo],
    pin_label: &str,
) -> Result<Option<String>, CallError> {
    let pin = listed
        .iter()
        .find(|w| w.label == pin_label)
        .map(|w| w.workspace_id.clone());
    if let Some(id) = &pin {
        let first = listed.first().map(|w| w.workspace_id.as_str());
        if first != Some(id.as_str()) {
            workspace_move(client, id, PIN_INDEX)?;
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
    let listed = workspaces(client)?;
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
    if at != Some(PROMOTE_INDEX as usize) {
        workspace_move(client, &current, PROMOTE_INDEX)?;
    }
    Ok(())
}
