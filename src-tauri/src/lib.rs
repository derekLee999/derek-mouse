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
use input::{ActiveFeature, HotkeyConfig};
use rdev::{listen, Event};
use recorder::{RecorderRuntime, RenameRecordingRequest};
use tauri::{Emitter, Manager};

struct AppState {
    active_feature: Mutex<ActiveFeature>,
    clicker: Arc<ClickerRuntime>,
    recorder: Arc<RecorderRuntime>,
    show_window_on_global_hotkey_stop: AtomicBool,
    playback_speed: Mutex<f64>,
    loop_playback: AtomicBool,
}

impl AppState {
    fn persist_config(&self) {
        let clicker_config = self
            .clicker
            .config_snapshot()
            .unwrap_or_default();
        let record_hotkey = self.recorder.record_hotkey_config();
        let show_window_on_stop = self.show_window_on_global_hotkey_stop.load(Ordering::SeqCst);
        let playback_speed = self.playback_speed.lock().map(|s| *s).unwrap_or(1.0);

        let config = AppConfig {
            clicker: clicker_config,
            record_hotkey,
            show_window_on_stop,
            playback_speed,
            loop_playback: self.loop_playback.load(Ordering::SeqCst),
        };
        save_config(&config);
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct GlobalHotkeyOptions {
    show_window_on_stop: bool,
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
    let speed = *state.playback_speed.lock().map_err(|e| e.to_string())?;
    let loop_mode = state.loop_playback.load(Ordering::SeqCst);
    state.recorder.play_recording(id, app, false, speed, loop_mode)
}

#[tauri::command]
fn get_playback_speed(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<f64, String> {
    Ok(*state.playback_speed.lock().map_err(|e| e.to_string())?)
}

#[tauri::command]
fn set_playback_speed(
    speed: f64,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<f64, String> {
    let speed = speed.clamp(0.5, 10.0);
    *state.playback_speed.lock().map_err(|e| e.to_string())? = speed;
    state.persist_config();
    Ok(speed)
}

#[tauri::command]
fn stop_playback(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<recorder::RecorderState, String> {
    state.recorder.stop_playback()
}

#[tauri::command]
fn get_loop_playback(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<bool, String> {
    Ok(state.loop_playback.load(Ordering::SeqCst))
}

#[tauri::command]
fn set_loop_playback(
    value: bool,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<bool, String> {
    state.loop_playback.store(value, Ordering::SeqCst);
    state.persist_config();
    Ok(value)
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
            let _ = app.emit("input-listener-error", format!("{err:?}"));
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

    let playback_speed = state
        .playback_speed
        .lock()
        .map(|s| *s)
        .unwrap_or(1.0);

    state.clicker.handle_event(
        &event,
        app,
        active_feature == ActiveFeature::Clicker,
        show_window_on_global_hotkey_stop,
    );
    state.recorder.handle_event(
        &event,
        app,
        &hotkey,
        active_feature == ActiveFeature::Recorder,
        show_window_on_global_hotkey_stop,
        playback_speed,
        state.loop_playback.load(Ordering::SeqCst),
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
        playback_speed: Mutex::new(config.playback_speed),
        loop_playback: AtomicBool::new(config.loop_playback),
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
            get_playback_speed,
            set_playback_speed,
            get_loop_playback,
            set_loop_playback
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
