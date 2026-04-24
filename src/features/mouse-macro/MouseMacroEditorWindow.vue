<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { emitTo, listen, TauriEvent, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { Aim, Close, DocumentChecked, Plus, Top } from "@element-plus/icons-vue";
import { ElMessage } from "element-plus";
import {
  keyboardKeys,
  mouseButtonOptions,
  type MouseButton,
  type MouseMacroEvent,
  type MouseMacroDetail,
  type MouseMacroState,
} from "../../types";

type OperationObject = "mouse" | "keyboard" | "delay";
type MouseOperation = "mouseClick" | "mouseDoubleClick" | "mouseDown" | "mouseUp" | "mouseMove";
type KeyboardOperation = "keyClick" | "keyDown" | "keyUp";

type DraftEvent = MouseMacroEvent & {
  id: number;
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

const currentWindow = getCurrentWindow();

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
const appendDelay = ref(false);
const appendDelayMs = ref(100);
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
] as const;

const canSave = computed(() => macroName.value.trim().length > 0 && events.value.length > 0);
const canAddEvent = computed(() => {
  if (operationObject.value === "delay") {
    return Number.isInteger(delayMs.value) && delayMs.value >= 5 && delayMs.value <= 60000;
  }
  if (operationObject.value === "mouse" && mouseOperation.value === "mouseMove") {
    return Number.isInteger(moveX.value) && Number.isInteger(moveY.value) && moveX.value >= 0 && moveY.value >= 0;
  }
  return true;
});

onMounted(async () => {
  alwaysOnTop.value = await currentWindow.isAlwaysOnTop();

  if (isEditMode) {
    try {
      const detail = await invoke<MouseMacroDetail>("get_mouse_macro_detail", {
        id: Number(editMacroId),
      });
      macroName.value = detail.name;
      events.value = detail.events.map((event) => ({ ...event, id: nextEventId++ }));
    } catch (error) {
      ElMessage.error(String(error));
    }
  }

  unlistenCoordinate = await listen<PickedCoordinate>("mouse-coordinate-picked", (event) => {
    moveX.value = normalizeInteger(event.payload.x, 0, 0);
    moveY.value = normalizeInteger(event.payload.y, 0, 0);
    pickingCoordinate.value = false;
    ElMessage.success("已获取坐标。");
  });
  document.addEventListener("click", closeEventMenu);
  document.addEventListener("keydown", handleDocumentKeydown);
});

onBeforeUnmount(() => {
  unlistenCoordinate?.();
  if (pickingCoordinate.value) {
    void cancelCoordinatePick();
  }
  document.removeEventListener("click", closeEventMenu);
  document.removeEventListener("keydown", handleDocumentKeydown);
});

watch(operationObject, (value) => {
  closeEventMenu();
  appendDelay.value = false;
  if (value === "mouse") {
    mouseOperation.value = "mouseClick";
  } else if (value === "keyboard") {
    keyboardOperation.value = "keyClick";
  }
});

watch(mouseOperation, () => {
  appendDelay.value = false;
});

watch(keyboardOperation, () => {
  appendDelay.value = false;
});

function defaultMacroName() {
  return `宏方案 ${Date.now()}`;
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

function handleDelayChange(value: number | undefined) {
  delayMs.value = normalizeInteger(value, 100, 5, 60000);
}

function handleMoveXChange(value: number | undefined) {
  moveX.value = normalizeInteger(value, 0, 0);
}

function handleMoveYChange(value: number | undefined) {
  moveY.value = normalizeInteger(value, 0, 0);
}

async function startCoordinatePick() {
  if (pickingCoordinate.value) {
    await cancelCoordinatePick();
    return;
  }

  closeEventMenu();
  pickingCoordinate.value = true;
  try {
    const snapshot = await invoke<CoordinatePickSnapshotMeta>("start_mouse_coordinate_pick", {
      windowLabel: currentWindow.label,
    });

    const label = `coordinate-picker-${Date.now()}`;
    const picker = new WebviewWindow(label, {
      url: "/index.html?view=coordinate-picker",
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
    });
    picker.once("tauri://error", async (event) => {
      pickingCoordinate.value = false;
      await cancelCoordinatePick();
      ElMessage.error(String(event.payload));
    });
  } catch (error) {
    pickingCoordinate.value = false;
    ElMessage.error(String(error));
  }
}

async function cancelCoordinatePick() {
  pickingCoordinate.value = false;
  try {
    await invoke("cancel_mouse_coordinate_pick");
  } catch {}
}

function addEvent() {
  if (!canAddEvent.value) {
    ElMessage.warning("请先补全有效的操作输入。");
    return;
  }

  const newEvents: DraftEvent[] = [];
  const event = buildEvent();
  newEvents.push({ ...event, id: nextEventId++ } as DraftEvent);

  if (operationObject.value !== "delay" && appendDelay.value) {
    newEvents.push({
      kind: "delay",
      ms: normalizeInteger(appendDelayMs.value, 100, 5, 60000),
      id: nextEventId++,
    } as DraftEvent);
  }

  events.value = [...events.value, ...newEvents];
  closeEventMenu();
}

function buildEvent(): MouseMacroEvent {
  if (operationObject.value === "delay") {
    return { kind: "delay", ms: normalizeInteger(delayMs.value, 100, 5, 60000) };
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
    }
  };

  document.addEventListener("mousemove", onMouseMove);
  document.addEventListener("mouseup", onMouseUp);
}

function deleteEvent(eventId: number | null) {
  if (eventId === null) return;
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
          events: events.value.map(stripDraftId),
        },
      });
    } else {
      newState = await invoke<MouseMacroState>("create_mouse_macro", {
        request: {
          name,
          events: events.value.map(stripDraftId),
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

function stripDraftId(event: DraftEvent): MouseMacroEvent {
  const { id: _id, ...payload } = event;
  return payload;
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

function eventAction(event: MouseMacroEvent) {
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
  }
}

function eventTarget(event: MouseMacroEvent) {
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
          <el-form-item label="操作对象">
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
                    @click.stop.prevent="startCoordinatePick"
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
        </el-form>

        <div v-if="operationObject !== 'delay'" class="append-delay-row">
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

        <el-button type="primary" plain :icon="Plus" :disabled="!canAddEvent" @click="addEvent">
          添加
        </el-button>
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
  grid-template-columns: minmax(0, 1fr) 238px;
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
  grid-template-columns: 64px 110px minmax(0, 1fr);
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
  background: var(--el-bg-color);
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 8px;
}

.operation-form {
  display: grid;
  gap: 10px;
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

.append-delay-row {
  display: flex;
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

.drop-indicator {
  height: 2px;
  background: var(--el-color-primary);
  border-radius: 1px;
  margin: 2px 0;
  pointer-events: none;
}
</style>
