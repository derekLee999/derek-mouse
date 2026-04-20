use rdev::{Button, Key};
use serde::{Deserialize, Serialize};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ActiveFeature {
    Clicker,
    Recorder,
}

impl ActiveFeature {
    pub fn from_name(name: &str) -> Self {
        match name {
            "recorder" => Self::Recorder,
            _ => Self::Clicker,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyConfig {
    pub ctrl: bool,
    pub alt: bool,
    pub key: String,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            ctrl: false,
            alt: false,
            key: "F8".to_string(),
        }
    }
}

#[derive(Default)]
pub struct KeyboardTracker {
    ctrl_down: bool,
    alt_down: bool,
    hotkey_down: bool,
}

impl KeyboardTracker {
    pub fn handle_press(&mut self, key: Key, hotkey: &HotkeyConfig) -> bool {
        self.update_modifier(key, true);

        let Some(hotkey_key) = key_from_name(&hotkey.key) else {
            return false;
        };

        if key != hotkey_key || self.hotkey_down {
            return false;
        }

        let ctrl_matches = !hotkey.ctrl || self.ctrl_down;
        let alt_matches = !hotkey.alt || self.alt_down;

        if ctrl_matches && alt_matches {
            self.hotkey_down = true;
            return true;
        }

        false
    }

    pub fn handle_release(&mut self, key: Key, hotkey: &HotkeyConfig) {
        self.update_modifier(key, false);

        if let Some(hotkey_key) = key_from_name(&hotkey.key) {
            if key == hotkey_key {
                self.hotkey_down = false;
            }
        }
    }

    pub fn clear_hotkey_down(&mut self) {
        self.hotkey_down = false;
    }

    fn update_modifier(&mut self, key: Key, pressed: bool) {
        match key {
            Key::ControlLeft | Key::ControlRight => self.ctrl_down = pressed,
            Key::Alt | Key::AltGr => self.alt_down = pressed,
            _ => {}
        }
    }
}

pub fn validate_hotkey(hotkey: &mut HotkeyConfig) -> Result<(), String> {
    hotkey.key = hotkey.key.trim().to_uppercase();

    if key_from_name(&hotkey.key).is_none() {
        return Err("Unsupported keyboard hotkey".to_string());
    }

    if key_needs_modifier(&hotkey.key) {
        if !hotkey.ctrl && !hotkey.alt {
            return Err("Letters and numbers must be combined with Ctrl or Alt".to_string());
        }
    } else if hotkey.ctrl || hotkey.alt {
        return Err(
            "Function keys, Space, Enter, and Esc cannot be combined with Ctrl or Alt".to_string(),
        );
    }

    Ok(())
}

pub fn button_from_name(name: &str) -> Option<Button> {
    match name.to_lowercase().as_str() {
        "left" => Some(Button::Left),
        "middle" => Some(Button::Middle),
        "right" => Some(Button::Right),
        _ => None,
    }
}

pub fn key_from_name(name: &str) -> Option<Key> {
    match name.trim().to_uppercase().as_str() {
        "A" => Some(Key::KeyA),
        "B" => Some(Key::KeyB),
        "C" => Some(Key::KeyC),
        "D" => Some(Key::KeyD),
        "E" => Some(Key::KeyE),
        "F" => Some(Key::KeyF),
        "G" => Some(Key::KeyG),
        "H" => Some(Key::KeyH),
        "I" => Some(Key::KeyI),
        "J" => Some(Key::KeyJ),
        "K" => Some(Key::KeyK),
        "L" => Some(Key::KeyL),
        "M" => Some(Key::KeyM),
        "N" => Some(Key::KeyN),
        "O" => Some(Key::KeyO),
        "P" => Some(Key::KeyP),
        "Q" => Some(Key::KeyQ),
        "R" => Some(Key::KeyR),
        "S" => Some(Key::KeyS),
        "T" => Some(Key::KeyT),
        "U" => Some(Key::KeyU),
        "V" => Some(Key::KeyV),
        "W" => Some(Key::KeyW),
        "X" => Some(Key::KeyX),
        "Y" => Some(Key::KeyY),
        "Z" => Some(Key::KeyZ),
        "0" => Some(Key::Num0),
        "1" => Some(Key::Num1),
        "2" => Some(Key::Num2),
        "3" => Some(Key::Num3),
        "4" => Some(Key::Num4),
        "5" => Some(Key::Num5),
        "6" => Some(Key::Num6),
        "7" => Some(Key::Num7),
        "8" => Some(Key::Num8),
        "9" => Some(Key::Num9),
        "F1" => Some(Key::F1),
        "F2" => Some(Key::F2),
        "F3" => Some(Key::F3),
        "F4" => Some(Key::F4),
        "F5" => Some(Key::F5),
        "F6" => Some(Key::F6),
        "F7" => Some(Key::F7),
        "F8" => Some(Key::F8),
        "F9" => Some(Key::F9),
        "F10" => Some(Key::F10),
        "F11" => Some(Key::F11),
        "F12" => Some(Key::F12),
        "SPACE" => Some(Key::Space),
        "ENTER" => Some(Key::Return),
        "ESC" | "ESCAPE" => Some(Key::Escape),
        _ => None,
    }
}

pub fn key_needs_modifier(name: &str) -> bool {
    let normalized = name.trim().to_uppercase();
    normalized.len() == 1 && normalized.chars().all(|ch| ch.is_ascii_alphanumeric())
}
