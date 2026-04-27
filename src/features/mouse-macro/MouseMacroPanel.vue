<script setup lang="ts">
import { nextTick, onBeforeUnmount, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { TauriEvent, listen, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { WebviewWindow } from "@tauri-apps/api/webviewWindow";
import { ElMessage, ElMessageBox } from "element-plus";
import { Plus, RefreshRight, VideoPlay } from "@element-plus/icons-vue";
import type { MouseMacroState, MouseMacroSummary } from "../../types";

const MOUSE_MACRO_EDITOR_WINDOW_SIZE_KEY = "mouse-macro-editor-window-size";
const MOUSE_MACRO_EDITOR_DEFAULT_WIDTH = 880;
const MOUSE_MACRO_EDITOR_DEFAULT_HEIGHT = 792;
const MOUSE_MACRO_EDITOR_MIN_WIDTH = 760;
const MOUSE_MACRO_EDITOR_MIN_HEIGHT = 682;

const state = ref<MouseMacroState>({
  macros: [],
  selectedId: null,
  playing: false,
});
const editingId = ref<number | null>(null);
const editingName = ref("");
const renameInput = ref<any[]>([]);
const speedMenu = ref<{
  macroId: number | null;
  x: number;
  y: number;
}>({
  macroId: null,
  x: 0,
  y: 0,
});
const macroMenu = ref<{
  visible: boolean;
  x: number;
  y: number;
  macro: MouseMacroSummary | null;
}>({
  visible: false,
  x: 0,
  y: 0,
  macro: null,
});

const speedOptions = [1, 1.5, 2, 2.5, 3];

let unlistenState: UnlistenFn | undefined;
let unlistenStatus: UnlistenFn | undefined;

onMounted(async () => {
  await refreshState();
  unlistenState = await listen<MouseMacroState>("mouse-macro-state", (event) => {
    state.value = event.payload;
  });
  unlistenStatus = await listen<boolean>("mouse-macro-status", async () => {
    await refreshState();
  });
  document.addEventListener("click", closeFloatingMenus);
  document.addEventListener("keydown", handleDocumentKeydown);
});

onBeforeUnmount(() => {
  unlistenState?.();
  unlistenStatus?.();
  document.removeEventListener("click", closeFloatingMenus);
  document.removeEventListener("keydown", handleDocumentKeydown);
});

async function refreshState() {
  state.value = await invoke<MouseMacroState>("get_mouse_macro_state");
}

async function selectMacro(id: number) {
  if (state.value.playing) return;
  state.value = await invoke<MouseMacroState>("select_mouse_macro", { id });
}

async function beginRename(macro: MouseMacroSummary) {
  if (state.value.playing) return;

  closeFloatingMenus();
  editingId.value = macro.id;
  editingName.value = macro.name;
  await nextTick();
  renameInput.value?.[0]?.focus();
}

async function commitRename() {
  if (editingId.value === null || state.value.playing) return;

  const trimmed = editingName.value.trim();
  if (!trimmed) {
    ElMessage.warning("方案名称不能为空。");
    await nextTick();
    renameInput.value?.[0]?.focus();
    return;
  }
  if (trimmed.length > 20) {
    ElMessage.warning("方案名称不能超过20个字。");
    await nextTick();
    renameInput.value?.[0]?.focus();
    return;
  }

  state.value = await invoke<MouseMacroState>("rename_mouse_macro", {
    request: {
      id: editingId.value,
      name: trimmed,
    },
  });
  editingId.value = null;
}

function getMouseMacroEditorWindowSize() {
  const fallback = {
    width: MOUSE_MACRO_EDITOR_DEFAULT_WIDTH,
    height: MOUSE_MACRO_EDITOR_DEFAULT_HEIGHT,
  };
  const saved = localStorage.getItem(MOUSE_MACRO_EDITOR_WINDOW_SIZE_KEY);
  if (!saved) return fallback;

  try {
    const parsed = JSON.parse(saved) as { width?: number; height?: number };
    const width = Number.isFinite(parsed.width)
      ? Math.max(Math.round(parsed.width ?? fallback.width), MOUSE_MACRO_EDITOR_MIN_WIDTH)
      : fallback.width;
    const height = Number.isFinite(parsed.height)
      ? Math.max(Math.round(parsed.height ?? fallback.height), MOUSE_MACRO_EDITOR_MIN_HEIGHT)
      : fallback.height;
    return { width, height };
  } catch {
    return fallback;
  }
}

function handleMacroCheck(macro: MouseMacroSummary, checked: string | number | boolean) {
  if (!checked || state.value.playing) return;
  void selectMacro(macro.id);
}

async function openCreateWindow() {
  if (state.value.playing) return;

  const label = `mouse-macro-editor-${Date.now()}`;
  const mainWindow = getCurrentWindow();
  const size = getMouseMacroEditorWindowSize();
  await mainWindow.setEnabled(false);

  const restoreMainWindow = async () => {
    await mainWindow.setEnabled(true);
    await mainWindow.setFocus();
    await refreshState();
  };

  const editor = new WebviewWindow(label, {
    url: "/index.html?view=mouse-macro-editor",
    title: "新增鼠标宏",
    width: size.width,
    height: size.height,
    minWidth: MOUSE_MACRO_EDITOR_MIN_WIDTH,
    minHeight: MOUSE_MACRO_EDITOR_MIN_HEIGHT,
    center: true,
    resizable: true,
    decorations: false,
    minimizable: false,
    maximizable: false,
    parent: mainWindow,
    focus: true,
  });

  editor.once("tauri://created", async () => {
    await editor.setFocus();
  });
  editor.once(TauriEvent.WINDOW_DESTROYED, () => {
    void restoreMainWindow();
  });
  editor.once("tauri://error", async (event) => {
    await restoreMainWindow();
    ElMessage.error(String(event.payload));
  });
}

function openMacroMenu(event: MouseEvent, macro: MouseMacroSummary) {
  event.preventDefault();
  if (state.value.playing) return;
  closeSpeedMenu();

  const menuWidth = 112;
  const menuHeight = 84;
  macroMenu.value = {
    visible: true,
    x: Math.min(event.clientX, window.innerWidth - menuWidth - 8),
    y: Math.min(event.clientY, window.innerHeight - menuHeight - 8),
    macro,
  };
}

function closeMacroMenu() {
  macroMenu.value.visible = false;
}

function closeSpeedMenu() {
  speedMenu.value.macroId = null;
}

function closeFloatingMenus() {
  closeMacroMenu();
  closeSpeedMenu();
}

function handleDocumentKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    closeFloatingMenus();
  }
}

