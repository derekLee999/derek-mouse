use std::{fmt, mem::size_of, sync::Mutex, time::SystemTime};

use serde::{Deserialize, Serialize};
use windows::Win32::{
    Foundation::{LPARAM, LRESULT, WPARAM},
    UI::{
        Input::KeyboardAndMouse::{
            MapVirtualKeyW, SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT,
            KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, MAPVK_VK_TO_VSC_EX,
            MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
            MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE, MOUSEEVENTF_RIGHTDOWN,
            MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL, MOUSEEVENTF_XDOWN,
            MOUSEEVENTF_XUP, MOUSEINPUT, MOUSE_EVENT_FLAGS, VIRTUAL_KEY,
        },
        WindowsAndMessaging::{
            CallNextHookEx, GetMessageW, GetSystemMetrics, SetWindowsHookExW, UnhookWindowsHookEx,
            HC_ACTION, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, SM_CXVIRTUALSCREEN,
            SM_CYVIRTUALSCREEN, WH_KEYBOARD_LL, WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN,
            WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE,
            WM_MOUSEWHEEL, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
            WM_XBUTTONDOWN, WM_XBUTTONUP,
        },
    },
};

const WHEEL_DELTA: i16 = 120;

static GLOBAL_CALLBACK: Mutex<Option<Box<dyn FnMut(Event) + Send>>> = Mutex::new(None);

#[derive(Debug, Copy, Clone)]
struct KeyScan {
    code: u16,
    extended: bool,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ActiveFeature {
    Clicker,
    Recorder,
    MouseMacro,
}

impl ActiveFeature {
    pub fn from_name(name: &str) -> Self {
        match name {
            "recorder" => Self::Recorder,
            "mouse-macro" => Self::MouseMacro,
            _ => Self::Clicker,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HotkeyConfig {
    pub ctrl: bool,
    pub alt: bool,
    pub key: String,
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            ctrl: false,
            alt: false,
            key: "F8".to_string(),
        }
    }
}

#[derive(Default)]
pub struct KeyboardTracker {
    ctrl_down: bool,
    alt_down: bool,
    hotkey_down: bool,
}

impl KeyboardTracker {
    pub fn handle_press(&mut self, key: Key, hotkey: &HotkeyConfig) -> bool {
        self.update_modifier(key, true);

        let Some(hotkey_key) = key_from_name(&hotkey.key) else {
            return false;
        };

        if key != hotkey_key || self.hotkey_down {
            return false;
        }

        let ctrl_matches = !hotkey.ctrl || self.ctrl_down;
        let alt_matches = !hotkey.alt || self.alt_down;

        if ctrl_matches && alt_matches {
            self.hotkey_down = true;
            return true;
        }

        false
    }

    pub fn handle_release(&mut self, key: Key, hotkey: &HotkeyConfig) {
        self.update_modifier(key, false);

        if let Some(hotkey_key) = key_from_name(&hotkey.key) {
            if key == hotkey_key {
                self.hotkey_down = false;
            }
        }
    }

    pub fn clear_hotkey_down(&mut self) {
        self.hotkey_down = false;
    }

    fn update_modifier(&mut self, key: Key, pressed: bool) {
        match key {
            Key::ControlLeft | Key::ControlRight => self.ctrl_down = pressed,
            Key::Alt | Key::AltGr => self.alt_down = pressed,
            _ => {}
        }
    }
}

#[derive(Debug)]
pub enum ListenError {
    KeyHookError(String),
    MouseHookError(String),
    MessageLoopError,
    CallbackPoisoned,
}

#[derive(Debug)]
pub struct SimulateError;

impl fmt::Display for ListenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::KeyHookError(err) => write!(f, "keyboard hook failed: {err}"),
            Self::MouseHookError(err) => write!(f, "mouse hook failed: {err}"),
            Self::MessageLoopError => write!(f, "input message loop failed"),
            Self::CallbackPoisoned => write!(f, "input callback lock was poisoned"),
        }
    }
}

