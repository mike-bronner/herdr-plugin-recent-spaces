use std::path::{Path, PathBuf};

use serde_json::{json, Value};

pub const CLAIM_PREFIX: &str = "recent-spaces-watcher-";

pub trait Claim {
    fn write(&self, token: &str) -> Result<(), String>;
    fn read(&self) -> Option<String>;
}

pub fn key_for(socket: &Path) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in socket.to_string_lossy().as_bytes() {
        hash ^= *byte as u64;
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    format!("{:016x}", hash)
}

pub fn claim_path(state_dir: &Path, socket: &Path) -> PathBuf {
    state_dir.join(format!("{}{}.json", CLAIM_PREFIX, key_for(socket)))
}

pub struct FileClaim {
    path: PathBuf,
    socket: PathBuf,
}

impl FileClaim {
    pub fn new(state_dir: &Path, socket: &Path) -> FileClaim {
        FileClaim {
            path: claim_path(state_dir, socket),
            socket: socket.to_path_buf(),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Claim for FileClaim {
    fn write(&self, token: &str) -> Result<(), String> {
        let Some(dir) = self.path.parent() else {
            return Err(format!(
                "{} has no directory to write into",
                self.path.display()
            ));
        };
        std::fs::create_dir_all(dir)
            .map_err(|e| format!("cannot make {}: {}", dir.display(), e))?;
        let staged = self.path.with_extension("json.tmp");
        let body = json!({"token": token,
                          "pid": std::process::id(),
                          "socket": self.socket.to_string_lossy()});
        std::fs::write(&staged, format!("{}\n", body))
            .map_err(|e| format!("cannot write {}: {}", staged.display(), e))?;
        std::fs::rename(&staged, &self.path)
            .map_err(|e| format!("cannot place {}: {}", self.path.display(), e))
    }

    fn read(&self) -> Option<String> {
        let text = std::fs::read_to_string(&self.path).ok()?;
        let parsed: Value = serde_json::from_str(&text).ok()?;
        parsed.get("token")?.as_str().map(|t| t.to_string())
    }
}
