use std::{
    env, fmt, fs,
    io::Write,
    mem::size_of,
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Mutex, OnceLock}, thread, time::SystemTime,
};

use serde::{Deserialize, Serialize};
use windows::Win32::{
    Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM},
    Graphics::Gdi::{ClientToScreen, ScreenToClient},
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
            CallNextHookEx, EnumWindows, GetAncestor, GetClientRect, GetCursorPos, GetMessageW,
            GetParent, GetSystemMetrics, GetWindowTextW, IsWindow, IsWindowVisible,
            PostMessageW, RealChildWindowFromPoint, SendMessageW, SetWindowsHookExW,
            UnhookWindowsHookEx, WindowFromPoint, GA_ROOT, HC_ACTION, KBDLLHOOKSTRUCT, MSG,
            MSLLHOOKSTRUCT, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, WH_KEYBOARD_LL,
            WH_MOUSE_LL, WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDOWN, WM_LBUTTONUP,
            WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEHWHEEL, WM_MOUSEMOVE, WM_MOUSEWHEEL,
            WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SYSKEYDOWN, WM_SYSKEYUP, WM_XBUTTONDOWN,
            WM_XBUTTONUP,
        },
    },
};

const WHEEL_DELTA: i16 = 120;
const MK_LBUTTON_STATE: usize = 0x0001;
const MK_RBUTTON_STATE: usize = 0x0002;
const MK_MBUTTON_STATE: usize = 0x0010;
const CREATE_NO_WINDOW_FLAG: u32 = 0x0800_0000;
const LDPLAYER_REGISTRY_KEYS: &[&str] = &[
    r"HKCU\Software\leidian",
    r"HKLM\SOFTWARE\leidian",
    r"HKLM\SOFTWARE\WOW6432Node\leidian",
];
const LDPLAYER_SHORTCUT_PATTERNS: &[&str] = &["ldplayer", "dnplayer", "雷电"];

static GLOBAL_CALLBACK: Mutex<Option<Box<dyn FnMut(Event) + Send>>> = Mutex::new(None);
static LDPLAYER_CONSOLE_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();
static LDPLAYER_ADB_PATH: OnceLock<Option<PathBuf>> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct LdPlayerTarget {
    pub index: i32,
    pub width: i32,
    pub height: i32,
    pub serial: String,
}

pub struct LdPlayerShellSession {
    serial: String,
    child: Child,
    stdin: ChildStdin,
}

#[derive(Debug, Clone)]
struct LdPlayerInstance {
    index: i32,
    title: String,
    top_hwnd: isize,
    bind_hwnd: isize,
    width: i32,
    height: i32,
}

impl Drop for LdPlayerShellSession {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

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


// ==================== 后台点击相关函数 ====================

pub fn enum_visible_windows() -> Vec<(String, isize)> {
    let mut windows: Vec<(String, isize)> = Vec::new();
    unsafe {
        let _ = EnumWindows(
            Some(enum_window_callback),
            LPARAM(&mut windows as *mut _ as isize),
        );
    }
    windows
}

unsafe extern "system" fn enum_window_callback(hwnd: HWND, lparam: LPARAM) -> windows::core::BOOL {
    if IsWindowVisible(hwnd).as_bool() {
        let mut text: [u16; 512] = [0; 512];
        let len = unsafe { GetWindowTextW(hwnd, &mut text) };
        if len > 0 {
            let title = String::from_utf16_lossy(&text[..len as usize]);
            let windows = unsafe { &mut *(lparam.0 as *mut Vec<(String, isize)>) };
            windows.push((title, hwnd.0 as isize));
        }
    }
    windows::core::BOOL(1)
}

pub fn window_from_point(x: i32, y: i32) -> Option<isize> {
    unsafe {
        let point = POINT { x, y };
        let hwnd = WindowFromPoint(point);
        if !hwnd.0.is_null() {
            Some(hwnd.0 as isize)
        } else {
            None
        }
    }
}

pub fn screen_to_client(hwnd: isize, x: i32, y: i32) -> Option<(i32, i32)> {
    unsafe {
        let mut point = POINT { x, y };
        if ScreenToClient(HWND(hwnd as *mut _), &mut point).as_bool() {
            Some((point.x, point.y))
        } else {
            None
        }
    }
}

pub fn client_to_screen(hwnd: isize, x: i32, y: i32) -> Option<(i32, i32)> {
    unsafe {
        let mut point = POINT { x, y };
        if ClientToScreen(HWND(hwnd as *mut _), &mut point).as_bool() {
            Some((point.x, point.y))
        } else {
            None
        }
    }
}

fn client_size(hwnd: isize) -> Option<(i32, i32)> {
    unsafe {
        let mut rect = RECT::default();
        if GetClientRect(HWND(hwnd as *mut _), &mut rect).is_ok() {
            let width = rect.right - rect.left;
            let height = rect.bottom - rect.top;
            if width > 0 && height > 0 {
                Some((width, height))
            } else {
                None
            }
        } else {
            None
        }
    }
}

fn hidden_program_command(program: &str) -> Command {
    let mut command = Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW_FLAG);
    command
}