impl std::error::Error for ListenError {}

impl fmt::Display for SimulateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "could not simulate input event")
    }
}

impl std::error::Error for SimulateError {}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Key {
    Alt,
    AltGr,
    Backspace,
    CapsLock,
    ControlLeft,
    ControlRight,
    Delete,
    DownArrow,
    End,
    Escape,
    F1,
    F10,
    F11,
    F12,
    F2,
    F3,
    F4,
    F5,
    F6,
    F7,
    F8,
    F9,
    Home,
    LeftArrow,
    MetaLeft,
    MetaRight,
    PageDown,
    PageUp,
    Return,
    RightArrow,
    ShiftLeft,
    ShiftRight,
    Space,
    Tab,
    UpArrow,
    PrintScreen,
    ScrollLock,
    Pause,
    NumLock,
    BackQuote,
    Num1,
    Num2,
    Num3,
    Num4,
    Num5,
    Num6,
    Num7,
    Num8,
    Num9,
    Num0,
    Minus,
    Equal,
    KeyQ,
    KeyW,
    KeyE,
    KeyR,
    KeyT,
    KeyY,
    KeyU,
    KeyI,
    KeyO,
    KeyP,
    LeftBracket,
    RightBracket,
    KeyA,
    KeyS,
    KeyD,
    KeyF,
    KeyG,
    KeyH,
    KeyJ,
    KeyK,
    KeyL,
    SemiColon,
    Quote,
    BackSlash,
    IntlBackslash,
    KeyZ,
    KeyX,
    KeyC,
    KeyV,
    KeyB,
    KeyN,
    KeyM,
    Comma,
    Dot,
    Slash,
    Insert,
    KpReturn,
    KpMinus,
    KpPlus,
    KpMultiply,
    KpDivide,
    Kp0,
    Kp1,
    Kp2,
    Kp3,
    Kp4,
    Kp5,
    Kp6,
    Kp7,
    Kp8,
    Kp9,
    KpDelete,
    Function,
    Unknown(u32),
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Button {
    Left,
    Right,
    Middle,
    Unknown(u8),
}

#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
pub enum EventType {
    KeyPress(Key),
    KeyRelease(Key),
    ButtonPress(Button),
    ButtonRelease(Button),
    MouseMove { x: f64, y: f64 },
    Wheel { delta_x: i64, delta_y: i64 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Event {
    pub time: SystemTime,
    pub name: Option<String>,
    pub position: Option<(f64, f64)>,
    pub event_type: EventType,
}

pub fn listen<T>(callback: T) -> Result<(), ListenError>
where
    T: FnMut(Event) + Send + 'static,
{
    *GLOBAL_CALLBACK
        .lock()
        .map_err(|_| ListenError::CallbackPoisoned)? = Some(Box::new(callback));

    let key_hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(raw_callback), None, 0) }
        .map_err(|err| ListenError::KeyHookError(err.to_string()))?;

    let mouse_hook = match unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(raw_callback), None, 0) } {
        Ok(hook) => hook,
        Err(err) => {
            let _ = unsafe { UnhookWindowsHookEx(key_hook) };
            return Err(ListenError::MouseHookError(err.to_string()));
        }
    };

    let loop_result = run_message_loop();

    let _ = unsafe { UnhookWindowsHookEx(key_hook) };
    let _ = unsafe { UnhookWindowsHookEx(mouse_hook) };
    *GLOBAL_CALLBACK
        .lock()
        .map_err(|_| ListenError::CallbackPoisoned)? = None;

    loop_result
}

