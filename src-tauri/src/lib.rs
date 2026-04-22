mod clicker;
mod config;
mod input;
mod recorder;
mod tray;

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
};

use clicker::{ClickerConfig, ClickerRuntime};
use config::{load_config, save_config, AppConfig};
use input::{listen, ActiveFeature, Event, HotkeyConfig};
use recorder::{
    RecorderRuntime, RenameRecordingRequest, UpdateRecordingLoopPlaybackRequest,
    UpdateRecordingPlaybackSpeedRequest,
};
use tauri::{Emitter, Manager};

struct AppState {
    active_feature: Mutex<ActiveFeature>,
    clicker: Arc<ClickerRuntime>,
    recorder: Arc<RecorderRuntime>,
    show_window_on_global_hotkey_stop: AtomicBool,
    auto_hide_on_hotkey: AtomicBool,
}

impl AppState {
    fn persist_config(&self) {
        let clicker_config = self.clicker.config_snapshot().unwrap_or_default();
        let record_hotkey = self.recorder.record_hotkey_config();
        let show_window_on_stop = self
            .show_window_on_global_hotkey_stop
            .load(Ordering::SeqCst);
        let auto_hide_on_hotkey = self.auto_hide_on_hotkey.load(Ordering::SeqCst);

        let config = AppConfig {
            clicker: clicker_config,
            record_hotkey,
            show_window_on_stop,
            auto_hide_on_hotkey,
        };
        save_config(&config);
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct GlobalHotkeyOptions {
    show_window_on_stop: bool,
    auto_hide_on_hotkey: bool,
}

#[tauri::command]
fn set_active_feature(
    feature: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    *state.active_feature.lock().map_err(|err| err.to_string())? =
        ActiveFeature::from_name(&feature);
    Ok(())
}

#[tauri::command]
fn get_clicker_state(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<clicker::ClickerState, String> {
    state.clicker.state()
}

#[tauri::command]
fn update_clicker_config(
    config: ClickerConfig,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<clicker::ClickerState, String> {
    let result = state.clicker.update_config(config);
    state.persist_config();
    result
}

#[tauri::command]
fn get_hotkey_config(state: tauri::State<'_, Arc<AppState>>) -> Result<HotkeyConfig, String> {
    Ok(state.clicker.hotkey_config())
}

#[tauri::command]
fn update_hotkey_config(
    hotkey: HotkeyConfig,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<HotkeyConfig, String> {
    if recorder::same_hotkey(&hotkey, &state.recorder.record_hotkey_config()) {
        return Err("Playback hotkey cannot be the same as recording hotkey".to_string());
    }
    let result = state.clicker.update_hotkey(hotkey);
    state.persist_config();
    result
}

#[tauri::command]
fn get_global_hotkey_options(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<GlobalHotkeyOptions, String> {
    Ok(GlobalHotkeyOptions {
        show_window_on_stop: state
            .show_window_on_global_hotkey_stop
            .load(Ordering::SeqCst),
        auto_hide_on_hotkey: state.auto_hide_on_hotkey.load(Ordering::SeqCst),
    })
}

#[tauri::command]
fn update_global_hotkey_options(
    options: GlobalHotkeyOptions,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<GlobalHotkeyOptions, String> {
    state
        .show_window_on_global_hotkey_stop
        .store(options.show_window_on_stop, Ordering::SeqCst);
    state
        .auto_hide_on_hotkey
        .store(options.auto_hide_on_hotkey, Ordering::SeqCst);
    state.persist_config();
    Ok(options)
}

#[tauri::command]
fn get_recorder_hotkey_config(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<HotkeyConfig, String> {
    Ok(state.recorder.record_hotkey_config())
}

#[tauri::command]
fn update_recorder_hotkey_config(
    hotkey: HotkeyConfig,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<HotkeyConfig, String> {
    let playback_hotkey = state.clicker.hotkey_config();
    let result = state
        .recorder
        .update_record_hotkey(hotkey, &playback_hotkey);
    state.persist_config();
    result
}

#[tauri::command]
fn start_clicker(state: tauri::State<'_, Arc<AppState>>) -> Result<clicker::ClickerState, String> {
    Ok(state.clicker.start())
}

#[tauri::command]
fn stop_clicker(state: tauri::State<'_, Arc<AppState>>) -> Result<clicker::ClickerState, String> {
    Ok(state.clicker.stop())
}

#[tauri::command]
fn get_recorder_state(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<recorder::RecorderState, String> {
    state.recorder.state()
}

#[tauri::command]
fn start_recording(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<recorder::RecorderState, String> {
    state.recorder.start_recording()
}

#[tauri::command]
fn stop_recording(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<recorder::RecorderState, String> {
    state.recorder.stop_recording()
}

#[tauri::command]
fn rename_recording(
    request: RenameRecordingRequest,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<recorder::RecorderState, String> {
    state.recorder.rename_recording(request)
}

#[tauri::command]
fn select_recording(
    id: u64,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<recorder::RecorderState, String> {
    state.recorder.select_recording(id)
}

#[tauri::command]
fn play_recording(
    id: u64,
    state: tauri::State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) -> Result<recorder::RecorderState, String> {
    state.recorder.play_recording(id, app, false)
}

#[tauri::command]
fn stop_playback(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<recorder::RecorderState, String> {
    state.recorder.stop_playback()
}

#[tauri::command]
fn update_recording_playback_speed(
    request: UpdateRecordingPlaybackSpeedRequest,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<recorder::RecorderState, String> {
    state.recorder.update_recording_playback_speed(request)
}

#[tauri::command]
fn update_recording_loop_playback(
    request: UpdateRecordingLoopPlaybackRequest,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<recorder::RecorderState, String> {
    state.recorder.update_recording_loop_playback(request)
}

#[tauri::command]
fn delete_recording(
    id: u64,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<recorder::RecorderState, String> {
    state.recorder.delete_recording(id)
}

fn spawn_global_listener(state: Arc<AppState>, app: tauri::AppHandle) {
    thread::spawn(move || {
        let listener_state = state.clone();
        let listener_app = app.clone();

        let result = listen(move |event| {
            handle_global_event(&listener_state, &listener_app, event);
        });

        if let Err(err) = result {
            let _ = app.emit("input-listener-error", err.to_string());
        }
    });
}

fn handle_global_event(state: &Arc<AppState>, app: &tauri::AppHandle, event: Event) {
    let active_feature = state
        .active_feature
        .lock()
        .map(|feature| *feature)
        .unwrap_or(ActiveFeature::Clicker);
    let hotkey = state.clicker.hotkey_config();
    let show_window_on_global_hotkey_stop = state
        .show_window_on_global_hotkey_stop
        .load(Ordering::SeqCst);
    let auto_hide_on_hotkey = state.auto_hide_on_hotkey.load(Ordering::SeqCst);

    state.clicker.handle_event(
        &event,
        app,
        active_feature == ActiveFeature::Clicker,
        show_window_on_global_hotkey_stop,
        auto_hide_on_hotkey,
    );
    state.recorder.handle_event(
        &event,
        app,
        &hotkey,
        active_feature == ActiveFeature::Recorder,
        show_window_on_global_hotkey_stop,
        auto_hide_on_hotkey,
    );
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config = load_config();

    let clicker = Arc::new(ClickerRuntime::from_config(config.clicker));
    let recorder = Arc::new(RecorderRuntime::new_with_hotkey(config.record_hotkey));
    let state = Arc::new(AppState {
        active_feature: Mutex::new(ActiveFeature::Clicker),
        clicker: clicker.clone(),
        recorder: recorder.clone(),
        show_window_on_global_hotkey_stop: AtomicBool::new(config.show_window_on_stop),
        auto_hide_on_hotkey: AtomicBool::new(config.auto_hide_on_hotkey),
    });

    tauri::Builder::default()
        .manage(state.clone())
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            // 当第二个实例尝试启动时，聚焦已有窗口
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .setup(move |app| {
            let app_handle = app.handle().clone();
            tray::init(&app_handle)?;
            clicker.spawn_worker(app_handle.clone());
            spawn_global_listener(state.clone(), app_handle);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            set_active_feature,
            get_clicker_state,
            update_clicker_config,
            get_hotkey_config,
            update_hotkey_config,
            get_global_hotkey_options,
            update_global_hotkey_options,
            get_recorder_hotkey_config,
            update_recorder_hotkey_config,
            start_clicker,
            stop_clicker,
            get_recorder_state,
            start_recording,
            stop_recording,
            rename_recording,
            select_recording,
            play_recording,
            stop_playback,
            delete_recording,
            update_recording_playback_speed,
            update_recording_loop_playback
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
