<script setup lang="ts">
import { computed, h, nextTick, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen, TauriEvent, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { Aim, Camera, Close, Delete, DocumentChecked, FolderOpened, Plus, Search, Top } from "@element-plus/icons-vue";
import { ElMessage, ElMessageBox } from "element-plus";
import {
  keyboardKeys,
  mouseButtonOptions,
  type MouseButton,
  type MouseMacroFindImageAction,
  type MouseMacroFindImageEvent,
  type MouseMacroFindColorEvent,
  type MouseMacroFindTextEvent,
  type MouseMacroEvent,
  type MouseMacroDetail,
  type MouseMacroState,
} from "../../types";

type OperationObject = "mouse" | "keyboard" | "delay" | "findImage" | "findColor" | "findText";
type MouseOperation = "mouseClick" | "mouseDoubleClick" | "mouseDown" | "mouseUp" | "mouseMove";
type KeyboardOperation = "keyClick" | "keyDown" | "keyUp";

type DraftEvent = MouseMacroEvent & {
  id: number;
  followUp?: "mouseClick" | "mouseDoubleClick";
};

type PickedCoordinate = {
  x: number;
  y: number;
};

type CoordinatePickSnapshotMeta = {
  left: number;
  top: number;
  width: number;
  height: number;
};

type CoordinatePickMode = "move" | "find-region" | "template" | "color";

type FindImageResult = {
  found: boolean;
  score: number;
  x: number;
  y: number;
  width: number;
  height: number;
};

type FindColorResult = {
  found: boolean;
  x: number;
  y: number;
};

type FindTextResult = {
  found: boolean;
  x: number;
  y: number;
  text: string;
};

type SavedWindowSize = {
  width: number;
  height: number;
};

type CaptureImageResult = {
  dataUrl: string;
  width: number;
  height: number;
};

type PickedRegion = {
  x1: number;
  y1: number;
  x2: number;
  y2: number;
};

const currentWindow = getCurrentWindow();
const MOUSE_MACRO_EDITOR_WINDOW_SIZE_KEY = "mouse-macro-editor-window-size";
const WINDOW_RESIZE_SAVE_DELAY_MS = 150;

const urlParams = new URLSearchParams(window.location.search);
const editMacroId = urlParams.get("id");
const isEditMode = editMacroId !== null;

const macroName = ref(defaultMacroName());
const events = ref<DraftEvent[]>([]);
const saving = ref(false);
const alwaysOnTop = ref(false);
const operationObject = ref<OperationObject>("mouse");
const mouseOperation = ref<MouseOperation>("mouseClick");
const keyboardOperation = ref<KeyboardOperation>("keyClick");
const selectedButton = ref<MouseButton>("left");
const selectedKey = ref("A");
const delayMs = ref(100);
const moveX = ref(0);
const moveY = ref(0);
const pickingCoordinate = ref(false);
const coordinatePickMode = ref<CoordinatePickMode | null>(null);
const appendDelay = ref(false);
const appendDelayMs = ref(100);
const followUpAction = ref<"none" | "mouseClick" | "mouseDoubleClick">("none");
const screenBounds = ref<CoordinatePickSnapshotMeta | null>(null);
const findRegionMode = ref<"full" | "custom">("full");
const findX1 = ref(0);
const findY1 = ref(0);
const findX2 = ref(0);
const findY2 = ref(0);
const findImageData = ref("");
const findImageName = ref("");
const findThreshold = ref(65);
const findScale = ref(1);
const findAction = ref<MouseMacroFindImageAction>("click");
const findWaitUntilFound = ref(false);
const findImageOffsetX = ref(0);
const findImageOffsetY = ref(0);
const findingTest = ref(false);
const capturingTemplate = ref(false);

// 找色相关
const findColorHex = ref("#FF0000");
const findColorThreshold = ref(10);
const findColorAction = ref<MouseMacroFindImageAction>("click");
const findColorWaitUntilFound = ref(false);
const findColorOffsetX = ref(0);
const findColorOffsetY = ref(0);
const findingColorTest = ref(false);
const pickingColor = ref(false);

// 文字识别相关
const findTextValue = ref("");
const findTextAction = ref<MouseMacroFindImageAction>("click");
const findTextWaitUntilFound = ref(false);
const findTextOffsetX = ref(0);
const findTextOffsetY = ref(0);
const findingTextTest = ref(false);

// OCR 引擎安装
const ocrInstalled = ref<boolean | null>(null);
const ocrInstalling = ref(false);
const ocrInstallDialogVisible = ref(false);
const ocrInstallProgress = ref(0);
const ocrInstallStatus = ref("ready");
const ocrInstallMessage = ref("已就绪，点击安装开始下载安装...");
const fileInputRef = ref<HTMLInputElement | null>(null);
const selectedEventId = ref<number | null>(null);
const eventMenu = ref<{
  visible: boolean;
  x: number;
  y: number;
  eventId: number | null;
}>({
  visible: false,
  x: 0,
  y: 0,
  eventId: null,
});

const dropIndicatorIndex = ref<number | null>(null);

let nextEventId = 1;
let unlistenCoordinate: UnlistenFn | undefined;
let unlistenRegion: UnlistenFn | undefined;
let unlistenColor: UnlistenFn | undefined;
let unlistenOcrInstall: UnlistenFn | undefined;
let unlistenWindowResize: UnlistenFn | undefined;
let restoringEventToEditor = false;
let resizeSaveTimer: number | undefined;

type DragState = {
  index: number;
  ghostEl: HTMLElement;
  listEl: HTMLElement;
  itemHeight: number;
};

let dragState: DragState | null = null;

const mouseOperationOptions = [
  { label: "鼠标单击", value: "mouseClick" },
  { label: "鼠标双击", value: "mouseDoubleClick" },
  { label: "鼠标按下", value: "mouseDown" },
  { label: "鼠标释放", value: "mouseUp" },
  { label: "移动到", value: "mouseMove" },
] as const;

const keyboardOperationOptions = [
  { label: "键盘点击", value: "keyClick" },
  { label: "键盘按下", value: "keyDown" },
  { label: "键盘释放", value: "keyUp" },
] as const;

const operationObjectOptions = [
  { label: "鼠标操作", value: "mouse" },
  { label: "键盘操作", value: "keyboard" },
  { label: "延迟等待", value: "delay" },
  { label: "找图", value: "findImage" },
  { label: "找色", value: "findColor" },
  { label: "文字识别", value: "findText" },
] as const;

const canSave = computed(() => macroName.value.trim().length > 0 && events.value.length > 0);
const canAddEvent = computed(() => {
  if (operationObject.value === "delay") {
    return Number.isInteger(delayMs.value) && delayMs.value >= 5 && delayMs.value <= 60000;
  }
  if (operationObject.value === "findImage") {
    return (
      !!findImageData.value &&
      findX1.value !== findX2.value &&
      findY1.value !== findY2.value &&
      Number.isFinite(findThreshold.value) &&
      findThreshold.value >= 1 &&
      findThreshold.value <= 100 &&
      Number.isFinite(findScale.value) &&
      findScale.value >= 0.1 &&
      findScale.value <= 5
    );
  }
  if (operationObject.value === "findColor") {
    return (
      findX1.value !== findX2.value &&
      findY1.value !== findY2.value &&
      isValidHexColor(findColorHex.value) &&
      Number.isInteger(findColorThreshold.value) &&
      findColorThreshold.value >= 0 &&
      findColorThreshold.value <= 255
    );
  }
  if (operationObject.value === "findText") {
    return (
      findX1.value !== findX2.value &&
      findY1.value !== findY2.value &&
      findTextValue.value.trim().length > 0
    );
  }
  if (operationObject.value === "mouse" && mouseOperation.value === "mouseMove") {
    return Number.isInteger(moveX.value) && Number.isInteger(moveY.value) && moveX.value >= 0 && moveY.value >= 0;
  }
  return true;
});

onMounted(async () => {
  alwaysOnTop.value = await currentWindow.isAlwaysOnTop();
  await loadScreenBounds();

  if (isEditMode) {
    try {
      const detail = await invoke<MouseMacroDetail>("get_mouse_macro_detail", {
        id: Number(editMacroId),
      });
      macroName.value = detail.name;
      events.value = mergeFollowUpEvents(detail.events);
    } catch (error) {
      ElMessage.error(String(error));
    }
  }

  unlistenCoordinate = await listen<PickedCoordinate>("mouse-coordinate-picked", (event) => {
    handlePickedCoordinate(event.payload);
    pickingCoordinate.value = false;
  });
  unlistenRegion = await listen<PickedRegion>("mouse-region-picked", (event) => {
    void handlePickedRegion(event.payload);
    pickingCoordinate.value = false;
  });
  unlistenColor = await listen<{ color: string }>("mouse-color-picked", (event) => {
    handlePickedColor(event.payload.color);
    pickingCoordinate.value = false;
  });
  unlistenOcrInstall = await listen<{ status: string; progress: number; message: string }>(
    "ocr-engine-install-progress",
    (event) => {
      const payload = event.payload;
      ocrInstallStatus.value = payload.status;
      ocrInstallProgress.value = payload.progress;
      ocrInstallMessage.value = payload.message;
      if (payload.status === "completed") {
        ocrInstalling.value = false;
        ocrInstalled.value = true;
        ocrInstallDialogVisible.value = false;
        ElMessage.success("OCR 引擎安装完成。");
      } else if (payload.status === "error") {
        ocrInstalling.value = false;
        ElMessage.error(payload.message || "安装失败");
      }
    },
  );
  unlistenWindowResize = await currentWindow.onResized(() => {
    scheduleWindowSizeSave();
  });
  document.addEventListener("click", closeEventMenu);
  document.addEventListener("keydown", handleDocumentKeydown);
});

onBeforeUnmount(() => {
  unlistenCoordinate?.();
  unlistenRegion?.();
  unlistenColor?.();
  unlistenOcrInstall?.();
  unlistenWindowResize?.();
  if (pickingCoordinate.value) {
    void cancelCoordinatePick();
  }
  flushWindowSizeSave();
  document.removeEventListener("click", closeEventMenu);
  document.removeEventListener("keydown", handleDocumentKeydown);
});

watch(operationObject, (value) => {
  closeEventMenu();
  appendDelay.value = false;
  if (!restoringEventToEditor && selectedEventId.value !== null) {
    selectedEventId.value = null;
  }
  if (value === "mouse") {
    mouseOperation.value = "mouseClick";
  } else if (value === "keyboard") {
    keyboardOperation.value = "keyClick";
  } else if (value === "findImage" && screenBounds.value && findRegionMode.value === "full") {
    applyFullSearchRegion();
  } else if (value === "findColor" && screenBounds.value && findRegionMode.value === "full") {
    applyFullSearchRegion();
  } else if (value === "findText" && screenBounds.value && findRegionMode.value === "full") {
    applyFullSearchRegion();
    void checkOcrEngine();
  }
});

watch(mouseOperation, () => {
  appendDelay.value = false;
  followUpAction.value = "none";
});

watch(keyboardOperation, () => {
  appendDelay.value = false;
});

function defaultMacroName() {
  return `宏方案 ${Date.now()}`;
}

function saveWindowSize(size: SavedWindowSize) {
  localStorage.setItem(MOUSE_MACRO_EDITOR_WINDOW_SIZE_KEY, JSON.stringify(size));
}

function formatElapsedSeconds(elapsedMs: number) {
  return (elapsedMs / 1000).toFixed(2);
}

async function persistCurrentWindowSize() {
  const [size, scaleFactor] = await Promise.all([currentWindow.innerSize(), currentWindow.scaleFactor()]);
  const logicalSize = size.toLogical(scaleFactor);
  saveWindowSize({
    width: Math.round(logicalSize.width),
    height: Math.round(logicalSize.height),
  });
}

function scheduleWindowSizeSave() {
  if (resizeSaveTimer !== undefined) {
    window.clearTimeout(resizeSaveTimer);
  }
  resizeSaveTimer = window.setTimeout(() => {
    resizeSaveTimer = undefined;
    void persistCurrentWindowSize();
  }, WINDOW_RESIZE_SAVE_DELAY_MS);
}

function flushWindowSizeSave() {
  if (resizeSaveTimer !== undefined) {
    window.clearTimeout(resizeSaveTimer);
    resizeSaveTimer = undefined;
  }
  void persistCurrentWindowSize();
}

function handleDocumentKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    if (pickingCoordinate.value) {
      event.preventDefault();
      void cancelCoordinatePick();
      return;
    }
    if (eventMenu.value.visible) {
      closeEventMenu();
      return;
    }
    event.preventDefault();
    void closeWindow();
  }
}