pub fn simulate(event_type: &EventType) -> Result<(), SimulateError> {
    match *event_type {
        EventType::KeyPress(key) => simulate_key(key, false),
        EventType::KeyRelease(key) => simulate_key(key, true),
        EventType::ButtonPress(button) => match button {
            Button::Left => simulate_mouse(MOUSEEVENTF_LEFTDOWN, 0, 0, 0),
            Button::Middle => simulate_mouse(MOUSEEVENTF_MIDDLEDOWN, 0, 0, 0),
            Button::Right => simulate_mouse(MOUSEEVENTF_RIGHTDOWN, 0, 0, 0),
            Button::Unknown(code) => simulate_mouse(MOUSEEVENTF_XDOWN, u32::from(code), 0, 0),
        },
        EventType::ButtonRelease(button) => match button {
            Button::Left => simulate_mouse(MOUSEEVENTF_LEFTUP, 0, 0, 0),
            Button::Middle => simulate_mouse(MOUSEEVENTF_MIDDLEUP, 0, 0, 0),
            Button::Right => simulate_mouse(MOUSEEVENTF_RIGHTUP, 0, 0, 0),
            Button::Unknown(code) => simulate_mouse(MOUSEEVENTF_XUP, u32::from(code), 0, 0),
        },
        EventType::MouseMove { x, y } => {
            let width = unsafe { GetSystemMetrics(SM_CXVIRTUALSCREEN) };
            let height = unsafe { GetSystemMetrics(SM_CYVIRTUALSCREEN) };
            if width == 0 || height == 0 {
                return Err(SimulateError);
            }

            simulate_mouse(
                MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK,
                0,
                ((x as i32 + 1) * 65_535) / width,
                ((y as i32 + 1) * 65_535) / height,
            )
        }
        EventType::Wheel { delta_x, delta_y } => {
            if delta_x != 0 {
                let delta = i16::try_from(delta_x).map_err(|_| SimulateError)?;
                simulate_mouse(
                    MOUSEEVENTF_HWHEEL,
                    (i32::from(delta) * i32::from(WHEEL_DELTA)) as u32,
                    0,
                    0,
                )?;
            }

            if delta_y != 0 {
                let delta = i16::try_from(delta_y).map_err(|_| SimulateError)?;
                simulate_mouse(
                    MOUSEEVENTF_WHEEL,
                    (i32::from(delta) * i32::from(WHEEL_DELTA)) as u32,
                    0,
                    0,
                )?;
            }

            Ok(())
        }
    }
}

pub fn validate_hotkey(hotkey: &mut HotkeyConfig) -> Result<(), String> {
    hotkey.key = hotkey.key.trim().to_uppercase();

    if key_from_name(&hotkey.key).is_none() {
        return Err("Unsupported keyboard hotkey".to_string());
    }

    if key_needs_modifier(&hotkey.key) {
        if !hotkey.ctrl && !hotkey.alt {
            return Err("Letters and numbers must be combined with Ctrl or Alt".to_string());
        }
    } else if hotkey.ctrl || hotkey.alt {
        return Err(
            "Function keys, Space, Enter, and Esc cannot be combined with Ctrl or Alt".to_string(),
        );
    }

    Ok(())
}

pub fn button_from_name(name: &str) -> Option<Button> {
    match name.to_lowercase().as_str() {
        "left" => Some(Button::Left),
        "middle" => Some(Button::Middle),
        "right" => Some(Button::Right),
        _ => None,
    }
}

