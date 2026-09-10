use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

pub const DWELL_VAR: &str = "HERDR_RECENT_DWELL";
pub const PIN_VAR: &str = "HERDR_RECENT_PIN";

pub const RECENT_KEYS: [(&str, &str); 2] = [("recent.dwell", DWELL_VAR), ("recent.pin", PIN_VAR)];

pub const CONFIG_DIR_VAR: &str = "HERDR_PLUGIN_CONFIG_DIR";
pub const STATE_DIR_VAR: &str = "HERDR_PLUGIN_STATE_DIR";
pub const PLUGIN_ROOT_VAR: &str = "HERDR_PLUGIN_ROOT";

pub const DEFAULT_DWELL_SECONDS: f64 = 10.0;
pub const DEFAULT_PIN_LABEL: &str = "~";
pub const DEFAULT_STATE_DIR: &str = ".config/herdr";

#[derive(Debug, Clone, Default)]
pub struct Environment {
    vars: BTreeMap<String, String>,
}

impl Environment {
    pub fn from_process() -> Environment {
        Environment {
            vars: std::env::vars().collect(),
        }
    }

    pub fn from_pairs(pairs: &[(&str, &str)]) -> Environment {
        Environment {
            vars: pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.vars.get(key).map(String::as_str)
    }

    pub fn home(&self) -> PathBuf {
        PathBuf::from(self.get("HOME").unwrap_or("/"))
    }
}

pub fn state_dir(env: &Environment) -> PathBuf {
    match env.get(STATE_DIR_VAR) {
        Some(named) if !named.is_empty() => PathBuf::from(named),
        _ => env.home().join(DEFAULT_STATE_DIR),
    }
}

pub fn plugin_root(env: &Environment) -> PathBuf {
    if let Some(root) = env.get(PLUGIN_ROOT_VAR).filter(|r| !r.is_empty()) {
        return PathBuf::from(root);
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.ancestors().nth(3).map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn parse_env_file(text: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || !line.contains('=') {
            continue;
        }
        let (key, value) = match line.split_once('=') {
            Some(split) => split,
            None => continue,
        };
        let key = key.trim();
        let mut value = value.trim().to_string();
        let quoted: Vec<char> = value.chars().collect();
        if quoted.len() >= 2
            && quoted[0] == quoted[quoted.len() - 1]
            && (quoted[0] == '"' || quoted[0] == '\'')
        {
            value = quoted[1..quoted.len() - 1].iter().collect();
        }
        if !key.is_empty() {
            pairs.push((key.to_string(), value));
        }
    }
    pairs
}

pub fn read_env_file(path: &Path) -> Vec<(String, String)> {
    match std::fs::read_to_string(path) {
        Ok(text) => parse_env_file(&text),
        Err(_) => Vec::new(),
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct RecentTable {
    pub dwell: Option<toml::Value>,
    pub pin: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RecentFile {
    recent: Option<RecentTable>,
}

#[derive(Debug, Clone, Default)]
pub struct RecentConfig {
    pub table: RecentTable,
    pub unknown: Vec<String>,
    pub error: Option<String>,
}

pub fn parse_recent_config(text: &str) -> RecentConfig {
    let mut unknown = Vec::new();
    let deserializer = toml::Deserializer::new(text);
    let parsed: Result<RecentFile, toml::de::Error> =
        serde_ignored::deserialize(deserializer, |path| {
            unknown.push(path.to_string().replace("?.", ""))
        });
    match parsed {
        Ok(file) => RecentConfig {
            table: file.recent.unwrap_or_default(),
            unknown,
            error: None,
        },
        Err(e) => RecentConfig {
            table: RecentTable::default(),
            unknown: Vec::new(),
            error: Some(first_line(&e.to_string())),
        },
    }
}

fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or("").trim().to_string()
}

pub fn read_recent_config(path: &Path) -> RecentConfig {
    match std::fs::read_to_string(path) {
        Ok(text) => parse_recent_config(&text),
        Err(_) => RecentConfig::default(),
    }
}

pub struct Sources {
    pub config: RecentConfig,
    pub env_file: Vec<(String, String)>,
    pub defaults: RecentConfig,
}

pub fn read_sources(env: &Environment, own_root: &Path) -> Sources {
    let dir = env
        .get(CONFIG_DIR_VAR)
        .filter(|d| !d.is_empty())
        .map(PathBuf::from);
    Sources {
        config: dir
            .as_ref()
            .map(|d| read_recent_config(&d.join("config.toml")))
            .unwrap_or_default(),
        env_file: dir
            .as_ref()
            .map(|d| read_env_file(&d.join(".env")))
            .unwrap_or_default(),
        defaults: read_recent_config(&own_root.join("defaults.toml")),
    }
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub dwell: f64,
    pub pin: String,
    pub complaints: Vec<String>,
}

enum Named {
    Text(String),
    Toml(toml::Value),
}

fn seconds_of(value: &Named) -> Option<f64> {
    match value {
        Named::Text(text) => text.trim().parse::<f64>().ok().filter(|n| n.is_finite()),
        Named::Toml(toml::Value::Float(n)) => Some(*n).filter(|n| n.is_finite()),
        Named::Toml(toml::Value::Integer(n)) => Some(*n as f64),
        Named::Toml(toml::Value::String(text)) => text.trim().parse::<f64>().ok(),
        Named::Toml(_) => None,
    }
}

fn shown(value: &Named) -> String {
    match value {
        Named::Text(text) => text.clone(),
        Named::Toml(value) => value.to_string(),
    }
}

fn from_env_file(sources: &Sources, name: &str) -> Option<String> {
    sources
        .env_file
        .iter()
        .find(|(k, _)| k == name)
        .map(|(_, v)| v.clone())
}

fn layered(
    env: &Environment,
    sources: &Sources,
    var: &str,
    pick: fn(&RecentTable) -> Option<Named>,
) -> Option<(&'static str, Named)> {
    if let Some(value) = env.get(var) {
        return Some(("the environment", Named::Text(value.to_string())));
    }
    if let Some(value) = pick(&sources.config.table) {
        return Some(("config.toml", value));
    }
    if let Some(value) = from_env_file(sources, var) {
        return Some((".env", Named::Text(value)));
    }
    pick(&sources.defaults.table).map(|value| ("defaults.toml", value))
}

pub fn resolve_settings(env: &Environment, sources: &Sources) -> Settings {
    let mut complaints = Vec::new();
    for (which, config) in [
        ("config.toml", &sources.config),
        ("defaults.toml", &sources.defaults),
    ] {
        if let Some(error) = &config.error {
            complaints.push(format!(
                "{} does not parse, so none of it applies: {}",
                which, error
            ));
        }
        for key in &config.unknown {
            complaints.push(format!(
                "{} names {}, which this plugin has no setting for",
                which, key
            ));
        }
    }

    let dwell = match layered(env, sources, DWELL_VAR, |t| {
        t.dwell.clone().map(Named::Toml)
    }) {
        Some((which, value)) => seconds_of(&value).unwrap_or_else(|| {
            complaints.push(format!(
                "{} gives the dwell as {}, which is not a number of seconds, so {} is used",
                which,
                shown(&value),
                DEFAULT_DWELL_SECONDS
            ));
            DEFAULT_DWELL_SECONDS
        }),
        None => DEFAULT_DWELL_SECONDS,
    };

    let pin = match layered(env, sources, PIN_VAR, |t| t.pin.clone().map(Named::Text)) {
        Some((_, Named::Text(label))) => label,
        Some((_, value)) => shown(&value),
        None => DEFAULT_PIN_LABEL.to_string(),
    };

    Settings {
        dwell,
        pin,
        complaints,
    }
}
