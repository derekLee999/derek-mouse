<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { emitTo, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Close, Delete, DocumentChecked, Top } from "@element-plus/icons-vue";
import { ElMessage, ElMessageBox } from "element-plus";
import type { RecordingDetail, RecordingEventSummary, RecorderState } from "../../types";

type SaveMode = "append" | "replace";

const recordingIdParam = new URLSearchParams(window.location.search).get("id");
const recordingId = recordingIdParam ? Number(recordingIdParam) : NaN;
const currentWindow = getCurrentWindow();

const loading = ref(true);
const saving = ref(false);
const recording = ref<RecordingDetail | null>(null);
const events = ref<RecordingEventSummary[]>([]);
const selectedEventIndices = ref<Set<number>>(new Set());
const removedEventIndices = ref<Set<number>>(new Set());
const activeEventIndex = ref<number | null>(null);
const closingProgrammatically = ref(false);
const alwaysOnTop = ref(false);
const closeConfirmVisible = ref(false);
const eventMenu = ref<{
  visible: boolean;
  x: number;
  y: number;
  event: RecordingEventSummary | null;
}>({
  visible: false,
  x: 0,
  y: 0,
  event: null,
});

let unlistenClose: UnlistenFn | undefined;

const selectedCount = computed(() => selectedEventIndices.value.size);
const eventCount = computed(() => events.value.length);
const hasRemovedEvents = computed(() => removedEventIndices.value.size > 0);
const activeEventPosition = computed(() => {
  if (activeEventIndex.value === null) return -1;
  return events.value.findIndex((event) => event.index === activeEventIndex.value);
});
const previousCriticalEvent = computed(() => {
  if (activeEventPosition.value <= 0) return null;
  return [...events.value.slice(0, activeEventPosition.value)]
    .reverse()
    .find((event) => event.critical) ?? null;
});
const nextCriticalEvent = computed(() => {
  if (activeEventPosition.value < 0 || activeEventPosition.value >= events.value.length - 1) {
    return null;
  }
  return events.value.slice(activeEventPosition.value + 1).find((event) => event.critical) ?? null;
});

onMounted(async () => {
  unlistenClose = await currentWindow.onCloseRequested(async (event) => {
    if (closingProgrammatically.value || !hasRemovedEvents.value) return;

    event.preventDefault();
    closeConfirmVisible.value = true;
  });

  alwaysOnTop.value = await currentWindow.isAlwaysOnTop();
  await loadRecording();
  document.addEventListener("click", closeEventMenu);
  document.addEventListener("keydown", handleDocumentKeydown);
});

onBeforeUnmount(() => {
  unlistenClose?.();
  document.removeEventListener("click", closeEventMenu);
  document.removeEventListener("keydown", handleDocumentKeydown);
});

async function loadRecording() {
  if (!Number.isFinite(recordingId)) {
    ElMessage.error("录制方案不存在。");
    await closeWindow();
    return;
  }

  loading.value = true;
  try {
    const detail = await invoke<RecordingDetail>("get_recording_detail", { id: recordingId });
    recording.value = detail;
    events.value = detail.events;
  } catch (error) {
    ElMessage.error(String(error));
    await closeWindow();
  } finally {
    loading.value = false;
  }
}

function toggleEvent(index: number, checked: string | number | boolean) {
  const next = new Set(selectedEventIndices.value);
  if (checked) {
    next.add(index);
  } else {
    next.delete(index);
  }
  selectedEventIndices.value = next;
}

function handleEventRowClick(mouseEvent: MouseEvent, event: RecordingEventSummary) {
  closeEventMenu();

  if (mouseEvent.shiftKey && activeEventIndex.value !== null) {
    selectCheckboxRange(event.index);
    return;
  }

  toggleActiveEvent(event.index);
}

function toggleActiveEvent(index: number) {
  closeEventMenu();
  activeEventIndex.value = activeEventIndex.value === index ? null : index;
}