pub fn key_from_name(name: &str) -> Option<Key> {
    match name.trim().to_uppercase().as_str() {
        "A" => Some(Key::KeyA),
        "B" => Some(Key::KeyB),
        "C" => Some(Key::KeyC),
        "D" => Some(Key::KeyD),
        "E" => Some(Key::KeyE),
        "F" => Some(Key::KeyF),
        "G" => Some(Key::KeyG),
        "H" => Some(Key::KeyH),
        "I" => Some(Key::KeyI),
        "J" => Some(Key::KeyJ),
        "K" => Some(Key::KeyK),
        "L" => Some(Key::KeyL),
        "M" => Some(Key::KeyM),
        "N" => Some(Key::KeyN),
        "O" => Some(Key::KeyO),
        "P" => Some(Key::KeyP),
        "Q" => Some(Key::KeyQ),
        "R" => Some(Key::KeyR),
        "S" => Some(Key::KeyS),
        "T" => Some(Key::KeyT),
        "U" => Some(Key::KeyU),
        "V" => Some(Key::KeyV),
        "W" => Some(Key::KeyW),
        "X" => Some(Key::KeyX),
        "Y" => Some(Key::KeyY),
        "Z" => Some(Key::KeyZ),
        "0" => Some(Key::Num0),
        "1" => Some(Key::Num1),
        "2" => Some(Key::Num2),
        "3" => Some(Key::Num3),
        "4" => Some(Key::Num4),
        "5" => Some(Key::Num5),
        "6" => Some(Key::Num6),
        "7" => Some(Key::Num7),
        "8" => Some(Key::Num8),
        "9" => Some(Key::Num9),
        "F1" => Some(Key::F1),
        "F2" => Some(Key::F2),
        "F3" => Some(Key::F3),
        "F4" => Some(Key::F4),
        "F5" => Some(Key::F5),
        "F6" => Some(Key::F6),
        "F7" => Some(Key::F7),
        "F8" => Some(Key::F8),
        "F9" => Some(Key::F9),
        "F10" => Some(Key::F10),
        "F11" => Some(Key::F11),
        "F12" => Some(Key::F12),
        "SPACE" => Some(Key::Space),
        "ENTER" => Some(Key::Return),
        "ESC" | "ESCAPE" => Some(Key::Escape),
        _ => None,
    }
}

pub fn key_needs_modifier(name: &str) -> bool {
    let normalized = name.trim().to_uppercase();
    normalized.len() == 1 && normalized.chars().all(|ch| ch.is_ascii_alphanumeric())
}

unsafe extern "system" fn raw_callback(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code == HC_ACTION as i32 {
        if let Some((event_type, position)) = convert_event(wparam, lparam) {
            let event = Event {
                time: SystemTime::now(),
                name: None,
                position,
                event_type,
            };

            if let Ok(mut callback) = GLOBAL_CALLBACK.lock() {
                if let Some(callback) = callback.as_mut() {
                    callback(event);
                }
            }
        }
    }

    unsafe { CallNextHookEx(None, code, wparam, lparam) }
}

fn run_message_loop() -> Result<(), ListenError> {
    let mut msg = MSG::default();

    loop {
        let result = unsafe { GetMessageW(&mut msg, None, 0, 0) };
        if result.0 == -1 {
            return Err(ListenError::MessageLoopError);
        }
        if result.0 == 0 {
            return Ok(());
        }
    }
}

