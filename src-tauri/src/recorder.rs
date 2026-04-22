use std::{
    fs,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

use crate::{
    input::{key_from_name, simulate, Event, EventType, HotkeyConfig, Key, KeyboardTracker},
    tray::{self, TrayStatus},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TimedEvent {
    delay_ms: u64,
    event_type: EventType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Recording {
    id: u64,
    name: String,
    created_at: u64,
    events: Vec<TimedEvent>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingSummary {
    id: u64,
    name: String,
    created_at: u64,
    event_count: usize,
    duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecorderState {
    recordings: Vec<RecordingSummary>,
    selected_id: Option<u64>,
    recording: bool,
    playing: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameRecordingRequest {
    id: u64,
    name: String,
}

struct RecorderSession {
    id: u64,
    name: String,
    created_at: u64,
    last_event_at: Instant,
    events: Vec<TimedEvent>,
}

pub struct RecorderRuntime {
    recordings: Mutex<Vec<Recording>>,
    selected_id: Mutex<Option<u64>>,
    record_hotkey: Mutex<HotkeyConfig>,
    session: Mutex<Option<RecorderSession>>,
    record_keyboard: Mutex<KeyboardTracker>,
    playback_keyboard: Mutex<KeyboardTracker>,
    recording: AtomicBool,
    playing: Arc<AtomicBool>,
    injecting_event: Arc<AtomicBool>,
    suppress_record_hotkey_release: AtomicBool,
    suppress_playback_hotkey_release: AtomicBool,
    next_id: Mutex<u64>,
}

impl RecorderRuntime {
    pub fn new_with_hotkey(record_hotkey: HotkeyConfig) -> Self {
        let (recordings, next_id) = load_recordings();
        let first_id = recordings.first().map(|r| r.id);
        Self {
            recordings: Mutex::new(recordings),
            selected_id: Mutex::new(first_id),
            record_hotkey: Mutex::new(record_hotkey),
            session: Mutex::new(None),
            record_keyboard: Mutex::new(KeyboardTracker::default()),
            playback_keyboard: Mutex::new(KeyboardTracker::default()),
            recording: AtomicBool::new(false),
            playing: Arc::new(AtomicBool::new(false)),
            injecting_event: Arc::new(AtomicBool::new(false)),
            suppress_record_hotkey_release: AtomicBool::new(false),
            suppress_playback_hotkey_release: AtomicBool::new(false),
            next_id: Mutex::new(next_id),
        }
    }
    pub fn state(&self) -> Result<RecorderState, String> {
        self.make_state()
    }

    pub fn record_hotkey_config(&self) -> HotkeyConfig {
        self.record_hotkey
            .lock()
            .map(|hotkey| hotkey.clone())
            .unwrap_or_else(|_| HotkeyConfig {
                ctrl: false,
                alt: false,
                key: "F9".to_string(),
            })
    }

    pub fn update_record_hotkey(
        &self,
        mut hotkey: HotkeyConfig,
        playback_hotkey: &HotkeyConfig,
    ) -> Result<HotkeyConfig, String> {
        crate::input::validate_hotkey(&mut hotkey)?;
        if same_hotkey(&hotkey, playback_hotkey) {
            return Err("Recording hotkey cannot be the same as playback hotkey".to_string());
        }

        *self.record_hotkey.lock().map_err(|err| err.to_string())? = hotkey.clone();
        Ok(hotkey)
    }

    pub fn start_recording(&self) -> Result<RecorderState, String> {
        if self.playing.load(Ordering::SeqCst) {
            return Err("Playback is running".to_string());
        }

        let id = self.take_next_id()?;
        let now_ms = unix_ms();
        let now = Instant::now();
        let session = RecorderSession {
            id,
            name: default_recording_name(now_ms),
            created_at: now_ms,
            last_event_at: now,
            events: Vec::new(),
        };

        *self.session.lock().map_err(|err| err.to_string())? = Some(session);
        self.recording.store(true, Ordering::SeqCst);
        tray::set_status(TrayStatus::Recording);
        self.make_state()
    }

    pub fn stop_recording(&self) -> Result<RecorderState, String> {
        let session = self.session.lock().map_err(|err| err.to_string())?.take();
        self.recording.store(false, Ordering::SeqCst);
        tray::set_status(TrayStatus::Stopped);

        if let Some(session) = session {
            let recording = Recording {
                id: session.id,
                name: session.name,
                created_at: session.created_at,
                events: session.events,
            };
            let id = recording.id;

            self.recordings
                .lock()
                .map_err(|err| err.to_string())?
                .insert(0, recording);
            *self.selected_id.lock().map_err(|err| err.to_string())? = Some(id);
            self.persist();
        }

        self.make_state()
    }

    pub fn rename_recording(
        &self,
        request: RenameRecordingRequest,
    ) -> Result<RecorderState, String> {
        let name = request.name.trim();
        if name.is_empty() {
            return Err("Recording name cannot be empty".to_string());
        }

        let mut recordings = self.recordings.lock().map_err(|err| err.to_string())?;
        let Some(recording) = recordings.iter_mut().find(|item| item.id == request.id) else {
            return Err("Recording not found".to_string());
        };
        recording.name = name.to_string();
        drop(recordings);

        self.persist();
        self.make_state()
    }

    pub fn select_recording(&self, id: u64) -> Result<RecorderState, String> {
        let exists = self
            .recordings
            .lock()
            .map_err(|err| err.to_string())?
            .iter()
            .any(|item| item.id == id);

        if !exists {
            return Err("Recording not found".to_string());
        }

        *self.selected_id.lock().map_err(|err| err.to_string())? = Some(id);
        self.make_state()
    }

    pub fn delete_recording(&self, id: u64) -> Result<RecorderState, String> {
        self.recordings
            .lock()
            .map_err(|err| err.to_string())?
            .retain(|item| item.id != id);

        let mut selected_id = self.selected_id.lock().map_err(|err| err.to_string())?;
        if *selected_id == Some(id) {
            *selected_id = None;
        }
        drop(selected_id);

        self.persist();
        self.make_state()
    }

    pub fn play_recording(
        &self,
        id: u64,
        app: tauri::AppHandle,
        show_window_on_stop: bool,
        speed: f64,
        loop_mode: bool,
    ) -> Result<RecorderState, String> {
        if self.recording.load(Ordering::SeqCst) {
            return Err("Recording is running".to_string());
        }
        if self.playing.swap(true, Ordering::SeqCst) {
            return Err("Playback is already running".to_string());
        }
        tray::set_status(TrayStatus::Running);

        let recording = self
            .recordings
            .lock()
            .map_err(|err| err.to_string())?
            .iter()
            .find(|item| item.id == id)
            .cloned()
            .ok_or_else(|| "Recording not found".to_string())?;

        *self.selected_id.lock().map_err(|err| err.to_string())? = Some(id);

        let playback_flag = self.playing.clone();
        let injecting_flag = self.injecting_event.clone();

        thread::spawn(move || {
            loop {
                for event in &recording.events {
                    if !playback_flag.load(Ordering::SeqCst) {
                        break;
                    }
                    let adjusted_delay = (event.delay_ms as f64 / speed).round() as u64;
                    thread::sleep(Duration::from_millis(adjusted_delay));
                    injecting_flag.store(true, Ordering::SeqCst);
                    let _ = simulate(&event.event_type);
                    injecting_flag.store(false, Ordering::SeqCst);
                }
                if !loop_mode || !playback_flag.load(Ordering::SeqCst) {
                    break;
                }
            }

            playback_flag.store(false, Ordering::SeqCst);
            tray::set_status(TrayStatus::Stopped);
            if show_window_on_stop {
                show_main_window(&app);
            }
            let _ = app.emit("recorder-status", false);
        });

        self.make_state()
    }

    pub fn stop_playback(&self) -> Result<RecorderState, String> {
        self.playing.store(false, Ordering::SeqCst);
        tray::set_status(TrayStatus::Stopped);
        self.make_state()
    }

    pub fn toggle_selected_playback(
        &self,
        app: tauri::AppHandle,
        show_window_on_stop: bool,
        speed: f64,
        loop_mode: bool,
    ) -> Result<RecorderState, String> {
        if self.playing.load(Ordering::SeqCst) {
            return self.stop_playback();
        }

        let Some(id) = *self.selected_id.lock().map_err(|err| err.to_string())? else {
            return Err("No recording selected".to_string());
        };

        self.play_recording(id, app, show_window_on_stop, speed, loop_mode)
    }

    pub fn handle_event(
        &self,
        event: &Event,
        app: &tauri::AppHandle,
        playback_hotkey: &HotkeyConfig,
        active: bool,
        show_window_on_playback_stop: bool,
        playback_speed: f64,
        loop_mode: bool,
    ) {
        let record_hotkey = self.record_hotkey_config();

        match event.event_type {
            EventType::KeyPress(key) => {
                let record_hotkey_triggered = self
                    .record_keyboard
                    .lock()
                    .map(|mut keyboard| keyboard.handle_press(key, &record_hotkey))
                    .unwrap_or(false);
                let playback_hotkey_triggered = self
                    .playback_keyboard
                    .lock()
                    .map(|mut keyboard| keyboard.handle_press(key, playback_hotkey))
                    .unwrap_or(false);

                if record_hotkey_triggered && active {
                    self.suppress_record_hotkey_release
                        .store(true, Ordering::SeqCst);
                    if let Ok(mut keyboard) = self.record_keyboard.lock() {
                        keyboard.clear_hotkey_down();
                    }
                    let was_recording = self.recording.load(Ordering::SeqCst);
                    hide_main_window(app);
                    let state = if self.recording.load(Ordering::SeqCst) {
                        self.stop_recording()
                    } else {
                        self.start_recording()
                    };
                    if was_recording {
                        show_main_window(app);
                    }

                    if let Ok(state) = state {
                        let _ = app.emit("recorder-state", state);
                    }
                    return;
                }

                if playback_hotkey_triggered && active {
                    self.suppress_playback_hotkey_release
                        .store(true, Ordering::SeqCst);
                    if let Ok(mut keyboard) = self.playback_keyboard.lock() {
                        keyboard.clear_hotkey_down();
                    }
                    let was_playing = self.playing.load(Ordering::SeqCst);
                    hide_main_window(app);
                    if let Ok(state) = self.toggle_selected_playback(
                        app.clone(),
                        show_window_on_playback_stop,
                        playback_speed,
                        loop_mode,
                    ) {
                        tray::notify_global_hotkey_state(app, !was_playing);
                        if was_playing && show_window_on_playback_stop {
                            show_main_window(app);
                        }
                        let _ = app.emit("recorder-state", state);
                    }
                    return;
                }
            }
            EventType::KeyRelease(key) => {
                if let Ok(mut keyboard) = self.record_keyboard.lock() {
                    keyboard.handle_release(key, &record_hotkey);
                }
                if let Ok(mut keyboard) = self.playback_keyboard.lock() {
                    keyboard.handle_release(key, playback_hotkey);
                }

                if key_from_name(&record_hotkey.key) == Some(key)
                    && self
                        .suppress_record_hotkey_release
                        .swap(false, Ordering::SeqCst)
                {
                    return;
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

        if active
            && (is_hotkey_related_event(event, &record_hotkey)
                || is_hotkey_related_event(event, playback_hotkey))
        {
            return;
        }

        self.capture_event(event);
    }

    fn capture_event(&self, event: &Event) {
        if !self.recording.load(Ordering::SeqCst) || self.injecting_event.load(Ordering::SeqCst) {
            return;
        }

        let event_type = match event.event_type {
            EventType::KeyPress(_)
            | EventType::KeyRelease(_)
            | EventType::ButtonPress(_)
            | EventType::ButtonRelease(_)
            | EventType::MouseMove { .. }
            | EventType::Wheel { .. } => event.event_type,
        };

        let Ok(mut session) = self.session.lock() else {
            return;
        };
        let Some(session) = session.as_mut() else {
            return;
        };

        let now = Instant::now();
        let delay_ms = now.duration_since(session.last_event_at).as_millis() as u64;
        session.last_event_at = now;
        session.events.push(TimedEvent {
            delay_ms,
            event_type,
        });
    }

    fn make_state(&self) -> Result<RecorderState, String> {
        let selected_id = *self.selected_id.lock().map_err(|err| err.to_string())?;
        let recordings = self
            .recordings
            .lock()
            .map_err(|err| err.to_string())?
            .iter()
            .map(|recording| RecordingSummary {
                id: recording.id,
                name: recording.name.clone(),
                created_at: recording.created_at,
                event_count: recording.events.len(),
                duration_ms: recording.events.iter().map(|event| event.delay_ms).sum(),
            })
            .collect();

        Ok(RecorderState {
            recordings,
            selected_id,
            recording: self.recording.load(Ordering::SeqCst),
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
        if let Ok(recordings) = self.recordings.lock() {
            save_recordings(&recordings);
        }
    }
}

fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn default_recording_name(created_at: u64) -> String {
    format!("录制方案 {}", created_at)
}

fn data_dir() -> Option<PathBuf> {
    dirs::home_dir().map(|home| home.join(".derek-mouse"))
}

fn recordings_path() -> Option<PathBuf> {
    data_dir().map(|dir| dir.join("recordings.json"))
}

fn load_recordings() -> (Vec<Recording>, u64) {
    let Some(path) = recordings_path() else {
        return (Vec::new(), 1);
    };

    let data = match fs::read_to_string(&path) {
        Ok(data) => data,
        Err(_) => return (Vec::new(), 1),
    };

    let recordings: Vec<Recording> = match serde_json::from_str(&data) {
        Ok(recordings) => recordings,
        Err(_) => return (Vec::new(), 1),
    };

    let next_id = recordings.iter().map(|r| r.id).max().unwrap_or(0) + 1;
    (recordings, next_id)
}

fn save_recordings(recordings: &[Recording]) {
    let Some(path) = recordings_path() else {
        return;
    };

    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }

    let data = match serde_json::to_string(recordings) {
        Ok(data) => data,
        Err(_) => return,
    };

    let _ = fs::write(&path, data);
}

fn is_hotkey_related_event(event: &Event, hotkey: &HotkeyConfig) -> bool {
    let key = match event.event_type {
        EventType::KeyPress(key) | EventType::KeyRelease(key) => key,
        _ => return false,
    };

    if key_from_name(&hotkey.key) == Some(key) {
        return true;
    }

    matches!(
        (key, hotkey.ctrl, hotkey.alt),
        (Key::ControlLeft | Key::ControlRight, true, _) | (Key::Alt | Key::AltGr, _, true)
    )
}

pub fn same_hotkey(left: &HotkeyConfig, right: &HotkeyConfig) -> bool {
    left.ctrl == right.ctrl
        && left.alt == right.alt
        && left.key.trim().eq_ignore_ascii_case(right.key.trim())
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
