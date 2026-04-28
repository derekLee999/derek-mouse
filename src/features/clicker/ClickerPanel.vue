<script setup lang="ts">
import { onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, TauriEvent, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { WebviewWindow, type WebviewWindow as WebviewWindowType } from "@tauri-apps/api/webviewWindow";
import { Close, QuestionFilled } from "@element-plus/icons-vue";
import { ElMessage } from "element-plus";
import {
  type ClickerConfig,
  type ClickerState,
  type HotkeyConfig,
  mouseButtonOptions,
} from "../../types";

const props = defineProps<{
  hotkey: HotkeyConfig;
}>();

const currentWindow = getCurrentWindow();

const modeOptions = [
  { label: "热键切换连点", value: "toggle" },
  { label: "按住鼠标连点", value: "hold" },
] as const;

const holdButtonOptions = [
  { label: "左键", value: "left" },
  { label: "右键", value: "right" },
] as const;

const running = ref(false);
const errorMessage = ref("");
const initialized = ref(false);
const pickingWindow = ref(false);
let unlistenCoordinate: UnlistenFn | undefined;
let pickerWindow: WebviewWindowType | null = null;

const config = reactive<ClickerConfig>({
  clickButton: "left",
  intervalSecs: 0.2,
  clickLimit: 0,
  mode: "toggle",
  holdButton: "left",
  hotkey: {
    ctrl: false,
    alt: false,
    key: "F8",
  },
  backendClick: false,
  targetWindowTitle: "",
  targetClientX: null,
  targetClientY: null,
});

let unlistenStatus: UnlistenFn | undefined;

onMounted(async () => {
  await loadState();
  initialized.value = true;
  unlistenStatus = await listen<boolean>("clicker-status", (event) => {
    running.value = event.payload;
  });
});

onBeforeUnmount(() => {
  unlistenStatus?.();
  unlistenCoordinate?.();
  if (pickingWindow.value) {
    cancelPickWindow();
  }
});

watch(
  config,
  () => {
    if (!initialized.value || running.value) {
      return;
    }

    void saveConfig();
  },
  { deep: true },
);

async function loadState() {
  const state = await invoke<ClickerState>("get_clicker_state");
  applyState(state);
}

async function saveConfig() {
  errorMessage.value = "";

  try {
    ensureValidInterval();
    ensureValidClickLimit();
    const state = await invoke<ClickerState>("update_clicker_config", {
      config: buildConfigPayload(),
    });
    running.value = state.running;
  } catch (error) {
    errorMessage.value = String(error);
    ElMessage.error(errorMessage.value);
  }
}

function buildConfigPayload(): ClickerConfig {
  const backendClickEnabled = config.mode === "toggle" && config.backendClick && hasSelectedTargetWindow();
  return {
    clickButton: config.clickButton,
    intervalSecs: config.intervalSecs,
    clickLimit: config.clickLimit,
    mode: config.mode,
    holdButton: config.holdButton,
    hotkey: {
      ctrl: props.hotkey.ctrl,
      alt: props.hotkey.alt,
      key: props.hotkey.key,
    },
    backendClick: backendClickEnabled,
    targetWindowTitle: config.targetWindowTitle,
    targetClientX: config.targetClientX,
    targetClientY: config.targetClientY,
  };
}

function hasSelectedTargetWindow() {
  return !!config.targetWindowTitle.trim() && config.targetClientX !== null && config.targetClientY !== null;
}

function applyState(state: ClickerState) {
  Object.assign(config, state.config);
  running.value = state.running;
}

function ensureValidInterval() {
  if (!Number.isFinite(config.intervalSecs) || config.intervalSecs <= 0) {
    config.intervalSecs = 0.2;
  }

  config.intervalSecs = Number(config.intervalSecs.toFixed(2));
}

function handleIntervalChange(value: number | undefined) {
  config.intervalSecs = typeof value === "number" ? Number(value.toFixed(2)) : 0.2;
}

function ensureValidClickLimit() {
  if (!Number.isFinite(config.clickLimit) || config.clickLimit < 0) {
    config.clickLimit = 0;
    return;
  }

  config.clickLimit = Math.floor(config.clickLimit);
}

function handleClickLimitChange(value: number | undefined) {
  config.clickLimit = typeof value === "number" && Number.isFinite(value) ? Math.max(0, Math.floor(value)) : 0;
}

function handleModeChange() {
  if (config.mode === "hold" && config.holdButton === "middle") {
    config.holdButton = "left";
  }
  if (config.mode === "hold") {
    config.backendClick = false;
  }
}

async function startPickWindow() {
  if (pickingWindow.value) return;
  pickingWindow.value = true;

  try {
    const snapshot = await invoke<{ left: number; top: number; width: number; height: number }>(
      "start_mouse_coordinate_pick",
      { windowLabel: currentWindow.label }
    );

    unlistenCoordinate = await listen<{ x: number; y: number }>("mouse-coordinate-picked", (event) => {
      void finishWindowPick(event.payload.x, event.payload.y);
    });

    const label = `coordinate-picker-window-${Date.now()}`;
    pickerWindow = new WebviewWindow(label, {
      url: `/index.html?view=coordinate-picker&mode=coordinate`,
      title: "选取窗口",
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

    pickerWindow.once(TauriEvent.WINDOW_DESTROYED, () => {
      pickerWindow = null;
      if (pickingWindow.value) {
        cancelPickWindow();
      }
    });
  } catch (error) {
    pickingWindow.value = false;
    ElMessage.error(String(error));
  }
}

async function finishWindowPick(x: number, y: number) {
  unlistenCoordinate?.();
  // 先关闭 coordinate picker 覆盖窗口，避免 WindowFromPoint 取到它自身
  try {
    await pickerWindow?.close();
  } catch {}
  pickerWindow = null;
  pickingWindow.value = false;
  try {
    const result = await invoke<{ title: string; clientX: number; clientY: number }>("pick_window_at_cursor", {
      x,
      y,
    });
    config.targetWindowTitle = result.title;
    config.targetClientX = result.clientX;
    config.targetClientY = result.clientY;
    ElMessage.success(`已选取窗口：${result.title}`);
  } catch (error) {
    ElMessage.error(String(error));
  }
}

function cancelPickWindow() {
  unlistenCoordinate?.();
  try {
    pickerWindow?.close();
  } catch {}
  pickerWindow = null;
  pickingWindow.value = false;
  try {
    invoke("cancel_mouse_coordinate_pick");
  } catch {}
}

function clearTargetWindow() {
  if (running.value) return;

  config.backendClick = false;
  config.targetWindowTitle = "";
  config.targetClientX = null;
  config.targetClientY = null;
}
</script>

<template>
  <section class="settings-panel">
    <el-form label-position="left" label-width="86px" class="settings-form">
      <el-form-item>
        <template #label>
          <span class="mode-label">
            <span>连点方式</span>
            <el-tooltip
              v-if="config.mode === 'hold'"
              content="先按下全局启停热键后，按住鼠标连点"
              placement="top"
            >
              <el-icon class="mode-help-icon" :size="14"><QuestionFilled /></el-icon>
            </el-tooltip>
          </span>
        </template>
        <el-segmented
          v-model="config.mode"
          :options="modeOptions"
          block
          :disabled="running"
          @change="handleModeChange"
        />
      </el-form-item>

      <el-form-item label="点击类型">
        <el-segmented
          v-model="config.clickButton"
          :options="mouseButtonOptions"
          block
          :disabled="running"
        />
      </el-form-item>

      <el-form-item label="点击间隔">
        <div class="interval-row">
          <el-input-number
            v-model="config.intervalSecs"
            :min="0.01"
            :step="0.05"
            :precision="2"
            controls-position="right"
            :disabled="running"
            @change="handleIntervalChange"
          />
          <span>秒</span>
        </div>
      </el-form-item>

      <el-form-item v-if="config.mode === 'toggle'" label="限制次数">
        <div class="interval-row">
          <el-input-number
            v-model="config.clickLimit"
            :min="0"
            :step="1"
            :precision="0"
            controls-position="right"
            :disabled="running"
            @change="handleClickLimitChange"
          />
          <span>次（0 为不限制）</span>
        </div>
      </el-form-item>

      <el-form-item v-if="config.mode === 'hold'" label="按住触发键">
        <el-segmented
          v-model="config.holdButton"
          :options="holdButtonOptions"
          block
          :disabled="running"
        />
      </el-form-item>

      <el-form-item v-if="config.mode === 'toggle'">
        <template #label>
          <span>后台点击</span>
        </template>
        <div class="backend-click-row">
          <el-checkbox v-model="config.backendClick" :disabled="running">
            向目标窗口发送消息（不移动鼠标）
          </el-checkbox>
          <el-tooltip placement="top">
            <template #content>目前仅支持浏览器和雷电模拟器。其它应用未做验证！ </template>
            <el-icon class="mode-help-icon" :size="14"><QuestionFilled /></el-icon>
          </el-tooltip>
        </div>
      </el-form-item>

      <el-form-item v-if="config.mode === 'toggle' && config.backendClick">
        <template #label>
          <span>目标窗口</span>
        </template>
        <div class="target-window-row">
          <div v-if="config.targetWindowTitle" class="target-window-box">
            <span class="target-window-title" :title="config.targetWindowTitle">
              {{ config.targetWindowTitle }}
            </span>
            <button
              class="target-window-clear"
              type="button"
              title="清空目标窗口"
              aria-label="清空目标窗口"
              :disabled="running"
              @click="clearTargetWindow"
            >
              <el-icon :size="12"><Close /></el-icon>
            </button>
          </div>
          <span v-else class="target-window-placeholder">未选择</span>
          <el-button
            size="small"
            :type="pickingWindow ? 'primary' : 'default'"
            :disabled="running"
            @click="startPickWindow"
          >
            {{ pickingWindow ? "点击确认 " : "选取窗口" }}
          </el-button>
        </div>
      </el-form-item>
    </el-form>

    <el-alert
      v-if="errorMessage"
      :title="errorMessage"
      type="error"
      show-icon
      :closable="false"
    />
  </section>
</template>

<style scoped>
.settings-panel {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-width: 0;
  padding: 14px;
  background: var(--el-bg-color);
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 8px;
}

.settings-form {
  display: grid;
  gap: 10px;
}

.settings-form :deep(.el-form-item) {
  align-items: center;
  margin-bottom: 0;
}

.settings-form :deep(.el-form-item__label) {
  justify-content: flex-start;
  padding-bottom: 0;
  color: var(--el-text-color-regular);
  font-size: 13px;
  font-weight: 600;
  line-height: 32px;
}

.mode-label {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.mode-help-icon {
  color: var(--el-text-color-secondary);
  cursor: help;
}

.mode-help-icon:hover {
  color: var(--el-color-primary);
}

.settings-form :deep(.el-form-item__content) {
  min-width: 0;
}

.interval-row {
  display: grid;
  grid-template-columns: minmax(130px, 180px) auto;
  gap: 10px;
  align-items: center;
}

.interval-row span {
  color: var(--el-text-color-regular);
  font-weight: 600;
}

.backend-click-row {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.target-window-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  gap: 8px;
  align-items: center;
}

.target-window-box {
  position: relative;
  min-width: 0;
  padding: 8px 26px 8px 10px;
  background: var(--el-fill-color-light);
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 8px;
}

.target-window-title {
  display: block;
  min-width: 0;
  overflow: hidden;
  color: var(--el-text-color-primary);
  font-size: 13px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.target-window-clear {
  position: absolute;
  top: 5px;
  right: 5px;
  display: inline-grid;
  place-items: center;
  width: 16px;
  height: 16px;
  padding: 0;
  color: var(--el-text-color-placeholder);
  background: transparent;
  border: 0;
  border-radius: 50%;
  cursor: pointer;
}

.target-window-clear:hover {
  color: var(--el-color-danger);
  background: var(--el-fill-color);
}

.target-window-clear:disabled,
.target-window-clear:disabled:hover {
  color: var(--el-text-color-disabled);
  background: transparent;
  cursor: not-allowed;
}

.target-window-placeholder {
  color: var(--el-color-danger);
  font-size: 13px;
}

.status-strip {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
  gap: 10px;
  align-items: center;
  min-height: 34px;
  padding: 8px 10px;
  color: var(--el-text-color-regular);
  font-size: 13px;
  background: var(--el-fill-color-light);
  border-radius: 8px;
}

.status-strip span {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.status-strip small {
  color: var(--el-text-color-secondary);
  white-space: nowrap;
}
</style>