fn push_console_candidate(candidates: &mut Vec<PathBuf>, path: PathBuf) {
    if path.is_file() && !candidates.iter().any(|candidate| candidate == &path) {
        candidates.push(path);
    }
}

fn push_install_dir_candidate(candidates: &mut Vec<PathBuf>, install_dir: PathBuf) {
    push_console_candidate(candidates, install_dir.join("ldconsole.exe"));
}

fn ldplayer_registry_install_dirs() -> Vec<PathBuf> {
    let mut install_dirs = Vec::new();

    for key in LDPLAYER_REGISTRY_KEYS {
        let Ok(output) = hidden_program_command("reg")
            .args(["query", key, "/s"])
            .output()
        else {
            continue;
        };

        if !output.status.success() {
            continue;
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("InstallDir") {
                if let Some((_, value)) = trimmed.split_once("REG_SZ") {
                    let path = PathBuf::from(value.trim());
                    if path.is_dir() && !install_dirs.iter().any(|existing| existing == &path) {
                        install_dirs.push(path);
                    }
                }
            }
        }
    }

    install_dirs
}

fn shortcut_search_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Some(profile) = env::var_os("USERPROFILE") {
        let profile = PathBuf::from(profile);
        roots.push(profile.join("Desktop"));
    }
    if let Some(public) = env::var_os("PUBLIC") {
        roots.push(PathBuf::from(public).join("Desktop"));
    }
    if let Some(appdata) = env::var_os("APPDATA") {
        roots.push(PathBuf::from(appdata).join(r"Microsoft\Windows\Start Menu\Programs"));
    }
    if let Some(program_data) = env::var_os("ProgramData") {
        roots.push(PathBuf::from(program_data).join(r"Microsoft\Windows\Start Menu\Programs"));
    }

    roots
}

fn collect_ldplayer_shortcuts(root: &Path, shortcuts: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_ldplayer_shortcuts(&path, shortcuts);
            continue;
        }

        let Some(extension) = path.extension().and_then(|ext| ext.to_str()) else {
            continue;
        };
        if !extension.eq_ignore_ascii_case("lnk") {
            continue;
        }

        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let normalized = name.to_lowercase();
        if LDPLAYER_SHORTCUT_PATTERNS
            .iter()
            .any(|pattern| normalized.contains(&pattern.to_lowercase()))
        {
            shortcuts.push(path);
        }
    }
}

fn powershell_escape_single_quoted_path(path: &Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}

fn resolve_shortcut_target(shortcut: &Path) -> Option<PathBuf> {
    let escaped = powershell_escape_single_quoted_path(shortcut);
    let script = format!(
        "$s=(New-Object -ComObject WScript.Shell).CreateShortcut('{}'); [Console]::Out.Write($s.TargetPath)",
        escaped
    );

    let output = hidden_program_command("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &script,
        ])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let target = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if target.is_empty() {
        None
    } else {
        Some(PathBuf::from(target))
    }
}

