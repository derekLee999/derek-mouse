<script setup lang="ts">
import { computed, nextTick, onMounted, reactive, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { ElMessage } from "element-plus";
import {
  type HotkeyConfig,
  hotkeyText,
  keyboardKeys,
  keyNeedsModifier,
} from "../types";

const props = defineProps<{
  modelValue: HotkeyConfig;
  disabled?: boolean;
  showSpeedSelect?: boolean;
}>();

const emit = defineEmits<{
  "update:modelValue": [value: HotkeyConfig];
  "speed-change": [value: number];
}>();

const hotkey = reactive<HotkeyConfig>({
  ctrl: props.modelValue.ctrl,
  alt: props.modelValue.alt,
  key: props.modelValue.key,
});
const initialized = ref(false);
const syncing = ref(false);
const saving = ref(false);
const autoCtrl = ref(false);
const playbackSpeed = ref(1);

const selectedKeyNeedsModifier = computed(() => keyNeedsModifier(hotkey.key));
const displayHotkey = computed(() => hotkeyText(hotkey));

onMounted(async () => {
  try {
    const [loaded, speed] = await Promise.all([
      invoke<HotkeyConfig>("get_hotkey_config"),
      invoke<number>("get_playback_speed"),
    ]);
    syncFrom(loaded);
    playbackSpeed.value = speed;
    emit("update:modelValue", cloneHotkey());
    emit("speed-change", speed);
  } catch (error) {
    ElMessage.error(String(error));
  } finally {
    initialized.value = true;
  }
});

watch(
  () => props.modelValue,
  (value) => {
    if (sameHotkey(value, hotkey)) return;
    syncFrom(value);
  },
  { deep: true },
);

watch(playbackSpeed, () => {
  if (!initialized.value) return;
  void saveSpeed();
});

watch(
  hotkey,
  () => {
    if (!initialized.value || syncing.value) return;
    void saveHotkey();
  },
  { deep: true, flush: "sync" },
);

function handleModifierChange() {
  autoCtrl.value = false;
  normalizeHotkey("modifier");
}

function handleKeyChange() {
  normalizeHotkey("key");
}

async function saveHotkey() {
  normalizeHotkey("save");
  saving.value = true;

  try {
    const updated = await invoke<HotkeyConfig>("update_hotkey_config", {
      hotkey: cloneHotkey(),
    });
    syncFrom(updated);
    emit("update:modelValue", cloneHotkey());
  } catch (error) {
    ElMessage.error(String(error));
  } finally {
    saving.value = false;
  }
}

async function saveSpeed() {
  try {
    const speed = await invoke<number>("set_playback_speed", {
      speed: playbackSpeed.value,
    });
    playbackSpeed.value = speed;
    emit("speed-change", speed);
  } catch (error) {
    ElMessage.error(String(error));
  }
}

function normalizeHotkey(source: "key" | "modifier" | "save") {
  if (selectedKeyNeedsModifier.value && !hotkey.ctrl && !hotkey.alt) {
    hotkey.ctrl = true;
    autoCtrl.value = true;
    return;
  }

  if (!selectedKeyNeedsModifier.value && autoCtrl.value && hotkey.ctrl && !hotkey.alt) {
    hotkey.ctrl = false;
    autoCtrl.value = false;
    return;
  }

  if (!selectedKeyNeedsModifier.value && (hotkey.ctrl || hotkey.alt)) {
    hotkey.ctrl = false;
    hotkey.alt = false;
    if (source === "modifier") {
      ElMessage.warning("F1-F12、Space、Enter、Esc 不支持组合 Ctrl 或 Alt。");
    }
    return;
  }

  if (!hotkey.ctrl) {
    autoCtrl.value = false;
  }
}

async function syncFrom(value: HotkeyConfig) {
  syncing.value = true;
  autoCtrl.value = false;
  hotkey.ctrl = value.ctrl;
  hotkey.alt = value.alt;
  hotkey.key = value.key;
  await nextTick();
  syncing.value = false;
}

function cloneHotkey(): HotkeyConfig {
  return {
    ctrl: hotkey.ctrl,
    alt: hotkey.alt,
    key: hotkey.key,
  };
}

function sameHotkey(left: HotkeyConfig, right: HotkeyConfig) {
  return left.ctrl === right.ctrl && left.alt === right.alt && left.key === right.key;
}
</script>

<template>
  <section class="global-hotkey-bar">
    <strong class="hotkey-label">全局热键</strong>

    <div class="hotkey-controls">
      <el-checkbox
        v-model="hotkey.ctrl"
        border
        :disabled="props.disabled"
        @change="handleModifierChange"
      >
        Ctrl
      </el-checkbox>
      <el-checkbox
        v-model="hotkey.alt"
        border
        :disabled="props.disabled"
        @change="handleModifierChange"
      >
        Alt
      </el-checkbox>
      <el-select
        v-model="hotkey.key"
        filterable
        :disabled="props.disabled"
        :loading="saving"
        @change="handleKeyChange"
      >
        <el-option
          v-for="key in keyboardKeys"
          :key="key"
          :label="key"
          :value="key"
        />
      </el-select>
    </div>

    <span class="hotkey-value">{{ displayHotkey }}</span>

    <el-select
      v-if="props.showSpeedSelect"
      v-model="playbackSpeed"
      class="speed-select"
      :disabled="props.disabled"
    >
      <el-option :value="1" label="1x" />
      <el-option :value="1.5" label="1.5x" />
      <el-option :value="2" label="2x" />
      <el-option :value="2.5" label="2.5x" />
      <el-option :value="3" label="3x" />
      <el-option :value="3.5" label="3.5x" />
      <el-option :value="4" label="4x" />
    </el-select>
  </section>
</template>

<style scoped>
.global-hotkey-bar {
  display: flex;
  flex-wrap: nowrap;
  gap: 8px;
  align-items: center;
  max-width: 1080px;
  width: 100%;
  min-height: 48px;
  margin: 10px auto 0;
  padding: 8px 14px;
  background: var(--el-bg-color);
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 8px;
}

.hotkey-label {
  color: var(--el-text-color-regular);
  font-size: 13px;
  font-weight: 600;
}

.hotkey-value {
  color: var(--el-text-color-primary);
  font-size: 14px;
  font-weight: 700;
  white-space: nowrap;
}

.hotkey-controls {
  display: flex;
  gap: 10px;
  align-items: center;
}

.hotkey-controls :deep(.el-checkbox) {
  margin-right: 0;
}

.hotkey-controls :deep(.el-select) {
  width: 80px;
}

.speed-select {
  margin-left: auto;
  width: 80px;
}

@media (max-width: 720px) {
  .global-hotkey-bar {
    grid-template-columns: 1fr;
    gap: 8px;
  }
}
</style>