fn convert_event(wparam: WPARAM, lparam: LPARAM) -> Option<(EventType, Option<(f64, f64)>)> {
    match wparam.0 as u32 {
        WM_KEYDOWN | WM_SYSKEYDOWN => Some((
            EventType::KeyPress(key_from_vk(unsafe {
                (*(lparam.0 as *const KBDLLHOOKSTRUCT)).vkCode
            } as u16)),
            None,
        )),
        WM_KEYUP | WM_SYSKEYUP => Some((
            EventType::KeyRelease(key_from_vk(unsafe {
                (*(lparam.0 as *const KBDLLHOOKSTRUCT)).vkCode
            } as u16)),
            None,
        )),
        WM_LBUTTONDOWN => Some((EventType::ButtonPress(Button::Left), mouse_position(lparam))),
        WM_LBUTTONUP => Some((
            EventType::ButtonRelease(Button::Left),
            mouse_position(lparam),
        )),
        WM_MBUTTONDOWN => Some((
            EventType::ButtonPress(Button::Middle),
            mouse_position(lparam),
        )),
        WM_MBUTTONUP => Some((
            EventType::ButtonRelease(Button::Middle),
            mouse_position(lparam),
        )),
        WM_RBUTTONDOWN => Some((
            EventType::ButtonPress(Button::Right),
            mouse_position(lparam),
        )),
        WM_RBUTTONUP => Some((
            EventType::ButtonRelease(Button::Right),
            mouse_position(lparam),
        )),
        WM_XBUTTONDOWN => Some((
            EventType::ButtonPress(Button::Unknown(mouse_hiword(lparam) as u8)),
            mouse_position(lparam),
        )),
        WM_XBUTTONUP => Some((
            EventType::ButtonRelease(Button::Unknown(mouse_hiword(lparam) as u8)),
            mouse_position(lparam),
        )),
        WM_MOUSEMOVE => {
            let mouse = unsafe { *(lparam.0 as *const MSLLHOOKSTRUCT) };
            let x = f64::from(mouse.pt.x);
            let y = f64::from(mouse.pt.y);
            Some((EventType::MouseMove { x, y }, Some((x, y))))
        }
        WM_MOUSEWHEEL => {
            let delta = mouse_hiword(lparam) as i16;
            Some((
                EventType::Wheel {
                    delta_x: 0,
                    delta_y: i64::from(delta / WHEEL_DELTA),
                },
                mouse_position(lparam),
            ))
        }
        WM_MOUSEHWHEEL => {
            let delta = mouse_hiword(lparam) as i16;
            Some((
                EventType::Wheel {
                    delta_x: i64::from(delta / WHEEL_DELTA),
                    delta_y: 0,
                },
                mouse_position(lparam),
            ))
        }
        _ => None,
    }
}

fn mouse_position(lparam: LPARAM) -> Option<(f64, f64)> {
    let mouse = unsafe { *(lparam.0 as *const MSLLHOOKSTRUCT) };
    Some((f64::from(mouse.pt.x), f64::from(mouse.pt.y)))
}

fn mouse_hiword(lparam: LPARAM) -> u16 {
    let mouse = unsafe { *(lparam.0 as *const MSLLHOOKSTRUCT) };
    ((mouse.mouseData >> 16) & 0xffff) as u16
}

fn simulate_key(key: Key, release: bool) -> Result<(), SimulateError> {
    let scan = scan_from_key(key).ok_or(SimulateError)?;
    let mut flags = KEYEVENTF_SCANCODE;
    if release {
        flags |= KEYEVENTF_KEYUP;
    }
    if scan.extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }

    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: scan.code,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };

    send_one(input)
}

fn simulate_mouse(
    flags: MOUSE_EVENT_FLAGS,
    data: u32,
    dx: i32,
    dy: i32,
) -> Result<(), SimulateError> {
    let input = INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                mouseData: data,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };

    send_one(input)
}

fn send_one(input: INPUT) -> Result<(), SimulateError> {
    let sent = unsafe { SendInput(&[input], size_of::<INPUT>() as i32) };
    if sent == 1 {
        Ok(())
    } else {
        Err(SimulateError)
    }
}