function normalizeInteger(value: number | undefined, fallback: number, min: number, max?: number) {
  const normalized = typeof value === "number" && Number.isFinite(value) ? Math.floor(value) : fallback;
  return Math.min(Math.max(normalized, min), max ?? Number.MAX_SAFE_INTEGER);
}

function isValidHexColor(hex: string): boolean {
  return /^#[0-9A-Fa-f]{6}$/.test(hex);
}

function handleColorHexChange(value: string) {
  const hex = value.trim().toUpperCase();
  if (!hex.startsWith("#")) {
    findColorHex.value = "#" + hex;
    return;
  }
  findColorHex.value = hex;
}

function handleColorThresholdChange(value: number | undefined) {
  findColorThreshold.value = normalizeInteger(value, 10, 0, 255);
}

async function checkOcrEngine() {
  try {
    ocrInstalled.value = await invoke<boolean>("check_ocr_engine_installed");
  } catch {
    ocrInstalled.value = false;
  }
}

function openOcrRepo() {
  window.open("https://github.com/hiroi-sora/RapidOCR-json", "_blank");
}

async function installOcrEngine() {
  if (ocrInstalling.value) return;
  ocrInstalling.value = true;
  ocrInstallProgress.value = 0;
  ocrInstallStatus.value = "downloading";
  ocrInstallMessage.value = "开始下载 OCR 引擎...";
  try {
    await invoke("install_ocr_engine");
  } catch (error) {
    ocrInstalling.value = false;
    ElMessage.error(String(error));
  }
}

function buildFindTextEvent(): MouseMacroFindTextEvent {
  return {
    kind: "findText",
    region: {
      x1: normalizeInteger(findX1.value, 0, Number.MIN_SAFE_INTEGER),
      y1: normalizeInteger(findY1.value, 0, Number.MIN_SAFE_INTEGER),
      x2: normalizeInteger(findX2.value, 0, Number.MIN_SAFE_INTEGER),
      y2: normalizeInteger(findY2.value, 0, Number.MIN_SAFE_INTEGER),
    },
    text: findTextValue.value.trim(),
    action: findTextAction.value,
    waitUntilFound: findTextWaitUntilFound.value,
    offsetX: normalizeInteger(findTextOffsetX.value, 0, -500, 500),
    offsetY: normalizeInteger(findTextOffsetY.value, 0, -500, 500),
  };
}

async function testFindText() {
  if (!canAddEvent.value || operationObject.value !== "findText") {
    ElMessage.warning("请先补全文字识别配置。");
    return;
  }
  if (!ocrInstalled.value) {
    ElMessage.warning("OCR 引擎未安装，请先安装。");
    return;
  }

  findingTextTest.value = true;
  try {
    const startedAt = performance.now();
    const result = await invoke<FindTextResult>("test_mouse_macro_find_text", {
      request: buildFindTextEvent(),
    });
    const elapsedMs = Math.round(performance.now() - startedAt);
    const elapsedSeconds = formatElapsedSeconds(elapsedMs);
    if (result.found) {
      await ElMessageBox.alert(
        h("div", { class: "find-text-test-result" }, [
          h("p", null, `识别文字：${result.text}`),
          h("p", null, `坐标：${result.x}, ${result.y}`),
          h("p", null, `用时：${elapsedSeconds} 秒`),
        ]),
        "文字识别测试成功",
        {
          confirmButtonText: "确定",
          type: "success",
        },
      ).catch(() => undefined);
    } else {
      ElMessage.warning(`未在区域内找到指定文字，用时 ${elapsedSeconds} 秒。`);
    }
  } catch (error) {
    ElMessage.error(String(error));
  } finally {
    findingTextTest.value = false;
  }
}

async function loadScreenBounds() {
  try {
    screenBounds.value = await invoke<CoordinatePickSnapshotMeta>("get_mouse_macro_screen_bounds");
    applyFullSearchRegion();
  } catch (error) {
    ElMessage.error(String(error));
  }
}

function applyFullSearchRegion() {
  if (!screenBounds.value) return;
  findRegionMode.value = "full";
  findX1.value = screenBounds.value.left;
  findY1.value = screenBounds.value.top;
  findX2.value = screenBounds.value.left + screenBounds.value.width;
  findY2.value = screenBounds.value.top + screenBounds.value.height;
}

function handleFindRegionNumberChange() {
  findRegionMode.value = "custom";
  findX1.value = normalizeInteger(findX1.value, 0, Number.MIN_SAFE_INTEGER);
  findY1.value = normalizeInteger(findY1.value, 0, Number.MIN_SAFE_INTEGER);
  findX2.value = normalizeInteger(findX2.value, 0, Number.MIN_SAFE_INTEGER);
  findY2.value = normalizeInteger(findY2.value, 0, Number.MIN_SAFE_INTEGER);
}

function handleThresholdChange(value: number | undefined) {
  findThreshold.value = normalizeInteger(value, 65, 1, 100);
}