fn ldplayer_shortcut_install_dirs() -> Vec<PathBuf> {
    let mut shortcuts = Vec::new();
    for root in shortcut_search_roots() {
        collect_ldplayer_shortcuts(&root, &mut shortcuts);
    }

    let mut install_dirs = Vec::new();
    for shortcut in shortcuts {
        let Some(target) = resolve_shortcut_target(&shortcut) else {
            continue;
        };
        let Some(parent) = target.parent() else {
            continue;
        };
        let parent = parent.to_path_buf();
        if parent.join("ldconsole.exe").is_file()
            && !install_dirs.iter().any(|existing| existing == &parent)
        {
            install_dirs.push(parent);
        }
    }

    install_dirs
}

fn ldplayer_console_path() -> Option<&'static PathBuf> {
    LDPLAYER_CONSOLE_PATH
        .get_or_init(|| {
            let mut candidates = Vec::new();

            for install_dir in ldplayer_registry_install_dirs() {
                push_install_dir_candidate(&mut candidates, install_dir);
            }

            for install_dir in ldplayer_shortcut_install_dirs() {
                push_install_dir_candidate(&mut candidates, install_dir);
            }

            for path in [
                PathBuf::from(r"C:\leidian\LDPlayer9\ldconsole.exe"),
                PathBuf::from(r"C:\LDPlayer\LDPlayer9\ldconsole.exe"),
                PathBuf::from(r"C:\Program Files\ldplayer9\ldconsole.exe"),
                PathBuf::from(r"C:\Program Files\dnplayerext2\ldconsole.exe"),
                PathBuf::from(r"C:\leidian\LDPlayer4.0\ldconsole.exe"),
                PathBuf::from(r"C:\LDPlayer\LDPlayer4.0\ldconsole.exe"),
                PathBuf::from(r"C:\leidian\dnplayer\ldconsole.exe"),
            ] {
                push_console_candidate(&mut candidates, path);
            }

            for key in ["ProgramFiles", "ProgramFiles(x86)"] {
                if let Some(base) = env::var_os(key) {
                    let base = PathBuf::from(base);
                    push_console_candidate(&mut candidates, base.join("ldplayer9").join("ldconsole.exe"));
                    push_console_candidate(&mut candidates, base.join("dnplayerext2").join("ldconsole.exe"));
                    push_console_candidate(&mut candidates, base.join("dnplayer").join("ldconsole.exe"));
                }
            }

            candidates.into_iter().find(|path| path.is_file())
        })
        .as_ref()
}

fn hidden_command(program: &Path) -> Command {
    let mut command = Command::new(program);
    command.creation_flags(CREATE_NO_WINDOW_FLAG);
    command
}

fn ldplayer_adb_path() -> Option<&'static PathBuf> {
    LDPLAYER_ADB_PATH
        .get_or_init(|| {
            ldplayer_console_path()
                .and_then(|console| console.parent().map(|parent| parent.join("adb.exe")))
                .filter(|path| path.is_file())
        })
        .as_ref()
}

pub fn is_likely_ldplayer_window(title: &str) -> bool {
    let normalized = title.to_lowercase();
    normalized.contains("雷电")
        || normalized.contains("ldplayer")
        || normalized.contains("dnplayer")
}

fn ldplayer_instances() -> Result<Vec<LdPlayerInstance>, String> {
    let Some(console) = ldplayer_console_path() else {
        return Err("LDPlayer console not found".to_string());
    };

    let output = hidden_command(console)
        .arg("list2")
        .output()
        .map_err(|err| err.to_string())?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut instances = Vec::new();

    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        let parts: Vec<&str> = line.split(',').collect();
        if parts.len() < 10 {
            continue;
        }

        let Ok(index) = parts[0].trim().parse::<i32>() else {
            continue;
        };
        let Ok(top_hwnd) = parts[2].trim().parse::<isize>() else {
            continue;
        };
        let Ok(bind_hwnd) = parts[3].trim().parse::<isize>() else {
            continue;
        };
        let Ok(width) = parts[7].trim().parse::<i32>() else {
            continue;
        };
        let Ok(height) = parts[8].trim().parse::<i32>() else {
            continue;
        };

        instances.push(LdPlayerInstance {
            index,
            title: parts[1].trim().to_string(),
            top_hwnd,
            bind_hwnd,
            width,
            height,
        });
    }

    Ok(instances)
}

