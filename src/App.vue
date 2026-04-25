<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { KnifeFork, Mouse, VideoCamera } from "@element-plus/icons-vue";
import TitleBar from "./components/TitleBar.vue";
import ClickerPanel from "./features/clicker/ClickerPanel.vue";
import CoordinatePickerWindow from "./features/mouse-macro/CoordinatePickerWindow.vue";
import MouseMacroEditorWindow from "./features/mouse-macro/MouseMacroEditorWindow.vue";
import MouseMacroPanel from "./features/mouse-macro/MouseMacroPanel.vue";
import RecordingEditorWindow from "./features/recorder/RecordingEditorWindow.vue";
import RecorderPanel from "./features/recorder/RecorderPanel.vue";
import { hotkeyText, type HotkeyConfig } from "./types";

type ActiveTab = "clicker" | "recorder" | "mouse-macro";

const view = new URLSearchParams(window.location.search).get("view");
const isRecordingEditor = view === "recording-editor";
const isMouseMacroEditor = view === "mouse-macro-editor";
const isCoordinatePicker = view === "coordinate-picker";
const isSubWindow = isRecordingEditor || isMouseMacroEditor || isCoordinatePicker;
const activeTab = ref<ActiveTab>("clicker");
const recorderBusy = ref(false);
const globalHotkey = ref<HotkeyConfig>({
  ctrl: false,
  alt: false,
  key: "F8",
});
const displayGlobalHotkey = computed(() => hotkeyText(globalHotkey.value));

watch(
  activeTab,
  (feature) => {
    if (isSubWindow) return;
    void invoke("set_active_feature", { feature });
  },
  { immediate: true },
);

onMounted(() => {
  if (isSubWindow) return;
  window.addEventListener("keydown", handleWindowKeydown);
});

onBeforeUnmount(() => {
  if (isSubWindow) return;
  window.removeEventListener("keydown", handleWindowKeydown);
});

function handleWindowKeydown(event: KeyboardEvent) {
  if (event.defaultPrevented || event.key !== "Tab" || event.ctrlKey || event.altKey || event.metaKey) {
    return;
  }

  event.preventDefault();
  const tabs: ActiveTab[] = ["clicker", "recorder", "mouse-macro"];
  const currentIndex = tabs.indexOf(activeTab.value);
  activeTab.value = tabs[(currentIndex + 1) % tabs.length];
}
</script>

<template>
  <RecordingEditorWindow v-if="isRecordingEditor" />
  <MouseMacroEditorWindow v-else-if="isMouseMacroEditor" />
  <CoordinatePickerWindow v-else-if="isCoordinatePicker" />
  <main v-else class="app-shell">
    <TitleBar v-model="globalHotkey" :hotkey-disabled="activeTab === 'recorder' && recorderBusy" />

    <section class="workspace-tabs-shell">
      <el-tabs v-model="activeTab" class="workspace-tabs">
        <el-tab-pane name="clicker">
          <template #label>
            <span class="tab-label">
              <el-icon><Mouse /></el-icon>
              鼠标连点
            </span>
          </template>

          <ClickerPanel :hotkey="globalHotkey" />
        </el-tab-pane>

        <el-tab-pane name="recorder">
          <template #label>
            <span class="tab-label">
              <el-icon><VideoCamera /></el-icon>
              键鼠录制
            </span>
          </template>

          <RecorderPanel :hotkey="globalHotkey" @busy-change="recorderBusy = $event" />
        </el-tab-pane>

        <el-tab-pane name="mouse-macro">
          <template #label>
            <span class="tab-label">
              <el-icon><KnifeFork /></el-icon>
              鼠标宏
            </span>
          </template>

          <MouseMacroPanel />
        </el-tab-pane>
      </el-tabs>
      <div class="workspace-hotkey-indicator" aria-label="当前全局热键">
        <span class="workspace-hotkey-label">热键</span>
        <span class="workspace-hotkey-value">{{ displayGlobalHotkey }}</span>
      </div>
    </section>
  </main>
</template>

<style>
* {
  box-sizing: border-box;
}

:root {
  color: var(--el-text-color-primary);
  background: var(--el-bg-color-page);
  font-family:
    Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI",
    sans-serif;
  font-synthesis: none;
  text-rendering: optimizeLegibility;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
}

body {
  margin: 0;
  min-width: 360px;
  min-height: 100vh;
  overflow: hidden;
}

button,
input {
  font: inherit;
}

#app {
  min-height: 100vh;
}
</style>

<style scoped>
.app-shell {
  display: grid;
  grid-template-rows: 36px minmax(0, 1fr);
  height: 100vh;
  overflow: hidden;
}

.workspace-tabs-shell {
  position: relative;
  min-height: 0;
}

.workspace-tabs {
  min-height: 0;
  max-width: 1080px;
  width: 100%;
  height: calc(100% - 20px);
  margin: 10px auto;
  padding: 10px 14px 14px;
  overflow: hidden;
  background: var(--el-bg-color);
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 8px;
}

.workspace-tabs :deep(.el-tabs__header) {
  margin-bottom: 10px;
}

.workspace-tabs :deep(.el-tabs__nav-wrap) {
  padding-right: 148px;
}

.workspace-tabs :deep(.el-tabs__content) {
  height: calc(100% - 44px);
}

.workspace-tabs :deep(.el-tab-pane) {
  height: 100%;
}

.tab-label {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.workspace-hotkey-indicator {
  position: absolute;
  top: 24px;
  right: calc(50% - 540px + 24px);
  display: inline-flex;
  align-items: center;
  gap: 8px;
  padding: 4px 10px;
  color: var(--el-text-color-regular);
  background: color-mix(in srgb, var(--el-fill-color-light) 80%, transparent);
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 999px;
  pointer-events: none;
  z-index: 1;
}

.workspace-hotkey-label {
  color: var(--el-text-color-secondary);
  font-size: 12px;
  font-weight: 600;
}

.workspace-hotkey-value {
  color: var(--el-text-color-primary);
  font-size: 12px;
  font-weight: 700;
  white-space: nowrap;
}

@media (max-width: 1120px) {
  .workspace-hotkey-indicator {
    right: 24px;
  }
}

@media (max-width: 720px) {
  .workspace-tabs :deep(.el-tabs__nav-wrap) {
    padding-right: 116px;
  }

  .workspace-hotkey-indicator {
    gap: 6px;
    padding: 4px 8px;
  }

  .workspace-hotkey-label,
  .workspace-hotkey-value {
    font-size: 11px;
  }
}
</style>