function selectCheckboxRange(targetIndex: number) {
  const activeIndex = activeEventIndex.value;
  if (activeIndex === null) return;

  const activePosition = events.value.findIndex((event) => event.index === activeIndex);
  const targetPosition = events.value.findIndex((event) => event.index === targetIndex);
  if (activePosition < 0 || targetPosition < 0) return;

  const start = Math.min(activePosition, targetPosition);
  const end = Math.max(activePosition, targetPosition);
  const next = new Set(selectedEventIndices.value);
  events.value.slice(start, end + 1).forEach((event) => next.add(event.index));
  selectedEventIndices.value = next;
}

async function deleteSelectedEvents() {
  if (selectedEventIndices.value.size === 0) {
    ElMessage.warning("请选择要删除的事件。");
    return;
  }

  await deleteEvents(Array.from(selectedEventIndices.value));
}

async function deleteEvents(indices: number[]) {
  if (indices.length === 0) return;

  try {
    await ElMessageBox.confirm(
      indices.length === 1 ? "删除该事件？" : `删除选中的 ${indices.length} 个事件？`,
      "删除事件",
      {
        confirmButtonText: "删除",
        cancelButtonText: "取消",
        type: "warning",
      },
    );
  } catch {
    return;
  }

  const selected = new Set(indices);
  const removed = new Set(removedEventIndices.value);
  selected.forEach((index) => removed.add(index));
  removedEventIndices.value = removed;
  events.value = events.value.filter((event) => !selected.has(event.index));
  selectedEventIndices.value = new Set(
    Array.from(selectedEventIndices.value).filter((index) => !selected.has(index)),
  );
  if (activeEventIndex.value !== null && selected.has(activeEventIndex.value)) {
    activeEventIndex.value = null;
  }
  closeEventMenu();
}

function openEventMenu(event: MouseEvent, recordingEvent: RecordingEventSummary) {
  event.preventDefault();

  const menuWidth = 112;
  const menuHeight = 42;
  eventMenu.value = {
    visible: true,
    x: Math.min(event.clientX, window.innerWidth - menuWidth - 8),
    y: Math.min(event.clientY, window.innerHeight - menuHeight - 8),
    event: recordingEvent,
  };
}

function closeEventMenu() {
  eventMenu.value.visible = false;
}

function handleDocumentKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    closeEventMenu();
  }
}

async function deleteEventFromMenu() {
  const event = eventMenu.value.event;
  if (!event) return;

  await deleteEvents([event.index]);
}

function selectCriticalEvent(event: RecordingEventSummary | null) {
  if (!event) return;

  activeEventIndex.value = event.index;
  requestAnimationFrame(() => {
    const target = document.querySelector<HTMLElement>(
      `[data-event-index="${event.index}"]`,
    );
    target?.scrollIntoView({ block: "center", behavior: "auto" });
  });
}

async function saveRecording() {
  if (!hasRemovedEvents.value) {
    ElMessage.warning("还没有删除事件。");
    return false;
  }

  const mode = await chooseSaveMode();
  if (!mode) return false;

  return saveRecordingWithMode(mode);
}

async function saveRecordingWithMode(mode: SaveMode) {
  if (!hasRemovedEvents.value) {
    await closeWindow();
    return true;
  }

  saving.value = true;
  try {
    const state = await invoke<RecorderState>("save_edited_recording", {
      request: {
        id: recordingId,
        removedEventIndices: Array.from(removedEventIndices.value),
        mode,
      },
    });
    await emitTo("main", "recorder-state", state);
    ElMessage.success("已保存编辑方案。");
    removedEventIndices.value = new Set();
    await closeWindow();
    return true;
  } catch (error) {
    ElMessage.error(String(error));
    return false;
  } finally {
    saving.value = false;
  }
}

async function chooseSaveMode(): Promise<SaveMode | null> {
  try {
    await ElMessageBox.confirm("保存为新增方案或覆盖原方案？", "保存编辑", {
      confirmButtonText: "新增",
      cancelButtonText: "覆盖",
      distinguishCancelAndClose: true,
      type: "warning",
    });
    return "append";
  } catch (action) {
    return action === "cancel" ? "replace" : null;
  }
}

async function saveAndClose(mode: SaveMode) {
  closeConfirmVisible.value = false;
  await saveRecordingWithMode(mode);
}

async function toggleAlwaysOnTop() {
  const next = !alwaysOnTop.value;
  await currentWindow.setAlwaysOnTop(next);
  alwaysOnTop.value = next;
}

