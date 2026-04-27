mod clicker;
mod config;
mod input;
mod mouse_macro;
mod ocr_engine;
mod recorder;
mod tray;

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::Duration,
};

use clicker::{ClickerConfig, ClickerRuntime};
use config::{load_config, save_config, AppConfig};
use input::{listen, ActiveFeature, Event, HotkeyConfig};
use mouse_macro::{
    CaptureImageResult, CreateMacroRequest, FindColorRequest, FindColorResult, FindImageRegion,
    FindImageRequest, FindImageResult, MacroDetail, MouseMacroRuntime, RenameMacroRequest,
    UpdateMacroLoopPlaybackRequest, UpdateMacroPlaybackSpeedRequest, UpdateMacroRequest,
};
use recorder::{
    RecorderRuntime, RenameRecordingRequest, SaveEditedRecordingRequest,
    UpdateRecordingLoopPlaybackRequest, UpdateRecordingPlaybackSpeedRequest,
};
use reqwest::header::{ACCEPT, USER_AGENT};
use semver::Version;
use tauri::{Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;
use windows::Win32::{
    Graphics::Gdi::{GetDC, GetPixel, ReleaseDC},
    UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    },
};

const GITHUB_LATEST_RELEASE_API: &str =
    "https://api.github.com/repos/derekLee999/derek-mouse/releases/latest";
const GITHUB_UPDATER_ENDPOINT: &str =
    "https://github.com/derekLee999/derek-mouse/releases/latest/download/latest.json";
const UPDATER_PUBLIC_KEY: &str = include_str!("../updater.pubkey");

struct AppState {
    active_feature: Mutex<ActiveFeature>,
    clicker: Arc<ClickerRuntime>,
    recorder: Arc<RecorderRuntime>,
    mouse_macro: Arc<MouseMacroRuntime>,
    coordinate_pick_session: Mutex<Option<CoordinatePickSession>>,
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

    fn sync_tray_status(&self) {
        let active_feature = self
            .active_feature
            .lock()
            .map(|feature| *feature)
            .unwrap_or(ActiveFeature::Clicker);

        let status = if self.recorder.is_recording() {
            tray::TrayStatus::Recording
        } else if self.recorder.is_playing()
            || self.mouse_macro.is_playing()
            || self.clicker.is_running()
        {
            tray::TrayStatus::Running
        } else if active_feature == ActiveFeature::Recorder {
            tray::TrayStatus::ReadyToRecord
        } else {
            tray::TrayStatus::Stopped
        };

        tray::set_status(status);
    }
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct GlobalHotkeyOptions {
    show_window_on_stop: bool,
    auto_hide_on_hotkey: bool,
}

#[derive(Debug, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PickedCoordinate {
    x: i32,
    y: i32,
}

#[derive(Debug, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PickedRegion {
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
}

#[derive(Debug, Clone)]
struct CoordinatePickSession {
    target_label: String,
    left: i32,
    top: i32,
    width: u32,
    height: u32,
}

#[derive(Debug, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CoordinatePickSnapshotMeta {
    left: i32,
    top: i32,
    width: u32,
    height: u32,
}

#[derive(Debug, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct CoordinatePickSnapshotPayload {
    target_label: String,
    left: i32,
    top: i32,
    width: u32,
    height: u32,
}

#[derive(Debug, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct PixelColor {
    r: u8,
    g: u8,
    b: u8,
    hex: String,
}

#[derive(Debug, serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct AppUpdateInfo {
    current_version: String,
    available: bool,
    install_ready: bool,
    latest_version: Option<String>,
    latest_tag: Option<String>,
    notes: Option<String>,
    published_at: Option<String>,
    release_url: Option<String>,
    install_hint: Option<String>,
}

