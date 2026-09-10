#![allow(dead_code)]

use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{json, Value};

use recent_spaces::api::{Client, Workspace};
use recent_spaces::claim::Claim;
use recent_spaces::config::{Environment, Settings};
use recent_spaces::version::CRATE_VERSION;

pub const LAUNCHD_PATH: &str = "/usr/bin:/bin:/usr/sbin:/sbin";

pub fn rebuild_verdict(manifest_version: &str) -> String {
    format!(
        "STALE: this binary is {} but the manifest is {}. Rebuild it with `cargo build --release`.",
        CRATE_VERSION, manifest_version
    )
}

pub fn reinstall_verdict(manifest_version: &str) -> String {
    format!(
        "STALE: this binary is {} but the manifest is {}. This binary was downloaded, so reinstall the plugin to get the {} binary.",
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

pub fn set_mtime(path: &Path, when: u64) {
    use std::time::{Duration, UNIX_EPOCH};
    let file = std::fs::File::open(path).expect("cannot open the path to stamp it");
    let times = std::fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(when));
    file.set_times(times).expect("cannot set the mtime");
}

pub fn listed(label: &str, id: &str, focused: bool) -> Value {
    json!({"workspace_id": id, "label": label, "focused": focused,
           "number": 1, "pane_count": 1, "tab_count": 1, "active_tab_id": "t1"})
}

pub fn open(label: &str, id: &str, focused: bool) -> Workspace {
    Workspace {
        workspace_id: id.to_string(),
        label: label.to_string(),
        focused,
    }
}

pub fn settings(dwell: f64, pin: &str) -> Settings {
    Settings {
        dwell,
        pin: pin.to_string(),
        complaints: Vec::new(),
    }
}

#[derive(Clone, Default)]
pub struct Script {
    pub workspaces: Vec<Value>,
    pub fail: Vec<(String, String)>,
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
        Client::new(self.socket.to_path_buf())
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
    let answer = answer_for(&method, &params, script, &id);
    let mut out = stream;
    let _ = out.write_all(format!("{}\n", answer).as_bytes());
    let _ = out.flush();
}

fn answer_for(method: &str, params: &Value, script: &Script, id: &Value) -> Value {
    let fail = |code: &str| {
        json!({"id": id, "error": {"code": code,
               "message": format!("stub refused {}", method)}})
    };
    let ok = |result: Value| json!({"id": id, "result": result});

    if let Some((_, code)) = script.fail.iter().find(|(m, _)| m == method) {
        return fail(code);
    }

    match method {
        "workspace.list" => ok(json!({"type": "workspace_list",
                                      "workspaces": script.workspaces})),
        "workspace.move" => ok(json!({"type": "workspace_moved",
                                      "workspace_id": params.get("workspace_id")})),
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

pub fn fake_root(dir: &TempDir, real_build: bool) -> PathBuf {
    let root = dir.dir("plugin");
    std::fs::create_dir_all(root.join("bin")).unwrap();
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("target/release")).unwrap();
    std::fs::copy(manifest_dir().join("bin/watch"), root.join("bin/watch")).unwrap();
    for shared in ["bin/asset-name", "bin/find-cargo"] {
        std::fs::copy(manifest_dir().join(shared), root.join(shared)).unwrap();
    }
    if real_build {
        std::fs::copy(manifest_dir().join("bin/build"), root.join("bin/build")).unwrap();
    }
    std::fs::write(root.join("Cargo.toml"), "").unwrap();
    std::fs::write(root.join("Cargo.lock"), "").unwrap();
    std::fs::write(root.join("src/main.rs"), "").unwrap();
    root
}

pub fn declare(root: &Path, version: &str, repository: &str) {
    std::fs::write(
        root.join("herdr-plugin.toml"),
        format!(
            "id = \"probe.fake\"\nmin_herdr_version = \"0.9.0\"\nversion = \"{}\"\n",
            version
        ),
    )
    .unwrap();
    std::fs::write(
        root.join("Cargo.toml"),
        format!(
            "[package]\nname = \"fake\"\nversion = \"{}\"\nrepository = \"{}\"\n",
            version, repository
        ),
    )
    .unwrap();
}

pub fn executable(path: &Path, body: &str) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::write(path, body).unwrap();
    let mut perms = std::fs::metadata(path).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(path, perms).unwrap();
}

pub fn mode_of(path: &Path) -> u32 {
    std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(path).unwrap().permissions())
}

pub struct Shim {
    pub status: i32,
    pub stdout: String,
    pub stderr: String,
}

pub fn run_script(root: &Path, script: &str, args: &[&str], extra: &[(&str, &str)]) -> Shim {
    let mut command = Command::new("/bin/sh");
    command
        .arg(root.join(script))
        .args(args)
        .env_clear()
        .env("PATH", LAUNCHD_PATH)
        .env("HOME", "/private/tmp")
        .env("HERDR_PLUGIN_ROOT", root);
    for (key, value) in extra {
        command.env(key, value);
    }
    let out = command.output().expect("cannot run the script");
    Shim {
        status: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).to_string(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
    }
}

