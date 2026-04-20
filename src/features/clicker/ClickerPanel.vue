<script setup lang="ts">
import { onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
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

const modeOptions = [
  { label: "热键切换连点", value: "toggle" },
  { label: "按住鼠标连点", value: "hold" },
] as const;

const running = ref(false);
const errorMessage = ref("");
const initialized = ref(false);

const config = reactive<ClickerConfig>({
  clickButton: "left",
  intervalSecs: 0.2,
  mode: "toggle",
  holdButton: "left",
  hotkey: {
    ctrl: false,
    alt: false,
    key: "F8",
  },
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
  return {
    clickButton: config.clickButton,
    intervalSecs: config.intervalSecs,
    mode: config.mode,
    holdButton: config.holdButton,
    hotkey: {
      ctrl: props.hotkey.ctrl,
      alt: props.hotkey.alt,
      key: props.hotkey.key,
    },
  };
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

function handleModeChange() {
  if (config.mode === "hold") {
    config.holdButton = "left";
  }
}
</script>

<template>
  <section class="settings-panel">
    <el-form label-position="left" label-width="86px" class="settings-form">
      <el-form-item label="连点方式">
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

      <el-form-item v-if="config.mode === 'hold'" label="按住触发键">
        <el-segmented
          v-model="config.holdButton"
          :options="mouseButtonOptions"
          block
          :disabled="running"
        />
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
