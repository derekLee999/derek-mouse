use std::{fs, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{clicker::ClickerConfig, input::HotkeyConfig};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    #[serde(default)]
    pub clicker: ClickerConfig,
    #[serde(default = "default_record_hotkey")]
    pub record_hotkey: HotkeyConfig,
    #[serde(default = "default_true")]
    pub show_window_on_stop: bool,
    #[serde(default = "default_true")]
    pub auto_hide_on_hotkey: bool,
}

fn default_record_hotkey() -> HotkeyConfig {
    HotkeyConfig {
        ctrl: false,
        alt: false,
        key: "F9".to_string(),
    }
}

fn default_true() -> bool {
    true
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            clicker: ClickerConfig::default(),
            record_hotkey: default_record_hotkey(),
            show_window_on_stop: true,
            auto_hide_on_hotkey: true,
        }
    }
}

fn config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".derek-mouse").join("config.json"))
}

pub fn load_config() -> AppConfig {
    let Some(path) = config_path() else {
        return AppConfig::default();
    };

    let data = match fs::read_to_string(&path) {
        Ok(data) => data,
        Err(_) => return AppConfig::default(),
    };

    serde_json::from_str(&data).unwrap_or_default()
}

pub fn save_config(config: &AppConfig) {
    let Some(path) = config_path() else {
        return;
    };

    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }

    if let Ok(data) = serde_json::to_string(config) {
        let _ = fs::write(&path, data);
    }
}
