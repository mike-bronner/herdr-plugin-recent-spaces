#![allow(dead_code)]

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use herdr_plugin_kit::api::client::{Client, Socket};
use herdr_plugin_kit::api::generated::{WorkspaceInfo, GENERATED_PROTOCOL};
use herdr_plugin_kit::env::Environment;
use recent_spaces::claim::Claim;
use recent_spaces::config::Settings;

pub const LAUNCHD_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";

pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const COMMIT: &str = env!("HERDR_PLUGIN_COMMIT");

pub const BUILT: &str = env!("HERDR_PLUGIN_BUILT");

pub fn rebuild_verdict(manifest_version: &str) -> String {
    format!(
        "STALE: this binary is {} but the manifest is {}. Rebuild it with `cargo build --release`.",
        CRATE_VERSION, manifest_version
    )
}

pub fn reinstall_verdict(manifest_version: &str) -> String {
    format!(
        "STALE: this binary is {} but the manifest is {}. This binary was fetched, so reinstall the plugin to get the {} binary.",
        CRATE_VERSION, manifest_version, manifest_version
    )
}

pub struct TempDir {
    path: PathBuf,
}

static NEXT_DIR: AtomicU32 = AtomicU32::new(0);

impl TempDir {
    pub fn new() -> TempDir {
        let n = NEXT_DIR.fetch_add(1, Ordering::SeqCst);
        let since = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let path = PathBuf::from(format!(
            "/private/tmp/recent-spaces-t{}-{:x}-{}",
            std::process::id(),
            since,
            n
        ));
        std::fs::create_dir_all(&path).expect("cannot make the temporary directory");
        TempDir { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }

    pub fn dir(&self, name: &str) -> PathBuf {
        let path = self.join(name);
        std::fs::create_dir_all(&path).expect("cannot make the directory");
        path
    }

    pub fn write(&self, name: &str, body: &str) -> PathBuf {
        let path = self.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("cannot make the parent directory");
        }
        std::fs::write(&path, body).expect("cannot write the file");
        path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

pub fn listed(label: &str, id: &str, focused: bool) -> Value {
    json!({"workspace_id": id, "label": label, "focused": focused,
           "number": 1, "pane_count": 1, "tab_count": 1, "active_tab_id": "t1",
           "agent_status": "idle"})
}

pub fn open(label: &str, id: &str, focused: bool) -> WorkspaceInfo {
    serde_json::from_value(listed(label, id, focused)).expect("the fixture row is not a workspace")
}

pub fn settings(dwell: f64, pin: &str) -> Settings {
    Settings {
        dwell,
        pin: pin.to_string(),
        complaints: Vec::new(),
    }
}

#[derive(Clone)]
pub struct Script {
    pub workspaces: Vec<Value>,
    pub fail: Vec<(String, String)>,
    pub answer: Vec<(String, Value)>,
    pub protocol: u32,
}

impl Default for Script {
    fn default() -> Script {
        Script {
            workspaces: Vec::new(),
            fail: Vec::new(),
            answer: Vec::new(),
            protocol: GENERATED_PROTOCOL,
        }
    }
}

impl Script {
    pub fn open(mut self, workspaces: Vec<Value>) -> Script {
        self.workspaces = workspaces;
        self
    }

    pub fn failing(mut self, method: &str, code: &str) -> Script {
        self.fail.push((method.to_string(), code.to_string()));
        self
    }

    pub fn answering(mut self, method: &str, result: Value) -> Script {
        self.answer.push((method.to_string(), result));
        self
    }

    pub fn speaking(mut self, protocol: u32) -> Script {
        self.protocol = protocol;
        self
    }
}

pub struct Stub {
    socket: PathBuf,
    recorded: Arc<Mutex<Vec<Value>>>,
    stop: Arc<AtomicBool>,
    _dir: TempDir,
}

impl Stub {
    pub fn start(script: Script) -> Stub {
        let dir = TempDir::new();
        let socket = dir.join("herdr.sock");
        let listener = UnixListener::bind(&socket).expect("cannot bind the stub socket");
        let recorded = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));

        let thread_log = Arc::clone(&recorded);
        let thread_stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if thread_stop.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(stream) = stream else { break };
                serve(&stream, &script, &thread_log);
            }
        });

        Stub {
            socket,
            recorded,
            stop,
            _dir: dir,
        }
    }

    pub fn socket(&self) -> &Path {
        &self.socket
    }

    pub fn client(&self) -> Client {
        Client::new(Socket::at(&self.socket), "test")
    }

    pub fn requests(&self) -> Vec<Value> {
        self.recorded.lock().unwrap().clone()
    }

    pub fn methods(&self) -> Vec<String> {
        self.requests()
            .iter()
            .filter_map(|r| r.get("method")?.as_str().map(|s| s.to_string()))
            .collect()
    }

    pub fn moves(&self) -> Vec<(String, u64)> {
        self.requests()
            .iter()
            .filter(|r| r.get("method").and_then(Value::as_str) == Some("workspace.move"))
            .filter_map(|r| {
                let params = r.get("params")?;
                Some((
                    params.get("workspace_id")?.as_str()?.to_string(),
                    params.get("insert_index")?.as_u64()?,
                ))
            })
            .collect()
    }
}