// function handleScaleChange(value: number | undefined) {
//   const normalized = typeof value === "number" && Number.isFinite(value) ? value : 1;
//   findScale.value = Math.min(Math.max(Math.round(normalized * 100) / 100, 0.1), 5);
// }

function handleDelayChange(value: number | undefined) {
  delayMs.value = normalizeInteger(value, 100, 5, 60000);
}

function handleMoveXChange(value: number | undefined) {
  moveX.value = normalizeInteger(value, 0, 0);
}

function handleMoveYChange(value: number | undefined) {
  moveY.value = normalizeInteger(value, 0, 0);
}

async function startCoordinatePick(mode: CoordinatePickMode = "move") {
  if (pickingCoordinate.value) {
    await cancelCoordinatePick();
    return;
  }

  closeEventMenu();
  pickingCoordinate.value = true;
  coordinatePickMode.value = mode;
  try {
    const snapshot = await invoke<CoordinatePickSnapshotMeta>("start_mouse_coordinate_pick", {
      windowLabel: currentWindow.label,
    });

    const label = `coordinate-picker-${Date.now()}`;
    const modeParam = mode === "move" ? "coordinate" : mode === "color" ? "color" : "region";
    const picker = new WebviewWindow(label, {
      url: `/index.html?view=coordinate-picker&mode=${modeParam}`,
      title: "选择坐标",
      x: snapshot.left,
      y: snapshot.top,
      width: snapshot.width,
      height: snapshot.height,
      decorations: false,
      resizable: false,
      transparent: true,
      shadow: false,
      alwaysOnTop: true,
      skipTaskbar: true,
      focus: true,
    });

    picker.once(TauriEvent.WINDOW_DESTROYED, () => {
      pickingCoordinate.value = false;
      if (coordinatePickMode.value === mode) {
        coordinatePickMode.value = null;
      }
      if (mode === "template") {
        capturingTemplate.value = false;
      }
    });
    picker.once("tauri://error", async (event) => {
      pickingCoordinate.value = false;
      await cancelCoordinatePick();
      ElMessage.error(String(event.payload));
    });
  } catch (error) {
    pickingCoordinate.value = false;
    coordinatePickMode.value = null;
    if (mode === "template") {
      capturingTemplate.value = false;
    }
    if (mode === "color") {
      pickingColor.value = false;
    }
    ElMessage.error(String(error));
  }
}

async function cancelCoordinatePick() {
  pickingCoordinate.value = false;
  coordinatePickMode.value = null;
  capturingTemplate.value = false;
  pickingColor.value = false;
  try {
    await invoke("cancel_mouse_coordinate_pick");
  } catch {}
}

function handlePickedCoordinate(coordinate: PickedCoordinate) {
  moveX.value = normalizeInteger(coordinate.x, 0, 0);
  moveY.value = normalizeInteger(coordinate.y, 0, 0);
  coordinatePickMode.value = null;
  ElMessage.success("已获取坐标。");
}

async function handlePickedRegion(region: PickedRegion) {
  const mode = coordinatePickMode.value;
  coordinatePickMode.value = null;

  if (mode === "find-region") {
    findRegionMode.value = "custom";
    findX1.value = Math.min(region.x1, region.x2);
    findY1.value = Math.min(region.y1, region.y2);
    findX2.value = Math.max(region.x1, region.x2);
    findY2.value = Math.max(region.y1, region.y2);
    ElMessage.success("已选取找图区域。");
    return;
  }

  if (mode === "template") {
    await captureFindTemplateRegion({
      x1: Math.min(region.x1, region.x2),
      y1: Math.min(region.y1, region.y2),
      x2: Math.max(region.x1, region.x2),
      y2: Math.max(region.y1, region.y2),
    });
  }
}

function startFindRegionPick() {
  if (pickingCoordinate.value) {
    void cancelCoordinatePick();
    return;
  }
  void startCoordinatePick("find-region");
}

function startColorPick() {
  if (pickingCoordinate.value) {
    void cancelCoordinatePick();
    return;
  }
  pickingColor.value = true;
  void startCoordinatePick("color");
}

function handlePickedColor(color: string) {
  findColorHex.value = color.toUpperCase();
  pickingColor.value = false;
  ElMessage.success(`已获取颜色 ${color.toUpperCase()}`);
}

function addEvent() {
  if (!canAddEvent.value) {
    ElMessage.warning("请先补全有效的操作输入。");
    return;
  }

  const newEvents: DraftEvent[] = [];

  if (operationObject.value === "mouse" && mouseOperation.value === "mouseMove") {
    newEvents.push({
      kind: "mouseMove",
      x: normalizeInteger(moveX.value, 0, 0),
      y: normalizeInteger(moveY.value, 0, 0),
      ...(followUpAction.value !== "none" ? { followUp: followUpAction.value } : {}),
      id: nextEventId++,
    } as DraftEvent);
  } else {
    const event = buildEvent();
    newEvents.push({ ...event, id: nextEventId++ } as DraftEvent);
  }

  if (operationObject.value !== "delay" && appendDelay.value) {
    newEvents.push({
      kind: "delay",
      ms: normalizeInteger(appendDelayMs.value, 100, 5, 60000),
      id: nextEventId++,
    } as DraftEvent);
  }

  events.value = [...events.value, ...newEvents];
  selectedEventId.value = null;
  closeEventMenu();
}

function buildEvent(): MouseMacroEvent {
  if (operationObject.value === "delay") {
    return { kind: "delay", ms: normalizeInteger(delayMs.value, 100, 5, 60000) };
  }

  if (operationObject.value === "findImage") {
    return buildFindImageEvent();
  }

  if (operationObject.value === "findColor") {
    return buildFindColorEvent();
  }

  if (operationObject.value === "findText") {
    return buildFindTextEvent();
  }

  if (operationObject.value === "keyboard") {
    return { kind: keyboardOperation.value, key: selectedKey.value };
  }

  if (mouseOperation.value === "mouseMove") {
    return {
      kind: "mouseMove",
      x: normalizeInteger(moveX.value, 0, 0),
      y: normalizeInteger(moveY.value, 0, 0),
    };
  }

  return { kind: mouseOperation.value, button: selectedButton.value };
}

function buildFindColorEvent(): MouseMacroFindColorEvent {
  return {
    kind: "findColor",
    region: {
      x1: normalizeInteger(findX1.value, 0, Number.MIN_SAFE_INTEGER),
      y1: normalizeInteger(findY1.value, 0, Number.MIN_SAFE_INTEGER),
      x2: normalizeInteger(findX2.value, 0, Number.MIN_SAFE_INTEGER),
      y2: normalizeInteger(findY2.value, 0, Number.MIN_SAFE_INTEGER),
    },
    color: findColorHex.value.toUpperCase(),
    threshold: normalizeInteger(findColorThreshold.value, 10, 0, 255),
    action: findColorAction.value,
    waitUntilFound: findColorWaitUntilFound.value,
    offsetX: normalizeInteger(findColorOffsetX.value, 0, -500, 500),
    offsetY: normalizeInteger(findColorOffsetY.value, 0, -500, 500),
  };
}

function buildFindImageEvent(): MouseMacroFindImageEvent {
  return {
    kind: "findImage",
    region: {
      x1: normalizeInteger(findX1.value, 0, Number.MIN_SAFE_INTEGER),
      y1: normalizeInteger(findY1.value, 0, Number.MIN_SAFE_INTEGER),
      x2: normalizeInteger(findX2.value, 0, Number.MIN_SAFE_INTEGER),
      y2: normalizeInteger(findY2.value, 0, Number.MIN_SAFE_INTEGER),
    },
    imageData: findImageData.value,
    threshold: Math.min(Math.max(Math.round(findThreshold.value * 10) / 10, 1), 100),
    scale: Math.min(Math.max(Math.round(findScale.value * 100) / 100, 0.1), 5),
    action: findAction.value,
    waitUntilFound: findWaitUntilFound.value,
    offsetX: normalizeInteger(findImageOffsetX.value, 0, -500, 500),
    offsetY: normalizeInteger(findImageOffsetY.value, 0, -500, 500),
  };
}

async function captureFindTemplate() {
  if (pickingCoordinate.value) {
    await cancelCoordinatePick();
    return;
  }

  capturingTemplate.value = true;
  void startCoordinatePick("template");
}

async function captureFindTemplateRegion(region: PickedRegion) {
  capturingTemplate.value = true;
  try {
    const image = await invoke<CaptureImageResult>("capture_mouse_macro_region_image", {
      region,
    });
    findImageData.value = image.dataUrl;
    findImageName.value = `屏幕截图 ${image.width}x${image.height}`;
    ElMessage.success("已截取模板图。");
  } catch (error) {
    ElMessage.error(String(error));
  } finally {
    capturingTemplate.value = false;
  }
}

function openLocalImagePicker() {
  fileInputRef.value?.click();
}