fn scan_from_key(key: Key) -> Option<KeyScan> {
    let (code, extended) = match key {
        Key::Escape => (0x01, false),
        Key::Num1 => (0x02, false),
        Key::Num2 => (0x03, false),
        Key::Num3 => (0x04, false),
        Key::Num4 => (0x05, false),
        Key::Num5 => (0x06, false),
        Key::Num6 => (0x07, false),
        Key::Num7 => (0x08, false),
        Key::Num8 => (0x09, false),
        Key::Num9 => (0x0A, false),
        Key::Num0 => (0x0B, false),
        Key::Minus => (0x0C, false),
        Key::Equal => (0x0D, false),
        Key::Backspace => (0x0E, false),
        Key::Tab => (0x0F, false),
        Key::KeyQ => (0x10, false),
        Key::KeyW => (0x11, false),
        Key::KeyE => (0x12, false),
        Key::KeyR => (0x13, false),
        Key::KeyT => (0x14, false),
        Key::KeyY => (0x15, false),
        Key::KeyU => (0x16, false),
        Key::KeyI => (0x17, false),
        Key::KeyO => (0x18, false),
        Key::KeyP => (0x19, false),
        Key::LeftBracket => (0x1A, false),
        Key::RightBracket => (0x1B, false),
        Key::Return => (0x1C, false),
        Key::KpReturn => (0x1C, true),
        Key::ControlLeft => (0x1D, false),
        Key::ControlRight => (0x1D, true),
        Key::KeyA => (0x1E, false),
        Key::KeyS => (0x1F, false),
        Key::KeyD => (0x20, false),
        Key::KeyF => (0x21, false),
        Key::KeyG => (0x22, false),
        Key::KeyH => (0x23, false),
        Key::KeyJ => (0x24, false),
        Key::KeyK => (0x25, false),
        Key::KeyL => (0x26, false),
        Key::SemiColon => (0x27, false),
        Key::Quote => (0x28, false),
        Key::BackQuote => (0x29, false),
        Key::ShiftLeft => (0x2A, false),
        Key::BackSlash => (0x2B, false),
        Key::KeyZ => (0x2C, false),
        Key::KeyX => (0x2D, false),
        Key::KeyC => (0x2E, false),
        Key::KeyV => (0x2F, false),
        Key::KeyB => (0x30, false),
        Key::KeyN => (0x31, false),
        Key::KeyM => (0x32, false),
        Key::Comma => (0x33, false),
        Key::Dot => (0x34, false),
        Key::Slash => (0x35, false),
        Key::KpDivide => (0x35, true),
        Key::ShiftRight => (0x36, false),
        Key::KpMultiply => (0x37, false),
        Key::Alt => (0x38, false),
        Key::AltGr => (0x38, true),
        Key::Space => (0x39, false),
        Key::CapsLock => (0x3A, false),
        Key::F1 => (0x3B, false),
        Key::F2 => (0x3C, false),
        Key::F3 => (0x3D, false),
        Key::F4 => (0x3E, false),
        Key::F5 => (0x3F, false),
        Key::F6 => (0x40, false),
        Key::F7 => (0x41, false),
        Key::F8 => (0x42, false),
        Key::F9 => (0x43, false),
        Key::F10 => (0x44, false),
        Key::NumLock => (0x45, false),
        Key::ScrollLock => (0x46, false),
        Key::Kp7 => (0x47, false),
        Key::Home => (0x47, true),
        Key::Kp8 => (0x48, false),
        Key::UpArrow => (0x48, true),
        Key::Kp9 => (0x49, false),
        Key::PageUp => (0x49, true),
        Key::KpMinus => (0x4A, false),
        Key::Kp4 => (0x4B, false),
        Key::LeftArrow => (0x4B, true),
        Key::Kp5 => (0x4C, false),
        Key::Kp6 => (0x4D, false),
        Key::RightArrow => (0x4D, true),
        Key::KpPlus => (0x4E, false),
        Key::Kp1 => (0x4F, false),
        Key::End => (0x4F, true),
        Key::Kp2 => (0x50, false),
        Key::DownArrow => (0x50, true),
        Key::Kp3 => (0x51, false),
        Key::PageDown => (0x51, true),
        Key::Kp0 => (0x52, false),
        Key::Insert => (0x52, true),
        Key::KpDelete => (0x53, false),
        Key::Delete => (0x53, true),
        Key::IntlBackslash => (0x56, false),
        Key::F11 => (0x57, false),
        Key::F12 => (0x58, false),
        Key::MetaLeft => (0x5B, true),
        Key::MetaRight => (0x5C, true),
        Key::PrintScreen => (0x37, true),
        Key::Pause => return None,
        Key::Function => return None,
        Key::Unknown(code) => {
            return u16::try_from(code).ok().and_then(scan_from_vk);
        }
    };

    Some(KeyScan { code, extended })
}

