mod support;

use std::path::Path;

use recent_spaces::api::socket_path;
use recent_spaces::config::{
    parse_env_file, parse_recent_config, read_sources, resolve_settings, state_dir, Settings,
    DEFAULT_DWELL_SECONDS, DEFAULT_PIN_LABEL, DWELL_VAR, PIN_VAR,
};
use support::*;

fn settled(dir: &Path, own_root: &Path, pairs: &[(&str, &str)]) -> Settings {
    let mut all: Vec<(&str, &str)> = vec![("HERDR_PLUGIN_CONFIG_DIR", dir.to_str().unwrap())];
    all.extend(pairs.iter().copied());
    let env = env_for(&all);
    resolve_settings(&env, &read_sources(&env, own_root))
}

fn shipped(dir: &Path, pairs: &[(&str, &str)]) -> Settings {
    settled(dir, &manifest_dir(), pairs)
}

#[test]
fn an_empty_config_dir_leaves_the_settings_this_plugin_ships() {
    let dir = TempDir::new();
    let settings = shipped(dir.path(), &[]);
    assert_eq!(settings.dwell, 10.0);
    assert_eq!(settings.pin, "~");
    assert_eq!(settings.complaints, Vec::<String>::new());
}

#[test]
fn no_config_dir_at_all_leaves_the_settings_this_plugin_ships() {
    let env = env_for(&[]);
    let settings = resolve_settings(&env, &read_sources(&env, &manifest_dir()));
    assert_eq!(settings.dwell, 10.0);
    assert_eq!(settings.pin, "~");
}

#[test]
fn the_shipped_defaults_are_what_the_code_falls_back_to_without_them() {
    let dir = TempDir::new();
    let nothing = TempDir::new();
    let shipped_settings = shipped(dir.path(), &[]);
    let bare = settled(dir.path(), nothing.path(), &[]);
    assert_eq!(shipped_settings.dwell, bare.dwell);
    assert_eq!(shipped_settings.pin, bare.pin);
    assert_eq!(bare.dwell, DEFAULT_DWELL_SECONDS);
    assert_eq!(bare.pin, DEFAULT_PIN_LABEL);
}

#[test]
fn the_shipped_defaults_file_is_read_rather_than_assumed() {
    let own_root = TempDir::new();
    own_root.write("defaults.toml", "[recent]\ndwell = 42\npin = \"shipped\"\n");
    let dir = TempDir::new();
    let settings = settled(dir.path(), own_root.path(), &[]);
    assert_eq!(settings.dwell, 42.0);
    assert_eq!(settings.pin, "shipped");
}

#[test]
fn a_setting_in_either_file_outranks_the_shipped_default() {
    let own_root = TempDir::new();
    own_root.write("defaults.toml", "[recent]\ndwell = 42\npin = \"shipped\"\n");
    let dir = TempDir::new();
    dir.write("config.toml", "[recent]\ndwell = 3\n");
    dir.write(".env", "HERDR_RECENT_PIN=mine\n");
    let settings = settled(dir.path(), own_root.path(), &[]);
    assert_eq!(settings.dwell, 3.0);
    assert_eq!(settings.pin, "mine");
}

#[test]
fn config_toml_supplies_settings() {
    let dir = TempDir::new();
    dir.write("config.toml", "[recent]\ndwell = 2.5\npin = \"home\"\n");
    let settings = shipped(dir.path(), &[]);
    assert_eq!(settings.dwell, 2.5);
    assert_eq!(settings.pin, "home");
}

#[test]
fn an_env_file_still_supplies_settings() {
    let dir = TempDir::new();
    dir.write(".env", "HERDR_RECENT_DWELL=2.5\nHERDR_RECENT_PIN=home\n");
    let settings = shipped(dir.path(), &[]);
    assert_eq!(settings.dwell, 2.5);
    assert_eq!(settings.pin, "home");
}