#[derive(Debug, serde::Deserialize)]
struct GithubReleaseInfo {
    tag_name: String,
    #[serde(default)]
    body: String,
    #[serde(default)]
    published_at: Option<String>,
    html_url: String,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct FinishMouseCoordinatePickRequest {
    x: i32,
    y: i32,
}

#[derive(Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct FinishMouseRegionPickRequest {
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct FinishMouseColorPickRequest {
    color: String,
}

fn normalize_release_version(version: &str) -> &str {
    version.trim().trim_start_matches(['v', 'V'])
}

async fn fetch_latest_github_release() -> Result<GithubReleaseInfo, String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(|err| err.to_string())?;

    client
        .get(GITHUB_LATEST_RELEASE_API)
        .header(USER_AGENT, "derek-mouse-updater")
        .header(ACCEPT, "application/vnd.github+json")
        .send()
        .await
        .map_err(|err| format!("Could not fetch latest GitHub release: {err}"))?
        .error_for_status()
        .map_err(|err| format!("Could not query latest GitHub release: {err}"))?
        .json::<GithubReleaseInfo>()
        .await
        .map_err(|err| format!("Could not decode latest GitHub release: {err}"))
}

async fn check_updater_ready(app: &tauri::AppHandle) -> Result<bool, String> {
    let update_endpoint =
        reqwest::Url::parse(GITHUB_UPDATER_ENDPOINT).map_err(|err| err.to_string())?;

    let updater = app
        .updater_builder()
        .pubkey(UPDATER_PUBLIC_KEY.trim())
        .timeout(Duration::from_secs(20))
        .endpoints(vec![update_endpoint])
        .map_err(|err| err.to_string())?
        .build()
        .map_err(|err| err.to_string())?;

    updater
        .check()
        .await
        .map(|update| update.is_some())
        .map_err(|err| err.to_string())
}