async function handleMenuEdit() {
  const macro = macroMenu.value.macro;
  closeMacroMenu();
  if (!macro || state.value.playing) return;
  await openEditWindow(macro.id);
}

async function handleMacroDoubleClick(macro: MouseMacroSummary, event: MouseEvent) {
  if (state.value.playing || editingId.value === macro.id) return;

  const target = event.target as HTMLElement | null;
  if (
    target?.closest(".rename-trigger") ||
    target?.closest(".macro-tags") ||
    target?.closest(".el-checkbox") ||
    target?.closest(".el-input")
  ) {
    return;
  }

  await selectMacro(macro.id);
  await openEditWindow(macro.id);
}

async function openEditWindow(macroId: number) {
  if (state.value.playing) return;

  const label = `mouse-macro-editor-${Date.now()}`;
  const mainWindow = getCurrentWindow();
  const size = getMouseMacroEditorWindowSize();
  await mainWindow.setEnabled(false);

  const restoreMainWindow = async () => {
    await mainWindow.setEnabled(true);
    await mainWindow.setFocus();
    await refreshState();
  };

  const editor = new WebviewWindow(label, {
    url: `/index.html?view=mouse-macro-editor&id=${macroId}`,
    title: "编辑鼠标宏",
    width: size.width,
    height: size.height,
    minWidth: MOUSE_MACRO_EDITOR_MIN_WIDTH,
    minHeight: MOUSE_MACRO_EDITOR_MIN_HEIGHT,
    center: true,
    resizable: true,
    decorations: false,
    minimizable: false,
    maximizable: false,
    parent: mainWindow,
    focus: true,
  });

  editor.once("tauri://created", async () => {
    await editor.setFocus();
  });
  editor.once(TauriEvent.WINDOW_DESTROYED, () => {
    void restoreMainWindow();
  });
  editor.once("tauri://error", async (event) => {
    await restoreMainWindow();
    ElMessage.error(String(event.payload));
  });
}

async function handleMenuDelete() {
  const macro = macroMenu.value.macro;
  closeMacroMenu();
  if (!macro || state.value.playing) return;

  try {
    await ElMessageBox.confirm(`删除“${macro.name}”？`, "删除宏方案", {
      confirmButtonText: "删除",
      cancelButtonText: "取消",
      type: "warning",
    });
  } catch {
    return;
  }

  state.value = await invoke<MouseMacroState>("delete_mouse_macro", { id: macro.id });
}

function formatSpeed(speed: number) {
  return Number.isInteger(speed) ? speed.toFixed(0) : speed.toString();
}

function speedLabel(macro: MouseMacroSummary) {
  return `${formatSpeed(macro.playbackSpeed)}x`;
}

function availableSpeeds(macro: MouseMacroSummary) {
  return speedOptions.filter((speed) => speed !== macro.playbackSpeed);
}