#[test]
fn config_toml_wins_over_an_env_file_left_behind() {
    let dir = TempDir::new();
    dir.write("config.toml", "[recent]\ndwell = 3\npin = \"toml\"\n");
    dir.write(".env", "HERDR_RECENT_DWELL=99\nHERDR_RECENT_PIN=env\n");
    let settings = shipped(dir.path(), &[]);
    assert_eq!(settings.dwell, 3.0);
    assert_eq!(settings.pin, "toml");
}

#[test]
fn a_real_environment_variable_wins_over_both_files() {
    let dir = TempDir::new();
    dir.write("config.toml", "[recent]\ndwell = 3\npin = \"toml\"\n");
    dir.write(".env", "HERDR_RECENT_DWELL=99\nHERDR_RECENT_PIN=env\n");
    let settings = shipped(dir.path(), &[(DWELL_VAR, "7"), (PIN_VAR, "shell")]);
    assert_eq!(settings.dwell, 7.0);
    assert_eq!(settings.pin, "shell");
}

#[test]
fn each_setting_is_taken_from_the_file_that_names_it() {
    let dir = TempDir::new();
    dir.write("config.toml", "[recent]\ndwell = 4\n");
    dir.write(".env", "HERDR_RECENT_PIN=from-env-file\n");
    let settings = shipped(dir.path(), &[]);
    assert_eq!(settings.dwell, 4.0);
    assert_eq!(settings.pin, "from-env-file");
}

#[test]
fn a_dwell_may_be_written_as_an_integer_a_float_or_a_quoted_number() {
    for written in ["dwell = 4", "dwell = 4.0", "dwell = \"4\""] {
        let dir = TempDir::new();
        dir.write("config.toml", &format!("[recent]\n{}\n", written));
        assert_eq!(shipped(dir.path(), &[]).dwell, 4.0, "{}", written);
    }
}

#[test]
fn a_dwell_that_is_not_a_number_of_seconds_falls_back_and_says_so() {
    let dir = TempDir::new();
    dir.write(".env", "HERDR_RECENT_DWELL=ten\nHERDR_RECENT_PIN=home\n");
    let settings = shipped(dir.path(), &[]);
    assert_eq!(settings.dwell, DEFAULT_DWELL_SECONDS);
    assert_eq!(settings.pin, "home", "one bad value does not lose the rest");
    assert_eq!(settings.complaints.len(), 1, "{:?}", settings.complaints);
    assert!(
        settings.complaints[0].contains("ten"),
        "{:?}",
        settings.complaints
    );
}

#[test]
fn a_dwell_written_as_a_table_falls_back_rather_than_stopping_the_watcher() {
    let dir = TempDir::new();
    dir.write("config.toml", "[recent.dwell]\nseconds = 4\n");
    let settings = shipped(dir.path(), &[]);
    assert_eq!(settings.dwell, DEFAULT_DWELL_SECONDS);
    assert!(!settings.complaints.is_empty());
}

#[test]
fn an_empty_dwell_in_the_environment_falls_back_to_the_default() {
    let dir = TempDir::new();
    let settings = shipped(dir.path(), &[(DWELL_VAR, "")]);
    assert_eq!(settings.dwell, DEFAULT_DWELL_SECONDS);
    assert!(!settings.complaints.is_empty());
}

#[test]
fn a_config_toml_that_does_not_parse_contributes_nothing_and_says_so() {
    let dir = TempDir::new();
    dir.write("config.toml", "[recent]\ndwell = 3\npin = \"unterminated\n");
    let settings = shipped(dir.path(), &[]);
    assert_eq!(settings.dwell, 10.0, "the shipped default, not the 3 above");
    assert_eq!(settings.pin, "~");
    assert!(
        settings
            .complaints
            .iter()
            .any(|c| c.contains("config.toml does not parse")),
        "{:?}",
        settings.complaints
    );
}

