<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { emitTo, type UnlistenFn } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Aim, Close, Delete, DocumentChecked, Remove, Top } from "@element-plus/icons-vue";
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
const eventListRef = ref<HTMLElement | null>(null);
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
const operationMenu = ref<{
  visible: boolean;
  x: number;
  y: number;
}>({
  visible: false,
  x: 0,
  y: 0,
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
const shouldShowUpdatedAt = computed(() => {
  if (!recording.value) return false;
  return recording.value.updatedAt !== recording.value.createdAt;
});

onMounted(async () => {
  unlistenClose = await currentWindow.onCloseRequested(async (event) => {
    if (closingProgrammatically.value || !hasRemovedEvents.value) return;

    event.preventDefault();
    closeConfirmVisible.value = true;
  });

  alwaysOnTop.value = await currentWindow.isAlwaysOnTop();
  await loadRecording();
  document.addEventListener("click", closeFloatingMenus);
  document.addEventListener("keydown", handleDocumentKeydown);
});

onBeforeUnmount(() => {
  unlistenClose?.();
  document.removeEventListener("click", closeFloatingMenus);
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
  closeOperationMenu();

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

function openOperationMenu(event: MouseEvent) {
  event.preventDefault();
  closeEventMenu();

  const menuWidth = 132;
  const menuHeight = 132;
  const margin = 8;
  const maxX = Math.max(margin, window.innerWidth - menuWidth - margin);
  const maxY = Math.max(margin, window.innerHeight - menuHeight - margin);
  operationMenu.value = {
    visible: true,
    x: Math.min(Math.max(event.clientX, margin), maxX),
    y: Math.min(Math.max(event.clientY, margin), maxY),
  };
}

function closeOperationMenu() {
  operationMenu.value.visible = false;
}

function closeFloatingMenus() {
  closeEventMenu();
  closeOperationMenu();
}

function handleDocumentKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    closeFloatingMenus();
    if (!closeConfirmVisible.value) {
      event.preventDefault();
      void requestCloseWindow();
    }
  }
}

async function deleteEventFromMenu() {
  const event = eventMenu.value.event;
  if (!event) return;

  closeEventMenu();
  await deleteEvents([event.index]);
}

function selectCriticalEvent(event: RecordingEventSummary | null) {
  if (!event) return;

  closeOperationMenu();
  activeEventIndex.value = event.index;
  scrollToEvent(event.index);
}

function scrollToEvent(index: number) {
  requestAnimationFrame(() => {
    const target = document.querySelector<HTMLElement>(
      `[data-event-index="${index}"]`,
    );
    target?.scrollIntoView({ block: "center", behavior: "auto" });
  });
}

function scrollToActiveEvent() {
  if (activeEventIndex.value === null) return;

  closeOperationMenu();
  scrollToEvent(activeEventIndex.value);
}

function clearSelectedEvents() {
  selectedEventIndices.value = new Set();
}

function scrollEventListToTop() {
  closeOperationMenu();
  eventListRef.value?.scrollTo({ top: 0, behavior: "auto" });
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

function formatDuration(ms: number) {
  return `${(ms / 1000).toFixed(2)} 秒`;
}

function formatTime(timestamp: number | undefined) {
  if (!timestamp) return "--";
  return new Date(timestamp).toLocaleString();
}
</script>

<template>
  <main class="editor-shell">
    <header class="titlebar" @mousedown="startWindowDrag">
      <div class="titlebar-title">
        <img src="/app-icon.png" alt="" class="titlebar-icon" />
        <span>编辑 - 优化方案  【事件：{{ eventCount }} &nbsp;&nbsp;时长：{{ recording ? formatDuration(recording.durationMs) : "--" }}】</span>
        <div class="titlebar-tags">
          <el-tag class="selected-tag" type="success" effect="light" size="small">
            已选 {{ selectedCount }}
          </el-tag>
          <el-tag v-if="hasRemovedEvents" class="removed-tag" type="danger" effect="light" size="small">
            已删除 {{ removedEventIndices.size }}
          </el-tag>
        </div>
      </div>
      <div class="window-actions" @mousedown.stop>
        <button
          class="window-action"
          type="button"
          title="保存"
          aria-label="保存"
          :disabled="loading || saving || !hasRemovedEvents"
          @click="saveRecording"
        >
          <el-icon><DocumentChecked /></el-icon>
        </button>
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
      <div class="recording-title-block">
        <strong>{{ recording?.name ?? "编辑方案" }}</strong>
        <div class="recording-summary">
          <span><b>创建</b>{{ formatTime(recording?.createdAt) }}</span>
          <span v-if="shouldShowUpdatedAt"><b>最后修改</b>{{ formatTime(recording?.updatedAt) }}</span>
        </div>
      </div>
    </header>

    <section v-loading="loading" class="event-workspace">
      <div ref="eventListRef" class="event-list">
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

      <aside class="event-operation-panel" @contextmenu="openOperationMenu">
        <el-dropdown
          trigger="hover"
          :disabled="previousCriticalEvent === null && nextCriticalEvent === null"
        >
          <el-button
            type="primary"
            plain
            :class="{ 'critical-dropdown-disabled': previousCriticalEvent === null && nextCriticalEvent === null }"
          >
            查找关键操作
          </el-button>
          <template #dropdown>
            <el-dropdown-menu>
              <el-dropdown-item
                :disabled="previousCriticalEvent === null"
                @click="selectCriticalEvent(previousCriticalEvent)"
              >
                上一个
              </el-dropdown-item>
              <el-dropdown-item
                :disabled="nextCriticalEvent === null"
                @click="selectCriticalEvent(nextCriticalEvent)"
              >
                下一个
              </el-dropdown-item>
            </el-dropdown-menu>
          </template>
        </el-dropdown>
        <el-button
          type="danger"
          plain
          :icon="Delete"
          :disabled="loading || selectedCount === 0"
          @click="deleteSelectedEvents"
        >
          删除选择项
        </el-button>
        <el-button
          plain
          :icon="Remove"
          :disabled="selectedCount === 0"
          @click="clearSelectedEvents"
        >
          取消已选择
        </el-button>
        <el-button
          plain
          :icon="Aim"
          :disabled="activeEventIndex === null"
          @click="scrollToActiveEvent"
        >
          定位到选中
        </el-button>
      </aside>
    </section>

    <div
      v-if="operationMenu.visible"
      class="operation-context-menu"
      :style="{ left: `${operationMenu.x}px`, top: `${operationMenu.y}px` }"
      @click.stop
      @mousedown.stop
      @contextmenu.prevent
    >
      <button
        type="button"
        class="operation-context-item"
        :disabled="previousCriticalEvent === null"
        @click="selectCriticalEvent(previousCriticalEvent)"
      >
        上一个关键操作
      </button>
      <button
        type="button"
        class="operation-context-item"
        :disabled="nextCriticalEvent === null"
        @click="selectCriticalEvent(nextCriticalEvent)"
      >
        下一个关键操作
      </button>
      <button
        type="button"
        class="operation-context-item"
        :disabled="activeEventIndex === null"
        @click="scrollToActiveEvent"
      >
        定位到选中
      </button>
      <button type="button" class="operation-context-item" @click="scrollEventListToTop">
        滚动到顶部
      </button>
    </div>

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
  grid-template-rows: 36px auto minmax(0, 1fr);
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

.window-action:disabled,
.window-action:disabled:hover {
  color: var(--el-text-color-placeholder);
  background: transparent;
  cursor: not-allowed;
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
  display: grid;
  grid-template-columns: minmax(0, 1fr) auto;
  align-items: center;
  justify-content: space-between;
  gap: 12px;
  min-width: 0;
}

.recording-title-block {
  display: flex;
  align-items: baseline;
  gap: 16px;
  min-width: 0;
}

.editor-header strong {
  flex: 0 1 auto;
  min-width: 160px;
  overflow: hidden;
  font-size: 18px;
  font-weight: 700;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.recording-summary {
  display: flex;
  flex-wrap: wrap;
  gap: 6px 12px;
  align-items: center;
  min-width: 0;
}

.recording-summary span {
  display: inline-flex;
  gap: 4px;
  align-items: baseline;
  color: var(--el-text-color-secondary);
  font-size: 12px;
  line-height: 1.5;
  white-space: nowrap;
}

.recording-summary b {
  color: var(--el-text-color-placeholder);
  font-weight: 600;
}

.titlebar-tags {
  display: inline-flex;
  flex: 0 0 auto;
  gap: 6px;
  align-items: center;
}

.selected-tag {
  font-weight: 700;
}

.removed-tag {
  --el-tag-bg-color: var(--el-color-danger-light-9);
  --el-tag-border-color: var(--el-color-danger-light-5);
  --el-tag-text-color: var(--el-color-danger);
  color: var(--el-color-danger);
  font-weight: 700;
}

.removed-tag :deep(.el-tag__content) {
  color: var(--el-color-danger);
}

.event-workspace {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 136px;
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
  border-color: var(--el-color-primary-light-5);
  background: var(--el-color-primary-light-9);
}

.event-row.active,
.event-row.active:hover,
.event-row.active.checked {
  border-color: var(--el-color-success-light-5);
  background: var(--el-color-success-light-9);
  box-shadow: inset 3px 0 0 var(--el-color-success);
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

.event-operation-panel :deep(.el-dropdown),
.event-operation-panel :deep(.el-button) {
  width: 100%;
}

.event-operation-panel :deep(.el-button) {
  margin-left: 0;
}

.event-operation-panel :deep(.critical-dropdown-disabled) {
  color: var(--el-text-color-placeholder);
  background: var(--el-fill-color-light);
  border-color: var(--el-border-color-lighter);
  cursor: not-allowed;
}

.event-operation-panel :deep(.critical-dropdown-disabled:hover),
.event-operation-panel :deep(.critical-dropdown-disabled:focus) {
  color: var(--el-text-color-placeholder);
  background: var(--el-fill-color-light);
  border-color: var(--el-border-color-lighter);
}

.operation-context-menu {
  position: fixed;
  z-index: 4000;
  display: grid;
  width: 132px;
  padding: 6px;
  background: var(--el-bg-color-overlay);
  border: 1px solid var(--el-border-color-light);
  border-radius: 6px;
  box-shadow: var(--el-box-shadow-light);
}

.operation-context-item {
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

.operation-context-item:hover {
  color: var(--el-color-primary);
  background: var(--el-fill-color-light);
}

.operation-context-item:disabled,
.operation-context-item:disabled:hover {
  color: var(--el-text-color-placeholder);
  background: transparent;
  cursor: not-allowed;
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