fn scan_from_vk(vk: u16) -> Option<KeyScan> {
    let mapped = unsafe { MapVirtualKeyW(u32::from(vk), MAPVK_VK_TO_VSC_EX) };
    if mapped == 0 {
        return None;
    }

    let extended = mapped & 0xff00 != 0;
    Some(KeyScan {
        code: (mapped & 0xff) as u16,
        extended,
    })
}

fn key_from_vk(code: u16) -> Key {
    match code {
        164 => Key::Alt,
        165 => Key::AltGr,
        0x08 => Key::Backspace,
        20 => Key::CapsLock,
        162 => Key::ControlLeft,
        163 => Key::ControlRight,
        46 => Key::Delete,
        40 => Key::DownArrow,
        35 => Key::End,
        27 => Key::Escape,
        112 => Key::F1,
        121 => Key::F10,
        122 => Key::F11,
        123 => Key::F12,
        113 => Key::F2,
        114 => Key::F3,
        115 => Key::F4,
        116 => Key::F5,
        117 => Key::F6,
        118 => Key::F7,
        119 => Key::F8,
        120 => Key::F9,
        36 => Key::Home,
        37 => Key::LeftArrow,
        91 => Key::MetaLeft,
        34 => Key::PageDown,
        33 => Key::PageUp,
        0x0D => Key::Return,
        39 => Key::RightArrow,
        160 => Key::ShiftLeft,
        161 => Key::ShiftRight,
        32 => Key::Space,
        0x09 => Key::Tab,
        38 => Key::UpArrow,
        44 => Key::PrintScreen,
        145 => Key::ScrollLock,
        19 => Key::Pause,
        144 => Key::NumLock,
        192 => Key::BackQuote,
        49 => Key::Num1,
        50 => Key::Num2,
        51 => Key::Num3,
        52 => Key::Num4,
        53 => Key::Num5,
        54 => Key::Num6,
        55 => Key::Num7,
        56 => Key::Num8,
        57 => Key::Num9,
        48 => Key::Num0,
        189 => Key::Minus,
        187 => Key::Equal,
        81 => Key::KeyQ,
        87 => Key::KeyW,
        69 => Key::KeyE,
        82 => Key::KeyR,
        84 => Key::KeyT,
        89 => Key::KeyY,
        85 => Key::KeyU,
        73 => Key::KeyI,
        79 => Key::KeyO,
        80 => Key::KeyP,
        219 => Key::LeftBracket,
        221 => Key::RightBracket,
        65 => Key::KeyA,
        83 => Key::KeyS,
        68 => Key::KeyD,
        70 => Key::KeyF,
        71 => Key::KeyG,
        72 => Key::KeyH,
        74 => Key::KeyJ,
        75 => Key::KeyK,
        76 => Key::KeyL,
        186 => Key::SemiColon,
        222 => Key::Quote,
        220 => Key::BackSlash,
        226 => Key::IntlBackslash,
        90 => Key::KeyZ,
        88 => Key::KeyX,
        67 => Key::KeyC,
        86 => Key::KeyV,
        66 => Key::KeyB,
        78 => Key::KeyN,
        77 => Key::KeyM,
        188 => Key::Comma,
        190 => Key::Dot,
        191 => Key::Slash,
        45 => Key::Insert,
        109 => Key::KpMinus,
        107 => Key::KpPlus,
        106 => Key::KpMultiply,
        111 => Key::KpDivide,
        96 => Key::Kp0,
        97 => Key::Kp1,
        98 => Key::Kp2,
        99 => Key::Kp3,
        100 => Key::Kp4,
        101 => Key::Kp5,
        102 => Key::Kp6,
        103 => Key::Kp7,
        104 => Key::Kp8,
        105 => Key::Kp9,
        110 => Key::KpDelete,
        _ => Key::Unknown(u32::from(code)),
    }
}
