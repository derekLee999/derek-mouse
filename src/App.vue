<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { openUrl } from "@tauri-apps/plugin-opener";
import { ElMessage, ElMessageBox } from "element-plus";
import { KnifeFork, Mouse, VideoCamera } from "@element-plus/icons-vue";
import TitleBar from "./components/TitleBar.vue";
import ClickerPanel from "./features/clicker/ClickerPanel.vue";
import CoordinatePickerWindow from "./features/mouse-macro/CoordinatePickerWindow.vue";
import MouseMacroEditorWindow from "./features/mouse-macro/MouseMacroEditorWindow.vue";
import MouseMacroPanel from "./features/mouse-macro/MouseMacroPanel.vue";
import RecordingEditorWindow from "./features/recorder/RecordingEditorWindow.vue";
import RecorderPanel from "./features/recorder/RecorderPanel.vue";
import { hotkeyText, type AppUpdateInfo, type HotkeyConfig } from "./types";

type ActiveTab = "clicker" | "recorder" | "mouse-macro";

const view = new URLSearchParams(window.location.search).get("view");
const isRecordingEditor = view === "recording-editor";
const isMouseMacroEditor = view === "mouse-macro-editor";
const isCoordinatePicker = view === "coordinate-picker";
const isSubWindow = isRecordingEditor || isMouseMacroEditor || isCoordinatePicker;
const activeTab = ref<ActiveTab>("clicker");
const recorderBusy = ref(false);
const updateInfo = ref<AppUpdateInfo | null>(null);
const updateDialogVisible = ref(false);
const checkingUpdate = ref(false);
const installingUpdate = ref(false);
const globalHotkey = ref<HotkeyConfig>({
  ctrl: false,
  alt: false,
  key: "F8",
});
const displayGlobalHotkey = computed(() => hotkeyText(globalHotkey.value));
const formattedUpdateNotes = computed(() => {
  const notes = updateInfo.value?.notes?.trim();
  return notes?.length ? notes : "暂无更新说明。";
});
const latestUpdateLabel = computed(
  () => formatVersionLabel(updateInfo.value?.latestTag ?? updateInfo.value?.latestVersion ?? ""),
);
const currentUpdateLabel = computed(
  () => formatVersionLabel(updateInfo.value?.currentVersion ?? ""),
);
const formattedPublishedAt = computed(
  () => formatPublishedAt(updateInfo.value?.publishedAt ?? null),
);

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
  void checkForAppUpdate(true);
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

async function checkForAppUpdate(promptOnAvailable = false) {
  if (checkingUpdate.value || isSubWindow) return;

  checkingUpdate.value = true;
  try {
    const nextInfo = await invoke<AppUpdateInfo>("check_app_update");
    updateInfo.value = nextInfo;
    if (promptOnAvailable && nextInfo.available) {
      updateDialogVisible.value = true;
    }
  } catch (error) {
    if (!promptOnAvailable) {
      ElMessage.error(String(error));
    }
  } finally {
    checkingUpdate.value = false;
  }
}

function openUpdateDialog() {
  if (updateInfo.value?.available) {
    updateDialogVisible.value = true;
  } else {
    void checkForAppUpdate(false);
  }
}

async function installLatestUpdate() {
  if (!updateInfo.value?.available || installingUpdate.value) return;

  if (!updateInfo.value.installReady) {
    if (updateInfo.value.releaseUrl) {
      await openUrl(updateInfo.value.releaseUrl);
    } else {
      ElMessage.warning(updateInfo.value.installHint ?? "当前无法直接安装该更新。");
    }
    return;
  }

  try {
    await ElMessageBox.confirm(
      `将下载并安装 ${latestUpdateLabel.value}，安装开始后应用可能会自动退出。`,
      "安装更新",
      {
        confirmButtonText: "立即安装",
        cancelButtonText: "稍后",
        type: "warning",
      },
    );
  } catch {
    return;
  }

  installingUpdate.value = true;
  try {
    await invoke("install_app_update");
    ElMessage.success("更新安装已开始。");
  } catch (error) {
    ElMessage.error(String(error));
  } finally {
    installingUpdate.value = false;
  }
}

