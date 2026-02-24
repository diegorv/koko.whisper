use crate::audio::SelectedDevice;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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

fn config_path() -> PathBuf {
    let app_support = dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("koko-notes-whisper");
    app_support.join("config.json")
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
        // Simulates loading a config file with only some fields
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
}
