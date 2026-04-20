<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { Delete, EditPen } from "@element-plus/icons-vue";
import { ElMessage, ElMessageBox } from "element-plus";
import {
  keyboardKeys,
  keyNeedsModifier,
  type HotkeyConfig,
  type RecorderState,
  type RecordingSummary,
} from "../../types";

const props = defineProps<{
  hotkey: HotkeyConfig;
  playbackSpeed: number;
}>();

const emit = defineEmits<{
  "busy-change": [value: boolean];
}>();

const state = ref<RecorderState>({
  recordings: [],
  selectedId: null,
  recording: false,
  playing: false,
});
const recordHotkey = reactive<HotkeyConfig>({
  ctrl: false,
  alt: false,
  key: "F9",
});
const previousRecordHotkey = ref<HotkeyConfig>({ ctrl: false, alt: false, key: "F9" });
const editingId = ref<number | null>(null);
const editingName = ref("");
const renameInput = ref<any[]>([]);
const initialized = ref(false);
const syncingHotkey = ref(false);
const savingHotkey = ref(false);
const autoCtrl = ref(false);
const loopPlayback = ref(false);

const busy = computed(() => state.value.recording || state.value.playing);
const selectedKeyNeedsModifier = computed(() => keyNeedsModifier(recordHotkey.key));
const speedLabel = computed(() => `${props.playbackSpeed}倍速`);

let unlistenState: UnlistenFn | undefined;
let unlistenStatus: UnlistenFn | undefined;

onMounted(async () => {
  await Promise.all([refreshState(), loadRecordHotkey(), loadLoopPlayback()]);
  initialized.value = true;
  unlistenState = await listen<RecorderState>("recorder-state", (event) => {
    state.value = event.payload;
  });
  unlistenStatus = await listen<boolean>("recorder-status", async () => {
    await refreshState();
  });
});

onBeforeUnmount(() => {
  unlistenState?.();
  unlistenStatus?.();
});

watch(
  busy,
  (value) => {
    emit("busy-change", value);
  },
  { immediate: true },
);

watch(
  recordHotkey,
  () => {
    if (!initialized.value || syncingHotkey.value || busy.value) return;
    void saveRecordHotkey();
  },
  { deep: true, flush: "sync" },
);

watch(
  () => props.hotkey,
  () => {
    if (sameHotkey(recordHotkey, props.hotkey)) {
      ElMessage.warning("录制热键不能和全局回放热键相同。");
      syncRecordHotkey(previousRecordHotkey.value);
    }
  },
  { deep: true },
);

async function refreshState() {
  state.value = await invoke<RecorderState>("get_recorder_state");
}

async function loadRecordHotkey() {
  const loaded = await invoke<HotkeyConfig>("get_recorder_hotkey_config");
  previousRecordHotkey.value = cloneHotkey(loaded);
  await syncRecordHotkey(loaded);
}

async function loadLoopPlayback() {
  loopPlayback.value = await invoke<boolean>("get_loop_playback");
}

async function setLoopPlayback(value: boolean) {
  loopPlayback.value = await invoke<boolean>("set_loop_playback", { value });
}

async function saveRecordHotkey() {
  normalizeRecordHotkey("save");

  if (sameHotkey(recordHotkey, props.hotkey)) {
    ElMessage.warning("录制热键不能和全局回放热键相同。");
    await syncRecordHotkey(previousRecordHotkey.value);
    return;
  }

  savingHotkey.value = true;
  try {
    const updated = await invoke<HotkeyConfig>("update_recorder_hotkey_config", {
      hotkey: cloneHotkey(recordHotkey),
    });
    previousRecordHotkey.value = cloneHotkey(updated);
    await syncRecordHotkey(updated);
  } catch (error) {
    ElMessage.error(String(error));
    await syncRecordHotkey(previousRecordHotkey.value);
  } finally {
    savingHotkey.value = false;
  }
}

async function selectRecording(id: number) {
  if (busy.value) return;
  state.value = await invoke<RecorderState>("select_recording", { id });
}

function handleRecordingCheck(recording: RecordingSummary, checked: string | number | boolean) {
  if (!checked || busy.value) return;
  void selectRecording(recording.id);
}