async function requestCloseWindow() {
  if (hasRemovedEvents.value) {
    closeConfirmVisible.value = true;
    return;
  }

  await closeWindow();
}

async function closeWindow() {
  closingProgrammatically.value = true;
  await currentWindow.close();
}

async function startWindowDrag() {
  await currentWindow.startDragging();
}

function formatDelay(ms: number) {
  return `${(ms / 1000).toFixed(3)}s`;
}
</script>

<template>
  <main class="editor-shell">
    <header class="titlebar" @mousedown="startWindowDrag">
      <div class="titlebar-title">
        <img src="/app-icon.png" alt="" class="titlebar-icon" />
        <span>编辑</span>
      </div>
      <div class="window-actions" @mousedown.stop>
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
          class="window-action close"
          type="button"
          title="关闭"
          aria-label="关闭"
          @click="requestCloseWindow"
        >
          <el-icon><Close /></el-icon>
        </button>
      </div>
    </header>

    <header class="editor-header">
      <div>
        <strong>{{ recording?.name ?? "编辑方案" }}</strong>
        <span>{{ eventCount }} 个事件</span>
      </div>
      <el-tag v-if="hasRemovedEvents" type="warning" size="small">
        已删除 {{ removedEventIndices.size }}
      </el-tag>
    </header>

    <section class="editor-toolbar">
      <div class="selection-state">
        已选 {{ selectedCount }}
      </div>
      <div class="toolbar-actions">
        <el-button
          type="danger"
          plain
          :icon="Delete"
          :disabled="loading || selectedCount === 0"
          @click="deleteSelectedEvents"
        >
          删除
        </el-button>
        <el-button
          type="primary"
          :icon="DocumentChecked"
          :loading="saving"
          :disabled="loading || !hasRemovedEvents"
          @click="saveRecording"
        >
          保存
        </el-button>
      </div>
    </section>

    <section v-loading="loading" class="event-workspace">
      <div class="event-list">
        <el-empty v-if="!loading && events.length === 0" description="没有事件" />
        <button
          v-for="event in events"
          v-else
          :key="event.index"
          type="button"
          class="event-row"
          :class="{
            checked: selectedEventIndices.has(event.index),
            active: activeEventIndex === event.index,
          }"
          :data-event-index="event.index"
          @click="handleEventRowClick($event, event)"
          @contextmenu="openEventMenu($event, event)"
        >
          <el-checkbox
            :model-value="selectedEventIndices.has(event.index)"
            @change="toggleEvent(event.index, $event)"
            @click.stop
          />
          <span class="event-index">#{{ event.index + 1 }}</span>
          <span class="event-delay">{{ formatDelay(event.delayMs) }}</span>
          <span class="event-action" :class="{ critical: event.critical }">
            {{ event.action }}
          </span>
          <span class="event-target">{{ event.target }}</span>
        </button>
      </div>

      <aside class="event-operation-panel">
        <el-button
          type="primary"
          plain
          :disabled="previousCriticalEvent === null"
          @click="selectCriticalEvent(previousCriticalEvent)"
        >
          上一个
        </el-button>
        <el-button
          type="primary"
          plain
          :disabled="nextCriticalEvent === null"
          @click="selectCriticalEvent(nextCriticalEvent)"
        >
          下一个
        </el-button>
      </aside>
    </section>

    <div
      v-if="eventMenu.visible"
      class="event-context-menu"
      :style="{ left: `${eventMenu.x}px`, top: `${eventMenu.y}px` }"
      @click.stop
      @mousedown.stop
      @contextmenu.prevent
    >
      <button type="button" class="event-context-item danger" @click="deleteEventFromMenu">
        删除
      </button>
    </div>

    <el-dialog
      v-model="closeConfirmVisible"
      title="关闭编辑"
      width="380px"
      :close-on-click-modal="false"
      :close-on-press-escape="false"
      append-to-body
    >
      <p class="close-confirm-text">
        当前已删除事件，关闭前请选择处理方式。
      </p>
      <template #footer>
        <div class="close-actions">
          <el-button @click="closeWindow">直接关闭</el-button>
          <el-button type="primary" plain :loading="saving" @click="saveAndClose('append')">
            新增
          </el-button>
          <el-button type="danger" plain :loading="saving" @click="saveAndClose('replace')">
            覆盖
          </el-button>
        </div>
      </template>
    </el-dialog>
  </main>
