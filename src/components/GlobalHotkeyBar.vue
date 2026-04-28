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

const props = withDefaults(defineProps<{
  modelValue: HotkeyConfig;
  disabled?: boolean;
  variant?: "bar" | "popover";
}>(), {
  disabled: false,
  variant: "bar",
});

const emit = defineEmits<{
  "update:modelValue": [value: HotkeyConfig];
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

const selectedKeyNeedsModifier = computed(() => keyNeedsModifier(hotkey.key));
const displayHotkey = computed(() => hotkeyText(hotkey));
const isPopover = computed(() => props.variant === "popover");
const hotkeyKeyOptions = computed(() =>
  keyboardKeys.filter((key) => !["SPACE", "ENTER", "ESC"].includes(key)),
);

onMounted(async () => {
  try {
    const loaded = await invoke<HotkeyConfig>("get_hotkey_config");
    syncFrom(loaded);
    emit("update:modelValue", cloneHotkey());
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
  <section class="global-hotkey-bar" :class="{ popover: isPopover, disabled: props.disabled }">
    <div class="hotkey-head">
      <div class="hotkey-title-block">
        <strong class="hotkey-label">全局启停热键 <span class="hotkey-value">{{ displayHotkey }}</span></strong>
        <span v-if="isPopover" class="hotkey-hint">
          用于启停鼠标连点、键鼠录制回放、鼠标宏回放
        </span>
      </div>
      
    </div>

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
          v-for="key in hotkeyKeyOptions"
          :key="key"
          :label="key"
          :value="key"
        />
      </el-select>
    </div>

    <span v-if="isPopover && props.disabled" class="hotkey-disabled-hint">
      键鼠录制忙碌时暂不可修改
    </span>
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

.global-hotkey-bar.popover {
  flex-direction: column;
  align-items: stretch;
  max-width: none;
  min-height: auto;
  margin: 0;
  padding: 0;
  background: transparent;
  border: 0;
}

.hotkey-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 12px;
}

.hotkey-title-block {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.hotkey-label {
  color: var(--el-text-color-regular);
  font-size: 13px;
  font-weight: 600;
}

.hotkey-hint {
  color: var(--el-text-color-secondary);
  font-size: 12px;
  line-height: 1.4;
}

.hotkey-value {
  margin-left: 12px;
  color: var(--el-text-color-primary);
  font-size: 14px;
  font-weight: 700;
  white-space: nowrap;
  flex-shrink: 0;
}

.hotkey-controls {
  display: flex;
  gap: 10px;
  align-items: center;
}

.global-hotkey-bar.popover .hotkey-controls {
  flex-wrap: wrap;
  gap: 10px;
  padding: 12px;
  background: color-mix(in srgb, var(--el-fill-color-light) 82%, transparent);
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 10px;
}

.hotkey-controls :deep(.el-checkbox) {
  margin-right: 0;
}

.hotkey-controls :deep(.el-select) {
  width: 80px;
}

.global-hotkey-bar.popover .hotkey-controls :deep(.el-select) {
  flex: 1;
  min-width: 110px;
}

.hotkey-disabled-hint {
  color: var(--el-text-color-secondary);
  font-size: 12px;
}

@media (max-width: 720px) {
  .global-hotkey-bar {
    grid-template-columns: 1fr;
    gap: 8px;
  }

  .hotkey-head {
    flex-direction: column;
    gap: 6px;
  }
}
</style>
