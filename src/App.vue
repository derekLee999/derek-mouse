<script setup lang="ts">
import { ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { Mouse, VideoCamera } from "@element-plus/icons-vue";
import TitleBar from "./components/TitleBar.vue";
import GlobalHotkeyBar from "./components/GlobalHotkeyBar.vue";
import ClickerPanel from "./features/clicker/ClickerPanel.vue";
import RecorderPanel from "./features/recorder/RecorderPanel.vue";
import type { HotkeyConfig } from "./types";

const activeTab = ref("clicker");
const recorderBusy = ref(false);
const globalHotkey = ref<HotkeyConfig>({
  ctrl: false,
  alt: false,
  key: "F8",
});

watch(
  activeTab,
  (feature) => {
    void invoke("set_active_feature", { feature });
  },
  { immediate: true },
);
</script>

<template>
  <main class="app-shell">
    <TitleBar />
    <GlobalHotkeyBar v-model="globalHotkey" :disabled="activeTab === 'recorder' && recorderBusy" />

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
    </el-tabs>
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
  grid-template-rows: 36px auto minmax(0, 1fr);
  height: 100vh;
  overflow: hidden;
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
</style>