fn ldplayer_serial(index: i32) -> Result<String, String> {
    let Some(console) = ldplayer_console_path() else {
        return Err("LDPlayer console not found".to_string());
    };

    let output = hidden_command(console)
        .args(["adb", "--index", &index.to_string(), "--command", "get-serialno"])
        .output()
        .map_err(|err| err.to_string())?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    let serial = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if serial.is_empty() {
        Err("LDPlayer adb serial is empty".to_string())
    } else {
        Ok(serial)
    }
}

pub fn resolve_ldplayer_target(root_hwnd: isize, title: &str) -> Option<LdPlayerTarget> {
    let instances = ldplayer_instances().ok()?;

    let matched = instances
        .iter()
        .find(|instance| instance.top_hwnd == root_hwnd || instance.bind_hwnd == root_hwnd)
        .or_else(|| instances.iter().find(|instance| instance.title == title))
        .or_else(|| {
            instances.iter().find(|instance| {
                instance.title.contains(title) || title.contains(instance.title.as_str())
            })
        })?;

    Some(LdPlayerTarget {
        index: matched.index,
        width: matched.width,
        height: matched.height,
        serial: ldplayer_serial(matched.index).ok()?,
    })
}

pub fn resolve_click_target_hwnd(root_hwnd: isize, x: i32, y: i32) -> Option<isize> {
    if !is_window(root_hwnd) {
        return None;
    }

    let mut current = HWND(root_hwnd as *mut _);

    loop {
        let Some((client_x, client_y)) = screen_to_client(current.0 as isize, x, y) else {
            break;
        };

        let child = unsafe {
            RealChildWindowFromPoint(
                current,
                POINT {
                    x: client_x,
                    y: client_y,
                },
            )
        };

        if child.0.is_null() || child == current {
            break;
        }

        current = child;
    }

    Some(current.0 as isize)
}

fn window_dispatch_chain(target_hwnd: isize, root_hwnd: isize) -> Vec<isize> {
    let mut chain = Vec::with_capacity(4);
    let mut current = target_hwnd;

    loop {
        if !is_window(current) || chain.contains(&current) {
            break;
        }

        chain.push(current);
        if current == root_hwnd {
            break;
        }

        let Ok(parent) = (unsafe { GetParent(HWND(current as *mut _)) }) else {
            if !chain.contains(&root_hwnd) && is_window(root_hwnd) {
                chain.push(root_hwnd);
            }
            break;
        };

        if parent.0.is_null() {
            if !chain.contains(&root_hwnd) && is_window(root_hwnd) {
                chain.push(root_hwnd);
            }
            break;
        }

        current = parent.0 as isize;
    }

    chain
}

pub fn is_window(hwnd: isize) -> bool {
    unsafe { IsWindow(Some(HWND(hwnd as *mut _))).as_bool() }
}

fn make_mouse_lparam(x: i32, y: i32) -> LPARAM {
    let x16 = (x as u16) as u32;
    let y16 = (y as u16) as u32;
    LPARAM(((y16 << 16) | x16) as isize)
}

fn mouse_button_messages(button: &str) -> Result<(u32, u32, usize), String> {
    match button {
        "left" => Ok((WM_LBUTTONDOWN, WM_LBUTTONUP, MK_LBUTTON_STATE)),
        "middle" => Ok((WM_MBUTTONDOWN, WM_MBUTTONUP, MK_MBUTTON_STATE)),
        "right" => Ok((WM_RBUTTONDOWN, WM_RBUTTONUP, MK_RBUTTON_STATE)),
        _ => Err(format!("Unsupported click button: {}", button)),
    }
}

