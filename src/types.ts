export type MouseButton = "left" | "middle" | "right";
export type ClickMode = "toggle" | "hold";

export type HotkeyConfig = {
  ctrl: boolean;
  alt: boolean;
  key: string;
};

export type GlobalHotkeyOptions = {
  showWindowOnStop: boolean;
  autoHideOnHotkey: boolean;
};

export type ClickerConfig = {
  clickButton: MouseButton;
  intervalSecs: number;
  clickLimit: number;
  mode: ClickMode;
  holdButton: MouseButton;
  hotkey: HotkeyConfig;
};

export type ClickerState = {
  config: ClickerConfig;
  running: boolean;
};

export type RecordingSummary = {
  id: number;
  name: string;
  playbackSpeed: number;
  loopPlayback: boolean;
  createdAt: number;
  updatedAt: number;
  eventCount: number;
  durationMs: number;
};

export type RecordingEventSummary = {
  index: number;
  delayMs: number;
  action: string;
  target: string;
  critical: boolean;
};

export type RecordingDetail = {
  id: number;
  name: string;
  playbackSpeed: number;
  loopPlayback: boolean;
  createdAt: number;
  updatedAt: number;
  durationMs: number;
  events: RecordingEventSummary[];
};

export type RecorderState = {
  recordings: RecordingSummary[];
  selectedId: number | null;
  recording: boolean;
  playing: boolean;
};

export type MouseMacroEvent =
  | { kind: "mouseClick"; button: MouseButton }
  | { kind: "mouseDoubleClick"; button: MouseButton }
  | { kind: "mouseDown"; button: MouseButton }
  | { kind: "mouseUp"; button: MouseButton }
  | { kind: "mouseMove"; x: number; y: number }
  | { kind: "keyClick"; key: string }
  | { kind: "keyDown"; key: string }
  | { kind: "keyUp"; key: string }
  | { kind: "delay"; ms: number };

export type MouseMacroSummary = {
  id: number;
  name: string;
  playbackSpeed: number;
  loopPlayback: boolean;
  createdAt: number;
  updatedAt: number;
  eventCount: number;
};

export type MouseMacroState = {
  macros: MouseMacroSummary[];
  selectedId: number | null;
  playing: boolean;
};

export const mouseButtonOptions = [
  { label: "左键", value: "left" },
  { label: "中键", value: "middle" },
  { label: "右键", value: "right" },
] as const;

export const keyboardKeys = [
  "F1",
  "F2",
  "F3",
  "F4",
  "F5",
  "F6",
  "F7",
  "F8",
  "F9",
  "F10",
  "F11",
  "F12",
  "A",
  "B",
  "C",
  "D",
  "E",
  "F",
  "G",
  "H",
  "I",
  "J",
  "K",
  "L",
  "M",
  "N",
  "O",
  "P",
  "Q",
  "R",
  "S",
  "T",
  "U",
  "V",
  "W",
  "X",
  "Y",
  "Z",
  "0",
  "1",
  "2",
  "3",
  "4",
  "5",
  "6",
  "7",
  "8",
  "9",
  "SPACE",
  "ENTER",
  "ESC",
];

export function hotkeyText(hotkey: HotkeyConfig) {
  const parts = [];
  if (hotkey.ctrl) parts.push("Ctrl");
  if (hotkey.alt) parts.push("Alt");
  parts.push(hotkey.key);
  return parts.join(" + ");
}

export function keyNeedsModifier(key: string) {
  return /^[A-Z0-9]$/.test(key);
}
