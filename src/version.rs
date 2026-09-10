use std::path::{Path, PathBuf};

use crate::config::{Environment, PLUGIN_ROOT_VAR};

pub const FLAG: &str = "--version";

pub const CRATE_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const COMMIT: &str = env!("RECENT_SPACES_COMMIT");

pub const BUILT: &str = env!("RECENT_SPACES_BUILT");

pub const UNKNOWN_COMMIT: &str = "unknown";

pub const MANIFEST_FILE: &str = "herdr-plugin.toml";

#[derive(Debug, Clone, PartialEq)]
pub enum Manifest {
    Found { version: String, path: PathBuf },
    Unreadable(PathBuf),
    Unparsed(PathBuf),
    NoRoot,
}

pub fn root_of(env: &Environment) -> Option<PathBuf> {
    if let Some(named) = env.get(PLUGIN_ROOT_VAR).filter(|r| !r.is_empty()) {
        return Some(PathBuf::from(named));
    }
    let exe = std::env::current_exe().ok()?;
    let root = exe.ancestors().nth(3)?;
    match root.join(MANIFEST_FILE).is_file() {
        true => Some(root.to_path_buf()),
        false => None,
    }
}

pub fn read_manifest(root: Option<&Path>) -> Manifest {
    let Some(root) = root else {
        return Manifest::NoRoot;
    };
    let path = root.join(MANIFEST_FILE);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return Manifest::Unreadable(path);
    };
    match text.parse::<toml::Table>() {
        Ok(table) => match table.get("version").and_then(toml::Value::as_str) {
            Some(version) => Manifest::Found {
                version: version.to_string(),
                path,
            },
            None => Manifest::Unparsed(path),
        },
        Err(_) => Manifest::Unparsed(path),
    }
}

fn manifest_line(manifest: &Manifest) -> String {
    match manifest {
        Manifest::Found { version, path } => {
            format!("manifest {} at {}", version, path.display())
        }
        Manifest::Unreadable(path) => format!("manifest unreadable at {}", path.display()),
        Manifest::Unparsed(path) => format!("manifest unparsed at {}", path.display()),
        Manifest::NoRoot => {
            format!(
                "manifest not found: set {} to the plugin checkout to read it",
                PLUGIN_ROOT_VAR
            )
        }
    }
}

fn stale_line(manifest: &Manifest) -> Option<String> {
    let Manifest::Found { version, .. } = manifest else {
        return None;
    };
    if version == CRATE_VERSION {
        return None;
    }
    Some(format!(
        "STALE: this binary is {} but the manifest is {}. Rebuild it with `cargo build --release`.",
        CRATE_VERSION, version
    ))
}

pub fn report(name: &str, manifest: &Manifest) -> String {
    let mut lines = vec![
        format!("{} {} ({}, built {})", name, CRATE_VERSION, COMMIT, BUILT),
        manifest_line(manifest),
    ];
    if let Some(stale) = stale_line(manifest) {
        lines.push(stale);
    }
    lines.push(String::new());
    lines.join("\n")
}
