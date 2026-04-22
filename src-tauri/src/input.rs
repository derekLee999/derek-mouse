use std::{fmt, mem::size_of, sync::Mutex, time::SystemTime};

use serde::{Deserialize, Serialize};
use windows::Win32::{
    Foundation::{LPARAM, LRESULT, WPARAM},
    UI::{
        Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYBD_EVENT_FLAGS,
            KEYEVENTF_KEYUP, MOUSEEVENTF_ABSOLUTE, MOUSEEVENTF_HWHEEL, MOUSEEVENTF_LEFTDOWN,
            MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_MOVE,
            MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEEVENTF_VIRTUALDESK, MOUSEEVENTF_WHEEL,
            MOUSEEVENTF_XDOWN, MOUSEEVENTF_XUP, MOUSEINPUT, MOUSE_EVENT_FLAGS, VIRTUAL_KEY,
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

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum ActiveFeature {
    Clicker,
    Recorder,
}

impl ActiveFeature {
    pub fn from_name(name: &str) -> Self {
        match name {
            "recorder" => Self::Recorder,
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
        if let Some(event_type) = convert_event(wparam, lparam) {
            let event = Event {
                time: SystemTime::now(),
                name: None,
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

fn convert_event(wparam: WPARAM, lparam: LPARAM) -> Option<EventType> {
    match wparam.0 as u32 {
        WM_KEYDOWN | WM_SYSKEYDOWN => Some(EventType::KeyPress(key_from_vk(unsafe {
            (*(lparam.0 as *const KBDLLHOOKSTRUCT)).vkCode
        } as u16))),
        WM_KEYUP | WM_SYSKEYUP => Some(EventType::KeyRelease(key_from_vk(unsafe {
            (*(lparam.0 as *const KBDLLHOOKSTRUCT)).vkCode
        } as u16))),
        WM_LBUTTONDOWN => Some(EventType::ButtonPress(Button::Left)),
        WM_LBUTTONUP => Some(EventType::ButtonRelease(Button::Left)),
        WM_MBUTTONDOWN => Some(EventType::ButtonPress(Button::Middle)),
        WM_MBUTTONUP => Some(EventType::ButtonRelease(Button::Middle)),
        WM_RBUTTONDOWN => Some(EventType::ButtonPress(Button::Right)),
        WM_RBUTTONUP => Some(EventType::ButtonRelease(Button::Right)),
        WM_XBUTTONDOWN => Some(EventType::ButtonPress(Button::Unknown(
            mouse_hiword(lparam) as u8,
        ))),
        WM_XBUTTONUP => Some(EventType::ButtonRelease(Button::Unknown(
            mouse_hiword(lparam) as u8,
        ))),
        WM_MOUSEMOVE => {
            let mouse = unsafe { *(lparam.0 as *const MSLLHOOKSTRUCT) };
            Some(EventType::MouseMove {
                x: f64::from(mouse.pt.x),
                y: f64::from(mouse.pt.y),
            })
        }
        WM_MOUSEWHEEL => {
            let delta = mouse_hiword(lparam) as i16;
            Some(EventType::Wheel {
                delta_x: 0,
                delta_y: i64::from(delta / WHEEL_DELTA),
            })
        }
        WM_MOUSEHWHEEL => {
            let delta = mouse_hiword(lparam) as i16;
            Some(EventType::Wheel {
                delta_x: i64::from(delta / WHEEL_DELTA),
                delta_y: 0,
            })
        }
        _ => None,
    }
}

fn mouse_hiword(lparam: LPARAM) -> u16 {
    let mouse = unsafe { *(lparam.0 as *const MSLLHOOKSTRUCT) };
    ((mouse.mouseData >> 16) & 0xffff) as u16
}

fn simulate_key(key: Key, release: bool) -> Result<(), SimulateError> {
    let vk = vk_from_key(key).ok_or(SimulateError)?;
    let flags = if release {
        KEYEVENTF_KEYUP
    } else {
        KEYBD_EVENT_FLAGS(0)
    };

    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(vk),
                wScan: 0,
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

fn vk_from_key(key: Key) -> Option<u16> {
    match key {
        Key::Alt => Some(164),
        Key::AltGr => Some(165),
        Key::Backspace => Some(0x08),
        Key::CapsLock => Some(20),
        Key::ControlLeft => Some(162),
        Key::ControlRight => Some(163),
        Key::Delete => Some(46),
        Key::DownArrow => Some(40),
        Key::End => Some(35),
        Key::Escape => Some(27),
        Key::F1 => Some(112),
        Key::F10 => Some(121),
        Key::F11 => Some(122),
        Key::F12 => Some(123),
        Key::F2 => Some(113),
        Key::F3 => Some(114),
        Key::F4 => Some(115),
        Key::F5 => Some(116),
        Key::F6 => Some(117),
        Key::F7 => Some(118),
        Key::F8 => Some(119),
        Key::F9 => Some(120),
        Key::Home => Some(36),
        Key::LeftArrow => Some(37),
        Key::MetaLeft => Some(91),
        Key::PageDown => Some(34),
        Key::PageUp => Some(33),
        Key::Return => Some(0x0D),
        Key::RightArrow => Some(39),
        Key::ShiftLeft => Some(160),
        Key::ShiftRight => Some(161),
        Key::Space => Some(32),
        Key::Tab => Some(0x09),
        Key::UpArrow => Some(38),
        Key::PrintScreen => Some(44),
        Key::ScrollLock => Some(145),
        Key::Pause => Some(19),
        Key::NumLock => Some(144),
        Key::BackQuote => Some(192),
        Key::Num1 => Some(49),
        Key::Num2 => Some(50),
        Key::Num3 => Some(51),
        Key::Num4 => Some(52),
        Key::Num5 => Some(53),
        Key::Num6 => Some(54),
        Key::Num7 => Some(55),
        Key::Num8 => Some(56),
        Key::Num9 => Some(57),
        Key::Num0 => Some(48),
        Key::Minus => Some(189),
        Key::Equal => Some(187),
        Key::KeyQ => Some(81),
        Key::KeyW => Some(87),
        Key::KeyE => Some(69),
        Key::KeyR => Some(82),
        Key::KeyT => Some(84),
        Key::KeyY => Some(89),
        Key::KeyU => Some(85),
        Key::KeyI => Some(73),
        Key::KeyO => Some(79),
        Key::KeyP => Some(80),
        Key::LeftBracket => Some(219),
        Key::RightBracket => Some(221),
        Key::KeyA => Some(65),
        Key::KeyS => Some(83),
        Key::KeyD => Some(68),
        Key::KeyF => Some(70),
        Key::KeyG => Some(71),
        Key::KeyH => Some(72),
        Key::KeyJ => Some(74),
        Key::KeyK => Some(75),
        Key::KeyL => Some(76),
        Key::SemiColon => Some(186),
        Key::Quote => Some(222),
        Key::BackSlash => Some(220),
        Key::IntlBackslash => Some(226),
        Key::KeyZ => Some(90),
        Key::KeyX => Some(88),
        Key::KeyC => Some(67),
        Key::KeyV => Some(86),
        Key::KeyB => Some(66),
        Key::KeyN => Some(78),
        Key::KeyM => Some(77),
        Key::Comma => Some(188),
        Key::Dot => Some(190),
        Key::Slash => Some(191),
        Key::Insert => Some(45),
        Key::KpMinus => Some(109),
        Key::KpPlus => Some(107),
        Key::KpMultiply => Some(106),
        Key::KpDivide => Some(111),
        Key::Kp0 => Some(96),
        Key::Kp1 => Some(97),
        Key::Kp2 => Some(98),
        Key::Kp3 => Some(99),
        Key::Kp4 => Some(100),
        Key::Kp5 => Some(101),
        Key::Kp6 => Some(102),
        Key::Kp7 => Some(103),
        Key::Kp8 => Some(104),
        Key::Kp9 => Some(105),
        Key::KpDelete => Some(110),
        Key::Unknown(code) => u16::try_from(code).ok(),
        Key::MetaRight | Key::KpReturn | Key::Function => None,
    }
}
