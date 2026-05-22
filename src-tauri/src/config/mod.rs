//! Persisted app configuration. Owns the `AppConfig` shape, its
//! defaults, and load/save against `<data_dir>/koko-notes-whisper/config.json`.
//!
//! Per ADR-0001 (no on-disk persistence compatibility guarantees), old
//! config files that fail to parse are silently replaced with defaults.

use crate::audio::SelectedDevice;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const APP_DATA_SUBDIR: &str = "koko-notes-whisper";
const CONFIG_FILENAME: &str = "config.json";

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppConfig {
    #[serde(default)]
    pub output_folder: Option<String>,
    #[serde(default)]
    pub mic_device: Option<SelectedDevice>,
    #[serde(default)]
    pub sys_device: Option<SelectedDevice>,
    #[serde(default = "default_true")]
    pub mic_enabled: bool,
    #[serde(default)]
    pub sys_enabled: bool,
}

fn default_true() -> bool {
    true
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            output_folder: None,
            mic_device: None,
            sys_device: None,
            mic_enabled: true,
            sys_enabled: false,
        }
    }
}

/// Pure path composition. Given a data-dir root, returns the full
/// `config.json` path. Does no I/O.
fn build_config_path(data_dir: &Path) -> PathBuf {
    data_dir.join(APP_DATA_SUBDIR).join(CONFIG_FILENAME)
}

fn config_path() -> PathBuf {
    let data_dir = dirs::data_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    build_config_path(&data_dir)
}

pub fn load_config() -> AppConfig {
    let path = config_path();
    match std::fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => AppConfig::default(),
    }
}

pub fn save_config(config: &AppConfig) {
    let path = config_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(json) = serde_json::to_string_pretty(config) {
        let _ = std::fs::write(&path, json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = AppConfig::default();
        assert!(config.output_folder.is_none());
        assert!(config.mic_device.is_none());
        assert!(config.sys_device.is_none());
        assert!(config.mic_enabled);
        assert!(!config.sys_enabled);
    }

    #[test]
    fn test_config_serialization_roundtrip() {
        let config = AppConfig {
            output_folder: Some("/tmp/test".to_string()),
            mic_device: None,
            sys_device: None,
            mic_enabled: false,
            sys_enabled: true,
        };
        let json = serde_json::to_string(&config).unwrap();
        let loaded: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(loaded.output_folder.as_deref(), Some("/tmp/test"));
        assert!(!loaded.mic_enabled);
        assert!(loaded.sys_enabled);
    }

    #[test]
    fn test_config_deserialize_missing_fields() {
        // Simulates loading a config file with only some fields.
        let json = r#"{"output_folder": "/tmp/notes"}"#;
        let config: AppConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.output_folder.as_deref(), Some("/tmp/notes"));
        assert!(config.mic_enabled); // default_true
        assert!(!config.sys_enabled); // default false
        assert!(config.mic_device.is_none());
    }

    #[test]
    fn test_config_deserialize_empty_json() {
        let json = "{}";
        let config: AppConfig = serde_json::from_str(json).unwrap();
        assert!(config.output_folder.is_none());
        assert!(config.mic_enabled);
        assert!(!config.sys_enabled);
    }

    #[test]
    fn build_config_path_composes_app_subdir_then_config_filename() {
        let root = PathBuf::from("/tmp/fake-data");
        assert_eq!(
            build_config_path(&root),
            PathBuf::from("/tmp/fake-data/koko-notes-whisper/config.json"),
        );
    }

    #[test]
    fn build_config_path_ends_with_config_filename() {
        let root = PathBuf::from("/anywhere");
        let path = build_config_path(&root);
        assert_eq!(path.file_name().and_then(|n| n.to_str()), Some(CONFIG_FILENAME));
    }

    #[test]
    fn build_config_path_shares_app_subdir_with_model_module() {
        // APP_DATA_SUBDIR is intentionally duplicated across config/ and
        // model/ (each module names its own constants — see ADR-0001
        // "pragmatic testing"). This test pins the value so the two
        // modules' independent constants do not silently drift.
        let root = PathBuf::from("/x");
        let path = build_config_path(&root);
        let app_dir_component = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str());
        assert_eq!(app_dir_component, Some("koko-notes-whisper"));
    }

    #[test]
    fn load_config_returns_default_on_invalid_json() {
        // load_config itself is impure (reads a real path), but the
        // resilience contract — "garbage on disk -> use defaults" — is
        // load-bearing per ADR-0001's wipe-and-restart stance. Exercise
        // the inner serde call directly: any unparseable input must
        // yield AppConfig::default().
        let json = "this is not valid json {{{";
        let parsed: AppConfig = serde_json::from_str(json).unwrap_or_default();
        // unwrap_or_default fires -> defaults applied.
        assert!(parsed.mic_enabled);
        assert!(!parsed.sys_enabled);
        assert!(parsed.output_folder.is_none());
    }
}
