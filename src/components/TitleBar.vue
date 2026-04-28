<script setup lang="ts">
import { onMounted, ref, watch } from "vue";
import { getVersion } from "@tauri-apps/api/app";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  Close,
  Coordinate,
  Minus,
  QuestionFilled,
  Setting,
  Top,
  UploadFilled,
} from "@element-plus/icons-vue";
import GlobalHotkeyBar from "./GlobalHotkeyBar.vue";
import type { AppUpdateInfo, GlobalHotkeyOptions, HotkeyConfig } from "../types";

type ThemeMode = "light" | "dark" | "system";
type CloseAction = "exit" | "hide";

const props = defineProps<{
  modelValue: HotkeyConfig;
  hotkeyDisabled?: boolean;
  updateInfo?: AppUpdateInfo | null;
  updateInstalling?: boolean;
}>();

const emit = defineEmits<{
  "update:modelValue": [value: HotkeyConfig];
  "open-update": [];
  "check-update": [];
}>();

const alwaysOnTop = ref(false);
const showWindowOnStop = ref(true);
const autoHideOnHotkey = ref(true);
const appWindow = getCurrentWindow();

const themeMode = ref<ThemeMode>("system");
const closeAction = ref<CloseAction>("hide");
const settingsVisible = ref(false);
const hotkeyPopoverVisible = ref(false);
const exitMenuVisible = ref(false);
const appVersion = ref("--");

const systemDarkQuery = window.matchMedia("(prefers-color-scheme: dark)");
const repoUrl = "https://github.com/derekLee999/derek-mouse";

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
  try {
    appVersion.value = await getVersion();
  } catch {}
  alwaysOnTop.value = await appWindow.isAlwaysOnTop();
  try {
    const options = await invoke<GlobalHotkeyOptions>("get_global_hotkey_options");
    showWindowOnStop.value = options.showWindowOnStop;
    autoHideOnHotkey.value = options.autoHideOnHotkey;
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
  await updateGlobalHotkeyOptions();
}

async function toggleAutoHideOnHotkey() {
  await updateGlobalHotkeyOptions();
}

async function updateGlobalHotkeyOptions() {
  try {
    const options = await invoke<GlobalHotkeyOptions>("update_global_hotkey_options", {
      options: {
        showWindowOnStop: showWindowOnStop.value,
        autoHideOnHotkey: autoHideOnHotkey.value,
      },
    });
    showWindowOnStop.value = options.showWindowOnStop;
    autoHideOnHotkey.value = options.autoHideOnHotkey;
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
  hotkeyPopoverVisible.value = false;
  await appWindow.startDragging();
}

function exitApp() {
  exitMenuVisible.value = false;
  void appWindow.destroy();
}

function showExitMenu() {
  exitMenuVisible.value = true;
  setTimeout(() => {
    // 监听 mousedown 而不是 click：
    // 点击标题栏拖动时 Tauri 会消费鼠标事件，导致 click 不触发，
    // 但 mousedown 一定会正常冒泡到 document
    document.addEventListener("mousedown", hideExitMenu, { once: true });
  }, 0);
}

function hideExitMenu() {
  exitMenuVisible.value = false;
}

function openUpdatePanel() {
  emit("open-update");
}

function checkUpdateFromVersion() {
  emit("check-update");
}

function formatVersionLabel(version: string | null | undefined) {
  return (version ?? "").trim().replace(/^[vV]/, "");
}

async function openRepoUrl() {
  await openUrl(repoUrl);
}
</script>

<template>
  <header class="titlebar" @mousedown="startWindowDrag">
    <div class="titlebar-title">
      <img src="/app-icon.png" alt="" class="titlebar-icon" />
      <span class="titlebar-name">鼠标连点器</span>
      <button
        class="settings-btn"
        type="button"
        title="设置"
        aria-label="设置"
        @mousedown.stop
        @click="settingsVisible = true"
      >
        <el-icon :size="14"><Setting /></el-icon>
      </button>
      <el-popover
        v-model:visible="hotkeyPopoverVisible"
        placement="bottom-start"
        :width="340"
        trigger="click"
      >
        <template #reference>
          <button
            class="settings-btn"
            type="button"
            title="全局启停热键"
            aria-label="全局启停热键"
            @mousedown.stop
          >
            <el-icon :size="14"><Coordinate /></el-icon>
          </button>
        </template>
        <div class="hotkey-popover-panel" @mousedown.stop>
          <GlobalHotkeyBar
            :model-value="props.modelValue"
            :disabled="props.hotkeyDisabled"
            variant="popover"
            @update:modelValue="emit('update:modelValue', $event)"
          />
        </div>
      </el-popover>
      <el-tooltip
        v-if="props.updateInfo?.available"
        :content="`检测到新版本 ${formatVersionLabel(props.updateInfo.latestTag ?? props.updateInfo.latestVersion)}`"
        placement="bottom"
      >
        <button
          class="settings-btn update-btn"
          type="button"
          title="发现新版本"
          aria-label="发现新版本"
          @mousedown.stop
          @click="openUpdatePanel"
        >
          <span class="update-pulse" :class="{ busy: props.updateInstalling }">
            <el-icon :size="14"><UploadFilled /></el-icon>
          </span>
        </button>
      </el-tooltip>
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
      <div class="close-wrapper">
        <button
          class="window-action close"
          type="button"
          title="关闭"
          aria-label="关闭"
          @click="closeWindow"
          @contextmenu.prevent="showExitMenu"
        >
          <el-icon><Close /></el-icon>
        </button>
        <div v-if="exitMenuVisible" class="exit-menu" @click.stop @mousedown.stop>
          <button type="button" class="exit-menu-item" @click="exitApp">
            退出程序
          </button>
        </div>
      </div>
    </div>

    <el-dialog
      v-model="settingsVisible"
      title="设置"
      width="340px"
      align-center
      append-to-body
      :show-close="true"
      @mousedown.stop
    >
      <div class="settings-panel">
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
        <div class="settings-item">
          <span class="settings-label with-help">
            自动隐藏
            <el-tooltip content="开启时按下热键后程序自动隐藏" placement="top">
              <el-icon class="help-icon" :size="14"><QuestionFilled /></el-icon>
            </el-tooltip>
          </span>
          <el-segmented
            v-model="autoHideOnHotkey"
            :options="[
              { label: '开启', value: true },
              { label: '关闭', value: false },
            ]"
            size="small"
            @change="toggleAutoHideOnHotkey"
          />
        </div>
        <div class="settings-meta">
          <button type="button" class="version-check-btn" @click="checkUpdateFromVersion">
            <span class="settings-meta-label">版本号</span>
            <span class="settings-meta-value version-check-value">v{{ appVersion }}</span>
          </button>
          <button type="button" class="repo-link" @click="openRepoUrl">
            <span class="settings-meta-label">GitHub</span>
            <span class="settings-meta-value repo-text">derekLee999/derek-mouse</span>
          </button>
        </div>
      </div>
    </el-dialog>
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