function handleLocalImageChange(event: Event) {
  const input = event.target as HTMLInputElement;
  const file = input.files?.[0];
  if (!file) return;

  if (!file.type.startsWith("image/")) {
    ElMessage.warning("请选择图片文件。");
    input.value = "";
    return;
  }

  const reader = new FileReader();
  reader.onload = () => {
    findImageData.value = String(reader.result ?? "");
    findImageName.value = file.name;
    input.value = "";
  };
  reader.onerror = () => {
    ElMessage.error("读取图片失败。");
    input.value = "";
  };
  reader.readAsDataURL(file);
}

function clearFindImage() {
  findImageData.value = "";
  findImageName.value = "";
}

async function testFindImage() {
  if (!canAddEvent.value || operationObject.value !== "findImage") {
    ElMessage.warning("请先补全找图配置。");
    return;
  }

  findingTest.value = true;
  try {
    const result = await invoke<FindImageResult>("test_mouse_macro_find_image", {
      request: buildFindImageEvent(),
    });
    const score = result.score.toFixed(1);
    if (result.found) {
      ElMessage.success(`匹配成功：${score}%，坐标 ${result.x}, ${result.y}`);
    } else {
      ElMessage.warning(`未达到阈值，最高匹配 ${score}%。`);
    }
  } catch (error) {
    ElMessage.error(String(error));
  } finally {
    findingTest.value = false;
  }
}

async function testFindColor() {
  if (!canAddEvent.value || operationObject.value !== "findColor") {
    ElMessage.warning("请先补全找色配置。");
    return;
  }

  findingColorTest.value = true;
  try {
    const result = await invoke<FindColorResult>("test_mouse_macro_find_color", {
      request: buildFindColorEvent(),
    });
    if (result.found) {
      ElMessage.success(`找到颜色，坐标 ${result.x}, ${result.y}`);
    } else {
      ElMessage.warning("未在区域内找到指定颜色。");
    }
  } catch (error) {
    ElMessage.error(String(error));
  } finally {
    findingColorTest.value = false;
  }
}

function updateEvent() {
  if (selectedEventId.value === null) return;

  const index = events.value.findIndex((e) => e.id === selectedEventId.value);
  if (index === -1) return;

  if (!canAddEvent.value) {
    ElMessage.warning("请先补全有效的操作输入。");
    return;
  }

  const updated = buildEvent();
  const oldEvent = events.value[index];

  if (updated.kind === "mouseMove" && followUpAction.value !== "none") {
    events.value[index] = {
      ...updated,
      followUp: followUpAction.value,
      id: oldEvent.id,
    } as DraftEvent;
  } else {
    events.value[index] = { ...updated, id: oldEvent.id } as DraftEvent;
  }

  events.value = [...events.value];
}

function openEventMenu(event: MouseEvent, macroEvent: DraftEvent) {
  event.preventDefault();

  const menuWidth = 112;
  const menuHeight = 42;
  eventMenu.value = {
    visible: true,
    x: Math.min(event.clientX, window.innerWidth - menuWidth - 8),
    y: Math.min(event.clientY, window.innerHeight - menuHeight - 8),
    eventId: macroEvent.id,
  };
}

function closeEventMenu() {
  eventMenu.value.visible = false;
}

function restoreEventToEditor(event: DraftEvent) {
  restoringEventToEditor = true;
  try {
    switch (event.kind) {
      case "mouseMove":
        operationObject.value = "mouse";
        mouseOperation.value = "mouseMove";
        moveX.value = event.x ?? 0;
        moveY.value = event.y ?? 0;
        appendDelay.value = false;
        // 延迟设置 followUpAction，避免被 mouseOperation 的 watcher 覆盖
        nextTick(() => {
          followUpAction.value = event.followUp ?? "none";
        });
        break;
      case "mouseClick":
      case "mouseDoubleClick":
      case "mouseDown":
      case "mouseUp":
        operationObject.value = "mouse";
        mouseOperation.value = event.kind;
        selectedButton.value = event.button as MouseButton;
        appendDelay.value = false;
        break;
      case "keyClick":
      case "keyDown":
      case "keyUp":
        operationObject.value = "keyboard";
        keyboardOperation.value = event.kind;
        selectedKey.value = event.key ?? "";
        appendDelay.value = false;
        break;
      case "delay":
        operationObject.value = "delay";
        delayMs.value = event.ms ?? 100;
        appendDelay.value = false;
        break;
      case "findImage":
        operationObject.value = "findImage";
        findRegionMode.value = "custom";
        findX1.value = event.region.x1;
        findY1.value = event.region.y1;
        findX2.value = event.region.x2;
        findY2.value = event.region.y2;
        findImageData.value = event.imageData;
        findImageName.value = "已保存的模板图";
        findThreshold.value = event.threshold;
        findScale.value = event.scale;
        findAction.value = event.action;
        findWaitUntilFound.value = event.waitUntilFound;
        findImageOffsetX.value = event.offsetX ?? 0;
        findImageOffsetY.value = event.offsetY ?? 0;
        appendDelay.value = false;
        break;
      case "findColor":
        operationObject.value = "findColor";
        findRegionMode.value = "custom";
        findX1.value = event.region.x1;
        findY1.value = event.region.y1;
        findX2.value = event.region.x2;
        findY2.value = event.region.y2;
        findColorHex.value = event.color;
        findColorThreshold.value = event.threshold;
        findColorAction.value = event.action;
        findColorWaitUntilFound.value = event.waitUntilFound;
        findColorOffsetX.value = event.offsetX ?? 0;
        findColorOffsetY.value = event.offsetY ?? 0;
        appendDelay.value = false;
        break;
      case "findText":
        operationObject.value = "findText";
        findRegionMode.value = "custom";
        findX1.value = event.region.x1;
        findY1.value = event.region.y1;
        findX2.value = event.region.x2;
        findY2.value = event.region.y2;
        findTextValue.value = event.text;
        findTextAction.value = event.action;
        findTextWaitUntilFound.value = event.waitUntilFound;
        findTextOffsetX.value = event.offsetX ?? 0;
        findTextOffsetY.value = event.offsetY ?? 0;
        appendDelay.value = false;
        void checkOcrEngine();
        break;
    }
  } finally {
    restoringEventToEditor = false;
  }
}

function handleMouseDown(event: MouseEvent, index: number) {
  const target = event.currentTarget as HTMLElement;
  const listEl = target.closest(".event-list") as HTMLElement;
  const startY = event.clientY;
  let hasMoved = false;

  const onMouseMove = (e: MouseEvent) => {
    if (!hasMoved && Math.abs(e.clientY - startY) < 4) return;

    if (!hasMoved) {
      hasMoved = true;
      const rect = target.getBoundingClientRect();
      const ghost = target.cloneNode(true) as HTMLElement;
      ghost.style.position = "fixed";
      ghost.style.left = `${rect.left}px`;
      ghost.style.top = `${rect.top}px`;
      ghost.style.width = `${rect.width}px`;
      ghost.style.height = `${rect.height}px`;
      ghost.style.opacity = "0.9";
      ghost.style.pointerEvents = "none";
      ghost.style.zIndex = "9999";
      ghost.style.boxShadow = "0 4px 16px rgba(0,0,0,0.3)";
      ghost.style.borderRadius = "8px";
      document.body.appendChild(ghost);

      target.style.opacity = "0.25";

      dragState = {
        index,
        ghostEl: ghost,
        listEl,
        itemHeight: rect.height,
      };
    }

    if (!dragState) return;

    const ghost = dragState.ghostEl;
    const rect = target.getBoundingClientRect();
    ghost.style.left = `${rect.left}px`;
    ghost.style.top = `${e.clientY - rect.height / 2}px`;

    const listRect = listEl.getBoundingClientRect();
    const scrollTop = listEl.scrollTop;
    const relativeY = e.clientY - listRect.top + scrollTop;
    const gap = 8;
    const itemTotalHeight = dragState.itemHeight + gap;
    let insertIndex = Math.round(relativeY / itemTotalHeight);
    insertIndex = Math.max(0, Math.min(insertIndex, events.value.length));

    if (insertIndex !== dropIndicatorIndex.value) {
      dropIndicatorIndex.value = insertIndex;
    }
  };

  const onMouseUp = () => {
    document.removeEventListener("mousemove", onMouseMove);
    document.removeEventListener("mouseup", onMouseUp);

    if (dragState) {
      const fromIndex = dragState.index;
      let toIndex = dropIndicatorIndex.value ?? events.value.length;

      document.body.removeChild(dragState.ghostEl);
      target.style.opacity = "";

      if (fromIndex < toIndex) toIndex--;

      if (toIndex !== fromIndex) {
        const item = events.value.splice(fromIndex, 1)[0];
        events.value.splice(toIndex, 0, item);
      }

      dragState = null;
      dropIndicatorIndex.value = null;
    } else if (!hasMoved) {
      const clickedEvent = events.value[index];
      if (!clickedEvent) return;

      if (selectedEventId.value === clickedEvent.id) {
        selectedEventId.value = null;
      } else {
        selectedEventId.value = clickedEvent.id;
        restoreEventToEditor(clickedEvent);
      }
    }
  };

  document.addEventListener("mousemove", onMouseMove);
  document.addEventListener("mouseup", onMouseUp);
}