#[tauri::command]
fn set_active_feature(
    feature: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<(), String> {
    *state.active_feature.lock().map_err(|err| err.to_string())? =
        ActiveFeature::from_name(&feature);
    state.sync_tray_status();
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
async fn check_app_update(app: tauri::AppHandle) -> Result<AppUpdateInfo, String> {
    let current_version = app.package_info().version.to_string();
    let release = fetch_latest_github_release().await?;
    let current = Version::parse(&current_version).map_err(|err| err.to_string())?;
    let latest_version = normalize_release_version(&release.tag_name).to_string();
    let latest = Version::parse(&latest_version)
        .map_err(|err| format!("Invalid GitHub release tag '{}': {err}", release.tag_name))?;
    let available = latest > current;

    let mut install_ready = false;
    let mut install_hint = None;

    if available {
        match check_updater_ready(&app).await {
            Ok(true) => {
                install_ready = true;
            }
            Ok(false) => {
                install_hint = Some(
                    "已检测到新版本，但当前 Release 尚未提供可安装的 updater 清单。".to_string(),
                );
            }
            Err(error) => {
                install_hint = Some(format!("自动更新资源不可用：{error}"));
            }
        }
    }

    Ok(AppUpdateInfo {
        current_version,
        available,
        install_ready,
        latest_version: Some(latest_version),
        latest_tag: Some(release.tag_name),
        notes: if release.body.trim().is_empty() {
            None
        } else {
            Some(release.body)
        },
        published_at: release.published_at,
        release_url: Some(release.html_url),
        install_hint,
    })
}

#[tauri::command]
async fn install_app_update(app: tauri::AppHandle) -> Result<(), String> {
    let update_endpoint =
        reqwest::Url::parse(GITHUB_UPDATER_ENDPOINT).map_err(|err| err.to_string())?;

    let updater = app
        .updater_builder()
        .pubkey(UPDATER_PUBLIC_KEY.trim())
        .timeout(Duration::from_secs(30))
        .endpoints(vec![update_endpoint])
        .map_err(|err| err.to_string())?
        .build()
        .map_err(|err| err.to_string())?;

    let Some(update) = updater.check().await.map_err(|err| err.to_string())? else {
        return Err("当前没有可安装的新版本。".to_string());
    };

    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|err| err.to_string())?;

    #[cfg(not(target_os = "windows"))]
    app.restart();

    Ok(())
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
    let result = state.clicker.start();
    state.sync_tray_status();
    Ok(result)
}

#[tauri::command]
fn stop_clicker(state: tauri::State<'_, Arc<AppState>>) -> Result<clicker::ClickerState, String> {
    let result = state.clicker.stop();
    state.sync_tray_status();
    Ok(result)
}

#[tauri::command]
fn get_recorder_state(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<recorder::RecorderState, String> {
    state.recorder.state()
}

#[tauri::command]
fn get_recording_detail(
    id: u64,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<recorder::RecordingDetail, String> {
    state.recorder.recording_detail(id)
}

#[tauri::command]
fn start_recording(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<recorder::RecorderState, String> {
    let result = state.recorder.start_recording();
    state.sync_tray_status();
    result
}

#[tauri::command]
fn stop_recording(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<recorder::RecorderState, String> {
    let result = state.recorder.stop_recording();
    state.sync_tray_status();
    result
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
    let result = state.recorder.play_recording(id, app, false);
    state.sync_tray_status();
    result
}

#[tauri::command]
fn stop_playback(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<recorder::RecorderState, String> {
    let result = state.recorder.stop_playback();
    state.sync_tray_status();
    result
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
fn save_edited_recording(
    request: SaveEditedRecordingRequest,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<recorder::RecorderState, String> {
    state.recorder.save_edited_recording(request)
}

#[tauri::command]
fn delete_recording(
    id: u64,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<recorder::RecorderState, String> {
    state.recorder.delete_recording(id)
}

#[tauri::command]
fn get_mouse_macro_state(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<mouse_macro::MacroState, String> {
    state.mouse_macro.state()
}

#[tauri::command]
fn get_mouse_macro_detail(
    id: u64,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<MacroDetail, String> {
    state.mouse_macro.macro_detail(id)
}

#[tauri::command]
fn create_mouse_macro(
    request: CreateMacroRequest,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<mouse_macro::MacroState, String> {
    state.mouse_macro.create_macro(request)
}

#[tauri::command]
fn update_mouse_macro(
    request: UpdateMacroRequest,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<mouse_macro::MacroState, String> {
    state.mouse_macro.update_macro(request)
}

#[tauri::command]
fn rename_mouse_macro(
    request: RenameMacroRequest,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<mouse_macro::MacroState, String> {
    state.mouse_macro.rename_macro(request)
}

#[tauri::command]
fn select_mouse_macro(
    id: u64,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<mouse_macro::MacroState, String> {
    state.mouse_macro.select_macro(id)
}

#[tauri::command]
fn delete_mouse_macro(
    id: u64,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<mouse_macro::MacroState, String> {
    state.mouse_macro.delete_macro(id)
}

#[tauri::command]
fn update_mouse_macro_playback_speed(
    request: UpdateMacroPlaybackSpeedRequest,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<mouse_macro::MacroState, String> {
    state.mouse_macro.update_macro_playback_speed(request)
}

#[tauri::command]
fn update_mouse_macro_loop_playback(
    request: UpdateMacroLoopPlaybackRequest,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<mouse_macro::MacroState, String> {
    state.mouse_macro.update_macro_loop_playback(request)
}

#[tauri::command]
fn play_mouse_macro(
    id: u64,
    state: tauri::State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) -> Result<mouse_macro::MacroState, String> {
    let result = state.mouse_macro.play_macro(id, app, false);
    state.sync_tray_status();
    result
}

#[tauri::command]
fn stop_mouse_macro(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<mouse_macro::MacroState, String> {
    let result = state.mouse_macro.stop_playback();
    state.sync_tray_status();
    result
}

#[tauri::command]
fn test_mouse_macro_find_image(
    request: FindImageRequest,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<FindImageResult, String> {
    state.mouse_macro.test_find_image(request)
}

#[tauri::command]
fn test_mouse_macro_find_color(
    request: FindColorRequest,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<FindColorResult, String> {
    state.mouse_macro.test_find_color(request)
}

#[tauri::command]
fn test_mouse_macro_find_text(
    request: mouse_macro::FindTextRequest,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<mouse_macro::FindTextResult, String> {
    state.mouse_macro.test_find_text(request)
}

#[tauri::command]
fn check_ocr_engine_installed() -> Result<bool, String> {
    Ok(ocr_engine::is_installed())
}

#[tauri::command]
async fn install_ocr_engine(app: tauri::AppHandle) -> Result<(), String> {
    let app_clone = app.clone();
    ocr_engine::download_and_install(move |progress| {
        let _ = app_clone.emit("ocr-engine-install-progress", progress);
    })
    .await
}

#[tauri::command]
fn capture_mouse_macro_region_image(
    region: FindImageRegion,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<CaptureImageResult, String> {
    state.mouse_macro.capture_region_image(region)
}

#[tauri::command]
fn get_mouse_macro_screen_bounds() -> Result<CoordinatePickSnapshotMeta, String> {
    let left = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let top = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };

    if width <= 0 || height <= 0 {
        return Err("Could not determine screen size".to_string());
    }

    Ok(CoordinatePickSnapshotMeta {
        left,
        top,
        width: width as u32,
        height: height as u32,
    })
}

#[tauri::command]
fn start_mouse_coordinate_pick(
    window_label: String,
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<CoordinatePickSnapshotMeta, String> {
    clear_coordinate_pick_session(&state)?;

    let left = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let top = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
    let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };

    if width <= 0 || height <= 0 {
        return Err("Could not determine screen size".to_string());
    }

    let meta = CoordinatePickSnapshotMeta {
        left,
        top,
        width: width as u32,
        height: height as u32,
    };

    *state
        .coordinate_pick_session
        .lock()
        .map_err(|err| err.to_string())? = Some(CoordinatePickSession {
        target_label: window_label,
        left,
        top,
        width: width as u32,
        height: height as u32,
    });

    Ok(meta)
}

#[tauri::command]
fn get_mouse_coordinate_pick_snapshot(
    state: tauri::State<'_, Arc<AppState>>,
) -> Result<CoordinatePickSnapshotPayload, String> {
    let session = state
        .coordinate_pick_session
        .lock()
        .map_err(|err| err.to_string())?
        .clone()
        .ok_or_else(|| "Coordinate picker is not active".to_string())?;

    Ok(CoordinatePickSnapshotPayload {
        target_label: session.target_label,
        left: session.left,
        top: session.top,
        width: session.width,
        height: session.height,
    })
}

#[tauri::command]
fn finish_mouse_coordinate_pick(
    request: FinishMouseCoordinatePickRequest,
    state: tauri::State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let session = state
        .coordinate_pick_session
        .lock()
        .map_err(|err| err.to_string())?
        .take()
        .ok_or_else(|| "Coordinate picker is not active".to_string())?;

    app.emit_to(
        session.target_label,
        "mouse-coordinate-picked",
        PickedCoordinate {
            x: request.x,
            y: request.y,
        },
    )
    .map_err(|err| err.to_string())
}

#[tauri::command]
fn finish_mouse_region_pick(
    request: FinishMouseRegionPickRequest,
    state: tauri::State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let session = state
        .coordinate_pick_session
        .lock()
        .map_err(|err| err.to_string())?
        .take()
        .ok_or_else(|| "Coordinate picker is not active".to_string())?;

    app.emit_to(
        session.target_label,
        "mouse-region-picked",
        PickedRegion {
            x1: request.x1,
            y1: request.y1,
            x2: request.x2,
            y2: request.y2,
        },
    )
    .map_err(|err| err.to_string())
}

#[tauri::command]
fn get_pixel_color(x: i32, y: i32) -> Result<PixelColor, String> {
    unsafe {
        let hdc = GetDC(None);
        if hdc.is_invalid() {
            return Err("Could not get screen DC".to_string());
        }
        let color = GetPixel(hdc, x, y);
        let _ = ReleaseDC(None, hdc);

        if color.0 == 0xFFFFFFFF {
            return Err("Could not read pixel color".to_string());
        }

        let color_val = color.0;
        let r = (color_val & 0xFF) as u8;
        let g = ((color_val >> 8) & 0xFF) as u8;
        let b = ((color_val >> 16) & 0xFF) as u8;

        Ok(PixelColor {
            r,
            g,
            b,
            hex: format!("#{:02X}{:02X}{:02X}", r, g, b),
        })
    }
}

#[tauri::command]
fn finish_mouse_color_pick(
    request: FinishMouseColorPickRequest,
    state: tauri::State<'_, Arc<AppState>>,
    app: tauri::AppHandle,
) -> Result<(), String> {
    let session = state
        .coordinate_pick_session
        .lock()
        .map_err(|err| err.to_string())?
        .take()
        .ok_or_else(|| "Coordinate picker is not active".to_string())?;

    app.emit_to(
        session.target_label,
        "mouse-color-picked",
        request,
    )
    .map_err(|err| err.to_string())
}

#[tauri::command]
fn cancel_mouse_coordinate_pick(state: tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    clear_coordinate_pick_session(&state)
}

fn clear_coordinate_pick_session(state: &tauri::State<'_, Arc<AppState>>) -> Result<(), String> {
    let _ = state
        .coordinate_pick_session
        .lock()
        .map_err(|err| err.to_string())?
        .take();
    Ok(())
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
    state.mouse_macro.handle_event(
        &event,
        app,
        &hotkey,
        active_feature == ActiveFeature::MouseMacro,
        show_window_on_global_hotkey_stop,
        auto_hide_on_hotkey,
    );
    state.sync_tray_status();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let config = load_config();

    let clicker = Arc::new(ClickerRuntime::from_config(config.clicker));
    let recorder = Arc::new(RecorderRuntime::new_with_hotkey(config.record_hotkey));
    let mouse_macro = Arc::new(MouseMacroRuntime::new());
    let state = Arc::new(AppState {
        active_feature: Mutex::new(ActiveFeature::Clicker),
        clicker: clicker.clone(),
        recorder: recorder.clone(),
        mouse_macro: mouse_macro.clone(),
        coordinate_pick_session: Mutex::new(None),
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
        .plugin(tauri_plugin_updater::Builder::new().pubkey(UPDATER_PUBLIC_KEY.trim()).build())
        .setup(move |app| {
            let app_handle = app.handle().clone();
            tray::init(&app_handle)?;
            state.sync_tray_status();
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
            check_app_update,
            install_app_update,
            get_recorder_hotkey_config,
            update_recorder_hotkey_config,
            start_clicker,
            stop_clicker,
            get_recorder_state,
            get_recording_detail,
            start_recording,
            stop_recording,
            rename_recording,
            select_recording,
            play_recording,
            stop_playback,
            delete_recording,
            update_recording_playback_speed,
            update_recording_loop_playback,
            save_edited_recording,
            get_mouse_macro_state,
            get_mouse_macro_detail,
            create_mouse_macro,
            update_mouse_macro,
            rename_mouse_macro,
            select_mouse_macro,
            delete_mouse_macro,
            update_mouse_macro_playback_speed,
            update_mouse_macro_loop_playback,
            play_mouse_macro,
            stop_mouse_macro,
            test_mouse_macro_find_image,
            test_mouse_macro_find_color,
            test_mouse_macro_find_text,
            check_ocr_engine_installed,
            install_ocr_engine,
            capture_mouse_macro_region_image,
            get_mouse_macro_screen_bounds,
            start_mouse_coordinate_pick,
            get_mouse_coordinate_pick_snapshot,
            finish_mouse_coordinate_pick,
            finish_mouse_region_pick,
            get_pixel_color,
            finish_mouse_color_pick,
            cancel_mouse_coordinate_pick
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
