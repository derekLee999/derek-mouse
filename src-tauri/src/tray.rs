use std::sync::{Mutex, OnceLock};

use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

const QUIT_MENU_ID: &str = "quit";

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TrayStatus {
    Stopped,
    Running,
    Recording,
    ReadyToRecord,
}

struct TrayIcons {
    stopped: Image<'static>,
    running: Image<'static>,
    recording: Image<'static>,
    ready_to_record: Image<'static>,
}

static TRAY: OnceLock<Mutex<Option<TrayIcon>>> = OnceLock::new();
static TRAY_STATUS: OnceLock<Mutex<Option<TrayStatus>>> = OnceLock::new();
static TRAY_ICONS: OnceLock<TrayIcons> = OnceLock::new();

pub fn init(app: &AppHandle) -> tauri::Result<()> {
    let quit = MenuItem::with_id(app, QUIT_MENU_ID, "退出程序", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&quit])?;
    let tray = TrayIconBuilder::new()
        .icon(icon_for(TrayStatus::Stopped)?)
        .tooltip("鼠标连点器")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            if event.id() == QUIT_MENU_ID {
                app.exit(0);
            }
        })
        .on_tray_icon_event(|tray, event| {
            if matches!(
                event,
                TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                }
            ) {
                show_main_window(tray.app_handle());
            }
        })
        .build(app)?;

    let storage = TRAY.get_or_init(|| Mutex::new(None));
    *storage.lock().expect("tray storage poisoned") = Some(tray);
    let status_storage = TRAY_STATUS.get_or_init(|| Mutex::new(None));
    *status_storage.lock().expect("tray status poisoned") = Some(TrayStatus::Stopped);
    Ok(())
}

pub fn set_status(status: TrayStatus) {
    let Some(status_storage) = TRAY_STATUS.get() else {
        return;
    };
    if status_storage
        .lock()
        .map(|current| *current == Some(status))
        .unwrap_or(false)
    {
        return;
    }
    let Ok(icon) = icon_for(status) else {
        return;
    };
    let Some(storage) = TRAY.get() else {
        return;
    };
    let Ok(guard) = storage.lock() else {
        return;
    };
    let Some(tray) = guard.as_ref() else {
        return;
    };

    let _ = tray.set_icon(Some(icon));
    let _ = tray.set_tooltip(Some(match status {
        TrayStatus::Stopped => "鼠标连点器 - 已停止",
        TrayStatus::Running => "鼠标连点器 - 运行中",
        TrayStatus::Recording => "鼠标连点器 - 录制中",
        TrayStatus::ReadyToRecord => "鼠标连点器 - 录制待命",
    }));

    if let Ok(mut current) = status_storage.lock() {
        *current = Some(status);
    }
}

fn icon_for(status: TrayStatus) -> tauri::Result<Image<'static>> {
    let icons = tray_icons()?;

    Ok(match status {
        TrayStatus::Stopped => icons.stopped.clone(),
        TrayStatus::Running => icons.running.clone(),
        TrayStatus::Recording => icons.recording.clone(),
        TrayStatus::ReadyToRecord => icons.ready_to_record.clone(),
    })
}

fn tray_icons() -> tauri::Result<&'static TrayIcons> {
    if let Some(icons) = TRAY_ICONS.get() {
        return Ok(icons);
    }

    let icons = TrayIcons {
        stopped: load_icon(include_bytes!("../icons/tray/stopped.png").as_slice())?,
        running: load_icon(include_bytes!("../icons/tray/running.png").as_slice())?,
        recording: load_icon(include_bytes!("../icons/tray/recording.png").as_slice())?,
        ready_to_record: load_icon(include_bytes!("../icons/tray/record.png").as_slice())?,
    };

    let _ = TRAY_ICONS.set(icons);
    Ok(TRAY_ICONS.get().expect("tray icons initialized"))
}

fn load_icon(bytes: &'static [u8]) -> tauri::Result<Image<'static>> {
    Image::from_bytes(bytes).map(Image::to_owned)
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}