function deleteEvent(eventId: number | null) {
  if (eventId === null) return;
  if (selectedEventId.value === eventId) {
    selectedEventId.value = null;
  }
  events.value = events.value.filter((event) => event.id !== eventId);
  closeEventMenu();
}

async function saveMacro() {
  const name = macroName.value.trim();
  if (!name) {
    ElMessage.warning("宏名称不能为空。");
    return;
  }
  if (name.length > 20) {
    ElMessage.warning("宏名称不能超过20个字。");
    return;
  }
  if (events.value.length === 0) {
    ElMessage.warning("键鼠事件列表为空，无法保存。");
    return;
  }

  saving.value = true;
  try {
    let newState: MouseMacroState;
    if (isEditMode) {
      newState = await invoke<MouseMacroState>("update_mouse_macro", {
        request: {
          id: Number(editMacroId),
          name,
          events: events.value.flatMap(stripDraftId),
        },
      });
    } else {
      newState = await invoke<MouseMacroState>("create_mouse_macro", {
        request: {
          name,
          events: events.value.flatMap(stripDraftId),
        },
      });
    }
    await emitTo("main", "mouse-macro-state", newState);
    ElMessage.success(isEditMode ? "已更新宏方案。" : "已保存宏方案。");
    await closeWindow();
  } catch (error) {
    ElMessage.error(String(error));
  } finally {
    saving.value = false;
  }
}

function stripDraftId(event: DraftEvent): MouseMacroEvent[] {
  if (event.kind === "mouseMove" && event.followUp) {
    return [
      { kind: "mouseMove", x: event.x, y: event.y } as MouseMacroEvent,
      { kind: event.followUp, button: "left" } as MouseMacroEvent,
    ];
  }
  const { id: _id, followUp: _followUp, ...rest } = event;
  return [rest as unknown as MouseMacroEvent];
}

function mergeFollowUpEvents(rawEvents: MouseMacroEvent[]): DraftEvent[] {
  const result: DraftEvent[] = [];
  let i = 0;
  while (i < rawEvents.length) {
    const event = rawEvents[i];
    const nextEvent = i + 1 < rawEvents.length ? rawEvents[i + 1] : null;
    if (
      event.kind === "mouseMove" &&
      nextEvent &&
      (nextEvent.kind === "mouseClick" || nextEvent.kind === "mouseDoubleClick")
    ) {
      result.push({
        kind: "mouseMove",
        x: event.x,
        y: event.y,
        followUp: nextEvent.kind,
        id: nextEventId++,
      } as DraftEvent);
      i += 2;
    } else {
      result.push({ ...event, id: nextEventId++ } as DraftEvent);
      i += 1;
    }
  }
  return result;
}

async function toggleAlwaysOnTop() {
  const next = !alwaysOnTop.value;
  await currentWindow.setAlwaysOnTop(next);
  alwaysOnTop.value = next;
}

async function closeWindow() {
  await currentWindow.close();
}

async function startWindowDrag() {
  await currentWindow.startDragging();
}

function eventAction(event: DraftEvent) {
  if (event.kind === "mouseMove" && event.followUp) {
    return event.followUp === "mouseClick" ? "移动到 → 点击" : "移动到 → 双击";
  }
  switch (event.kind) {
    case "mouseClick":
      return "鼠标单击";
    case "mouseDoubleClick":
      return "鼠标双击";
    case "mouseDown":
      return "鼠标按下";
    case "mouseUp":
      return "鼠标释放";
    case "mouseMove":
      return "移动到";
    case "keyClick":
      return "键盘点击";
    case "keyDown":
      return "键盘按下";
    case "keyUp":
      return "键盘释放";
    case "delay":
      return "延迟等待";
    case "findImage":
      return "找图";
    case "findColor":
      return "找色";
    case "findText":
      return "文字识别";
  }
}

function eventTarget(event: DraftEvent) {
  if (event.kind === "mouseMove" && event.followUp) {
    return `x ${event.x}, y ${event.y}`;
  }
  switch (event.kind) {
    case "mouseClick":
    case "mouseDoubleClick":
    case "mouseDown":
    case "mouseUp":
      return buttonLabel(event.button);
    case "mouseMove":
      return `x ${event.x}, y ${event.y}`;
    case "keyClick":
    case "keyDown":
    case "keyUp":
      return event.key;
    case "delay":
      return `${event.ms} ms`;
    case "findImage": {
      const actionLabel = event.action === "click" ? "点击" : event.action === "doubleClick" ? "双击" : "移动到";
      const offset = event.offsetX || event.offsetY ? ` · 偏移(${event.offsetX}, ${event.offsetY})` : "";
      return `${event.threshold}% · ${actionLabel}${offset}`;
    }
    case "findColor": {
      const actionLabel = event.action === "click" ? "点击" : event.action === "doubleClick" ? "双击" : "移动到";
      const offset = event.offsetX || event.offsetY ? ` · 偏移(${event.offsetX}, ${event.offsetY})` : "";
      return `${event.color} · ${actionLabel}${offset}`;
    }
    case "findText": {
      const actionLabel = event.action === "click" ? "点击" : event.action === "doubleClick" ? "双击" : "移动到";
      const offset = event.offsetX || event.offsetY ? ` · 偏移(${event.offsetX}, ${event.offsetY})` : "";
      return `${event.text} · ${actionLabel}${offset}`;
    }
  }
}

function buttonLabel(button: MouseButton) {
  return mouseButtonOptions.find((item) => item.value === button)?.label ?? button;
}
</script>