fn send_mouse_move(hwnd: isize, x: i32, y: i32, synchronous: bool) {
    let h = HWND(hwnd as *mut _);
    let lparam = make_mouse_lparam(x, y);
    unsafe {
        if synchronous {
            let _ = SendMessageW(h, WM_MOUSEMOVE, Some(WPARAM(0)), Some(lparam));
        } else {
            let _ = PostMessageW(Some(h), WM_MOUSEMOVE, WPARAM(0), lparam);
        }
    }
}

fn send_mouse_click(hwnd: isize, button: &str, x: i32, y: i32, synchronous: bool) -> Result<(), String> {
    let h = HWND(hwnd as *mut _);
    let (down_msg, up_msg, down_wparam) = mouse_button_messages(button)?;
    let lparam = make_mouse_lparam(x, y);
    unsafe {
        if synchronous {
            let _ = SendMessageW(h, down_msg, Some(WPARAM(down_wparam)), Some(lparam));
            let _ = SendMessageW(h, up_msg, Some(WPARAM(0)), Some(lparam));
        } else {
            let _ = PostMessageW(Some(h), down_msg, WPARAM(down_wparam), lparam);
            thread::sleep(std::time::Duration::from_millis(2));
            let _ = PostMessageW(Some(h), up_msg, WPARAM(0), lparam);
        }
    }
    Ok(())
}

pub fn dispatch_background_click(
    root_hwnd: isize,
    button: &str,
    root_client_x: i32,
    root_client_y: i32,
) -> Result<(), String> {
    let (screen_x, screen_y) = client_to_screen(root_hwnd, root_client_x, root_client_y)
        .ok_or("Failed to convert client position to screen position".to_string())?;

    let target_hwnd = resolve_click_target_hwnd(root_hwnd, screen_x, screen_y).unwrap_or(root_hwnd);
    let dispatch_chain = window_dispatch_chain(target_hwnd, root_hwnd);

    for hwnd in dispatch_chain.iter().rev() {
        if let Some((client_x, client_y)) = screen_to_client(*hwnd, screen_x, screen_y) {
            send_mouse_move(*hwnd, client_x, client_y, true);
        }
    }

    let (target_client_x, target_client_y) = screen_to_client(target_hwnd, screen_x, screen_y)
        .ok_or("Failed to convert target position to client position".to_string())?;

    send_mouse_click(target_hwnd, button, target_client_x, target_client_y, true)
}

