<script setup lang="ts">
import { onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Close, Minus, Top, Setting } from "@element-plus/icons-vue";
import type { GlobalHotkeyOptions } from "../types";

type ThemeMode = "light" | "dark" | "system";
type CloseAction = "exit" | "hide";

const alwaysOnTop = ref(false);
const showWindowOnStop = ref(true);
const appWindow = getCurrentWindow();

const themeMode = ref<ThemeMode>("system");
const closeAction = ref<CloseAction>("hide");
const settingsVisible = ref(false);

const systemDarkQuery = window.matchMedia("(prefers-color-scheme: dark)");

function applyTheme() {
  let dark = false;
  if (themeMode.value === "dark") {
    dark = true;
  } else if (themeMode.value === "system") {
    dark = systemDarkQuery.matches;
  }
  document.documentElement.classList.toggle("dark", dark);
}

function onSystemThemeChange() {
  if (themeMode.value === "system") {
    applyTheme();
  }
}

function loadSettings() {
  const saved = localStorage.getItem("app-settings");
  if (saved) {
    try {
      const settings = JSON.parse(saved);
      if (["light", "dark", "system"].includes(settings.themeMode)) {
        themeMode.value = settings.themeMode;
      }
      if (["exit", "hide"].includes(settings.closeAction)) {
        closeAction.value = settings.closeAction;
      }
    } catch {}
  }
}

function saveSettings() {
  localStorage.setItem(
    "app-settings",
    JSON.stringify({ themeMode: themeMode.value, closeAction: closeAction.value }),
  );
}

onMounted(async () => {
  alwaysOnTop.value = await appWindow.isAlwaysOnTop();
  try {
    const options = await invoke<GlobalHotkeyOptions>("get_global_hotkey_options");
    showWindowOnStop.value = options.showWindowOnStop;
  } catch {}

  loadSettings();
  applyTheme();
  systemDarkQuery.addEventListener("change", onSystemThemeChange);
});

watch(themeMode, () => {
  applyTheme();
  saveSettings();
});

watch(closeAction, () => {
  saveSettings();
});

async function toggleShowWindowOnStop() {
  try {
    const options = await invoke<GlobalHotkeyOptions>("update_global_hotkey_options", {
      options: { showWindowOnStop: showWindowOnStop.value },
    });
    showWindowOnStop.value = options.showWindowOnStop;
  } catch {}
}

async function toggleAlwaysOnTop() {
  const nextValue = !alwaysOnTop.value;
  await appWindow.setAlwaysOnTop(nextValue);
  alwaysOnTop.value = nextValue;
}

async function minimizeWindow() {
  await appWindow.minimize();
}

async function closeWindow() {
  if (closeAction.value === "exit") {
    await appWindow.destroy();
  } else {
    await appWindow.hide();
  }
}

async function startWindowDrag() {
  settingsVisible.value = false;
  await appWindow.startDragging();
}
</script>

<template>
  <header class="titlebar" @mousedown="startWindowDrag">
    <div class="titlebar-title">
      <img src="/app-icon.png" alt="" class="titlebar-icon" />
      <span>鼠标连点器</span>
      <el-popover
        v-model:visible="settingsVisible"
        placement="bottom-start"
        :width="220"
        trigger="click"
        :show-arrow="false"
        :offset="8"
      >
        <template #reference>
          <button
            class="settings-btn"
            type="button"
            title="设置"
            aria-label="设置"
            @mousedown.stop
          >
            <el-icon :size="14"><Setting /></el-icon>
          </button>
        </template>

        <div class="settings-panel" @mousedown.stop>
          <div class="settings-item">
            <span class="settings-label">外观</span>
            <el-segmented
              v-model="themeMode"
              :options="[
                { label: '浅色', value: 'light' },
                { label: '深色', value: 'dark' },
                { label: '跟随系统', value: 'system' },
              ]"
              size="small"
            />
          </div>
          <div class="settings-item">
            <span class="settings-label">关闭按钮</span>
            <el-segmented
              v-model="closeAction"
              :options="[
                { label: '隐藏窗口', value: 'hide' },
                { label: '退出程序', value: 'exit' },
              ]"
              size="small"
            />
          </div>
        </div>
      </el-popover>
    </div>
    <div class="window-actions" @mousedown.stop>
      <el-checkbox
        v-model="showWindowOnStop"
        class="show-window-option"
        @change="toggleShowWindowOnStop"
      >
        停止后显示窗口
      </el-checkbox>
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
        class="window-action"
        type="button"
        title="最小化"
        aria-label="最小化"
        @click="minimizeWindow"
      >
        <el-icon><Minus /></el-icon>
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
</template>

<style scoped>
.titlebar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  min-width: 0;
  height: 36px;
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

.titlebar-title span {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}

.settings-btn {
  display: grid;
  place-items: center;
  width: 26px;
  height: 26px;
  padding: 0;
  color: var(--el-text-color-secondary);
  background: transparent;
  border: 0;
  border-radius: 4px;
  cursor: pointer;
  flex-shrink: 0;
}

.settings-btn:hover {
  color: var(--el-text-color-primary);
  background: var(--el-fill-color-light);
}

.settings-panel {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.settings-item {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.settings-label {
  font-size: 12px;
  font-weight: 600;
  color: var(--el-text-color-secondary);
}

.window-actions {
  display: flex;
  align-items: center;
  height: 100%;
}

.show-window-option {
  margin-right: 8px;
  font-size: 12px;
  white-space: nowrap;
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

.window-action.close:hover {
  color: #ffffff;
  background: #e81123;
}
</style>
