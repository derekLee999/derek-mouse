use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, OnceLock,
    },
    thread,
    time::Duration,
};

use tauri::{
    image::Image,
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};
use tauri_plugin_notification::NotificationExt;

const QUIT_MENU_ID: &str = "quit";

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum TrayStatus {
    Stopped,
    Running,
    Recording,
}

static TRAY: OnceLock<Mutex<Option<TrayIcon>>> = OnceLock::new();
static NOTIFICATION_SEQUENCE: AtomicU64 = AtomicU64::new(0);

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
    Ok(())
}

pub fn set_status(status: TrayStatus) {
    let Some(storage) = TRAY.get() else {
        return;
    };
    let Ok(guard) = storage.lock() else {
        return;
    };
    let Some(tray) = guard.as_ref() else {
        return;
    };
    let Ok(icon) = icon_for(status) else {
        return;
    };

    let _ = tray.set_icon(Some(icon));
    let _ = tray.set_tooltip(Some(match status {
        TrayStatus::Stopped => "鼠标连点器 - 已停止",
        TrayStatus::Running => "鼠标连点器 - 运行中",
        TrayStatus::Recording => "鼠标连点器 - 录制中",
    }));
}

pub fn notify_global_hotkey_state(app: &AppHandle, started: bool) {
    let sequence = NOTIFICATION_SEQUENCE.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();

    thread::spawn(move || {
        thread::sleep(Duration::from_millis(1200));
        if NOTIFICATION_SEQUENCE.load(Ordering::SeqCst) != sequence {
            return;
        }

        show_global_hotkey_notification(&app, started);
    });
}

fn show_global_hotkey_notification(app: &AppHandle, started: bool) {
    let _ = app
        .notification()
        .builder()
        .title("鼠标连点器")
        .body(if started { "已启动" } else { "已停止" })
        .icon(notification_icon_path())
        .show();
}

fn icon_for(status: TrayStatus) -> tauri::Result<Image<'static>> {
    let bytes = match status {
        TrayStatus::Stopped => include_bytes!("../icons/tray/stopped.png").as_slice(),
        TrayStatus::Running => include_bytes!("../icons/tray/running.png").as_slice(),
        TrayStatus::Recording => include_bytes!("../icons/tray/recording.png").as_slice(),
    };

    Image::from_bytes(bytes).map(Image::to_owned)
}

fn show_main_window(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.set_focus();
    }
}

fn notification_icon_path() -> String {
    format!(r"{}\icons\icon.ico", env!("CARGO_MANIFEST_DIR"))
}