pub fn dispatch_ldplayer_click(
    target: &LdPlayerTarget,
    root_hwnd: isize,
    button: &str,
    root_client_x: i32,
    root_client_y: i32,
) -> Result<(), String> {
    let (tap_x, tap_y) =
        ldplayer_tap_position(target, button, root_hwnd, root_client_x, root_client_y)?;

    let Some(console) = ldplayer_console_path() else {
        return Err("LDPlayer console not found".to_string());
    };

    let output = hidden_command(console)
        .args([
            "adb",
            "--index",
            &target.index.to_string(),
            "--command",
            &format!("shell input tap {} {}", tap_x, tap_y),
        ])
        .output()
        .map_err(|err| err.to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

fn ldplayer_tap_position(
    target: &LdPlayerTarget,
    button: &str,
    root_hwnd: isize,
    root_client_x: i32,
    root_client_y: i32,
) -> Result<(i32, i32), String> {
    if button != "left" {
        return Err("LDPlayer fallback only supports left click".to_string());
    }

    let (screen_x, screen_y) = client_to_screen(root_hwnd, root_client_x, root_client_y)
        .ok_or("Failed to convert root client position to screen position".to_string())?;

    let target_hwnd = resolve_click_target_hwnd(root_hwnd, screen_x, screen_y).unwrap_or(root_hwnd);
    let (view_x, view_y) = screen_to_client(target_hwnd, screen_x, screen_y)
        .or_else(|| screen_to_client(root_hwnd, screen_x, screen_y))
        .ok_or("Failed to resolve LDPlayer view coordinates".to_string())?;

    let (view_width, view_height) = client_size(target_hwnd)
        .or_else(|| client_size(root_hwnd))
        .ok_or("Failed to resolve LDPlayer view size".to_string())?;

    if target.width <= 0 || target.height <= 0 {
        return Err("Invalid LDPlayer device resolution".to_string());
    }

    let tap_x = ((i64::from(view_x) * i64::from(target.width)) / i64::from(view_width))
        .clamp(0, i64::from(target.width.saturating_sub(1))) as i32;
    let tap_y = ((i64::from(view_y) * i64::from(target.height)) / i64::from(view_height))
        .clamp(0, i64::from(target.height.saturating_sub(1))) as i32;

    Ok((tap_x, tap_y))
}

fn spawn_ldplayer_shell(target: &LdPlayerTarget) -> Result<LdPlayerShellSession, String> {
    let Some(adb) = ldplayer_adb_path() else {
        return Err("LDPlayer adb executable not found".to_string());
    };

    let mut child = hidden_command(adb)
        .args(["-s", &target.serial, "shell"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|err| err.to_string())?;

    let stdin = child
        .stdin
        .take()
        .ok_or("Failed to open LDPlayer adb shell stdin".to_string())?;

    Ok(LdPlayerShellSession {
        serial: target.serial.clone(),
        child,
        stdin,
    })
}

pub fn dispatch_ldplayer_click_via_shell(
    session: &mut Option<LdPlayerShellSession>,
    target: &LdPlayerTarget,
    button: &str,
    root_hwnd: isize,
    root_client_x: i32,
    root_client_y: i32,
) -> Result<(), String> {
    if button != "left" {
        return Err("LDPlayer fallback only supports left click".to_string());
    }

    let (tap_x, tap_y) =
        ldplayer_tap_position(target, button, root_hwnd, root_client_x, root_client_y)?;

    let needs_restart = match session.as_mut() {
        Some(existing) if existing.serial == target.serial => {
            existing.child.try_wait().map_err(|err| err.to_string())?.is_some()
        }
        Some(_) => true,
        None => true,
    };

    if needs_restart {
        *session = Some(spawn_ldplayer_shell(target)?);
    }

    let mut last_error = None;
    for attempt in 0..2 {
        let Some(existing) = session.as_mut() else {
            return Err("LDPlayer adb shell session unavailable".to_string());
        };

        if let Err(err) = writeln!(existing.stdin, "input tap {} {}", tap_x, tap_y)
            .and_then(|_| existing.stdin.flush())
        {
            last_error = Some(err.to_string());
            *session = None;
            if attempt == 0 {
                *session = Some(spawn_ldplayer_shell(target)?);
                continue;
            }
        } else {
            return Ok(());
        }
    }

    Err(last_error.unwrap_or_else(|| "Failed to write to LDPlayer adb shell".to_string()))
}


pub fn get_cursor_pos() -> Option<(i32, i32)> {
    unsafe {
        let mut point = POINT { x: 0, y: 0 };
        if GetCursorPos(&mut point).is_ok() {
            Some((point.x, point.y))
        } else {
            None
        }
    }
}

pub fn get_window_title(hwnd: isize) -> String {
    unsafe {
        let mut text: [u16; 512] = [0; 512];
        let len = GetWindowTextW(HWND(hwnd as *mut _), &mut text);
        if len > 0 {
            String::from_utf16_lossy(&text[..len as usize])
        } else {
            String::new()
        }
    }
}

pub fn get_root_window(hwnd: isize) -> isize {
    unsafe {
        let root = GetAncestor(HWND(hwnd as *mut _), GA_ROOT);
        if root.0.is_null() {
            hwnd
        } else {
            root.0 as isize
        }
    }
}