impl Drop for Stub {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = UnixStream::connect(&self.socket);
    }
}

fn serve(stream: &UnixStream, script: &Script, log: &Arc<Mutex<Vec<Value>>>) {
    let mut line = String::new();
    if BufReader::new(stream).read_line(&mut line).is_err() || line.trim().is_empty() {
        return;
    }
    let Ok(request) = serde_json::from_str::<Value>(&line) else {
        return;
    };
    let method = request
        .get("method")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let params = request.get("params").cloned().unwrap_or(json!({}));

    log.lock()
        .unwrap()
        .push(json!({"method": method, "params": params}));

    let id = request.get("id").cloned().unwrap_or(json!("stub"));
    let answer = answer_for(&method, script, &id);
    let mut out = stream;
    let _ = out.write_all(format!("{}\n", answer).as_bytes());
    let _ = out.flush();
}

fn answer_for(method: &str, script: &Script, id: &Value) -> Value {
    let fail = |code: &str| {
        json!({"id": id, "error": {"code": code,
               "message": format!("stub refused {}", method)}})
    };
    let ok = |result: Value| json!({"id": id, "result": result});

    if let Some((_, code)) = script.fail.iter().find(|(m, _)| m == method) {
        return fail(code);
    }
    if let Some((_, result)) = script.answer.iter().find(|(m, _)| m == method) {
        return ok(result.clone());
    }

    match method {
        "ping" => ok(json!({"type": "pong", "version": "0.9.0",
                            "protocol": script.protocol})),
        "workspace.list" => ok(json!({"type": "workspace_list",
                                      "workspaces": script.workspaces})),
        "workspace.move" => ok(json!({"type": "workspace_list",
                                      "workspaces": script.workspaces})),
        _ => fail("unhandled_by_stub"),
    }
}

pub fn env_for(pairs: &[(&str, &str)]) -> Environment {
    let mut all: Vec<(&str, &str)> = vec![("PATH", LAUNCHD_PATH), ("HOME", "/private/tmp")];
    all.extend(pairs.iter().copied());
    Environment::from_pairs(&all)
}

enum Answer {
    Stored(Mutex<Option<String>>),
    Fixed(Option<String>),
}

pub struct StubClaim {
    answer: Answer,
    writable: bool,
    writes: Mutex<Vec<String>>,
}

impl StubClaim {
    pub fn owning() -> StubClaim {
        StubClaim {
            answer: Answer::Stored(Mutex::new(None)),
            writable: true,
            writes: Mutex::new(Vec::new()),
        }
    }

    pub fn held_by(token: &str) -> StubClaim {
        StubClaim {
            answer: Answer::Fixed(Some(token.to_string())),
            writable: true,
            writes: Mutex::new(Vec::new()),
        }
    }

    pub fn missing() -> StubClaim {
        StubClaim {
            answer: Answer::Fixed(None),
            writable: true,
            writes: Mutex::new(Vec::new()),
        }
    }

    pub fn unwritable_but_answering(token: &str) -> StubClaim {
        StubClaim {
            answer: Answer::Fixed(Some(token.to_string())),
            writable: false,
            writes: Mutex::new(Vec::new()),
        }
    }

    pub fn writes(&self) -> Vec<String> {
        self.writes.lock().unwrap().clone()
    }
}

impl Claim for StubClaim {
    fn write(&self, token: &str) -> Result<(), String> {
        if !self.writable {
            return Err("the state directory is read-only".to_string());
        }
        self.writes.lock().unwrap().push(token.to_string());
        if let Answer::Stored(held) = &self.answer {
            *held.lock().unwrap() = Some(token.to_string());
        }
        Ok(())
    }

    fn read(&self) -> Option<String> {
        match &self.answer {
            Answer::Stored(held) => held.lock().unwrap().clone(),
            Answer::Fixed(token) => token.clone(),
        }
    }
}

pub fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn read_repo_file(name: &str) -> String {
    std::fs::read_to_string(manifest_dir().join(name))
        .unwrap_or_else(|_| panic!("cannot read {}", name))
}