<template>
  <main class="macro-editor-shell" :class="{ picking: pickingCoordinate }">
    <header class="titlebar" @mousedown="startWindowDrag">
      <div class="titlebar-title">
        <img src="/app-icon.png" alt="" class="titlebar-icon" />
        <span>{{ isEditMode ? "编辑鼠标宏" : "新增鼠标宏" }}</span>
        <el-tag class="count-tag" type="success" effect="light" size="small">
          {{ events.length }} 个事件
        </el-tag>
      </div>
      <div class="window-actions" @mousedown.stop>
        <button
          class="window-action"
          :class="{ active: alwaysOnTop }"
          type="button"
          title="置顶"
          aria-label="置顶"
          @click="toggleAlwaysOnTop"
        >
          <el-icon><Top /></el-icon>
        </button>
        <button
          class="window-action close"
          type="button"
          title="关闭"
          aria-label="关闭"
          @click="closeWindow"
        >
          <el-icon><Close /></el-icon>
        </button>
      </div>
    </header>

    <header class="macro-header">
      <el-input
        v-model="macroName"
        maxlength="20"
        show-word-limit
        placeholder="宏名称"
        size="large"
      />
      <el-button
        type="primary"
        size="large"
        :icon="DocumentChecked"
        :loading="saving"
        :disabled="!canSave"
        @click="saveMacro"
      >
        保存
      </el-button>
    </header>

    <section class="macro-workspace">
      <div class="event-list">
        <el-empty v-if="events.length === 0" description="还没有键鼠事件" />
        <template v-for="(event, index) in events" v-else :key="event.id">
          <div v-if="dropIndicatorIndex === index" class="drop-indicator" />
          <div
            class="event-row"
            :class="{ composite: event.followUp, selected: selectedEventId === event.id }"
            role="button"
            tabindex="0"
            @mousedown="handleMouseDown($event, index)"
            @contextmenu="openEventMenu($event, event)"
          >
            <span class="event-index">#{{ index + 1 }}</span>
            <span class="event-action">{{ eventAction(event) }}</span>
            <span class="event-target">{{ eventTarget(event) }}</span>
          </div>
        </template>
        <div v-if="dropIndicatorIndex === events.length" class="drop-indicator" />
      </div>

      <aside class="operation-panel">
        <el-form label-position="top" class="operation-form">
          <el-form-item>
            <template #label>
              <span class="operation-object-label">
                <span>操作对象</span>
                <span v-if="operationObject === 'findText'" class="operation-object-hint">
                  若非必要请使用找图
                </span>
              </span>
            </template>
            <el-select v-model="operationObject">
              <el-option
                v-for="option in operationObjectOptions"
                :key="option.value"
                :label="option.label"
                :value="option.value"
              />
            </el-select>
          </el-form-item>

          <el-form-item v-if="operationObject === 'mouse'" label="操作">
            <el-select v-model="mouseOperation">
              <el-option
                v-for="option in mouseOperationOptions"
                :key="option.value"
                :label="option.label"
                :value="option.value"
              />
            </el-select>
          </el-form-item>

          <el-form-item v-if="operationObject === 'keyboard'" label="操作">
            <el-select v-model="keyboardOperation">
              <el-option
                v-for="option in keyboardOperationOptions"
                :key="option.value"
                :label="option.label"
                :value="option.value"
              />
            </el-select>
          </el-form-item>

          <el-form-item
            v-if="operationObject === 'mouse' && mouseOperation !== 'mouseMove'"
            label="鼠标按键"
          >
            <el-segmented v-model="selectedButton" :options="mouseButtonOptions" block />
          </el-form-item>

          <el-form-item
            v-if="operationObject === 'mouse' && mouseOperation === 'mouseMove'"
          >
            <template #label>
              <span class="coordinate-label">
                坐标
                <el-tooltip
                  content="点击拾取位置"
                  placement="top"
                >
                  <button
                    class="coordinate-pick-btn"
                    :class="{ active: pickingCoordinate }"
                    type="button"
                    aria-label="取坐标"
                    @click.stop.prevent="startCoordinatePick()"
                  >
                    <el-icon><Aim /></el-icon>
                  </button>
                </el-tooltip>
              </span>
            </template>
            <div class="coordinate-row">
              <el-input-number
                v-model="moveX"
                :min="0"
                :step="1"
                :precision="0"
                controls-position="right"
                placeholder="X"
                @change="handleMoveXChange"
              />
              <el-input-number
                v-model="moveY"
                :min="0"
                :step="1"
                :precision="0"
                controls-position="right"
                placeholder="Y"
                @change="handleMoveYChange"
              />
            </div>
          </el-form-item>

          <el-form-item
            v-if="operationObject === 'mouse' && mouseOperation === 'mouseMove'"
            label="后续操作"
          >
            <el-select v-model="followUpAction">
              <el-option label="无" value="none" />
              <el-option label="左键点击" value="mouseClick" />
              <el-option label="左键双击" value="mouseDoubleClick" />
            </el-select>
          </el-form-item>

          <el-form-item v-if="operationObject === 'keyboard'" label="按键">
            <el-select v-model="selectedKey" filterable>
              <el-option
                v-for="key in keyboardKeys"
                :key="key"
                :label="key"
                :value="key"
              />
            </el-select>
          </el-form-item>

          <el-form-item v-if="operationObject === 'delay'" label="等待时间">
            <div class="delay-row">
              <el-input-number
                v-model="delayMs"
                :min="5"
                :max="60000"
                :step="5"
                :precision="0"
                controls-position="right"
                @change="handleDelayChange"
              />
              <span>毫秒</span>
            </div>
          </el-form-item>

          <template v-if="operationObject === 'findImage'">
            <el-form-item>
              <template #label>
                找图区域
                <span class="region-hint">尽可能缩小区域范围</span>
              </template>
              <div class="find-region-tools">
                <el-button size="small" :disabled="!screenBounds" @click="applyFullSearchRegion">
                  全图
                </el-button>
                <el-button size="small" :icon="Aim" @click="startFindRegionPick">
                  选取区域
                </el-button>
              </div>
              <div class="find-coordinate-grid">
                <span>X1</span>
                <el-input-number
                  v-model="findX1"
                  :step="1"
                  :precision="0"
                  :controls="false"
                  @change="handleFindRegionNumberChange"
                />
                <span>Y1</span>
                <el-input-number
                  v-model="findY1"
                  :step="1"
                  :precision="0"
                  :controls="false"
                  @change="handleFindRegionNumberChange"
                />
                <span>X2</span>
                <el-input-number
                  v-model="findX2"
                  :step="1"
                  :precision="0"
                  :controls="false"
                  @change="handleFindRegionNumberChange"
                />
                <span>Y2</span>
                <el-input-number
                  v-model="findY2"
                  :step="1"
                  :precision="0"
                  :controls="false"
                  @change="handleFindRegionNumberChange"
                />
              </div>
            </el-form-item>

            <el-form-item label="要查找的图">
              <div class="find-image-box">
                <div class="find-preview">
                  <img v-if="findImageData" :src="findImageData" alt="" />
                  <span v-else>无图片</span>
                </div>
                <div class="find-image-actions">
                  <el-button
                    size="small"
                    :icon="Search"
                    :loading="findingTest"
                    :disabled="!findImageData"
                    @click="testFindImage"
                  >
                    测试
                  </el-button>
                  <el-button
                    size="small"
                    :icon="Camera"
                    :loading="capturingTemplate"
                    @click="captureFindTemplate"
                  >
                    屏幕截图
                  </el-button>
                  <el-button size="small" :icon="FolderOpened" @click="openLocalImagePicker">
                    本地图片
                  </el-button>
                  <el-button size="small" :icon="Delete" :disabled="!findImageData" @click="clearFindImage">
                    清除图片
                  </el-button>
                </div>
              </div>
              <input
                ref="fileInputRef"
                class="file-input"
                type="file"
                accept="image/*"
                @change="handleLocalImageChange"
              />
              <span v-if="findImageName" class="find-image-name">{{ findImageName }}</span>
            </el-form-item>

            <el-form-item label="匹配度大于" class="inline-find-field">
              <div class="threshold-row">
                <el-input-number
                  v-model="findThreshold"
                  :min="1"
                  :max="100"
                  :step="1"
                  :precision="0"
                  controls-position="right"
                  @change="handleThresholdChange"
                />
                <span>%</span>
              </div>
            </el-form-item>

            <el-form-item label="后续操作" class="inline-find-field">
              <el-select v-model="findAction">
                <el-option label="点击" value="click" />
                <el-option label="双击" value="doubleClick" />
                <el-option label="移动到" value="move" />
              </el-select>
            </el-form-item>

            <el-form-item>
              <div class="offset-row">
                <span>X偏</span>
                <el-input-number
                  v-model="findImageOffsetX"
                  :min="-500"
                  :max="500"
                  :step="1"
                  :precision="0"
                  :controls="false"
                />
                <span>Y偏</span>
                <el-input-number
                  v-model="findImageOffsetY"
                  :min="-500"
                  :max="500"
                  :step="1"
                  :precision="0"
                  :controls="false"
                />
              </div>
            </el-form-item>

            <el-checkbox v-model="findWaitUntilFound" class="wait-until-found">
              直到找到为止(一般不勾选)
            </el-checkbox>
          </template>

          <template v-if="operationObject === 'findColor'">
            <el-form-item>
              <template #label>
                找色区域
                <span class="region-hint">尽可能缩小区域范围</span>
              </template>
              <div class="find-region-tools">
                <el-button size="small" :disabled="!screenBounds" @click="applyFullSearchRegion">
                  全图
                </el-button>
                <el-button size="small" :icon="Aim" @click="startFindRegionPick">
                  选取区域
                </el-button>
              </div>
              <div class="find-coordinate-grid">
                <span>X1</span>
                <el-input-number
                  v-model="findX1"
                  :step="1"
                  :precision="0"
                  :controls="false"
                  @change="handleFindRegionNumberChange"
                />
                <span>Y1</span>
                <el-input-number
                  v-model="findY1"
                  :step="1"
                  :precision="0"
                  :controls="false"
                  @change="handleFindRegionNumberChange"
                />
                <span>X2</span>
                <el-input-number
                  v-model="findX2"
                  :step="1"
                  :precision="0"
                  :controls="false"
                  @change="handleFindRegionNumberChange"
                />
                <span>Y2</span>
                <el-input-number
                  v-model="findY2"
                  :step="1"
                  :precision="0"
                  :controls="false"
                  @change="handleFindRegionNumberChange"
                />
              </div>
            </el-form-item>

            <el-form-item label="目标颜色">
              <div class="find-color-box">
                <div class="find-color-preview" :style="{ backgroundColor: findColorHex }"></div>
                <div class="find-color-inputs">
                  <el-input
                    v-model="findColorHex"
                    maxlength="7"
                    placeholder="#RRGGBB"
                    @change="handleColorHexChange"
                  />
                  <el-button
                    size="small"
                    :icon="Aim"
                    :class="{ active: pickingColor }"
                    @click="startColorPick"
                  >
                    屏幕取色
                  </el-button>
                </div>
              </div>
            </el-form-item>

            <el-form-item label="色差阈值" class="inline-find-field">
              <div class="threshold-row">
                <el-input-number
                  v-model="findColorThreshold"
                  :min="0"
                  :max="255"
                  :step="1"
                  :precision="0"
                  controls-position="right"
                  @change="handleColorThresholdChange"
                />
                <span>(0~255)</span>
              </div>
            </el-form-item>

            <el-form-item label="后续操作" class="inline-find-field">
              <el-select v-model="findColorAction">
                <el-option label="点击" value="click" />
                <el-option label="双击" value="doubleClick" />
                <el-option label="移动到" value="move" />
              </el-select>
            </el-form-item>

            <el-form-item>
              <div class="offset-row">
                <span>X偏</span>
                <el-input-number
                  v-model="findColorOffsetX"
                  :min="-500"
                  :max="500"
                  :step="1"
                  :precision="0"
                  :controls="false"
                />
                <span>Y偏</span>
                <el-input-number
                  v-model="findColorOffsetY"
                  :min="-500"
                  :max="500"
                  :step="1"
                  :precision="0"
                  :controls="false"
                />
              </div>
            </el-form-item>

            <el-checkbox v-model="findColorWaitUntilFound" class="wait-until-found">
              直到找到为止(一般不勾选)
            </el-checkbox>

            <el-button
              size="small"
              :icon="Search"
              :loading="findingColorTest"
              :disabled="!canAddEvent"
              @click="testFindColor"
            >
              测试
            </el-button>
          </template>

          <template v-if="operationObject === 'findText'">
            <div v-if="ocrInstalled === false" class="ocr-install-hint">
              <el-alert
                title="文字识别引擎未安装"
                type="warning"
                :closable="false"
                show-icon
              >
                <template #default>
                  <div class="ocr-install-hint-content">
                    <span>请安装 RapidOCR-json 文字识别引擎以使用此功能。</span>
                    <el-button size="small" type="primary" @click="ocrInstallDialogVisible = true">
                      安装引擎
                    </el-button>
                  </div>
                </template>
              </el-alert>
            </div>

            <el-form-item>
              <template #label>
                识别区域
                <span class="region-hint">尽可能缩小区域范围</span>
              </template>
              <div class="find-region-tools">
                <el-button size="small" :disabled="!screenBounds" @click="applyFullSearchRegion">
                  全图
                </el-button>
                <el-button size="small" :icon="Aim" @click="startFindRegionPick">
                  选取区域
                </el-button>
              </div>
              <div class="find-coordinate-grid">
                <span>X1</span>
                <el-input-number
                  v-model="findX1"
                  :step="1"
                  :precision="0"
                  :controls="false"
                  @change="handleFindRegionNumberChange"
                />
                <span>Y1</span>
                <el-input-number
                  v-model="findY1"
                  :step="1"
                  :precision="0"
                  :controls="false"
                  @change="handleFindRegionNumberChange"
                />
                <span>X2</span>
                <el-input-number
                  v-model="findX2"
                  :step="1"
                  :precision="0"
                  :controls="false"
                  @change="handleFindRegionNumberChange"
                />
                <span>Y2</span>
                <el-input-number
                  v-model="findY2"
                  :step="1"
                  :precision="0"
                  :controls="false"
                  @change="handleFindRegionNumberChange"
                />
              </div>
            </el-form-item>

            <el-form-item label="查找文字">
              <el-input
                v-model="findTextValue"
                placeholder="输入要识别的文字"
                maxlength="7"
                show-word-limit
              />
            </el-form-item>

            <el-form-item label="后续操作" class="inline-find-field">
              <el-select v-model="findTextAction">
                <el-option label="点击" value="click" />
                <el-option label="双击" value="doubleClick" />
                <el-option label="移动到" value="move" />
              </el-select>
            </el-form-item>

            <el-form-item>
              <div class="offset-row">
                <span>X偏</span>
                <el-input-number
                  v-model="findTextOffsetX"
                  :min="-500"
                  :max="500"
                  :step="1"
                  :precision="0"
                  :controls="false"
                />
                <span>Y偏</span>
                <el-input-number
                  v-model="findTextOffsetY"
                  :min="-500"
                  :max="500"
                  :step="1"
                  :precision="0"
                  :controls="false"
                />
              </div>
            </el-form-item>

            <el-checkbox v-model="findTextWaitUntilFound" class="wait-until-found">
              直到找到为止(一般不勾选)
            </el-checkbox>

            <el-button
              size="small"
              :icon="Search"
              :loading="findingTextTest"
              :disabled="!canAddEvent || ocrInstalled !== true"
              @click="testFindText"
            >
              测试
            </el-button>
          </template>
        </el-form>

        <div v-if="operationObject !== 'delay' && operationObject !== 'findImage' && operationObject !== 'findColor' && operationObject !== 'findText'" class="append-delay-row">
          <el-checkbox v-model="appendDelay" size="small">添加延迟</el-checkbox>
          <el-input-number
            v-model="appendDelayMs"
            :min="5"
            :max="60000"
            :step="5"
            :precision="0"
            controls-position="right"
            :disabled="!appendDelay"
            size="small"
          />
          <span :class="{ disabled: !appendDelay }">毫秒</span>
        </div>

        <div class="action-buttons" :class="{ split: selectedEventId !== null }">
          <el-button type="primary" plain :icon="Plus" :disabled="!canAddEvent" @click="addEvent">
            添加
          </el-button>
          <el-button
            v-if="selectedEventId !== null"
            type="warning"
            plain
            :disabled="!canAddEvent"
            @click="updateEvent"
          >
            变更
          </el-button>
        </div>
      </aside>
    </section>

    <div
      v-if="eventMenu.visible"
      class="event-context-menu"
      :style="{ left: `${eventMenu.x}px`, top: `${eventMenu.y}px` }"
      @click.stop
      @mousedown.stop
      @contextmenu.prevent
    >
      <button type="button" class="event-context-item danger" @click="deleteEvent(eventMenu.eventId)">
        删除
      </button>
    </div>
    <el-dialog
      v-model="ocrInstallDialogVisible"
      title="安装插件"
      width="380px"
      :close-on-click-modal="false"
      :close-on-press-escape="!ocrInstalling"
      :show-close="!ocrInstalling"
      align-center
    >
      <div class="ocr-install-dialog">
        <div class="ocr-install-title">
          <span>文字识别</span>
          <span class="ocr-install-subtitle">(RapidOCR-json)</span>
        </div>
        <div class="ocr-install-desc">
          <p class="ocr-install-label">插件说明：</p>
          <div class="ocr-install-desc-box">
            此插件支持中(简/繁)文、英文、日文、韩文、俄文等多种语言、识别快、准确度高。
          </div>
          <p class="ocr-install-label">仓库地址：</p>
          <button type="button" class="ocr-repo-link" @click="openOcrRepo">
            https://github.com/hiroi-sora/RapidOCR-json
          </button>
        </div>
        <p class="ocr-install-status">{{ ocrInstallMessage }}</p>
        <el-progress
          :percentage="ocrInstallProgress"
          :status="ocrInstallStatus === 'error' ? 'exception' : undefined"
          :show-text="ocrInstallStatus !== 'ready'"
        />
      </div>
      <template #footer>
        <el-button
          type="success"
          :loading="ocrInstalling"
          :disabled="ocrInstalling"
          @click="installOcrEngine"
        >
          {{ ocrInstalling ? "安装中..." : "安装插件" }}
        </el-button>
      </template>
    </el-dialog>
  </main>
