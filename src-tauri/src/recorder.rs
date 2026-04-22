use std::{
    collections::HashSet,
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
    #[serde(default)]
    updated_at: u64,
    #[serde(default = "default_playback_speed")]
    playback_speed: f64,
    #[serde(default)]
    loop_playback: bool,
    events: Vec<TimedEvent>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingSummary {
    id: u64,
    name: String,
    playback_speed: f64,
    loop_playback: bool,
    created_at: u64,
    updated_at: u64,
    event_count: usize,
    duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingDetail {
    id: u64,
    name: String,
    playback_speed: f64,
    loop_playback: bool,
    created_at: u64,
    updated_at: u64,
    events: Vec<RecordingEventSummary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingEventSummary {
    index: usize,
    delay_ms: u64,
    action: String,
    target: String,
    critical: bool,
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRecordingPlaybackSpeedRequest {
    id: u64,
    speed: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateRecordingLoopPlaybackRequest {
    id: u64,
    value: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveEditedRecordingRequest {
    id: u64,
    removed_event_indices: Vec<usize>,
    mode: SaveEditedRecordingMode,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
enum SaveEditedRecordingMode {
    Append,
    Replace,
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

    pub fn recording_detail(&self, id: u64) -> Result<RecordingDetail, String> {
        let recording = self
            .recordings
            .lock()
            .map_err(|err| err.to_string())?
            .iter()
            .find(|item| item.id == id)
            .cloned()
            .ok_or_else(|| "Recording not found".to_string())?;

        Ok(RecordingDetail {
            id: recording.id,
            name: recording.name,
            playback_speed: recording.playback_speed,
            loop_playback: recording.loop_playback,
            created_at: recording.created_at,
            updated_at: recording.updated_at,
            events: recording
                .events
                .iter()
                .enumerate()
                .map(|(index, event)| summarize_event(index, event))
                .collect(),
        })
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

    pub fn is_recording(&self) -> bool {
        self.recording.load(Ordering::SeqCst)
    }

    pub fn is_playing(&self) -> bool {
        self.playing.load(Ordering::SeqCst)
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
            let cleaned_events = clean_recorded_events(session.events);
            let recording = Recording {
                id: session.id,
                name: session.name,
                created_at: session.created_at,
                updated_at: session.created_at,
                playback_speed: 1.0,
                loop_playback: false,
                events: cleaned_events,
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
        recording.updated_at = unix_ms();
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

    pub fn update_recording_playback_speed(
        &self,
        request: UpdateRecordingPlaybackSpeedRequest,
    ) -> Result<RecorderState, String> {
        if self.playing.load(Ordering::SeqCst) {
            return Err("Cannot change playback speed while playback is running".to_string());
        }

        let speed = normalize_playback_speed(request.speed)?;
        let mut recordings = self.recordings.lock().map_err(|err| err.to_string())?;
        let Some(recording) = recordings.iter_mut().find(|item| item.id == request.id) else {
            return Err("Recording not found".to_string());
        };
        recording.playback_speed = speed;
        drop(recordings);

        self.persist();
        self.make_state()
    }

    pub fn update_recording_loop_playback(
        &self,
        request: UpdateRecordingLoopPlaybackRequest,
    ) -> Result<RecorderState, String> {
        if self.playing.load(Ordering::SeqCst) {
            return Err("Cannot change loop mode while playback is running".to_string());
        }

        let mut recordings = self.recordings.lock().map_err(|err| err.to_string())?;
        let Some(recording) = recordings.iter_mut().find(|item| item.id == request.id) else {
            return Err("Recording not found".to_string());
        };
        recording.loop_playback = request.value;
        drop(recordings);

        self.persist();
        self.make_state()
    }

    pub fn save_edited_recording(
        &self,
        request: SaveEditedRecordingRequest,
    ) -> Result<RecorderState, String> {
        if self.recording.load(Ordering::SeqCst) {
            return Err("Recording is running".to_string());
        }
        if self.playing.load(Ordering::SeqCst) {
            return Err("Playback is running".to_string());
        }

        let mut recordings = self.recordings.lock().map_err(|err| err.to_string())?;
        let Some(original_index) = recordings.iter().position(|item| item.id == request.id) else {
            return Err("Recording not found".to_string());
        };
        let original = recordings[original_index].clone();

        if request
            .removed_event_indices
            .iter()
            .any(|index| *index >= original.events.len())
        {
            return Err("Edited event selection is invalid".to_string());
        }

        let removed_indices: HashSet<usize> = request.removed_event_indices.into_iter().collect();
        let events = original
            .events
            .iter()
            .enumerate()
            .filter(|(index, _)| !removed_indices.contains(index))
            .map(|(_, event)| event.clone())
            .collect();

        let now_ms = unix_ms();
        let new_id = self.take_next_id()?;
        let new_recording = Recording {
            id: new_id,
            name: match request.mode {
                SaveEditedRecordingMode::Append => format!("{} 编辑", original.name),
                SaveEditedRecordingMode::Replace => original.name,
            },
            created_at: now_ms,
            updated_at: now_ms,
            playback_speed: original.playback_speed,
            loop_playback: original.loop_playback,
            events,
        };

        if matches!(request.mode, SaveEditedRecordingMode::Replace) {
            recordings.remove(original_index);
        }
        recordings.insert(0, new_recording);
        drop(recordings);

        *self.selected_id.lock().map_err(|err| err.to_string())? = Some(new_id);
        self.persist();
        self.make_state()
    }

    pub fn play_recording(
        &self,
        id: u64,
        app: tauri::AppHandle,
        show_window_on_stop: bool,
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
        let speed = recording.playback_speed.max(0.1);
        let loop_mode = recording.loop_playback;

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
    ) -> Result<RecorderState, String> {
        if self.playing.load(Ordering::SeqCst) {
            return self.stop_playback();
        }

        let Some(id) = *self.selected_id.lock().map_err(|err| err.to_string())? else {
            return Err("No recording selected".to_string());
        };

        self.play_recording(id, app, show_window_on_stop)
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
                    if auto_hide_on_hotkey {
                        hide_main_window(app);
                    }
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
                    if auto_hide_on_hotkey {
                        hide_main_window(app);
                    }
                    if let Ok(state) =
                        self.toggle_selected_playback(app.clone(), show_window_on_playback_stop)
                    {
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
                playback_speed: recording.playback_speed,
                loop_playback: recording.loop_playback,
                created_at: recording.created_at,
                updated_at: recording.updated_at,
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

fn clean_recorded_events(events: Vec<TimedEvent>) -> Vec<TimedEvent> {
    let events = remove_mouse_jitter(events, 3.0);
    let events = merge_continuous_mouse_moves(events);
    let events = remove_redundant_events(events);
    normalize_event_delays(events)
}

fn remove_mouse_jitter(events: Vec<TimedEvent>, threshold_px: f64) -> Vec<TimedEvent> {
    let mut cleaned = Vec::with_capacity(events.len());
    let mut pending_delay = 0_u64;
    let mut last_kept_move: Option<(f64, f64)> = None;

    for mut event in events {
        event.delay_ms = event.delay_ms.saturating_add(pending_delay);
        pending_delay = 0;

        if let EventType::MouseMove { x, y } = event.event_type {
            if let Some((lx, ly)) = last_kept_move {
                let dx = x - lx;
                let dy = y - ly;
                let distance = (dx * dx + dy * dy).sqrt();
                if distance < threshold_px {
                    pending_delay = pending_delay.saturating_add(event.delay_ms);
                    continue;
                }
            }
            last_kept_move = Some((x, y));
        }

        cleaned.push(event);
    }

    cleaned
}

fn merge_continuous_mouse_moves(events: Vec<TimedEvent>) -> Vec<TimedEvent> {
    let mut merged = Vec::with_capacity(events.len());
    let mut index = 0;

    while index < events.len() {
        if !matches!(events[index].event_type, EventType::MouseMove { .. }) {
            merged.push(events[index].clone());
            index += 1;
            continue;
        }

        let mut end = index;
        while end < events.len() && matches!(events[end].event_type, EventType::MouseMove { .. }) {
            end += 1;
        }

        let run = &events[index..end];
        if run.len() < 3 {
            merged.extend(run.iter().cloned());
        } else {
            let smoothed = smooth_move_run(run);
            if smoothed.is_empty() {
                merged.extend(run.iter().cloned());
            } else {
                merged.extend(smoothed);
            }
        }

        index = end;
    }

    merged
}

fn smooth_move_run(run: &[TimedEvent]) -> Vec<TimedEvent> {
    let (sx, sy) = match run.first().map(|event| event.event_type) {
        Some(EventType::MouseMove { x, y }) => (x, y),
        _ => return Vec::new(),
    };

    let (ex, ey) = match run.last().map(|event| event.event_type) {
        Some(EventType::MouseMove { x, y }) => (x, y),
        _ => return Vec::new(),
    };

    let mut path_length = 0.0_f64;
    let mut previous = (sx, sy);
    let mut sum_x = 0.0_f64;
    let mut sum_y = 0.0_f64;

    for event in run {
        if let EventType::MouseMove { x, y } = event.event_type {
            let dx = x - previous.0;
            let dy = y - previous.1;
            path_length += (dx * dx + dy * dy).sqrt();
            previous = (x, y);
            sum_x += x;
            sum_y += y;
        }
    }

    let total_delay: u64 = run.iter().map(|event| event.delay_ms).sum();
    if path_length < 6.0 {
        return vec![TimedEvent {
            delay_ms: total_delay,
            event_type: EventType::MouseMove { x: ex, y: ey },
        }];
    }

    let mid = run[run.len() / 2].event_type;
    let (mx, my) = match mid {
        EventType::MouseMove { x, y } => (x, y),
        _ => ((sx + ex) / 2.0, (sy + ey) / 2.0),
    };

    let count = run.len() as f64;
    let avg = (sum_x / count, sum_y / count);
    let control = ((mx + avg.0) / 2.0, (my + avg.1) / 2.0);

    let target_samples = ((path_length / 12.0).round() as usize).clamp(2, run.len().min(12));
    let mut points: Vec<(f64, f64)> = Vec::with_capacity(target_samples + 1);

    for step in 1..=target_samples {
        let t = step as f64 / target_samples as f64;
        let inv = 1.0 - t;
        let x = inv * inv * sx + 2.0 * inv * t * control.0 + t * t * ex;
        let y = inv * inv * sy + 2.0 * inv * t * control.1 + t * t * ey;

        if let Some((lx, ly)) = points.last().copied() {
            let dx = x - lx;
            let dy = y - ly;
            if (dx * dx + dy * dy).sqrt() < 1.0 {
                continue;
            }
        }

        points.push((x, y));
    }

    if points
        .last()
        .map(|(x, y)| (x - ex).abs() > f64::EPSILON || (y - ey).abs() > f64::EPSILON)
        .unwrap_or(true)
    {
        points.push((ex, ey));
    }

    if points.is_empty() {
        return Vec::new();
    }

    let slice_count = points.len() as u64;
    let base_delay = total_delay / slice_count;
    let remainder = total_delay % slice_count;

    points
        .into_iter()
        .enumerate()
        .map(|(idx, (x, y))| TimedEvent {
            delay_ms: base_delay + if idx == 0 { remainder } else { 0 },
            event_type: EventType::MouseMove { x, y },
        })
        .collect()
}

fn remove_redundant_events(events: Vec<TimedEvent>) -> Vec<TimedEvent> {
    let mut optimized = Vec::with_capacity(events.len());
    let mut pending_delay = 0_u64;
    let mut pressed_keys: HashSet<Key> = HashSet::new();
    let mut pressed_buttons: HashSet<crate::input::Button> = HashSet::new();
    let mut last_move: Option<(f64, f64)> = None;

    for mut event in events {
        event.delay_ms = event.delay_ms.saturating_add(pending_delay);
        pending_delay = 0;

        let mut drop_event = false;
        match event.event_type {
            EventType::KeyPress(key) => {
                if !pressed_keys.insert(key) {
                    drop_event = true;
                }
                last_move = None;
            }
            EventType::KeyRelease(key) => {
                if !pressed_keys.remove(&key) {
                    drop_event = true;
                }
                last_move = None;
            }
            EventType::ButtonPress(button) => {
                if !pressed_buttons.insert(button) {
                    drop_event = true;
                }
                last_move = None;
            }
            EventType::ButtonRelease(button) => {
                if !pressed_buttons.remove(&button) {
                    drop_event = true;
                }
                last_move = None;
            }
            EventType::Wheel { delta_x, delta_y } => {
                if delta_x == 0 && delta_y == 0 {
                    drop_event = true;
                }
                last_move = None;
            }
            EventType::MouseMove { x, y } => {
                if let Some((lx, ly)) = last_move {
                    if (x - lx).abs() < f64::EPSILON && (y - ly).abs() < f64::EPSILON {
                        drop_event = true;
                    }
                }
                last_move = Some((x, y));
            }
        }

        if drop_event {
            pending_delay = pending_delay.saturating_add(event.delay_ms);
            continue;
        }

        optimized.push(event);
    }

    optimized
}

fn normalize_event_delays(events: Vec<TimedEvent>) -> Vec<TimedEvent> {
    events
        .into_iter()
        .map(|mut event| {
            event.delay_ms = normalize_delay_ms(event.delay_ms);
            event
        })
        .collect()
}

fn normalize_delay_ms(delay_ms: u64) -> u64 {
    if delay_ms <= 20 {
        return delay_ms;
    }

    if delay_ms <= 200 {
        return round_to_step(delay_ms, 5);
    }

    round_to_step(delay_ms, 10)
}

fn round_to_step(value: u64, step: u64) -> u64 {
    if step == 0 {
        return value;
    }
    ((value + step / 2) / step) * step
}

fn default_recording_name(created_at: u64) -> String {
    format!("录制方案 {}", created_at)
}

fn summarize_event(index: usize, event: &TimedEvent) -> RecordingEventSummary {
    let (action, target, critical) = match event.event_type {
        EventType::KeyPress(key) => ("键盘按下".to_string(), key_label(key), true),
        EventType::KeyRelease(key) => ("键盘释放".to_string(), key_label(key), true),
        EventType::ButtonPress(button) => ("鼠标按下".to_string(), button_label(button), true),
        EventType::ButtonRelease(button) => ("鼠标释放".to_string(), button_label(button), true),
        EventType::MouseMove { x, y } => (
            "鼠标移动".to_string(),
            format!("x {:.0}, y {:.0}", x, y),
            false,
        ),
        EventType::Wheel { delta_x, delta_y } => (
            "滚轮".to_string(),
            format!("水平 {delta_x}, 垂直 {delta_y}"),
            false,
        ),
    };

    RecordingEventSummary {
        index,
        delay_ms: event.delay_ms,
        action,
        target,
        critical,
    }
}

fn button_label(button: crate::input::Button) -> String {
    match button {
        crate::input::Button::Left => "左键".to_string(),
        crate::input::Button::Right => "右键".to_string(),
        crate::input::Button::Middle => "中键".to_string(),
        crate::input::Button::Unknown(code) => format!("扩展键 {code}"),
    }
}

fn key_label(key: Key) -> String {
    match key {
        Key::Alt | Key::AltGr => "Alt".to_string(),
        Key::Backspace => "Backspace".to_string(),
        Key::CapsLock => "CapsLock".to_string(),
        Key::ControlLeft | Key::ControlRight => "Ctrl".to_string(),
        Key::Delete => "Delete".to_string(),
        Key::DownArrow => "Down".to_string(),
        Key::End => "End".to_string(),
        Key::Escape => "Esc".to_string(),
        Key::Home => "Home".to_string(),
        Key::LeftArrow => "Left".to_string(),
        Key::MetaLeft | Key::MetaRight => "Meta".to_string(),
        Key::PageDown => "PageDown".to_string(),
        Key::PageUp => "PageUp".to_string(),
        Key::Return | Key::KpReturn => "Enter".to_string(),
        Key::RightArrow => "Right".to_string(),
        Key::ShiftLeft | Key::ShiftRight => "Shift".to_string(),
        Key::Space => "Space".to_string(),
        Key::Tab => "Tab".to_string(),
        Key::UpArrow => "Up".to_string(),
        Key::PrintScreen => "PrintScreen".to_string(),
        Key::ScrollLock => "ScrollLock".to_string(),
        Key::Pause => "Pause".to_string(),
        Key::NumLock => "NumLock".to_string(),
        Key::BackQuote => "`".to_string(),
        Key::Num1 => "1".to_string(),
        Key::Num2 => "2".to_string(),
        Key::Num3 => "3".to_string(),
        Key::Num4 => "4".to_string(),
        Key::Num5 => "5".to_string(),
        Key::Num6 => "6".to_string(),
        Key::Num7 => "7".to_string(),
        Key::Num8 => "8".to_string(),
        Key::Num9 => "9".to_string(),
        Key::Num0 => "0".to_string(),
        Key::Minus => "-".to_string(),
        Key::Equal => "=".to_string(),
        Key::KeyQ => "Q".to_string(),
        Key::KeyW => "W".to_string(),
        Key::KeyE => "E".to_string(),
        Key::KeyR => "R".to_string(),
        Key::KeyT => "T".to_string(),
        Key::KeyY => "Y".to_string(),
        Key::KeyU => "U".to_string(),
        Key::KeyI => "I".to_string(),
        Key::KeyO => "O".to_string(),
        Key::KeyP => "P".to_string(),
        Key::LeftBracket => "[".to_string(),
        Key::RightBracket => "]".to_string(),
        Key::KeyA => "A".to_string(),
        Key::KeyS => "S".to_string(),
        Key::KeyD => "D".to_string(),
        Key::KeyF => "F".to_string(),
        Key::KeyG => "G".to_string(),
        Key::KeyH => "H".to_string(),
        Key::KeyJ => "J".to_string(),
        Key::KeyK => "K".to_string(),
        Key::KeyL => "L".to_string(),
        Key::SemiColon => ";".to_string(),
        Key::Quote => "'".to_string(),
        Key::BackSlash => "\\".to_string(),
        Key::IntlBackslash => "\\".to_string(),
        Key::KeyZ => "Z".to_string(),
        Key::KeyX => "X".to_string(),
        Key::KeyC => "C".to_string(),
        Key::KeyV => "V".to_string(),
        Key::KeyB => "B".to_string(),
        Key::KeyN => "N".to_string(),
        Key::KeyM => "M".to_string(),
        Key::Comma => ",".to_string(),
        Key::Dot => ".".to_string(),
        Key::Slash => "/".to_string(),
        Key::Insert => "Insert".to_string(),
        Key::KpMinus => "Num -".to_string(),
        Key::KpPlus => "Num +".to_string(),
        Key::KpMultiply => "Num *".to_string(),
        Key::KpDivide => "Num /".to_string(),
        Key::Kp0 => "Num 0".to_string(),
        Key::Kp1 => "Num 1".to_string(),
        Key::Kp2 => "Num 2".to_string(),
        Key::Kp3 => "Num 3".to_string(),
        Key::Kp4 => "Num 4".to_string(),
        Key::Kp5 => "Num 5".to_string(),
        Key::Kp6 => "Num 6".to_string(),
        Key::Kp7 => "Num 7".to_string(),
        Key::Kp8 => "Num 8".to_string(),
        Key::Kp9 => "Num 9".to_string(),
        Key::KpDelete => "Num Del".to_string(),
        Key::Function => "Fn".to_string(),
        Key::Unknown(code) => format!("未知键 {code}"),
        key => format!("{key:?}"),
    }
}

fn default_playback_speed() -> f64 {
    1.0
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

    let mut recordings: Vec<Recording> = match serde_json::from_str(&data) {
        Ok(recordings) => recordings,
        Err(_) => return (Vec::new(), 1),
    };

    for recording in &mut recordings {
        if recording.updated_at == 0 {
            recording.updated_at = recording.created_at;
        }
    }

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