</template>

<style scoped>
.editor-shell {
  display: grid;
  grid-template-rows: 36px auto auto minmax(0, 1fr);
  gap: 12px;
  height: 100vh;
  padding: 0 18px 18px;
  overflow: hidden;
  color: var(--el-text-color-primary);
  background: var(--el-bg-color-page);
}

.titlebar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  height: 36px;
  margin: 0 -18px;
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

.window-actions {
  display: flex;
  align-items: center;
  height: 100%;
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

.editor-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  min-width: 0;
}

.editor-header div {
  display: grid;
  gap: 3px;
  min-width: 0;
}

.editor-header strong {
  overflow: hidden;
  font-size: 18px;
  font-weight: 700;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.editor-header span {
  color: var(--el-text-color-secondary);
  font-size: 12px;
}

.editor-toolbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  min-height: 48px;
  padding: 8px 10px;
  background: var(--el-bg-color);
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 8px;
}

.selection-state {
  color: var(--el-text-color-secondary);
  font-size: 13px;
  font-weight: 700;
  min-width: 120px;
}

.toolbar-actions {
  display: flex;
  gap: 8px;
  align-items: center;
}

.event-workspace {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 116px;
  gap: 10px;
  min-height: 0;
}

.event-list {
  display: grid;
  align-content: start;
  gap: 8px;
  min-height: 0;
  padding: 10px;
  overflow-y: auto;
  background: var(--el-bg-color);
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 8px;
}

.event-row {
  display: grid;
  grid-template-columns: auto 64px 76px 96px minmax(0, 1fr);
  gap: 10px;
  align-items: center;
  min-height: 40px;
  padding: 8px 10px;
  color: var(--el-text-color-regular);
  text-align: left;
  background: var(--el-fill-color-blank);
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 8px;
  cursor: pointer;
}

.event-row:hover {
  border-color: var(--el-border-color);
  background: var(--el-fill-color-light);
}

.event-row.checked {
  border-color: var(--el-color-success-light-5);
  background: var(--el-color-success-light-9);
}

.event-row.active,
.event-row.active:hover,
.event-row.active.checked {
  border-color: var(--el-color-primary-light-5);
  background: var(--el-color-primary-light-9);
  box-shadow: inset 3px 0 0 var(--el-color-primary);
}

.event-index {
  color: var(--el-text-color-secondary);
  font-size: 12px;
  font-weight: 700;
}

.event-delay {
  color: var(--el-text-color-secondary);
  font-size: 12px;
  font-variant-numeric: tabular-nums;
}

.event-action {
  font-size: 13px;
  font-weight: 700;
}

.event-action.critical {
  color: var(--el-color-danger);
}

.event-target {
  min-width: 0;
  overflow: hidden;
  color: var(--el-text-color-primary);
  font-size: 13px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.event-operation-panel {
  display: flex;
  flex-direction: column;
  gap: 8px;
  min-height: 0;
  padding: 10px;
  background: var(--el-bg-color);
  border: 1px solid var(--el-border-color-lighter);
  border-radius: 8px;
}

.event-operation-panel :deep(.el-button) {
  width: 100%;
  margin-left: 0;
}

.event-context-menu {
  position: fixed;
  z-index: 4000;
  display: grid;
  width: 112px;
  padding: 6px;
  background: var(--el-bg-color-overlay);
  border: 1px solid var(--el-border-color-light);
  border-radius: 6px;
  box-shadow: var(--el-box-shadow-light);
}

.event-context-item {
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

.event-context-item:hover {
  color: var(--el-color-primary);
  background: var(--el-fill-color-light);
}

.event-context-item.danger {
  color: var(--el-color-danger);
}

.event-context-item.danger:hover {
  color: #ffffff;
  background: var(--el-color-danger);
}

.close-confirm-text {
  margin: 0;
  color: var(--el-text-color-regular);
  line-height: 1.7;
}

.close-actions {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}
</style>