async function openReleasePage() {
  if (updateInfo.value?.releaseUrl) {
    await openUrl(updateInfo.value.releaseUrl);
  }
}

function formatVersionLabel(version: string | null | undefined) {
  return (version ?? "").trim().replace(/^[vV]/, "");
}

function formatPublishedAt(value: string | null) {
  if (!value) return "";

  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }

  const year = date.getFullYear();
  const month = String(date.getMonth() + 1).padStart(2, "0");
  const day = String(date.getDate()).padStart(2, "0");
  const hours = String(date.getHours()).padStart(2, "0");
  const minutes = String(date.getMinutes()).padStart(2, "0");
  const seconds = String(date.getSeconds()).padStart(2, "0");
  return `${year}-${month}-${day} ${hours}:${minutes}:${seconds}`;
}
</script>

<template>
  <RecordingEditorWindow v-if="isRecordingEditor" />
  <MouseMacroEditorWindow v-else-if="isMouseMacroEditor" />
  <CoordinatePickerWindow v-else-if="isCoordinatePicker" />
  <main v-else class="app-shell">
    <TitleBar
      v-model="globalHotkey"
      :hotkey-disabled="activeTab === 'recorder' && recorderBusy"
      :update-info="updateInfo"
      :update-installing="installingUpdate"
      @open-update="openUpdateDialog"
    />

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

    <el-dialog
      v-model="updateDialogVisible"
      width="460px"
      align-center
      append-to-body
      title="发现新版本"
    >
      <div v-if="updateInfo?.available" class="update-dialog">
        <div class="update-summary">
          <div class="update-version-row">
            <span class="update-version-label">当前版本</span>
            <b>{{ currentUpdateLabel }}</b>
          </div>
          <div class="update-version-row">
            <span class="update-version-label">最新版本</span>
            <b>{{ latestUpdateLabel }}</b>
          </div>
          <div v-if="formattedPublishedAt" class="update-version-row">
            <span class="update-version-label">发布时间</span>
            <span>{{ formattedPublishedAt }}</span>
          </div>
        </div>

        <div v-if="updateInfo.installHint" class="update-hint">
          {{ updateInfo.installHint }}
        </div>

        <div class="update-notes">
          <div class="update-notes-title">更新说明</div>
          <pre class="update-notes-content">{{ formattedUpdateNotes }}</pre>
        </div>
      </div>
      <template #footer>
        <div class="update-actions">
          <el-button @click="updateDialogVisible = false">稍后</el-button>
          <el-button
            v-if="updateInfo?.releaseUrl"
            plain
            @click="openReleasePage"
          >
            查看发布页
          </el-button>
          <el-button
            v-if="updateInfo?.installReady"
            type="primary"
            :loading="installingUpdate"
            @click="installLatestUpdate"
          >
            立即安装
          </el-button>
        </div>
      </template>
    </el-dialog>
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

.update-dialog {
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.update-summary {
  display: grid;
  gap: 8px;
  padding: 10px 12px;
  background: var(--el-fill-color-light);
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 10px;
}

.update-version-row {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  font-size: 13px;
}

.update-version-label {
  color: var(--el-text-color-secondary);
  font-weight: 600;
}

.update-hint {
  padding: 9px 12px;
  color: var(--el-color-warning-dark-2);
  font-size: 12px;
  line-height: 1.6;
  background: var(--el-color-warning-light-9);
  border: 1px solid var(--el-color-warning-light-5);
  border-radius: 8px;
}

.update-notes {
  display: grid;
  gap: 8px;
}

.update-notes-title {
  color: var(--el-text-color-secondary);
  font-size: 12px;
  font-weight: 700;
}

.update-notes-content {
  max-height: 220px;
  margin: 0;
  padding: 12px;
  overflow: auto;
  color: var(--el-text-color-regular);
  font-family: inherit;
  font-size: 13px;
  line-height: 1.6;
  white-space: pre-wrap;
  background: var(--el-fill-color-blank);
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 10px;
}

.update-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
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