function toggleSpeedMenu(event: MouseEvent, macro: MouseMacroSummary) {
  event.stopPropagation();
  if (state.value.playing) return;
  closeMacroMenu();

  if (speedMenu.value.macroId === macro.id) {
    closeSpeedMenu();
    return;
  }

  const target = event.currentTarget as HTMLElement | null;
  const rect = target?.getBoundingClientRect();
  if (!rect) return;

  const menuWidth = 190;
  const menuHeight = 36;
  const gap = 6;
  const minMargin = 8;
  const left = rect.left + rect.width / 2 - menuWidth / 2;
  const topAbove = rect.top - menuHeight - gap;
  const topBelow = rect.bottom + gap;

  speedMenu.value = {
    macroId: macro.id,
    x: Math.min(Math.max(left, minMargin), window.innerWidth - menuWidth - minMargin),
    y: topAbove >= minMargin ? topAbove : topBelow,
  };
}

async function updateMacroSpeed(macro: MouseMacroSummary, speed: number) {
  if (state.value.playing) return;

  closeSpeedMenu();
  try {
    state.value = await invoke<MouseMacroState>("update_mouse_macro_playback_speed", {
      request: {
        id: macro.id,
        speed,
      },
    });
  } catch (error) {
    ElMessage.error(String(error));
  }
}

async function toggleMacroLoop(macro: MouseMacroSummary) {
  if (state.value.playing) return;

  try {
    state.value = await invoke<MouseMacroState>("update_mouse_macro_loop_playback", {
      request: {
        id: macro.id,
        value: !macro.loopPlayback,
      },
    });
  } catch (error) {
    ElMessage.error(String(error));
  }
}

function formatTime(timestamp: number) {
  return new Date(timestamp).toLocaleString();
}
</script>

<template>
  <section class="macro-panel">
    <div class="macro-toolbar">
      <div>
        <strong>宏方案</strong>
        <span>{{ state.playing ? "正在回放所选宏" : "使用全局启停热键回放当前选择" }}</span>
      </div>
      <el-button type="primary" :icon="Plus" :disabled="state.playing" @click="openCreateWindow">
        新增
      </el-button>
    </div>

    <div class="macro-list">
      <el-empty v-if="state.macros.length === 0" description="还没有鼠标宏方案" />
      <div
        v-for="macro in state.macros"
        v-else
        :key="macro.id"
        class="macro-item"
        :class="{ active: macro.id === state.selectedId, disabled: state.playing }"
        @click="selectMacro(macro.id)"
        @dblclick="handleMacroDoubleClick(macro, $event)"
        @contextmenu="openMacroMenu($event, macro)"
      >
        <el-checkbox
          :model-value="macro.id === state.selectedId"
          :disabled="state.playing"
          @change="handleMacroCheck(macro, $event)"
          @click.stop
        />
        <div class="macro-main">
          <el-input
            v-if="editingId === macro.id"
            ref="renameInput"
            v-model="editingName"
            size="small"
            :maxlength="20"
            :disabled="state.playing"
            show-word-limit
            @click.stop
            @keydown.enter.stop.prevent="commitRename"
            @blur="commitRename"
          />
          <div v-else class="macro-name-row">
            <strong class="rename-trigger" @dblclick.stop="beginRename(macro)">
              {{ macro.name }}
            </strong>
            <div v-if="macro.id === state.selectedId" class="macro-tags" @click.stop>
              <el-tag
                class="option-tag speed-tag"
                :class="{ disabled: state.playing }"
                :type="macro.playbackSpeed !== 1 ? 'danger' : 'success'"
                size="small"
                @click="toggleSpeedMenu($event, macro)"
              >
                {{ speedLabel(macro) }}
              </el-tag>
              <el-tag
                class="option-tag mode-tag"
                :class="{ disabled: state.playing }"
                :type="macro.loopPlayback ? 'warning' : 'info'"
                size="small"
                @click.stop="toggleMacroLoop(macro)"
              >
                <el-icon>
                  <RefreshRight v-if="macro.loopPlayback" />
                  <VideoPlay v-else />
                </el-icon>
                {{ macro.loopPlayback ? "循环" : "单次" }}
              </el-tag>
            </div>
          </div>
          <span>
            {{ formatTime(macro.updatedAt) }} ·
            {{ macro.eventCount }} 个事件
          </span>
        </div>
      </div>
    </div>

    <div
      v-if="macroMenu.visible"
      class="macro-context-menu"
      :style="{ left: `${macroMenu.x}px`, top: `${macroMenu.y}px` }"
      @click.stop
      @mousedown.stop
      @contextmenu.prevent
    >
      <button type="button" class="context-menu-item" @click="handleMenuEdit">
        编辑
      </button>
      <button type="button" class="context-menu-item danger" @click="handleMenuDelete">
        删除
      </button>
    </div>

    <div
      v-if="speedMenu.macroId !== null && !state.playing"
      class="speed-menu"
      :style="{ left: `${speedMenu.x}px`, top: `${speedMenu.y}px` }"
      @click.stop
      @mousedown.stop
    >
      <template
        v-for="macro in state.macros.filter((item) => item.id === speedMenu.macroId)"
        :key="macro.id"
      >
        <button
          v-for="speed in availableSpeeds(macro)"
          :key="speed"
          type="button"
          class="speed-option"
          @click="updateMacroSpeed(macro, speed)"
        >
          {{ formatSpeed(speed) }}x
        </button>
      </template>
    </div>
  </section>