#[test]
fn a_key_this_plugin_has_no_setting_for_is_named_on_stderr() {
    let dir = TempDir::new();
    dir.write("config.toml", "[recent]\ndwell = 3\ndwel = 4\n");
    let settings = shipped(dir.path(), &[]);
    assert_eq!(settings.dwell, 3.0, "the typo does not lose the rest");
    assert!(
        settings
            .complaints
            .iter()
            .any(|c| c.contains("names recent.dwel,")),
        "{:?}",
        settings.complaints
    );
}

#[test]
fn an_env_file_keeps_the_parsing_the_python_watcher_had() {
    let parsed = parse_env_file(
        "# dwell in seconds\n\n  HERDR_RECENT_DWELL = '3'  \nHERDR_RECENT_PIN=\"my space\"\n#HERDR_RECENT_PIN=commented-out\nnot-a-setting\n=novalue\n",
    );
    assert_eq!(
        parsed,
        vec![
            ("HERDR_RECENT_DWELL".to_string(), "3".to_string()),
            ("HERDR_RECENT_PIN".to_string(), "my space".to_string()),
        ]
    );
}

#[test]
fn an_unmatched_quote_in_an_env_file_is_kept_verbatim() {
    let parsed = parse_env_file("HERDR_RECENT_PIN=\"half\n");
    assert_eq!(
        parsed,
        vec![("HERDR_RECENT_PIN".to_string(), "\"half".to_string())]
    );
}

#[test]
fn a_malformed_env_file_line_is_skipped_and_the_rest_still_loads() {
    let dir = TempDir::new();
    dir.write(".env", "HERDR_RECENT_DWELL 5\nHERDR_RECENT_PIN=home\n");
    let settings = shipped(dir.path(), &[]);
    assert_eq!(settings.dwell, 10.0);
    assert_eq!(settings.pin, "home");
}

#[test]
fn an_env_file_does_not_reach_any_other_setting() {
    let dir = TempDir::new();
    dir.write(".env", "HERDR_RECENT_PIN=home\nSOMETHING_ELSE=1\n");
    let settings = shipped(dir.path(), &[]);
    assert_eq!(settings.pin, "home");
    assert_eq!(settings.complaints, Vec::<String>::new());
}

#[test]
fn the_shipped_defaults_name_both_settings() {
    let parsed = parse_recent_config(&read_repo_file("defaults.toml"));
    assert_eq!(parsed.error, None);
    assert_eq!(parsed.unknown, Vec::<String>::new());
    assert!(parsed.table.dwell.is_some());
    assert_eq!(parsed.table.pin.as_deref(), Some("~"));
}

#[test]
fn the_socket_path_comes_from_herdr_socket_path() {
    let env = env_for(&[("HERDR_SOCKET_PATH", "/run/herdr/custom.sock")]);
    assert_eq!(socket_path(&env), Path::new("/run/herdr/custom.sock"));
}

#[test]
fn an_unset_or_empty_socket_path_falls_back_to_the_config_root() {
    for unset in [None, Some("")] {
        let env = match unset {
            Some(value) => env_for(&[("HERDR_SOCKET_PATH", value)]),
            None => env_for(&[]),
        };
        assert_eq!(
            socket_path(&env),
            Path::new("/private/tmp/.config/herdr/herdr.sock"),
            "{:?}",
            unset
        );
    }
}

#[test]
fn the_state_dir_comes_from_herdr_plugin_state_dir() {
    let env = env_for(&[("HERDR_PLUGIN_STATE_DIR", "/run/state")]);
    assert_eq!(state_dir(&env), Path::new("/run/state"));
}

#[test]
fn an_unset_or_empty_state_dir_falls_back_to_the_config_root() {
    for unset in [None, Some("")] {
        let env = match unset {
            Some(value) => env_for(&[("HERDR_PLUGIN_STATE_DIR", value)]),
            None => env_for(&[]),
        };
        assert_eq!(
            state_dir(&env),
            Path::new("/private/tmp/.config/herdr"),
            "{:?}",
            unset
        );
    }
}