.titlebar-name {
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

.update-btn {
  color: var(--el-color-warning);
}

.update-btn:hover {
  color: #ffffff;
  background: var(--el-color-warning);
}

.update-pulse {
  position: relative;
  display: inline-grid;
  place-items: center;
}

.update-pulse::after {
  content: "";
  position: absolute;
  inset: -4px;
  border: 1px solid color-mix(in srgb, var(--el-color-warning) 50%, transparent);
  border-radius: 999px;
  opacity: 0;
  animation: update-pulse-ring 1.8s ease-out infinite;
}

.update-pulse.busy::after {
  animation-duration: 1s;
}

@keyframes update-pulse-ring {
  0% {
    transform: scale(0.7);
    opacity: 0.65;
  }

  70% {
    transform: scale(1.18);
    opacity: 0;
  }

  100% {
    transform: scale(1.18);
    opacity: 0;
  }
}

.hotkey-popover-panel {
  padding: 2px;
}

.settings-panel {
  display: flex;
  flex-direction: column;
  gap: 14px;
  user-select: none;
}

.settings-meta {
  display: grid;
  gap: 10px;
  padding-top: 12px;
  margin-top: 2px;
  border-top: 1px solid var(--el-border-color-lighter);
}

.settings-meta-row,
.repo-link,
.version-check-btn {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
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

.settings-label.with-help {
  display: inline-flex;
  align-items: center;
  gap: 5px;
}

.settings-meta-label {
  color: var(--el-text-color-secondary);
  font-size: 12px;
  font-weight: 600;
}

.settings-meta-value {
  min-width: 0;
  color: var(--el-text-color-primary);
  font-size: 12px;
  font-weight: 700;
  text-align: right;
}

.repo-link {
  padding: 8px 10px;
  background: var(--el-fill-color-light);
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 8px;
  cursor: pointer;
}

.version-check-btn {
  padding: 0;
  color: inherit;
  background: transparent;
  border: 0;
  cursor: pointer;
}

.version-check-value {
  color: var(--el-color-primary);
}

.repo-link:hover {
  border-color: var(--el-color-primary-light-5);
  background: var(--el-color-primary-light-9);
}

.version-check-btn:hover .version-check-value {
  color: var(--el-color-primary-dark-2);
  text-decoration: underline;
  text-underline-offset: 2px;
}

.repo-text {
  color: var(--el-color-primary);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.help-icon {
  color: var(--el-text-color-placeholder);
  cursor: help;
}

.help-icon:hover {
  color: var(--el-color-primary);
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

.close-wrapper {
  position: relative;
  height: 100%;
}

.exit-menu {
  position: absolute;
  top: 100%;
  right: 0;
  z-index: 9999;
  display: grid;
  width: 112px;
  padding: 6px;
  background: var(--el-bg-color-overlay);
  border: 1px solid var(--el-border-color-light);
  border-radius: 6px;
  box-shadow: var(--el-box-shadow-light);
}

.exit-menu-item {
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

.exit-menu-item:hover {
  color: #ffffff;
  background: #e81123;
}
</style>