async function beginRename(recording: RecordingSummary) {
  if (busy.value) return;
  editingId.value = recording.id;
  editingName.value = recording.name;
  await nextTick();
  renameInput.value?.[0]?.focus();
}

async function commitRename() {
  if (editingId.value === null || busy.value) return;

  const trimmed = editingName.value.trim();
  if (!trimmed) {
    ElMessage.warning("方案名称不能为空。");
    await nextTick();
    renameInput.value?.[0]?.focus();
    return;
  }
  if (trimmed.length > 15) {
    ElMessage.warning("方案名称不能超过15个字。");
    await nextTick();
    renameInput.value?.[0]?.focus();
    return;
  }

  state.value = await invoke<RecorderState>("rename_recording", {
    request: {
      id: editingId.value,
      name: trimmed,
    },
  });
  editingId.value = null;
}

async function removeRecording(recording: RecordingSummary) {
  if (busy.value) return;

  await ElMessageBox.confirm(`删除「${recording.name}」？`, "删除录制方案", {
    confirmButtonText: "删除",
    cancelButtonText: "取消",
    type: "warning",
  });
  state.value = await invoke<RecorderState>("delete_recording", { id: recording.id });
  if (state.value.selectedId === null && state.value.recordings.length > 0) {
    await selectRecording(state.value.recordings[0].id);
  }
}

function handleRecordModifierChange() {
  autoCtrl.value = false;
  normalizeRecordHotkey("modifier");
}

function handleRecordKeyChange() {
  normalizeRecordHotkey("key");
}

function normalizeRecordHotkey(source: "key" | "modifier" | "save") {
  if (selectedKeyNeedsModifier.value && !recordHotkey.ctrl && !recordHotkey.alt) {
    recordHotkey.ctrl = true;
    autoCtrl.value = true;
    return;
  }

  if (!selectedKeyNeedsModifier.value && autoCtrl.value && recordHotkey.ctrl && !recordHotkey.alt) {
    recordHotkey.ctrl = false;
    autoCtrl.value = false;
    return;
  }

  if (!selectedKeyNeedsModifier.value && (recordHotkey.ctrl || recordHotkey.alt)) {
    recordHotkey.ctrl = false;
    recordHotkey.alt = false;
    if (source === "modifier") {
      ElMessage.warning("F1-F12、Space、Enter、Esc 不支持组合 Ctrl 或 Alt。");
    }
    return;
  }

  if (!recordHotkey.ctrl) {
    autoCtrl.value = false;
  }
}

async function syncRecordHotkey(value: HotkeyConfig) {
  syncingHotkey.value = true;
  autoCtrl.value = false;
  recordHotkey.ctrl = value.ctrl;
  recordHotkey.alt = value.alt;
  recordHotkey.key = value.key;
  await nextTick();
  syncingHotkey.value = false;
}

function cloneHotkey(value: HotkeyConfig): HotkeyConfig {
  return {
    ctrl: value.ctrl,
    alt: value.alt,
    key: value.key,
  };
}

function sameHotkey(left: HotkeyConfig, right: HotkeyConfig) {
  return left.ctrl === right.ctrl && left.alt === right.alt && left.key === right.key;
}

function formatDuration(ms: number) {
  return `${(ms / 1000).toFixed(2)} 秒`;
}

function formatTime(timestamp: number) {
  return new Date(timestamp).toLocaleString();
}
</script>