</template>

<style scoped>
.macro-editor-shell {
  display: grid;
  grid-template-rows: 36px auto minmax(0, 1fr);
  gap: 12px;
  height: 100vh;
  padding: 0 18px 18px;
  overflow: hidden;
  color: var(--el-text-color-primary);
  background: var(--el-bg-color-page);
}

.macro-editor-shell.picking {
  cursor: crosshair;
}

.titlebar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 36px;
  margin: 0 -18px;
  color: var(--el-text-color-primary);
  background: var(--el-bg-color);
  border-bottom: 1px solid var(--el-border-color-lighter);
  user-select: none;
}

.titlebar-title {
  display: flex;
  align-items: center;
  gap: 8px;
  flex: 1;
  min-width: 0;
  padding-left: 14px;
  overflow: hidden;
  font-size: 13px;
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.titlebar-icon {
  width: 18px;
  height: 18px;
  object-fit: contain;
}

.count-tag {
  font-weight: 700;
}

.window-actions {
  display: flex;
  align-items: center;
  height: 100%;
}

.window-action {
  display: grid;
  width: 46px;
  height: 100%;
  padding: 0;
  place-items: center;
  color: var(--el-text-color-regular);
  background: transparent;
  border: 0;
  border-radius: 0;
  cursor: pointer;
}

.window-action:hover,
.window-action.active {
  color: var(--el-text-color-primary);
  background: var(--el-fill-color-light);
}

.window-action.active {
  color: #ffffff;
  background: var(--el-color-primary);
  box-shadow: inset 0 -2px 0 var(--el-color-primary-dark-2);
}

.window-action.active:hover {
  color: #ffffff;
  background: var(--el-color-primary-dark-2);
}

.window-action.close:hover {
  color: #ffffff;
  background: #e81123;
}

.macro-header {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 108px;
  gap: 10px;
  align-items: center;
}

.macro-workspace {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 278px;
  gap: 10px;
  min-height: 0;
}

.event-list {
  display: grid;
  align-content: start;
  gap: 8px;
  min-height: 0;
  padding: 10px;
  overflow-y: auto;
  overflow-x: hidden;
  background: var(--el-bg-color);
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 8px;
}

.event-row {
  display: grid;
  grid-template-columns: 64px minmax(110px, auto) minmax(0, 1fr);
  gap: 10px;
  align-items: center;
  min-height: 40px;
  padding: 8px 10px;
  color: var(--el-text-color-regular);
  text-align: left;
  background: var(--el-fill-color-blank);
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 8px;
  cursor: grab;
  user-select: none;
}

.event-row:hover {
  border-color: var(--el-border-color);
  background: var(--el-fill-color-light);
}

.event-index {
  color: var(--el-text-color-secondary);
  font-size: 12px;
  font-weight: 700;
}

.event-action {
  color: var(--el-color-primary);
  font-size: 13px;
  font-weight: 700;
  white-space: nowrap;
}

.event-target {
  min-width: 0;
  overflow: hidden;
  color: var(--el-text-color-primary);
  font-size: 13px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.operation-panel {
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-height: 0;
  padding: 12px;
  overflow: hidden;
  background: var(--el-bg-color);
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 8px;
}

.operation-form {
  display: grid;
  gap: 10px;
  min-height: 0;
  overflow-y: auto;
  overflow-x: hidden;
  padding-right: 2px;
}

.operation-form::-webkit-scrollbar {
  display: none;
}

.operation-form :deep(.el-form-item) {
  margin-bottom: 0;
}

.operation-form :deep(.el-form-item__label) {
  padding-bottom: 4px;
  color: var(--el-text-color-regular);
  font-size: 13px;
  font-weight: 600;
}

.operation-object-label {
  display: inline-flex;
  align-items: center;
  gap: 8px;
}

.operation-object-hint {
  color: #f56c6c;
  font-size: 12px;
  font-weight: 500;
}

.operation-form :deep(.el-select),
.operation-form :deep(.el-input-number),
.operation-panel :deep(.el-button) {
  width: 100%;
}

.coordinate-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  gap: 8px;
}

.offset-row {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto minmax(0, 1fr);
  gap: 8px;
  align-items: center;
}

.coordinate-label {
  display: inline-flex;
  align-items: center;
  gap: 5px;
}

.coordinate-pick-btn {
  display: inline-grid;
  width: 20px;
  height: 20px;
  padding: 0;
  place-items: center;
  color: var(--el-text-color-secondary);
  background: transparent;
  border: 0;
  border-radius: 4px;
  cursor: pointer;
}

.coordinate-pick-btn:hover,
.coordinate-pick-btn.active {
  color: #ffffff;
  background: var(--el-color-primary);
}

.delay-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 8px;
  align-items: center;
}

