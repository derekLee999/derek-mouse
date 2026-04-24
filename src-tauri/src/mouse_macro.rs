use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

use crate::{
    input::{
        button_from_name, key_from_name, simulate, Button, Event, EventType, HotkeyConfig, Key,
        KeyboardTracker,
    },
    tray::{self, TrayStatus},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct MacroScheme {
    id: u64,
    name: String,
    created_at: u64,
    #[serde(default)]
    updated_at: u64,
    #[serde(default = "default_playback_speed")]
    playback_speed: f64,
    #[serde(default)]
    loop_playback: bool,
    events: Vec<MacroEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum MacroEvent {
    MouseClick { button: String },
    MouseDoubleClick { button: String },
    MouseDown { button: String },
    MouseUp { button: String },
    MouseMove { x: u32, y: u32 },
    KeyClick { key: String },
    KeyDown { key: String },
    KeyUp { key: String },
    Delay { ms: u64 },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacroSummary {
    id: u64,
    name: String,
    playback_speed: f64,
    loop_playback: bool,
    created_at: u64,
    updated_at: u64,
    event_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacroState {
    macros: Vec<MacroSummary>,
    selected_id: Option<u64>,
    playing: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateMacroRequest {
    name: String,
    events: Vec<MacroEvent>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMacroPlaybackSpeedRequest {
    id: u64,
    speed: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMacroLoopPlaybackRequest {
    id: u64,
    value: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MacroDetail {
    pub id: u64,
    pub name: String,
    pub playback_speed: f64,
    pub loop_playback: bool,
    pub created_at: u64,
    pub updated_at: u64,
    pub events: Vec<MacroEvent>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateMacroRequest {
    pub id: u64,
    pub name: String,
    pub events: Vec<MacroEvent>,
}

pub struct MouseMacroRuntime {
    macros: Mutex<Vec<MacroScheme>>,
    selected_id: Mutex<Option<u64>>,
    keyboard: Mutex<KeyboardTracker>,
    playing: Arc<AtomicBool>,
    injecting_event: Arc<AtomicBool>,
    suppress_playback_hotkey_release: AtomicBool,
    next_id: Mutex<u64>,
}

impl MouseMacroRuntime {
    pub fn new() -> Self {
        let (macros, next_id) = load_macros();
        let first_id = macros.first().map(|item| item.id);
        Self {
            macros: Mutex::new(macros),
            selected_id: Mutex::new(first_id),
            keyboard: Mutex::new(KeyboardTracker::default()),
            playing: Arc::new(AtomicBool::new(false)),
            injecting_event: Arc::new(AtomicBool::new(false)),
            suppress_playback_hotkey_release: AtomicBool::new(false),
            next_id: Mutex::new(next_id),
        }
    }

    pub fn state(&self) -> Result<MacroState, String> {
        self.make_state()
    }

    pub fn is_playing(&self) -> bool {
        self.playing.load(Ordering::SeqCst)
    }

    pub fn create_macro(&self, request: CreateMacroRequest) -> Result<MacroState, String> {
        if self.playing.load(Ordering::SeqCst) {
            return Err("Macro playback is running".to_string());
        }

        let name = normalize_name(&request.name)?;
        if request.events.is_empty() {
            return Err("Macro event list cannot be empty".to_string());
        }

        let events = request
            .events
            .into_iter()
            .map(validate_macro_event)
            .collect::<Result<Vec<_>, _>>()?;

        let now_ms = unix_ms();
        let id = self.take_next_id()?;
        let scheme = MacroScheme {
            id,
            name,
            created_at: now_ms,
            updated_at: now_ms,
            playback_speed: 1.0,
            loop_playback: false,
            events,
        };

        self.macros
            .lock()
            .map_err(|err| err.to_string())?
            .insert(0, scheme);
        *self.selected_id.lock().map_err(|err| err.to_string())? = Some(id);
        self.persist();
        self.make_state()
    }

    pub fn select_macro(&self, id: u64) -> Result<MacroState, String> {
        let exists = self
            .macros
            .lock()
            .map_err(|err| err.to_string())?
            .iter()
            .any(|item| item.id == id);

        if !exists {
            return Err("Macro not found".to_string());
        }

        *self.selected_id.lock().map_err(|err| err.to_string())? = Some(id);
        self.make_state()
    }

    pub fn delete_macro(&self, id: u64) -> Result<MacroState, String> {
        if self.playing.load(Ordering::SeqCst) {
            return Err("Cannot delete macro while playback is running".to_string());
        }

        let mut macros = self.macros.lock().map_err(|err| err.to_string())?;
        macros.retain(|item| item.id != id);
        let fallback_id = macros.first().map(|item| item.id);
        drop(macros);

        let mut selected_id = self.selected_id.lock().map_err(|err| err.to_string())?;
        if *selected_id == Some(id) {
            *selected_id = fallback_id;
        }
        drop(selected_id);

        self.persist();
        self.make_state()
    }

    pub fn update_macro_playback_speed(
        &self,
        request: UpdateMacroPlaybackSpeedRequest,
    ) -> Result<MacroState, String> {
        if self.playing.load(Ordering::SeqCst) {
            return Err("Cannot change playback speed while playback is running".to_string());
        }

        let speed = normalize_playback_speed(request.speed)?;
        let mut macros = self.macros.lock().map_err(|err| err.to_string())?;
        let Some(scheme) = macros.iter_mut().find(|item| item.id == request.id) else {
            return Err("Macro not found".to_string());
        };
        scheme.playback_speed = speed;
        scheme.updated_at = unix_ms();
        drop(macros);

        self.persist();
        self.make_state()
    }

    pub fn update_macro_loop_playback(
        &self,
        request: UpdateMacroLoopPlaybackRequest,
    ) -> Result<MacroState, String> {
        if self.playing.load(Ordering::SeqCst) {
            return Err("Cannot change loop mode while playback is running".to_string());
        }

        let mut macros = self.macros.lock().map_err(|err| err.to_string())?;
        let Some(scheme) = macros.iter_mut().find(|item| item.id == request.id) else {
            return Err("Macro not found".to_string());
        };
        scheme.loop_playback = request.value;
        scheme.updated_at = unix_ms();
        drop(macros);

        self.persist();
        self.make_state()
    }

    pub fn play_macro(
        &self,
        id: u64,
        app: tauri::AppHandle,
        show_window_on_stop: bool,
    ) -> Result<MacroState, String> {
        if self.playing.swap(true, Ordering::SeqCst) {
            return Err("Macro playback is already running".to_string());
        }
        tray::set_status(TrayStatus::Running);

        let scheme = self
            .macros
            .lock()
            .map_err(|err| err.to_string())?
            .iter()
            .find(|item| item.id == id)
            .cloned()
            .ok_or_else(|| {
                self.playing.store(false, Ordering::SeqCst);
                tray::set_status(TrayStatus::Stopped);
                "Macro not found".to_string()
            })?;

        *self.selected_id.lock().map_err(|err| err.to_string())? = Some(id);

        let playback_flag = self.playing.clone();
        let injecting_flag = self.injecting_event.clone();

        thread::spawn(move || {
            let speed = scheme.playback_speed.max(0.1);
            loop {
                for event in &scheme.events {
                    if !playback_flag.load(Ordering::SeqCst) {
                        break;
                    }
                    injecting_flag.store(true, Ordering::SeqCst);
                    let _ = play_macro_event(event, speed);
                    injecting_flag.store(false, Ordering::SeqCst);
                }

                if !scheme.loop_playback || !playback_flag.load(Ordering::SeqCst) {
                    break;
                }
            }

            playback_flag.store(false, Ordering::SeqCst);
            tray::set_status(TrayStatus::Stopped);
            if show_window_on_stop {
                show_main_window(&app);
            }
            let _ = app.emit("mouse-macro-status", false);
        });

        self.make_state()
    }

    pub fn stop_playback(&self) -> Result<MacroState, String> {
        self.playing.store(false, Ordering::SeqCst);
        tray::set_status(TrayStatus::Stopped);
        self.make_state()
    }

    pub fn toggle_selected_playback(
        &self,
        app: tauri::AppHandle,
        show_window_on_stop: bool,
    ) -> Result<MacroState, String> {
        if self.playing.load(Ordering::SeqCst) {
            return self.stop_playback();
        }

        let Some(id) = *self.selected_id.lock().map_err(|err| err.to_string())? else {
            return Err("No macro selected".to_string());
        };

        self.play_macro(id, app, show_window_on_stop)
    }

    pub fn handle_event(
        &self,
        event: &Event,
        app: &tauri::AppHandle,
        playback_hotkey: &HotkeyConfig,
        active: bool,
        show_window_on_playback_stop: bool,
        auto_hide_on_hotkey: bool,
    ) {
        if self.injecting_event.load(Ordering::SeqCst) {
            return;
        }

        match event.event_type {
            EventType::KeyPress(key) => {
                let triggered = self
                    .keyboard
                    .lock()
                    .map(|mut keyboard| keyboard.handle_press(key, playback_hotkey))
                    .unwrap_or(false);

                if triggered && active {
                    self.suppress_playback_hotkey_release
                        .store(true, Ordering::SeqCst);
                    if let Ok(mut keyboard) = self.keyboard.lock() {
                        keyboard.clear_hotkey_down();
                    }
                    let was_playing = self.playing.load(Ordering::SeqCst);
                    if auto_hide_on_hotkey {
                        hide_main_window(app);
                    }
                    if let Ok(state) =
                        self.toggle_selected_playback(app.clone(), show_window_on_playback_stop)
                    {
                        if was_playing && show_window_on_playback_stop {
                            show_main_window(app);
                        }
                        let _ = app.emit("mouse-macro-state", state);
                    }
                }
            }
            EventType::KeyRelease(key) => {
                if let Ok(mut keyboard) = self.keyboard.lock() {
                    keyboard.handle_release(key, playback_hotkey);
                }

                if key_from_name(&playback_hotkey.key) == Some(key)
                    && self
                        .suppress_playback_hotkey_release
                        .swap(false, Ordering::SeqCst)
                {
                    return;
                }
            }
            _ => {}
        }
    }

    pub fn macro_detail(&self, id: u64) -> Result<MacroDetail, String> {
        let macros = self.macros.lock().map_err(|err| err.to_string())?;
        let scheme = macros
            .iter()
            .find(|item| item.id == id)
            .cloned()
            .ok_or_else(|| "Macro not found".to_string())?;
        Ok(MacroDetail {
            id: scheme.id,
            name: scheme.name.clone(),
            playback_speed: scheme.playback_speed,
            loop_playback: scheme.loop_playback,
            created_at: scheme.created_at,
            updated_at: scheme.updated_at,
            events: scheme.events.clone(),
        })
    }

    pub fn update_macro(&self, request: UpdateMacroRequest) -> Result<MacroState, String> {
        if self.playing.load(Ordering::SeqCst) {
            return Err("Macro playback is running".to_string());
        }

        let name = normalize_name(&request.name)?;
        if request.events.is_empty() {
            return Err("Macro event list cannot be empty".to_string());
        }

        let events = request
            .events
            .into_iter()
            .map(validate_macro_event)
            .collect::<Result<Vec<_>, _>>()?;

        let mut macros = self.macros.lock().map_err(|err| err.to_string())?;
        let Some(scheme) = macros.iter_mut().find(|item| item.id == request.id) else {
            return Err("Macro not found".to_string());
        };

        scheme.name = name;
        scheme.events = events;
        scheme.updated_at = unix_ms();
        drop(macros);

        self.persist();
        self.make_state()
    }

    fn make_state(&self) -> Result<MacroState, String> {
        let selected_id = *self.selected_id.lock().map_err(|err| err.to_string())?;
        let macros = self
            .macros
            .lock()
            .map_err(|err| err.to_string())?
            .iter()
            .map(|scheme| MacroSummary {
                id: scheme.id,
                name: scheme.name.clone(),
                playback_speed: scheme.playback_speed,
                loop_playback: scheme.loop_playback,
                created_at: scheme.created_at,
                updated_at: scheme.updated_at,
                event_count: scheme.events.len(),
            })
            .collect();

        Ok(MacroState {
            macros,
            selected_id,
            playing: self.playing.load(Ordering::SeqCst),
        })
    }

    fn take_next_id(&self) -> Result<u64, String> {
        let mut next_id = self.next_id.lock().map_err(|err| err.to_string())?;
        let id = *next_id;
        *next_id += 1;
        Ok(id)
    }

    fn persist(&self) {
        if let Ok(macros) = self.macros.lock() {
            save_macros(&macros);
        }
    }
}

fn play_macro_event(event: &MacroEvent, speed: f64) -> Result<(), String> {
    match event {
        MacroEvent::MouseClick { button } => {
            let button = parse_button(button)?;
            click_button(button, speed)
        }
        MacroEvent::MouseDoubleClick { button } => {
            let button = parse_button(button)?;
            click_button(button, speed)?;
            sleep_adjusted(45, speed);
            click_button(button, speed)
        }
        MacroEvent::MouseDown { button } => {
            simulate(&EventType::ButtonPress(parse_button(button)?)).map_err(|err| err.to_string())
        }
        MacroEvent::MouseUp { button } => {
            simulate(&EventType::ButtonRelease(parse_button(button)?))
                .map_err(|err| err.to_string())
        }
        MacroEvent::MouseMove { x, y } => simulate(&EventType::MouseMove {
            x: f64::from(*x),
            y: f64::from(*y),
        })
        .map_err(|err| err.to_string()),
        MacroEvent::KeyClick { key } => {
            let key = parse_key(key)?;
            simulate(&EventType::KeyPress(key)).map_err(|err| err.to_string())?;
            sleep_adjusted(20, speed);
            simulate(&EventType::KeyRelease(key)).map_err(|err| err.to_string())
        }
        MacroEvent::KeyDown { key } => {
            simulate(&EventType::KeyPress(parse_key(key)?)).map_err(|err| err.to_string())
        }
        MacroEvent::KeyUp { key } => {
            simulate(&EventType::KeyRelease(parse_key(key)?)).map_err(|err| err.to_string())
        }
        MacroEvent::Delay { ms } => {
            sleep_adjusted(*ms, speed);
            Ok(())
        }
    }
}

fn click_button(button: Button, speed: f64) -> Result<(), String> {
    simulate(&EventType::ButtonPress(button)).map_err(|err| err.to_string())?;
    sleep_adjusted(20, speed);
    simulate(&EventType::ButtonRelease(button)).map_err(|err| err.to_string())
}

fn sleep_adjusted(ms: u64, speed: f64) {
    let adjusted = (ms as f64 / speed.max(0.1)).round().max(1.0) as u64;
    thread::sleep(Duration::from_millis(adjusted));
}

fn validate_macro_event(event: MacroEvent) -> Result<MacroEvent, String> {
    match &event {
        MacroEvent::MouseClick { button }
        | MacroEvent::MouseDoubleClick { button }
        | MacroEvent::MouseDown { button }
        | MacroEvent::MouseUp { button } => {
            parse_button(button)?;
        }
        MacroEvent::MouseMove { .. } => {}
        MacroEvent::KeyClick { key } | MacroEvent::KeyDown { key } | MacroEvent::KeyUp { key } => {
            parse_key(key)?;
        }
        MacroEvent::Delay { ms } => {
            if !(5..=60_000).contains(ms) {
                return Err("Delay must be between 5 and 60000 ms".to_string());
            }
        }
    }
    Ok(event)
}

fn parse_button(button: &str) -> Result<Button, String> {
    button_from_name(button).ok_or_else(|| "Unsupported mouse button".to_string())
}

fn parse_key(key: &str) -> Result<Key, String> {
    key_from_name(key).ok_or_else(|| "Unsupported keyboard key".to_string())
}

fn normalize_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("Macro name cannot be empty".to_string());
    }
    if name.chars().count() > 20 {
        return Err("Macro name cannot be longer than 20 characters".to_string());
    }
    Ok(name.to_string())
}

fn normalize_playback_speed(speed: f64) -> Result<f64, String> {
    const ALLOWED_SPEEDS: [f64; 5] = [1.0, 1.5, 2.0, 2.5, 3.0];

    if ALLOWED_SPEEDS
        .iter()
        .any(|allowed| (speed - allowed).abs() < f64::EPSILON)
    {
        Ok(speed)
    } else {
        Err("Unsupported playback speed".to_string())
    }
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn default_playback_speed() -> f64 {
    1.0
}

fn data_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".derek-mouse"))
}

fn macros_path() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join("macros.json"))
}

fn load_macros() -> (Vec<MacroScheme>, u64) {
    let Some(path) = macros_path() else {
        return (Vec::new(), 1);
    };

    let data = match fs::read_to_string(&path) {
        Ok(data) => data,
        Err(_) => return (Vec::new(), 1),
    };

    let mut macros: Vec<MacroScheme> = match serde_json::from_str(&data) {
        Ok(macros) => macros,
        Err(_) => return (Vec::new(), 1),
    };

    for scheme in &mut macros {
        if scheme.updated_at == 0 {
            scheme.updated_at = scheme.created_at;
        }
    }

    let next_id = macros.iter().map(|item| item.id).max().unwrap_or(0) + 1;
    (macros, next_id)
}

fn save_macros(macros: &[MacroScheme]) {
    let Some(path) = macros_path() else {
        return;
    };

    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }

    let data = match serde_json::to_string(macros) {
        Ok(data) => data,
        Err(_) => return,
    };

    let _ = fs::write(&path, data);
}

fn hide_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}
