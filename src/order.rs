use std::path::{Path, PathBuf};

use herdr_plugin_kit::api::client::{CallError, Client};
use herdr_plugin_kit::api::generated::{
    RequestMethod, WorkspaceInfo, WorkspaceListAnswer, WorkspaceMoveBlockParams,
};
use serde_json::{json, Value};

pub const ORDER_FILE: &str = "recent-spaces-order.json";

pub const ORDER_KEY: &str = "order";

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Recency,
    Alphabetical,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Recency => "recency",
            Mode::Alphabetical => "alphabetical",
        }
    }

    pub fn named(text: &str) -> Option<Mode> {
        [Mode::Recency, Mode::Alphabetical]
            .into_iter()
            .find(|mode| mode.name() == text)
    }

    pub fn other(self) -> Mode {
        match self {
            Mode::Recency => Mode::Alphabetical,
            Mode::Alphabetical => Mode::Recency,
        }
    }
}

pub fn order_path(state_dir: &Path) -> PathBuf {
    state_dir.join(ORDER_FILE)
}

pub fn read(state_dir: &Path) -> Mode {
    let Ok(text) = std::fs::read_to_string(order_path(state_dir)) else {
        return Mode::default();
    };
    serde_json::from_str::<Value>(&text)
        .ok()
        .and_then(|parsed| parsed.get(ORDER_KEY)?.as_str().and_then(Mode::named))
        .unwrap_or_default()
}

pub fn write(state_dir: &Path, mode: Mode) -> Result<(), String> {
    std::fs::create_dir_all(state_dir)
        .map_err(|e| format!("cannot make {}: {}", state_dir.display(), e))?;
    let path = order_path(state_dir);
    let staged = path.with_extension("json.tmp");
    std::fs::write(&staged, format!("{}\n", json!({ORDER_KEY: mode.name()})))
        .map_err(|e| format!("cannot write {}: {}", staged.display(), e))?;
    std::fs::rename(&staged, &path).map_err(|e| format!("cannot place {}: {}", path.display(), e))
}

pub fn toggle(state_dir: &Path) -> Result<Mode, String> {
    let wanted = read(state_dir).other();
    write(state_dir, wanted).map(|()| wanted)
}

pub fn alphabetical(listed: &[WorkspaceInfo], pin_label: &str) -> Vec<String> {
    let pin = listed.iter().position(|w| w.label == pin_label);
    let mut below: Vec<&WorkspaceInfo> = listed
        .iter()
        .enumerate()
        .filter(|(at, _)| Some(*at) != pin)
        .map(|(_, w)| w)
        .collect();
    below.sort_by_key(|w| w.label.to_lowercase());
    pin.map(|at| listed[at].workspace_id.clone())
        .into_iter()
        .chain(below.into_iter().map(|w| w.workspace_id.clone()))
        .collect()
}

pub fn move_block(client: &Client, workspace_ids: Vec<String>) -> Result<(), CallError> {
    client
        .call::<WorkspaceListAnswer>(RequestMethod::WorkspaceMoveBlock(
            WorkspaceMoveBlockParams {
                before_workspace_id: None,
                workspace_ids,
            },
        ))
        .map(|_| ())
}

pub fn hold_alphabetical(
    client: &Client,
    listed: &[WorkspaceInfo],
    pin_label: &str,
) -> Result<(), CallError> {
    let wanted = alphabetical(listed, pin_label);
    if wanted.iter().eq(listed.iter().map(|w| &w.workspace_id)) {
        return Ok(());
    }
    move_block(client, wanted)
}