.delay-row span {
  color: var(--el-text-color-regular);
  font-size: 13px;
  font-weight: 600;
  white-space: nowrap;
}

.find-region-tools {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px;
  width: 100%;
  margin-bottom: 8px;
}

.find-region-tools :deep(.el-button + .el-button) {
  margin-left: 0;
}

.find-coordinate-grid {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto minmax(0, 1fr);
  gap: 8px 6px;
  align-items: center;
}

.find-coordinate-grid span,
.threshold-row span,
.offset-row span {
  color: var(--el-text-color-regular);
  font-size: 13px;
  font-weight: 700;
  white-space: nowrap;
}

.find-image-box {
  display: grid;
  grid-template-columns: 96px minmax(0, 1fr);
  gap: 10px;
  width: 100%;
}

.find-preview {
  display: grid;
  width: 96px;
  height: 96px;
  place-items: center;
  overflow: hidden;
  color: var(--el-text-color-placeholder);
  font-size: 12px;
  background: var(--el-fill-color-light);
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 6px;
}

.find-preview img {
  max-width: 100%;
  max-height: 100%;
  object-fit: contain;
}

.find-image-actions {
  display: grid;
  gap: 7px;
  min-width: 0;
}

.find-image-actions :deep(.el-button + .el-button) {
  margin-left: 0;
}

.file-input {
  display: none;
}

.find-image-name {
  display: block;
  max-width: 100%;
  margin-top: 6px;
  overflow: hidden;
  color: var(--el-text-color-secondary);
  font-size: 12px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.threshold-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 8px;
  align-items: center;
}

.inline-find-field {
  display: grid;
  grid-template-columns: 78px minmax(0, 1fr);
  gap: 8px;
  align-items: center;
}

.inline-find-field :deep(.el-form-item__label) {
  justify-content: flex-start;
  min-width: 0;
  padding-bottom: 0;
  overflow: hidden;
  line-height: 32px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.inline-find-field :deep(.el-form-item__content) {
  min-width: 0;
}

.inline-find-field .threshold-row {
  grid-template-columns: minmax(88px, 118px) auto;
  width: 100%;
}

.inline-find-field :deep(.el-select) {
  width: 100%;
}

.wait-until-found {
  width: fit-content;
}

.find-color-box {
  display: grid;
  grid-template-columns: 48px minmax(0, 1fr);
  gap: 10px;
  width: 100%;
}

.find-color-preview {
  width: 48px;
  height: 48px;
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 6px;
}

.find-color-inputs {
  display: grid;
  gap: 8px;
  min-width: 0;
}



.append-delay-row {
  display: flex;
  flex: 0 0 auto;
  align-items: center;
  gap: 8px;
  padding: 8px 10px;
  background: var(--el-fill-color-light);
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 6px;
}

.append-delay-row .el-input-number {
  width: 110px;
}

.append-delay-row span {
  color: var(--el-text-color-regular);
  font-size: 12px;
  font-weight: 600;
  white-space: nowrap;
}

.append-delay-row span.disabled {
  color: var(--el-text-color-placeholder);
}

.event-context-menu {
  position: fixed;
  z-index: 4000;
  display: grid;
  width: 112px;
  padding: 6px;
  background: var(--el-bg-color-overlay);
  border: 1px solid var(--el-border-color-light);
  border-radius: 6px;
  box-shadow: var(--el-box-shadow-light);
}

.event-context-item {
  height: 30px;
  padding: 0 10px;
  color: var(--el-text-color-regular);
  font-size: 13px;
  text-align: left;
  background: transparent;
  border: 0;
  border-radius: 4px;
  cursor: pointer;
}

.event-context-item:hover {
  color: var(--el-color-primary);
  background: var(--el-fill-color-light);
}

.event-context-item.danger {
  color: var(--el-color-danger);
}

.event-context-item.danger:hover {
  color: #ffffff;
  background: var(--el-color-danger);
}

.event-row:active {
  cursor: grabbing;
}

.event-row.selected {
  border-color: var(--el-color-primary);
  background: var(--el-fill-color-light);
  box-shadow: 0 0 0 2px var(--el-color-primary-light-8);
}

.event-row.selected:hover {
  border-color: var(--el-color-primary-dark-2);
  background: var(--el-fill-color);
}

.event-row.composite {
  border-left: 3px solid var(--el-color-primary);
  padding-left: 7px;
}

.drop-indicator {
  height: 2px;
  background: var(--el-color-primary);
  border-radius: 1px;
  margin: 2px 0;
  pointer-events: none;
}

.action-buttons {
  display: grid;
  flex: 0 0 auto;
  width: 100%;
  gap: 8px;
}

.action-buttons:not(.split) {
  grid-template-columns: 1fr;
}

.action-buttons.split {
  grid-template-columns: 1fr 1fr;
}

/* 覆盖 Element Plus 相邻按钮默认的 12px 左间距，确保 grid 对齐 */
.action-buttons :deep(.el-button + .el-button) {
  margin-left: 0;
}

.ocr-install-hint {
  margin-bottom: 10px;
}

.ocr-install-hint-content {
  display: flex;
  flex-direction: column;
  gap: 8px;
  align-items: flex-start;
}

.ocr-install-dialog {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.ocr-install-title {
  font-size: 18px;
  font-weight: 600;
}

.ocr-install-subtitle {
  color: var(--el-color-success);
  font-weight: 500;
}

.ocr-install-label {
  margin: 0 0 4px;
  font-size: 14px;
  font-weight: 600;
}

.ocr-install-desc-box {
  padding: 8px 10px;
  font-size: 13px;
  line-height: 1.6;
  background: var(--el-fill-color-light);
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 4px;
}

.ocr-install-status {
  margin: 4px 0 0;
  color: var(--el-text-color-secondary);
  font-size: 13px;
  text-align: center;
}

.ocr-repo-link {
  display: block;
  width: 100%;
  padding: 6px 10px;
  overflow: hidden;
  color: var(--el-color-primary);
  font-size: 13px;
  text-align: left;
  text-overflow: ellipsis;
  white-space: nowrap;
  background: var(--el-fill-color-light);
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 4px;
  cursor: pointer;
}

.ocr-repo-link:hover {
  text-decoration: underline;
}

.region-hint {
  margin-left: 6px;
  color: var(--el-color-warning);
  font-size: 12px;
  font-weight: 400;
}
</style>
