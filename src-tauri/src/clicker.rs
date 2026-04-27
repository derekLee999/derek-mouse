use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};

use crate::{
    input::{
        button_from_name, dispatch_background_click, dispatch_ldplayer_click,
        dispatch_ldplayer_click_via_shell, is_likely_ldplayer_window, is_window,
        resolve_ldplayer_target, simulate, validate_hotkey, Button, Event, EventType,
        HotkeyConfig, Key, KeyboardTracker, LdPlayerShellSession, LdPlayerTarget,
    },
    tray::{self, TrayStatus},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClickerConfig {
    pub click_button: String,
    pub interval_secs: f64,
    #[serde(default)]
    pub click_limit: u64,
    pub mode: String,
    pub hold_button: String,
    pub hotkey: HotkeyConfig,
    #[serde(default)]
    pub backend_click: bool,
    #[serde(default)]
    pub target_window_title: String,
    #[serde(default)]
    pub target_client_x: Option<i32>,
    #[serde(default)]
    pub target_client_y: Option<i32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClickerState {
    pub config: ClickerConfig,
    pub running: bool,
}

pub struct ClickerRuntime {
    config: Mutex<ClickerConfig>,
    running: AtomicBool,
    mouse_held: AtomicBool,
    keyboard: Mutex<KeyboardTracker>,
    injecting_mouse_event: AtomicBool,
    clicked_count: AtomicU64,
    last_mouse_position: Mutex<Option<(f64, f64)>>,
    target_hwnd: Mutex<Option<isize>>,
    target_ldplayer: Mutex<Option<LdPlayerTarget>>,
    ldplayer_shell: Mutex<Option<LdPlayerShellSession>>,
}

impl Default for ClickerConfig {
    fn default() -> Self {
        Self {
            click_button: "left".to_string(),
            interval_secs: 0.2,
            click_limit: 0,
            mode: "toggle".to_string(),
            hold_button: "left".to_string(),
            hotkey: HotkeyConfig::default(),
            backend_click: false,
            target_window_title: String::new(),
            target_client_x: None,
            target_client_y: None,
        }
    }
}

impl Default for ClickerRuntime {
    fn default() -> Self {
        Self {
            config: Mutex::new(ClickerConfig::default()),
            running: AtomicBool::new(false),
            mouse_held: AtomicBool::new(false),
            keyboard: Mutex::new(KeyboardTracker::default()),
            injecting_mouse_event: AtomicBool::new(false),
            clicked_count: AtomicU64::new(0),
            last_mouse_position: Mutex::new(None),
            target_hwnd: Mutex::new(None),
            target_ldplayer: Mutex::new(None),
            ldplayer_shell: Mutex::new(None),
        }
    }
}

impl ClickerRuntime {
    pub fn from_config(config: ClickerConfig) -> Self {
        let target_hwnd = resolve_target_hwnd(&config.target_window_title);
        let target_ldplayer = target_hwnd.and_then(|hwnd| {
            if is_likely_ldplayer_window(&config.target_window_title) {
                resolve_ldplayer_target(hwnd, &config.target_window_title)
            } else {
                None
            }
        });
        Self {
            config: Mutex::new(config),
            running: AtomicBool::new(false),
            mouse_held: AtomicBool::new(false),
            keyboard: Mutex::new(KeyboardTracker::default()),
            injecting_mouse_event: AtomicBool::new(false),
            clicked_count: AtomicU64::new(0),
            last_mouse_position: Mutex::new(None),
            target_hwnd: Mutex::new(target_hwnd),
            target_ldplayer: Mutex::new(target_ldplayer),
            ldplayer_shell: Mutex::new(None),
        }
    }

    pub fn config_snapshot(&self) -> Result<ClickerConfig, String> {
        self.config
            .lock()
            .map(|c| c.clone())
            .map_err(|e| e.to_string())
    }

    pub fn state(&self) -> Result<ClickerState, String> {
        Ok(ClickerState {
            config: self.config.lock().map_err(|err| err.to_string())?.clone(),
            running: self.running.load(Ordering::SeqCst),
        })
    }

    pub fn update_config(&self, config: ClickerConfig) -> Result<ClickerState, String> {
        let normalized = normalize_config(config)?;
        let should_update_hwnd = {
            let current = self.config.lock().map_err(|err| err.to_string())?;
            current.backend_click != normalized.backend_click
                || current.target_window_title != normalized.target_window_title
        };
        if should_update_hwnd {
            let new_hwnd = resolve_target_hwnd(&normalized.target_window_title);
            if let Ok(mut hwnd) = self.target_hwnd.lock() {
                *hwnd = new_hwnd;
            }
            if let Ok(mut ldplayer) = self.target_ldplayer.lock() {
                *ldplayer = new_hwnd.and_then(|hwnd| {
                    if is_likely_ldplayer_window(&normalized.target_window_title) {
                        resolve_ldplayer_target(hwnd, &normalized.target_window_title)
                    } else {
                        None
                    }
                });
            }
            if let Ok(mut shell) = self.ldplayer_shell.lock() {
                *shell = None;
            }
        }
        *self.config.lock().map_err(|err| err.to_string())? = normalized.clone();

        Ok(ClickerState {
            config: normalized,
            running: self.running.load(Ordering::SeqCst),
        })
    }

    pub fn update_hotkey(&self, mut hotkey: HotkeyConfig) -> Result<HotkeyConfig, String> {
        validate_hotkey(&mut hotkey)?;
        self.config.lock().map_err(|err| err.to_string())?.hotkey = hotkey.clone();
        Ok(hotkey)
    }

    pub fn start(&self) -> ClickerState {
        self.clicked_count.store(0, Ordering::SeqCst);
        self.running.store(true, Ordering::SeqCst);
        tray::set_status(TrayStatus::Running);

        // 启动时重新解析目标窗口句柄（窗口可能已被关闭重开）
        {
            let config = self.config.lock().expect("clicker config poisoned").clone();
            if config.backend_click && !config.target_window_title.is_empty() {
                let new_hwnd = resolve_target_hwnd(&config.target_window_title);
                if let Ok(mut hwnd) = self.target_hwnd.lock() {
                    *hwnd = new_hwnd;
                }
                if let Ok(mut ldplayer) = self.target_ldplayer.lock() {
                    *ldplayer = new_hwnd.and_then(|hwnd| {
                        if is_likely_ldplayer_window(&config.target_window_title) {
                            resolve_ldplayer_target(hwnd, &config.target_window_title)
                        } else {
                            None
                        }
                    });
                }
                if let Ok(mut shell) = self.ldplayer_shell.lock() {
                    *shell = None;
                }
            }
        }

        ClickerState {
            config: self.config.lock().expect("clicker config poisoned").clone(),
            running: true,
        }
    }

    pub fn stop(&self) -> ClickerState {
        self.running.store(false, Ordering::SeqCst);
        self.mouse_held.store(false, Ordering::SeqCst);
        self.clicked_count.store(0, Ordering::SeqCst);
        tray::set_status(TrayStatus::Stopped);
        ClickerState {
            config: self.config.lock().expect("clicker config poisoned").clone(),
            running: false,
        }
    }

    pub fn hotkey_config(&self) -> HotkeyConfig {
        self.config
            .lock()
            .map(|config| config.hotkey.clone())
            .unwrap_or_default()
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub fn spawn_worker(self: &Arc<Self>, app: tauri::AppHandle) {
        let runtime = self.clone();

        thread::spawn(move || {
            let mut next_click_at = Instant::now();
            let mut was_clicking = false;

            loop {
                if !runtime.should_click() {
                    was_clicking = false;
                    next_click_at = Instant::now();
                    thread::sleep(Duration::from_millis(5));
                    continue;
                }

                let config = match runtime.config.lock() {
                    Ok(config) => config.clone(),
                    Err(_) => {
                        runtime.running.store(false, Ordering::SeqCst);
                        runtime.mouse_held.store(false, Ordering::SeqCst);
                        continue;
                    }
                };

                let interval = Duration::from_secs_f64(config.interval_secs.max(0.01));

                if !was_clicking {
                    next_click_at = Instant::now();
                    was_clicking = true;
                }

                let now = Instant::now();
                if now < next_click_at {
                    thread::sleep(next_click_at - now);
                    continue;
                }

                if let Some(button) = button_from_name(&config.click_button) {
                    runtime.injecting_mouse_event.store(true, Ordering::SeqCst);

                    if config.backend_click {
                        // 后台点击：命中最深子窗口，并沿祖先链补发鼠标移动后再发送点击
                        let hwnd = runtime.target_hwnd.lock().ok().and_then(|h| *h);
                        if let Some(hwnd) = hwnd {
                            if is_window(hwnd) {
                                let Some(target_client_x) = config.target_client_x else {
                                    runtime.injecting_mouse_event.store(false, Ordering::SeqCst);
                                    thread::sleep(interval);
                                    continue;
                                };
                                let Some(target_client_y) = config.target_client_y else {
                                    runtime.injecting_mouse_event.store(false, Ordering::SeqCst);
                                    thread::sleep(interval);
                                    continue;
                                };
                                let ldplayer_target = runtime
                                    .target_ldplayer
                                    .lock()
                                    .ok()
                                    .and_then(|target| target.clone());
                                let dispatch_result = if let Some(target) = ldplayer_target {
                                    let shell_result = if let Ok(mut shell) = runtime.ldplayer_shell.lock() {
                                        dispatch_ldplayer_click_via_shell(
                                            &mut shell,
                                            &target,
                                            &config.click_button,
                                            hwnd,
                                            target_client_x,
                                            target_client_y,
                                        )
                                    } else {
                                        Err("Failed to lock LDPlayer shell session".to_string())
                                    };

                                    shell_result
                                        .or_else(|_| {
                                            dispatch_ldplayer_click(
                                                &target,
                                                hwnd,
                                                &config.click_button,
                                                target_client_x,
                                                target_client_y,
                                            )
                                        })
                                        .or_else(|_| {
                                            dispatch_background_click(
                                                hwnd,
                                                &config.click_button,
                                                target_client_x,
                                                target_client_y,
                                            )
                                        })
                                } else {
                                    dispatch_background_click(
                                        hwnd,
                                        &config.click_button,
                                        target_client_x,
                                        target_client_y,
                                    )
                                };
                                let _ = dispatch_result;
                            } else {
                                // 目标窗口已关闭，自动停止连点
                                runtime.running.store(false, Ordering::SeqCst);
                                runtime.mouse_held.store(false, Ordering::SeqCst);
                                runtime.clicked_count.store(0, Ordering::SeqCst);
                                tray::set_status(TrayStatus::Stopped);
                                let _ = app.emit("clicker-status", false);
                                was_clicking = false;
                                next_click_at = Instant::now();
                                runtime.injecting_mouse_event.store(false, Ordering::SeqCst);
                                continue;
                            }
                        }
                    } else {
                        // In hold mode with right-button trigger and left-button auto-click,
                        // browsers may keep right-click context behavior active and swallow
                        // synthesized left clicks. Releasing right button first improves
                        // compatibility on web pages while keeping internal hold state.
                        if config.mode == "hold"
                            && config.hold_button == "right"
                            && config.click_button == "left"
                        {
                            let _ = simulate(&EventType::ButtonRelease(Button::Right));
                            thread::sleep(Duration::from_millis(1));
                        }

                        let _ = simulate(&EventType::ButtonPress(button));
                        thread::sleep(Duration::from_millis(2));
                        let _ = simulate(&EventType::ButtonRelease(button));
                    }
                    runtime.injecting_mouse_event.store(false, Ordering::SeqCst);

                    let total_clicks = runtime.clicked_count.fetch_add(1, Ordering::SeqCst) + 1;
                    if config.mode == "toggle"
                        && config.click_limit > 0
                        && total_clicks >= config.click_limit
                    {
                        runtime.running.store(false, Ordering::SeqCst);
                        runtime.mouse_held.store(false, Ordering::SeqCst);
                        runtime.clicked_count.store(0, Ordering::SeqCst);
                        tray::set_status(TrayStatus::Stopped);
                        let _ = app.emit("clicker-status", false);
                        was_clicking = false;
                        next_click_at = Instant::now();
                        continue;
                    }
                }

                let _ = app.emit("clicker-status", runtime.running.load(Ordering::SeqCst));
                next_click_at += interval;

                let after_click = Instant::now();
                if next_click_at <= after_click {
                    next_click_at = after_click + interval;
                }
            }
        });
    }

    pub fn handle_event(
        &self,
        event: &Event,
        app: &tauri::AppHandle,
        active: bool,
        show_window_on_stop: bool,
        auto_hide_on_hotkey: bool,
    ) {
        match event.event_type {
            EventType::KeyPress(key) => {
                self.handle_key_press(app, key, active, show_window_on_stop, auto_hide_on_hotkey)
            }
            EventType::KeyRelease(key) => self.handle_key_release(key),
            EventType::ButtonPress(button) => self.handle_button_press(button, app),
            EventType::ButtonRelease(button) => self.handle_button_release(button, app),
            EventType::MouseMove { x, y } => self.update_mouse_position(x, y),
            _ => {}
        }
    }

    fn should_click(&self) -> bool {
        if !self.running.load(Ordering::SeqCst) {
            return false;
        }

        let mode = self
            .config
            .lock()
            .map(|config| config.mode.clone())
            .unwrap_or_else(|_| "toggle".to_string());

        mode != "hold" || self.mouse_held.load(Ordering::SeqCst)
    }

    fn handle_key_press(
        &self,
        app: &tauri::AppHandle,
        key: Key,
        active: bool,
        show_window_on_stop: bool,
        auto_hide_on_hotkey: bool,
    ) {
        let hotkey = self.hotkey_config();
        let triggered = {
            let Ok(mut keyboard) = self.keyboard.lock() else {
                return;
            };
            let triggered = keyboard.handle_press(key, &hotkey);
            if triggered {
                keyboard.clear_hotkey_down();
            }
            triggered
        };

        if !triggered || !active {
            return;
        }

        let was_running = self.running.load(Ordering::SeqCst);
        if auto_hide_on_hotkey {
            hide_main_window(app);
        }
        let next_running = !self.running.load(Ordering::SeqCst);
        self.running.store(next_running, Ordering::SeqCst);
        if next_running {
            self.clicked_count.store(0, Ordering::SeqCst);
        }
        if !next_running {
            self.mouse_held.store(false, Ordering::SeqCst);
            self.clicked_count.store(0, Ordering::SeqCst);
        }
        tray::set_status(if next_running {
            TrayStatus::Running
        } else {
            TrayStatus::Stopped
        });
        if was_running && show_window_on_stop {
            show_main_window(app);
        }
        let _ = app.emit("clicker-status", next_running);
    }

    fn handle_key_release(&self, key: Key) {
        let hotkey = self.hotkey_config();
        if let Ok(mut keyboard) = self.keyboard.lock() {
            keyboard.handle_release(key, &hotkey);
        }
    }

    fn handle_button_press(&self, button: Button, app: &tauri::AppHandle) {
        if self.injecting_mouse_event.load(Ordering::SeqCst) {
            return;
        }

        let config = match self.config.lock() {
            Ok(config) => config.clone(),
            Err(_) => return,
        };

        if config.mode != "hold" || !self.running.load(Ordering::SeqCst) {
            return;
        }

        if self.is_mouse_inside_app_window(app) {
            return;
        }

        if button_from_name(&config.hold_button) == Some(button) {
            self.mouse_held.store(true, Ordering::SeqCst);
        }
    }

    fn handle_button_release(&self, button: Button, app: &tauri::AppHandle) {
        if self.injecting_mouse_event.load(Ordering::SeqCst) {
            return;
        }

        let config = match self.config.lock() {
            Ok(config) => config.clone(),
            Err(_) => return,
        };

        if config.mode != "hold" {
            return;
        }

        if self.is_mouse_inside_app_window(app) {
            return;
        }

        if button_from_name(&config.hold_button) == Some(button) {
            self.mouse_held.store(false, Ordering::SeqCst);
        }
    }

    fn update_mouse_position(&self, x: f64, y: f64) {
        if let Ok(mut position) = self.last_mouse_position.lock() {
            *position = Some((x, y));
        }
    }

    fn is_mouse_inside_app_window(&self, app: &tauri::AppHandle) -> bool {
        let Some((mouse_x, mouse_y)) = self
            .last_mouse_position
            .lock()
            .ok()
            .and_then(|position| *position)
        else {
            return false;
        };

        let Some(window) = app.get_webview_window("main") else {
            return false;
        };

        let Ok(position) = window.outer_position() else {
            return false;
        };
        let Ok(size) = window.outer_size() else {
            return false;
        };

        let left = f64::from(position.x);
        let top = f64::from(position.y);
        let right = left + f64::from(size.width);
        let bottom = top + f64::from(size.height);

        mouse_x >= left && mouse_x <= right && mouse_y >= top && mouse_y <= bottom
    }
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

fn normalize_config(mut config: ClickerConfig) -> Result<ClickerConfig, String> {
    if button_from_name(&config.click_button).is_none() {
        return Err("Unsupported click button".to_string());
    }

    if button_from_name(&config.hold_button).is_none() {
        return Err("Unsupported hold button".to_string());
    }

    config.interval_secs = if config.interval_secs.is_finite() {
        (config.interval_secs * 100.0).round() / 100.0
    } else {
        0.2
    };

    if config.click_limit > 10_000_000 {
        config.click_limit = 10_000_000;
    }

    if config.interval_secs <= 0.0 {
        config.interval_secs = 0.01;
    }

    config.mode = config.mode.to_lowercase();
    if config.mode != "toggle" && config.mode != "hold" {
        return Err("Unsupported click mode".to_string());
    }

    config.click_button = config.click_button.to_lowercase();
    config.hold_button = config.hold_button.to_lowercase();
    config.target_window_title = config.target_window_title.trim().to_string();

    if config.mode == "hold" && config.hold_button == "middle" {
        config.hold_button = "left".to_string();
    }

    if config.target_window_title.is_empty() {
        config.backend_click = false;
        config.target_client_x = None;
        config.target_client_y = None;
    }

    if config.target_client_x.is_none() || config.target_client_y.is_none() {
        config.backend_click = false;
        config.target_client_x = None;
        config.target_client_y = None;
    }

    validate_hotkey(&mut config.hotkey)?;

    Ok(config)
}


fn resolve_target_hwnd(title: &str) -> Option<isize> {
    if title.is_empty() {
        return None;
    }
    let windows = crate::input::enum_visible_windows();
    // 优先精确匹配
    if let Some((_, hwnd)) = windows.iter().find(|(t, _)| t == title) {
        return Some(*hwnd);
    }
    // 精确匹配失败时尝试包含匹配（网页标题可能变化）
    if let Some((_, hwnd)) = windows.iter().find(|(t, _)| t.contains(title) || title.contains(t)) {
        return Some(*hwnd);
    }
    None
}
