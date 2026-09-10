use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use serde_json::{json, Value};

pub const SOCKET_VAR: &str = "HERDR_SOCKET_PATH";

pub const DEFAULT_SOCKET: &str = ".config/herdr/herdr.sock";

pub const CALL_TIMEOUT: Duration = Duration::from_secs(5);

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug)]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

#[derive(Debug)]
pub enum CallError {
    Transport(String),
    Api(ApiError),
}

impl CallError {
    pub fn code(&self) -> Option<&str> {
        match self {
            CallError::Api(e) => Some(e.code.as_str()),
            CallError::Transport(_) => None,
        }
    }
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CallError::Transport(m) => write!(f, "{}", m),
            CallError::Api(e) => write!(f, "{} ({})", e.message, e.code),
        }
    }
}

pub fn socket_path(env: &crate::config::Environment) -> PathBuf {
    match env.get(SOCKET_VAR) {
        Some(named) if !named.is_empty() => PathBuf::from(named),
        _ => env.home().join(DEFAULT_SOCKET),
    }
}

pub struct Client {
    socket: PathBuf,
}

impl Client {
    pub fn new(socket: PathBuf) -> Client {
        Client { socket }
    }

    pub fn socket(&self) -> &Path {
        &self.socket
    }

    pub fn call(&self, method: &str, params: Value) -> Result<Value, CallError> {
        let id = NEXT_ID.fetch_add(1, Ordering::SeqCst);
        let request = json!({"id": format!("recent-{}", id),
                             "method": method,
                             "params": params});

        let stream = UnixStream::connect(&self.socket).map_err(|e| {
            CallError::Transport(format!("cannot reach {}: {}", self.socket.display(), e))
        })?;
        let _ = stream.set_read_timeout(Some(CALL_TIMEOUT));
        let _ = stream.set_write_timeout(Some(CALL_TIMEOUT));

        let mut writer = &stream;
        writer
            .write_all(format!("{}\n", request).as_bytes())
            .and_then(|()| writer.flush())
            .map_err(|e| CallError::Transport(format!("cannot send {}: {}", method, e)))?;

        let mut line = String::new();
        BufReader::new(&stream).read_line(&mut line).map_err(|e| {
            CallError::Transport(format!("cannot read the answer to {}: {}", method, e))
        })?;
        if line.trim().is_empty() {
            return Err(CallError::Transport(format!(
                "the server closed the connection without answering {}",
                method
            )));
        }

        let answer: Value = serde_json::from_str(&line).map_err(|e| {
            CallError::Transport(format!("the answer to {} is not JSON: {}", method, e))
        })?;

        if let Some(err) = answer.get("error") {
            return Err(CallError::Api(ApiError {
                code: string_at(err, "code").unwrap_or_default(),
                message: string_at(err, "message")
                    .unwrap_or_else(|| format!("{} failed with no message", method)),
            }));
        }
        match answer.get("result") {
            Some(result) => Ok(result.clone()),
            None => Err(CallError::Transport(format!(
                "the answer to {} carries neither a result nor an error",
                method
            ))),
        }
    }
}

fn string_at(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(|s| s.to_string())
}

#[derive(Debug, Clone, PartialEq)]
pub struct Workspace {
    pub workspace_id: String,
    pub label: String,
    pub focused: bool,
}

pub fn workspaces(client: &Client) -> Result<Vec<Workspace>, CallError> {
    let result = client.call("workspace.list", json!({}))?;
    let listed = result
        .get("workspaces")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(listed
        .iter()
        .filter_map(|w| {
            Some(Workspace {
                workspace_id: string_at(w, "workspace_id")?,
                label: string_at(w, "label").unwrap_or_default(),
                focused: w.get("focused").and_then(Value::as_bool).unwrap_or(false),
            })
        })
        .collect())
}

pub fn workspace_move(
    client: &Client,
    workspace_id: &str,
    insert_index: usize,
) -> Result<(), CallError> {
    client
        .call(
            "workspace.move",
            json!({"workspace_id": workspace_id, "insert_index": insert_index}),
        )
        .map(|_| ())
}