pub fn run_shim(root: &Path, extra: &[(&str, &str)]) -> Shim {
    run_script(root, "bin/watch", &[], extra)
}

pub fn run_build(root: &Path, extra: &[(&str, &str)]) -> Shim {
    run_script(root, "bin/build", &[], extra)
}

pub fn run_build_args(root: &Path, args: &[&str], extra: &[(&str, &str)]) -> Shim {
    run_script(root, "bin/build", args, extra)
}

pub fn fake_cargo(dir: &TempDir, rel: &str, log: &Path) -> PathBuf {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    executable(
        &path,
        &format!(
            "#!/bin/sh\nprintf '%s\\n%s\\n' \"$0\" \"$PATH\" > '{}'\n",
            log.to_string_lossy()
        ),
    );
    path
}

pub fn current(root: &Path) {
    for named in ["src/main.rs", "src", "Cargo.toml", "Cargo.lock"] {
        set_mtime(&root.join(named), 1000);
    }
    set_mtime(&root.join("target/release/watch"), 2000);
}

pub fn sha256_of(bytes: &[u8]) -> String {
    let dir = TempDir::new();
    let path = dir.join("body");
    std::fs::write(&path, bytes).unwrap();
    let out = Command::new("/usr/bin/shasum")
        .args(["-a", "256"])
        .arg(&path)
        .output()
        .expect("cannot run shasum");
    String::from_utf8_lossy(&out.stdout)
        .split_whitespace()
        .next()
        .expect("shasum printed nothing")
        .to_string()
}

pub fn only_these_tools(dir: &TempDir, named: &str, tools: &[&str]) -> String {
    let bin = dir.dir(named);
    for tool in tools {
        let mut found = None;
        for prefix in ["/usr/bin", "/bin", "/usr/sbin", "/sbin"] {
            let candidate = PathBuf::from(prefix).join(tool);
            if candidate.exists() {
                found = Some(candidate);
                break;
            }
        }
        let from = found.unwrap_or_else(|| panic!("{} is not on the launchd PATH", tool));
        let _ = std::os::unix::fs::symlink(&from, bin.join(tool));
    }
    bin.to_string_lossy().to_string()
}

type Published = Arc<Mutex<Vec<(String, Vec<u8>)>>>;

pub struct Assets {
    base: String,
    port: u16,
    files: Published,
    asked: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
}

impl Assets {
    pub fn start() -> Assets {
        let listener = TcpListener::bind("127.0.0.1:0").expect("cannot bind the asset server");
        let port = listener.local_addr().unwrap().port();
        let files = Arc::new(Mutex::new(Vec::new()));
        let asked = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));

        let thread_files = Arc::clone(&files);
        let thread_asked = Arc::clone(&asked);
        let thread_stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                if thread_stop.load(Ordering::SeqCst) {
                    break;
                }
                let Ok(mut stream) = stream else { break };
                serve_asset(&mut stream, &thread_files, &thread_asked);
            }
        });

        Assets {
            base: format!("http://127.0.0.1:{}/owner/repo", port),
            port,
            files,
            asked,
            stop,
        }
    }

    pub fn base(&self) -> &str {
        &self.base
    }

    pub fn publish(&self, path: &str, body: &[u8]) {
        self.files
            .lock()
            .unwrap()
            .push((path.to_string(), body.to_vec()));
    }

    pub fn asked(&self) -> Vec<String> {
        self.asked.lock().unwrap().clone()
    }
}

impl Drop for Assets {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }
}

fn serve_asset(stream: &mut TcpStream, files: &Published, asked: &Arc<Mutex<Vec<String>>>) {
    let Ok(peek) = stream.try_clone() else { return };
    let mut reader = BufReader::new(peek);
    let mut request = String::new();
    if reader.read_line(&mut request).is_err() {
        return;
    }
    loop {
        let mut header = String::new();
        match reader.read_line(&mut header) {
            Ok(0) => break,
            Ok(_) if header.trim().is_empty() => break,
            Ok(_) => continue,
            Err(_) => return,
        }
    }

    let path = request
        .split_whitespace()
        .nth(1)
        .unwrap_or_default()
        .to_string();
    asked.lock().unwrap().push(path.clone());

    let found = files
        .lock()
        .unwrap()
        .iter()
        .find(|(known, _)| *known == path)
        .map(|(_, body)| body.clone());

    let mut answer = match &found {
        Some(body) => format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes(),
        None => {
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 9\r\nConnection: close\r\n\r\nnot found"
                .to_vec()
        }
    };
    if let Some(body) = found {
        answer.extend_from_slice(&body);
    }
    let _ = stream.write_all(&answer);
    let _ = stream.flush();
}
