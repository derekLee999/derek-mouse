use std::{
    fs,
    io::Cursor,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use base64::{engine::general_purpose, Engine as _};
use image::{
    imageops::{self, FilterType},
    DynamicImage, GenericImage, GrayImage, ImageFormat, RgbaImage,
};
// OpenCV FFI: 高度优化的 SIMD 模板匹配
#[repr(C)]
struct CvMatchResult {
    score: f64,
    x: i32,
    y: i32,
    found: i32,
}

extern "C" {
    fn match_template_opencv(
        search_data: *const u8,
        search_w: i32,
        search_h: i32,
        template_data: *const u8,
        template_w: i32,
        template_h: i32,
        threshold: f64,
    ) -> CvMatchResult;
}
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};
use xcap::Monitor;

use crate::{
    input::{
        button_from_name, key_from_name, simulate, Button, Event, EventType, HotkeyConfig, Key,
        KeyboardTracker,
    },
    ocr_engine::{image_to_base64_png, OcrEngine},
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
    MouseClick {
        button: String,
    },
    MouseDoubleClick {
        button: String,
    },
    MouseDown {
        button: String,
    },
    MouseUp {
        button: String,
    },
    MouseMove {
        x: u32,
        y: u32,
    },
    KeyClick {
        key: String,
    },
    KeyDown {
        key: String,
    },
    KeyUp {
        key: String,
    },
    Delay {
        ms: u64,
    },
    FindImage {
        region: FindImageRegion,
        #[serde(rename = "imageData", alias = "image_data")]
        image_data: String,
        threshold: f64,
        scale: f64,
        action: String,
        #[serde(rename = "waitUntilFound", alias = "wait_until_found")]
        wait_until_found: bool,
        #[serde(rename = "offsetX", alias = "offset_x")]
        offset_x: i32,
        #[serde(rename = "offsetY", alias = "offset_y")]
        offset_y: i32,
    },
    FindColor {
        region: FindImageRegion,
        color: String,
        threshold: u8,
        action: String,
        #[serde(rename = "waitUntilFound", alias = "wait_until_found")]
        wait_until_found: bool,
        #[serde(rename = "offsetX", alias = "offset_x")]
        offset_x: i32,
        #[serde(rename = "offsetY", alias = "offset_y")]
        offset_y: i32,
    },
    FindText {
        region: FindImageRegion,
        text: String,
        action: String,
        #[serde(rename = "waitUntilFound", alias = "wait_until_found")]
        wait_until_found: bool,
        #[serde(rename = "offsetX", alias = "offset_x")]
        offset_x: i32,
        #[serde(rename = "offsetY", alias = "offset_y")]
        offset_y: i32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindImageRegion {
    pub x1: i32,
    pub y1: i32,
    pub x2: i32,
    pub y2: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindImageRequest {
    pub region: FindImageRegion,
    pub image_data: String,
    pub threshold: f64,
    pub scale: f64,
    pub action: String,
    pub wait_until_found: bool,
    pub offset_x: i32,
    pub offset_y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindColorRequest {
    pub region: FindImageRegion,
    pub color: String,
    pub threshold: u8,
    pub action: String,
    pub wait_until_found: bool,
    pub offset_x: i32,
    pub offset_y: i32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindImageResult {
    pub found: bool,
    pub score: f64,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindColorResult {
    pub found: bool,
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FindTextRequest {
    pub region: FindImageRegion,
    pub text: String,
    pub action: String,
    pub wait_until_found: bool,
    pub offset_x: i32,
    pub offset_y: i32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindTextResult {
    pub found: bool,
    pub x: i32,
    pub y: i32,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureImageResult {
    pub data_url: String,
    pub width: u32,
    pub height: u32,
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
    ocr_engine: Arc<Mutex<Option<OcrEngine>>>,
}

impl MouseMacroRuntime {
    pub fn new() -> Self {
        let (macros, next_id) = load_macros();
        let first_id = macros.first().map(|item| item.id);
        let ocr = OcrEngine::new().ok();
        Self {
            macros: Mutex::new(macros),
            selected_id: Mutex::new(first_id),
            keyboard: Mutex::new(KeyboardTracker::default()),
            playing: Arc::new(AtomicBool::new(false)),
            injecting_event: Arc::new(AtomicBool::new(false)),
            suppress_playback_hotkey_release: AtomicBool::new(false),
            next_id: Mutex::new(next_id),
            ocr_engine: Arc::new(Mutex::new(ocr)),
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
        let ocr_engine = self.ocr_engine.clone();

        thread::spawn(move || {
            let speed = scheme.playback_speed.max(0.1);
            loop {
                for (i, event) in scheme.events.iter().enumerate() {
                    if !playback_flag.load(Ordering::SeqCst) {
                        break;
                    }

                    // mouseMove 后的 mouseClick / mouseDoubleClick 前默认插入 10ms 延迟
                    if i > 0 {
                        if let (
                            MacroEvent::MouseMove { .. },
                            MacroEvent::MouseClick { .. } | MacroEvent::MouseDoubleClick { .. },
                        ) = (&scheme.events[i - 1], event)
                        {
                            sleep_adjusted(10, speed);
                        }
                    }

                    injecting_flag.store(true, Ordering::SeqCst);
                    let event_result = play_macro_event(event, speed, &playback_flag, &ocr_engine);
                    injecting_flag.store(false, Ordering::SeqCst);
                    if event_result.is_err() {
                        playback_flag.store(false, Ordering::SeqCst);
                        break;
                    }
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

    pub fn test_find_image(&self, request: FindImageRequest) -> Result<FindImageResult, String> {
        validate_find_image_request(&request)?;
        find_image_once(&request)
    }

    pub fn test_find_color(&self, request: FindColorRequest) -> Result<FindColorResult, String> {
        validate_find_color_request(&request)?;
        find_color_once(&request)
    }

    pub fn test_find_text(&self, request: FindTextRequest) -> Result<FindTextResult, String> {
        validate_find_text_request(&request)?;
        let mut engine = self.ocr_engine.lock().map_err(|e| e.to_string())?;
        if engine.is_none() {
            *engine = Some(OcrEngine::new()?);
        }
        let engine = engine.as_ref().unwrap();
        find_text_once(&request, engine)
    }

    pub fn capture_region_image(
        &self,
        region: FindImageRegion,
    ) -> Result<CaptureImageResult, String> {
        let region = normalize_region(&region)?;
        let image = capture_region(&region)?;
        let width = image.width();
        let height = image.height();
        let mut data = Vec::new();
        DynamicImage::ImageRgba8(image)
            .write_to(&mut Cursor::new(&mut data), ImageFormat::Png)
            .map_err(|err| err.to_string())?;

        Ok(CaptureImageResult {
            data_url: format!(
                "data:image/png;base64,{}",
                general_purpose::STANDARD.encode(data)
            ),
            width,
            height,
        })
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

fn play_macro_event(
    event: &MacroEvent,
    speed: f64,
    playback_flag: &AtomicBool,
    ocr_engine: &std::sync::Mutex<Option<OcrEngine>>,
) -> Result<(), String> {
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
        MacroEvent::FindImage {
            region,
            image_data,
            threshold,
            scale,
            action,
            wait_until_found,
            offset_x,
            offset_y,
        } => play_find_image_event(
            FindImageRequest {
                region: region.clone(),
                image_data: image_data.clone(),
                threshold: *threshold,
                scale: *scale,
                action: action.clone(),
                wait_until_found: *wait_until_found,
                offset_x: *offset_x,
                offset_y: *offset_y,
            },
            playback_flag,
            speed,
        ),
        MacroEvent::FindColor {
            region,
            color,
            threshold,
            action,
            wait_until_found,
            offset_x,
            offset_y,
        } => play_find_color_event(
            FindColorRequest {
                region: region.clone(),
                color: color.clone(),
                threshold: *threshold,
                action: action.clone(),
                wait_until_found: *wait_until_found,
                offset_x: *offset_x,
                offset_y: *offset_y,
            },
            playback_flag,
            speed,
        ),
        MacroEvent::FindText {
            region,
            text,
            action,
            wait_until_found,
            offset_x,
            offset_y,
        } => play_find_text_event(
            FindTextRequest {
                region: region.clone(),
                text: text.clone(),
                action: action.clone(),
                wait_until_found: *wait_until_found,
                offset_x: *offset_x,
                offset_y: *offset_y,
            },
            playback_flag,
            speed,
            ocr_engine,
        ),
    }
}

fn click_button(button: Button, speed: f64) -> Result<(), String> {
    simulate(&EventType::ButtonPress(button)).map_err(|err| err.to_string())?;
    sleep_adjusted(20, speed);
    simulate(&EventType::ButtonRelease(button)).map_err(|err| err.to_string())
}

/// 根据搜索区域面积计算自动下采样比例。
/// 当区域超过 1920×1080 时等比缩小，最低缩放到 25%，
/// 将高分辨率屏幕（如 5K）的匹配开销降到 1080p 级别。
fn compute_auto_scale(region: &FindImageRegion) -> f64 {
    const MAX_SEARCH_PIXELS: u32 = 1920 * 1080;
    const MIN_AUTO_SCALE: f64 = 0.25;

    let width = (region.x2 - region.x1) as u32;
    let height = (region.y2 - region.y1) as u32;
    let area = width.saturating_mul(height);

    if area <= MAX_SEARCH_PIXELS {
        1.0
    } else {
        let scale = (MAX_SEARCH_PIXELS as f64 / area as f64).sqrt();
        scale.max(MIN_AUTO_SCALE)
    }
}

fn play_find_image_event(
    request: FindImageRequest,
    playback_flag: &AtomicBool,
    speed: f64,
) -> Result<(), String> {
    validate_find_image_request(&request)?;
    // 搜索区域规范化、显示器预筛选、自动下采样比例均在循环外完成
    let region = normalize_region(&request.region)?;
    let monitors = Monitor::all().map_err(|e| e.to_string())?;
    let relevant_monitors = find_relevant_monitors(&region, &monitors)?;
    let auto_scale = compute_auto_scale(&region);

    // 模板图预处理：用户 scale × 自动下采样比例
    let effective_scale = request.scale * auto_scale;
    let template_gray = prepare_template(&request.image_data, effective_scale)?;

    loop {
        let search_image = capture_region_with_monitors(&region, &relevant_monitors)?;

        // 对搜索图下采样（高分辨率屏幕优化）
        let search_gray = if (auto_scale - 1.0).abs() >= f64::EPSILON {
            let sw = (f64::from(search_image.width()) * auto_scale).round().max(1.0) as u32;
            let sh = (f64::from(search_image.height()) * auto_scale).round().max(1.0) as u32;
            let resized = imageops::resize(&search_image, sw, sh, FilterType::Triangle);
            imageops::grayscale(&resized)
        } else {
            imageops::grayscale(&search_image)
        };

        if template_gray.width() > search_gray.width() || template_gray.height() > search_gray.height() {
            return Err("Template image is larger than the search region".to_string());
        }

        let cv_result = unsafe {
            match_template_opencv(
                search_gray.as_raw().as_ptr(),
                search_gray.width() as i32,
                search_gray.height() as i32,
                template_gray.as_raw().as_ptr(),
                template_gray.width() as i32,
                template_gray.height() as i32,
                request.threshold / 100.0,
            )
        };

        // 将缩小后的匹配坐标映射回原始分辨率
        let raw_loc_x = cv_result.x as f64 / auto_scale;
        let raw_loc_y = cv_result.y as f64 / auto_scale;
        let raw_template_w = template_gray.width() as f64 / auto_scale;
        let raw_template_h = template_gray.height() as f64 / auto_scale;
        let center_x = region.x1 + raw_loc_x.round() as i32;
        let center_y = region.y1 + raw_loc_y.round() as i32;

        let result = FindImageResult {
            found: cv_result.found != 0,
            score: cv_result.score,
            x: center_x,
            y: center_y,
            width: raw_template_w.round() as u32,
            height: raw_template_h.round() as u32,
        };

        if result.found {
            let target_x = result.x + request.offset_x;
            let target_y = result.y + request.offset_y;
            simulate(&EventType::MouseMove {
                x: f64::from(target_x),
                y: f64::from(target_y),
            })
            .map_err(|err| err.to_string())?;

            if request.action == "click" {
                click_button(Button::Left, speed)?;
            } else if request.action == "doubleClick" {
                click_button(Button::Left, speed)?;
                sleep_adjusted(45, speed);
                click_button(Button::Left, speed)?;
            }
            return Ok(());
        }

        if !request.wait_until_found || !playback_flag.load(Ordering::SeqCst) {
            return Err(format!(
                "Image match score {:.1}% is below threshold {:.1}%",
                result.score, request.threshold
            ));
        }

        sleep_adjusted(250, speed);
    }
}

fn play_find_color_event(
    request: FindColorRequest,
    playback_flag: &AtomicBool,
    speed: f64,
) -> Result<(), String> {
    validate_find_color_request(&request)?;
    let region = normalize_region(&request.region)?;
    let target_rgb = hex_to_rgb(&request.color)?;

    loop {
        let search_image = capture_region(&region)?;
        let result = find_color_in_image(&search_image, target_rgb, request.threshold, &region);

        if result.found {
            let target_x = result.x + request.offset_x;
            let target_y = result.y + request.offset_y;
            simulate(&EventType::MouseMove {
                x: f64::from(target_x),
                y: f64::from(target_y),
            })
            .map_err(|err| err.to_string())?;

            if request.action == "click" {
                click_button(Button::Left, speed)?;
            } else if request.action == "doubleClick" {
                click_button(Button::Left, speed)?;
                sleep_adjusted(45, speed);
                click_button(Button::Left, speed)?;
            }
            return Ok(());
        }

        if !request.wait_until_found || !playback_flag.load(Ordering::SeqCst) {
            return Err("Color not found in search region".to_string());
        }

        sleep_adjusted(250, speed);
    }
}

fn find_color_once(request: &FindColorRequest) -> Result<FindColorResult, String> {
    validate_find_color_request(request)?;
    let region = normalize_region(&request.region)?;
    let target_rgb = hex_to_rgb(&request.color)?;
    let search_image = capture_region(&region)?;
    Ok(find_color_in_image(&search_image, target_rgb, request.threshold, &region))
}

fn play_find_text_event(
    request: FindTextRequest,
    playback_flag: &AtomicBool,
    speed: f64,
    ocr_engine: &std::sync::Mutex<Option<OcrEngine>>,
) -> Result<(), String> {
    validate_find_text_request(&request)?;

    loop {
        let mut engine = ocr_engine.lock().map_err(|e| e.to_string())?;
        if engine.is_none() {
            *engine = Some(OcrEngine::new()?);
        }
        let engine = engine.as_ref().unwrap();

        let result = find_text_once(&request, engine)?;

        if result.found {
            let target_x = result.x + request.offset_x;
            let target_y = result.y + request.offset_y;
            simulate(&EventType::MouseMove {
                x: f64::from(target_x),
                y: f64::from(target_y),
            })
            .map_err(|err| err.to_string())?;

            if request.action == "click" {
                click_button(Button::Left, speed)?;
            } else if request.action == "doubleClick" {
                click_button(Button::Left, speed)?;
                sleep_adjusted(45, speed);
                click_button(Button::Left, speed)?;
            }
            return Ok(());
        }

        if !request.wait_until_found || !playback_flag.load(Ordering::SeqCst) {
            return Err(format!("Text '{}' not found in search region", request.text));
        }

        sleep_adjusted(500, speed);
    }
}

fn find_text_once(
    request: &FindTextRequest,
    engine: &OcrEngine,
) -> Result<FindTextResult, String> {
    let region = normalize_region(&request.region)?;
    let search_image = capture_region(&region)?;
    let image_data = image_to_base64_png(&search_image)?;
    let results = engine.recognize(&image_data)?;

    for item in &results {
        if item.text == request.text {
            return Ok(FindTextResult {
                found: true,
                x: region.x1 + item.center_x,
                y: region.y1 + item.center_y,
                text: item.text.clone(),
            });
        }
    }

    Ok(FindTextResult {
        found: false,
        x: 0,
        y: 0,
        text: String::new(),
    })
}

fn validate_find_text_request(request: &FindTextRequest) -> Result<(), String> {
    normalize_region(&request.region)?;
    validate_find_image_action(&request.action)?;
    if request.text.is_empty() {
        return Err("Search text cannot be empty".to_string());
    }
    validate_offset(request.offset_x, request.offset_y)?;
    Ok(())
}

fn find_color_in_image(
    image: &RgbaImage,
    target: (u8, u8, u8),
    threshold: u8,
    region: &FindImageRegion,
) -> FindColorResult {
    let (tr, tg, tb) = target;
    let threshold = threshold as i32;

    for y in 0..image.height() {
        for x in 0..image.width() {
            let pixel = image.get_pixel(x, y);
            let r = pixel[0] as i32;
            let g = pixel[1] as i32;
            let b = pixel[2] as i32;

            let dr = (r - tr as i32).abs();
            let dg = (g - tg as i32).abs();
            let db = (b - tb as i32).abs();

            if dr <= threshold && dg <= threshold && db <= threshold {
                return FindColorResult {
                    found: true,
                    x: region.x1 + x as i32,
                    y: region.y1 + y as i32,
                };
            }
        }
    }

    FindColorResult {
        found: false,
        x: 0,
        y: 0,
    }
}

fn validate_find_color_request(request: &FindColorRequest) -> Result<(), String> {
    normalize_region(&request.region)?;
    hex_to_rgb(&request.color)?;
    validate_find_image_action(&request.action)?;
    validate_offset(request.offset_x, request.offset_y)?;
    Ok(())
}

fn hex_to_rgb(hex: &str) -> Result<(u8, u8, u8), String> {
    let hex = hex.trim();
    let hex = if hex.starts_with('#') { &hex[1..] } else { hex };

    if hex.len() != 6 {
        return Err("Color must be a 6-digit hex value like #FF0000".to_string());
    }

    let r = u8::from_str_radix(&hex[0..2], 16)
        .map_err(|_| "Invalid red component in hex color".to_string())?;
    let g = u8::from_str_radix(&hex[2..4], 16)
        .map_err(|_| "Invalid green component in hex color".to_string())?;
    let b = u8::from_str_radix(&hex[4..6], 16)
        .map_err(|_| "Invalid blue component in hex color".to_string())?;

    Ok((r, g, b))
}

/// 预处理模板图：解码 -> 缩放 -> 转灰度。应在循环外调用，避免重复。
fn prepare_template(image_data: &str, scale: f64) -> Result<GrayImage, String> {
    let mut template = decode_macro_image(image_data)?;
    if (scale - 1.0).abs() >= f64::EPSILON {
        let next_width = (f64::from(template.width()) * scale)
            .round()
            .max(1.0) as u32;
        let next_height = (f64::from(template.height()) * scale)
            .round()
            .max(1.0) as u32;
        template = imageops::resize(&template, next_width, next_height, FilterType::Triangle);
    }
    Ok(DynamicImage::ImageRgba8(template).to_luma8())
}

/// 使用已预处理的灰度模板图执行单次查找。只做截图 + 匹配，不做模板预处理。
fn find_image_with_prepared_template(
    request: &FindImageRequest,
    template_gray: &GrayImage,
) -> Result<FindImageResult, String> {
    let region = normalize_region(&request.region)?;
    let search_image = capture_region(&region)?;

    if template_gray.width() > search_image.width() || template_gray.height() > search_image.height() {
        return Err("Template image is larger than the search region".to_string());
    }

    let search_gray = imageops::grayscale(&search_image);

    let result = unsafe {
        match_template_opencv(
            search_gray.as_raw().as_ptr(),
            search_gray.width() as i32,
            search_gray.height() as i32,
            template_gray.as_raw().as_ptr(),
            template_gray.width() as i32,
            template_gray.height() as i32,
            request.threshold / 100.0,
        )
    };

    Ok(FindImageResult {
        found: result.found != 0,
        score: result.score,
        x: region.x1 + result.x,
        y: region.y1 + result.y,
        width: template_gray.width(),
        height: template_gray.height(),
    })
}

fn find_image_once(request: &FindImageRequest) -> Result<FindImageResult, String> {
    let template_gray = prepare_template(&request.image_data, request.scale)?;
    find_image_with_prepared_template(request, &template_gray)
}

fn validate_find_image_request(request: &FindImageRequest) -> Result<(), String> {
    normalize_region(&request.region)?;
    normalize_find_image_threshold(request.threshold)?;
    normalize_find_image_scale(request.scale)?;
    validate_find_image_action(&request.action)?;
    validate_offset(request.offset_x, request.offset_y)?;
    Ok(())
}

fn normalize_region(region: &FindImageRegion) -> Result<FindImageRegion, String> {
    let x1 = region.x1.min(region.x2);
    let x2 = region.x1.max(region.x2);
    let y1 = region.y1.min(region.y2);
    let y2 = region.y1.max(region.y2);

    if x1 == x2 || y1 == y2 {
        return Err("Search region cannot be empty".to_string());
    }

    Ok(FindImageRegion { x1, y1, x2, y2 })
}

fn normalize_find_image_threshold(threshold: f64) -> Result<f64, String> {
    if threshold.is_finite() && (1.0..=100.0).contains(&threshold) {
        Ok((threshold * 10.0).round() / 10.0)
    } else {
        Err("Match threshold must be between 1 and 100 percent".to_string())
    }
}

fn normalize_find_image_scale(scale: f64) -> Result<f64, String> {
    if scale.is_finite() && (0.1..=5.0).contains(&scale) {
        Ok((scale * 100.0).round() / 100.0)
    } else {
        Err("Image scale must be between 0.1 and 5.0".to_string())
    }
}

fn validate_find_image_action(action: &str) -> Result<(), String> {
    match action {
        "click" | "doubleClick" | "move" => Ok(()),
        _ => Err("Unsupported find-image follow-up action".to_string()),
    }
}

fn validate_offset(offset_x: i32, offset_y: i32) -> Result<(), String> {
    const MAX_OFFSET: i32 = 500;
    if offset_x.abs() > MAX_OFFSET {
        return Err(format!("X offset must be between -{} and {}", MAX_OFFSET, MAX_OFFSET));
    }
    if offset_y.abs() > MAX_OFFSET {
        return Err(format!("Y offset must be between -{} and {}", MAX_OFFSET, MAX_OFFSET));
    }
    Ok(())
}

fn decode_macro_image(data_url: &str) -> Result<RgbaImage, String> {
    const MAX_IMAGE_DATA_LEN: usize = 10 * 1024 * 1024;

    if data_url.len() > MAX_IMAGE_DATA_LEN {
        return Err("Template image is too large".to_string());
    }

    let encoded = data_url
        .split_once(',')
        .map(|(_, data)| data)
        .unwrap_or(data_url);
    let bytes = general_purpose::STANDARD
        .decode(encoded)
        .map_err(|_| "Template image data is invalid".to_string())?;
    let image = image::load_from_memory(&bytes)
        .map_err(|_| "Template image could not be decoded".to_string())?;

    if image.width() == 0 || image.height() == 0 {
        return Err("Template image cannot be empty".to_string());
    }

    Ok(image.to_rgba8())
}

/// 筛选出与查找区域相交的显示器，避免每次循环遍历所有显示器
fn find_relevant_monitors<'a>(
    region: &FindImageRegion,
    monitors: &'a [Monitor],
) -> Result<Vec<&'a Monitor>, String> {
    let mut relevant = Vec::with_capacity(monitors.len());
    for monitor in monitors {
        let mx = monitor.x().map_err(|e| e.to_string())?;
        let my = monitor.y().map_err(|e| e.to_string())?;
        let mright = mx + monitor.width().map_err(|e| e.to_string())? as i32;
        let mbottom = my + monitor.height().map_err(|e| e.to_string())? as i32;

        let left = region.x1.max(mx);
        let top = region.y1.max(my);
        let right = region.x2.min(mright);
        let bottom = region.y2.min(mbottom);

        if left < right && top < bottom {
            relevant.push(monitor);
        }
    }
    Ok(relevant)
}

fn capture_region(region: &FindImageRegion) -> Result<RgbaImage, String> {
    let width = u32::try_from(region.x2 - region.x1)
        .map_err(|_| "Search region width is invalid".to_string())?;
    let height = u32::try_from(region.y2 - region.y1)
        .map_err(|_| "Search region height is invalid".to_string())?;
    let mut canvas = RgbaImage::new(width, height);
    let monitors = Monitor::all().map_err(|err| err.to_string())?;
    let mut captured_any = false;

    for monitor in monitors {
        let monitor_x = monitor.x().map_err(|err| err.to_string())?;
        let monitor_y = monitor.y().map_err(|err| err.to_string())?;
        let monitor_width = monitor.width().map_err(|err| err.to_string())?;
        let monitor_height = monitor.height().map_err(|err| err.to_string())?;
        let monitor_right = monitor_x + monitor_width as i32;
        let monitor_bottom = monitor_y + monitor_height as i32;

        let left = region.x1.max(monitor_x);
        let top = region.y1.max(monitor_y);
        let right = region.x2.min(monitor_right);
        let bottom = region.y2.min(monitor_bottom);

        if left >= right || top >= bottom {
            continue;
        }

        let part_width = u32::try_from(right - left)
            .map_err(|_| "Capture region width is invalid".to_string())?;
        let part_height = u32::try_from(bottom - top)
            .map_err(|_| "Capture region height is invalid".to_string())?;
        let part = monitor
            .capture_region(
                u32::try_from(left - monitor_x)
                    .map_err(|_| "Capture region x is invalid".to_string())?,
                u32::try_from(top - monitor_y)
                    .map_err(|_| "Capture region y is invalid".to_string())?,
                part_width,
                part_height,
            )
            .map_err(|err| err.to_string())?;
        canvas
            .copy_from(
                &part,
                u32::try_from(left - region.x1)
                    .map_err(|_| "Capture output x is invalid".to_string())?,
                u32::try_from(top - region.y1)
                    .map_err(|_| "Capture output y is invalid".to_string())?,
            )
            .map_err(|err| err.to_string())?;
        captured_any = true;
    }

    if captured_any {
        Ok(canvas)
    } else {
        Err("Search region is outside all screens".to_string())
    }
}

/// 使用已预筛选的显示器列表截图，避免遍历所有显示器
fn capture_region_with_monitors(
    region: &FindImageRegion,
    monitors: &[&Monitor],
) -> Result<RgbaImage, String> {
    let width = u32::try_from(region.x2 - region.x1)
        .map_err(|_| "Search region width is invalid".to_string())?;
    let height = u32::try_from(region.y2 - region.y1)
        .map_err(|_| "Search region height is invalid".to_string())?;
    let mut canvas = RgbaImage::new(width, height);
    let mut captured_any = false;

    for monitor in monitors {
        let monitor_x = monitor.x().map_err(|err| err.to_string())?;
        let monitor_y = monitor.y().map_err(|err| err.to_string())?;
        let monitor_width = monitor.width().map_err(|err| err.to_string())?;
        let monitor_height = monitor.height().map_err(|err| err.to_string())?;
        let monitor_right = monitor_x + monitor_width as i32;
        let monitor_bottom = monitor_y + monitor_height as i32;

        let left = region.x1.max(monitor_x);
        let top = region.y1.max(monitor_y);
        let right = region.x2.min(monitor_right);
        let bottom = region.y2.min(monitor_bottom);

        if left >= right || top >= bottom {
            continue;
        }

        let part_width = u32::try_from(right - left)
            .map_err(|_| "Capture region width is invalid".to_string())?;
        let part_height = u32::try_from(bottom - top)
            .map_err(|_| "Capture region height is invalid".to_string())?;
        let part = monitor
            .capture_region(
                u32::try_from(left - monitor_x)
                    .map_err(|_| "Capture region x is invalid".to_string())?,
                u32::try_from(top - monitor_y)
                    .map_err(|_| "Capture region y is invalid".to_string())?,
                part_width,
                part_height,
            )
            .map_err(|err| err.to_string())?;
        canvas
            .copy_from(
                &part,
                u32::try_from(left - region.x1)
                    .map_err(|_| "Capture output x is invalid".to_string())?,
                u32::try_from(top - region.y1)
                    .map_err(|_| "Capture output y is invalid".to_string())?,
            )
            .map_err(|err| err.to_string())?;
        captured_any = true;
    }

    if captured_any {
        Ok(canvas)
    } else {
        Err("Search region is outside all screens".to_string())
    }
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
        MacroEvent::FindImage {
            region,
            image_data,
            threshold,
            scale,
            action,
            wait_until_found,
            offset_x,
            offset_y,
        } => {
            validate_find_image_request(&FindImageRequest {
                region: region.clone(),
                image_data: image_data.clone(),
                threshold: *threshold,
                scale: *scale,
                action: action.clone(),
                wait_until_found: *wait_until_found,
                offset_x: *offset_x,
                offset_y: *offset_y,
            })?;
        }
        MacroEvent::FindColor {
            region,
            color,
            threshold,
            action,
            wait_until_found,
            offset_x,
            offset_y,
        } => {
            validate_find_color_request(&FindColorRequest {
                region: region.clone(),
                color: color.clone(),
                threshold: *threshold,
                action: action.clone(),
                wait_until_found: *wait_until_found,
                offset_x: *offset_x,
                offset_y: *offset_y,
            })?;
        }
        MacroEvent::FindText {
            region,
            text,
            action,
            wait_until_found,
            offset_x,
            offset_y,
        } => {
            validate_find_text_request(&FindTextRequest {
                region: region.clone(),
                text: text.clone(),
                action: action.clone(),
                wait_until_found: *wait_until_found,
                offset_x: *offset_x,
                offset_y: *offset_y,
            })?;
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