</template>

<style scoped>
.macro-panel {
  display: grid;
  grid-template-rows: auto minmax(0, 1fr);
  gap: 10px;
  height: 100%;
  min-height: 0;
}

.macro-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  min-height: 42px;
  padding: 8px 10px;
  color: var(--el-text-color-regular);
  font-size: 13px;
  background: var(--el-fill-color-light);
  border-radius: 8px;
}

.macro-toolbar > div {
  display: flex;
  align-items: baseline;
  gap: 10px;
  min-width: 0;
}

.macro-toolbar strong {
  color: var(--el-text-color-primary);
  font-size: 13px;
  font-weight: 600;
  white-space: nowrap;
}

.macro-toolbar span {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.macro-list {
  display: grid;
  align-content: start;
  gap: 8px;
  min-height: 0;
  padding: 14px;
  background: var(--el-bg-color);
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 8px;
  overflow-y: auto;
  overflow-x: hidden;
  scrollbar-gutter: stable;
}

.macro-list::-webkit-scrollbar {
  width: 6px;
}

.macro-list::-webkit-scrollbar-track {
  background: transparent;
}

.macro-list::-webkit-scrollbar-thumb {
  background: transparent;
  border-radius: 3px;
}

.macro-list:hover::-webkit-scrollbar-thumb,
.macro-list:active::-webkit-scrollbar-thumb {
  background: var(--el-text-color-placeholder);
}

.macro-list::-webkit-scrollbar-thumb:hover {
  background: var(--el-text-color-secondary);
}

.macro-item {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  gap: 10px;
  align-items: center;
  width: 100%;
  padding: 10px 12px;
  text-align: left;
  background: var(--el-bg-color);
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 8px;
  cursor: pointer;
}

.macro-item.active {
  border-color: var(--el-color-primary);
  box-shadow: 0 0 0 1px var(--el-color-primary-light-8);
}

.macro-item.disabled {
  cursor: not-allowed;
  opacity: 0.72;
}

.macro-main {
  display: grid;
  gap: 4px;
  min-width: 0;
}

.macro-main strong,
.macro-main > span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.macro-main > span {
  color: var(--el-text-color-secondary);
  font-size: 12px;
  user-select: none;
}

.macro-name-row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.macro-name-row strong {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.macro-tags {
  display: inline-flex;
  flex: 0 0 auto;
  gap: 6px;
  align-items: center;
}

.option-tag {
  display: inline-flex;
  align-items: center;
  gap: 3px;
  height: 22px;
  line-height: 20px;
  user-select: none;
  cursor: pointer;
}

.option-tag.disabled {
  cursor: not-allowed;
  opacity: 0.72;
}

.mode-tag :deep(.el-icon) {
  margin-right: 2px;
}

.speed-menu {
  position: fixed;
  z-index: 3000;
  display: inline-flex;
  gap: 4px;
  width: 190px;
  padding: 5px;
  background: var(--el-bg-color-overlay);
  border: 1px solid var(--el-border-color-light);
  border-radius: 6px;
  box-shadow: var(--el-box-shadow-light);
}

.speed-option {
  min-width: 38px;
  height: 24px;
  padding: 0 8px;
  color: var(--el-text-color-regular);
  font-size: 12px;
  font-weight: 600;
  line-height: 24px;
  text-align: center;
  white-space: nowrap;
  background: transparent;
  border: 0;
  border-radius: 4px;
  cursor: pointer;
}

.speed-option:hover {
  color: #ffffff;
  background: var(--el-color-primary);
}

.macro-context-menu {
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

.context-menu-item {
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

.context-menu-item:hover {
  color: var(--el-color-primary);
  background: var(--el-fill-color-light);
}

.context-menu-item.danger {
  color: var(--el-color-danger);
}

.context-menu-item.danger:hover {
  color: #ffffff;
  background: var(--el-color-danger);
}
</style>