<template>
  <section class="recorder-panel">
    <div class="record-hotkey-row">
      <strong>录制热键</strong>
      <div class="hotkey-controls">
        <el-checkbox
          v-model="recordHotkey.ctrl"
          border
          :disabled="busy"
          @change="handleRecordModifierChange"
        >
          Ctrl
        </el-checkbox>
        <el-checkbox
          v-model="recordHotkey.alt"
          border
          :disabled="busy"
          @change="handleRecordModifierChange"
        >
          Alt
        </el-checkbox>
        <el-select
          v-model="recordHotkey.key"
          filterable
          :disabled="busy"
          :loading="savingHotkey"
          @change="handleRecordKeyChange"
        >
          <el-option
            v-for="key in keyboardKeys"
            :key="key"
            :label="key"
            :value="key"
          />
        </el-select>
      </div>
      <el-select
        class="loop-select"
        :model-value="loopPlayback ? 'loop' : 'once'"
        :disabled="busy"
        @change="(v: string) => setLoopPlayback(v === 'loop')"
      >
        <el-option label="单次" value="once" />
        <el-option label="循环" value="loop" />
      </el-select>
    </div>

    <div class="recording-list">
      <el-empty v-if="state.recordings.length === 0" description="还没有录制方案" />
      <div
        v-for="recording in state.recordings"
        v-else
        :key="recording.id"
        class="recording-item"
        :class="{ active: recording.id === state.selectedId, disabled: busy }"
        @click="selectRecording(recording.id)"
      >
        <el-checkbox
          :model-value="recording.id === state.selectedId"
          :disabled="busy"
          @change="handleRecordingCheck(recording, $event)"
          @click.stop
        />
        <div class="recording-main">
          <el-input
            v-if="editingId === recording.id"
            ref="renameInput"
            v-model="editingName"
            size="small"
            :maxlength="15"
            :disabled="busy"
            @click.stop
            @keydown.enter.stop.prevent="commitRename"
            @blur="commitRename"
          />
          <div v-else class="recording-name-row">
            <strong>{{ recording.name }}</strong>
            <el-tag
              v-if="recording.id === state.selectedId"
              :type="playbackSpeed !== 1 ? 'danger' : 'success'"
              size="small"
            >
              {{ speedLabel }}
            </el-tag>
          </div>
          <span>
            {{ formatTime(recording.createdAt) }} ·
            {{ recording.eventCount }} 个事件 ·
            {{ formatDuration(recording.durationMs) }}
          </span>
        </div>
        <div class="recording-actions" @click.stop>
          <el-button
            text
            size="small"
            :disabled="busy"
            :icon="EditPen"
            @click="beginRename(recording)"
          />
          <el-button
            text
            size="small"
            type="danger"
            :disabled="busy"
            :icon="Delete"
            @click="removeRecording(recording)"
          />
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.recorder-panel {
  display: grid;
  grid-template-rows: auto minmax(0, 1fr);
  gap: 10px;
  height: 100%;
  min-height: 0;
}

.record-hotkey-row {
  display: grid;
  gap: 10px;
  align-items: center;
  min-height: 34px;
  padding: 8px 10px;
  color: var(--el-text-color-regular);
  font-size: 13px;
  background: var(--el-fill-color-light);
  border-radius: 8px;
}

.record-hotkey-row {
  grid-template-columns: 86px auto auto;
}

.record-hotkey-row strong {
  color: var(--el-text-color-primary);
  font-size: 13px;
  font-weight: 600;
}

.record-hotkey-row :deep(.loop-select) {
  width: 80px;
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
  width: 75px;
}

.recording-list {
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

.recording-list::-webkit-scrollbar {
  width: 6px;
}

.recording-list::-webkit-scrollbar-track {
  background: transparent;
}

.recording-list::-webkit-scrollbar-thumb {
  background: transparent;
  border-radius: 3px;
}

.recording-list:hover::-webkit-scrollbar-thumb,
.recording-list:active::-webkit-scrollbar-thumb {
  background: var(--el-text-color-placeholder);
}

.recording-list::-webkit-scrollbar-thumb:hover {
  background: var(--el-text-color-secondary);
}

.recording-item {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr) auto;
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

.recording-item.active {
  border-color: var(--el-color-primary);
  box-shadow: 0 0 0 1px var(--el-color-primary-light-8);
}

.recording-item.disabled {
  cursor: not-allowed;
  opacity: 0.72;
}

.recording-main {
  display: grid;
  gap: 4px;
  min-width: 0;
}

.recording-main strong,
.recording-main > span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.recording-main > span {
  color: var(--el-text-color-secondary);
  font-size: 12px;
}

.recording-name-row {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 0;
}

.recording-name-row strong {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.recording-actions {
  display: flex;
  align-items: center;
  gap: 2px;
}
</style>
