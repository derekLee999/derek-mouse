<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ElMessage } from "element-plus";

type ScreenBounds = {
  left: number;
  top: number;
  width: number;
  height: number;
};

type PickerMode = "coordinate" | "region" | "color";

type PixelColor = {
  r: number;
  g: number;
  b: number;
  hex: string;
};

const currentWindow = getCurrentWindow();
const urlMode = new URLSearchParams(window.location.search).get("mode");
const pickerMode: PickerMode = urlMode === "region" ? "region" : urlMode === "color" ? "color" : "coordinate";
const screenBounds = ref<ScreenBounds | null>(null);
const pointer = ref({ x: 0, y: 0 });
const dragStart = ref<{ x: number; y: number } | null>(null);
const viewport = ref({ width: window.innerWidth, height: window.innerHeight });
const closing = ref(false);
const pickedColor = ref<PixelColor | null>(null);

const displayCoordinate = computed(() => {
  if (!screenBounds.value) return { x: 0, y: 0 };

  return {
    x: Math.round(screenBounds.value.left + pointer.value.x * scaleX()),
    y: Math.round(screenBounds.value.top + pointer.value.y * scaleY()),
  };
});

const selectionRect = computed(() => {
  if (!dragStart.value) return null;
  const left = Math.min(dragStart.value.x, pointer.value.x);
  const top = Math.min(dragStart.value.y, pointer.value.y);
  const width = Math.abs(pointer.value.x - dragStart.value.x);
  const height = Math.abs(pointer.value.y - dragStart.value.y);
  return { left, top, width, height };
});

const displayRegion = computed(() => {
  if (!screenBounds.value || !dragStart.value) return null;
  const start = toScreenCoordinate(dragStart.value);
  const end = toScreenCoordinate(pointer.value);
  return {
    x1: Math.min(start.x, end.x),
    y1: Math.min(start.y, end.y),
    x2: Math.max(start.x, end.x),
    y2: Math.max(start.y, end.y),
  };
});

onMounted(async () => {
  try {
    screenBounds.value = await invoke<ScreenBounds>("get_mouse_coordinate_pick_snapshot");
    await currentWindow.setFocus();
  } catch (error) {
    ElMessage.error(String(error));
    await closePicker(false);
  }

  document.addEventListener("keydown", handleKeydown);
  window.addEventListener("resize", updateViewport);

  if (pickerMode === "color") {
    document.addEventListener("mousemove", handleColorPointerMove);
  }
});

onBeforeUnmount(() => {
  document.removeEventListener("keydown", handleKeydown);
  window.removeEventListener("resize", updateViewport);
  document.removeEventListener("mousemove", handleColorPointerMove);
  if (!closing.value) {
    void invoke("cancel_mouse_coordinate_pick");
  }
});

function handlePointerMove(event: MouseEvent) {
  pointer.value = {
    x: event.clientX,
    y: event.clientY,
  };
}

let colorFetchTimeout: ReturnType<typeof setTimeout> | null = null;

async function handleColorPointerMove(event: MouseEvent) {
  pointer.value = {
    x: event.clientX,
    y: event.clientY,
  };

  if (colorFetchTimeout) {
    clearTimeout(colorFetchTimeout);
  }

  colorFetchTimeout = setTimeout(async () => {
    if (!screenBounds.value || closing.value) return;
    const coord = displayCoordinate.value;
    try {
      const color = await invoke<PixelColor>("get_pixel_color", { x: coord.x, y: coord.y });
      pickedColor.value = color;
    } catch {
      // ignore
    }
  }, 30);
}

function handlePointerDown(event: MouseEvent) {
  if (pickerMode !== "region" || event.button !== 0) return;
  event.preventDefault();
  pointer.value = {
    x: event.clientX,
    y: event.clientY,
  };
  dragStart.value = {
    x: event.clientX,
    y: event.clientY,
  };
}

async function handlePointerUp(event: MouseEvent) {
  if (pickerMode !== "region" || event.button !== 0 || !dragStart.value) return;
  event.preventDefault();
  pointer.value = {
    x: event.clientX,
    y: event.clientY,
  };

  if (!displayRegion.value) return;
  const region = displayRegion.value;
  if (Math.abs(region.x2 - region.x1) < 2 || Math.abs(region.y2 - region.y1) < 2) {
    dragStart.value = null;
    return;
  }

  closing.value = true;
  await currentWindow.hide();
  await waitForOverlayToDisappear();
  await invoke("finish_mouse_region_pick", {
    request: region,
  });
  await currentWindow.close();
}

function updateViewport() {
  viewport.value = {
    width: window.innerWidth,
    height: window.innerHeight,
  };
}

async function pickCoordinate(event: MouseEvent) {
  if (pickerMode === "region") return;
  if (!screenBounds.value) return;

  pointer.value = {
    x: event.clientX,
    y: event.clientY,
  };

  if (pickerMode === "color") {
    closing.value = true;
    const color = pickedColor.value;
    if (!color) {
      await closePicker(true);
      return;
    }
    await invoke("finish_mouse_color_pick", {
      request: { color: color.hex },
    });
    await currentWindow.close();
    return;
  }

  closing.value = true;
  await invoke("finish_mouse_coordinate_pick", {
    request: displayCoordinate.value,
  });
  await currentWindow.close();
}

function handleKeydown(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.preventDefault();
    void closePicker(true);
  }
}

async function closePicker(cancel: boolean) {
  closing.value = true;
  if (cancel) {
    await invoke("cancel_mouse_coordinate_pick");
  }
  await currentWindow.close();
}

function scaleX() {
  return screenBounds.value ? screenBounds.value.width / Math.max(viewport.value.width, 1) : 1;
}

function scaleY() {
  return screenBounds.value ? screenBounds.value.height / Math.max(viewport.value.height, 1) : 1;
}

function toScreenCoordinate(point: { x: number; y: number }) {
  if (!screenBounds.value) return { x: 0, y: 0 };
  return {
    x: Math.round(screenBounds.value.left + point.x * scaleX()),
    y: Math.round(screenBounds.value.top + point.y * scaleY()),
  };
}

function waitForOverlayToDisappear() {
  return new Promise((resolve) => {
    window.setTimeout(resolve, 90);
  });
}
</script>

<template>
  <main
    class="coordinate-picker"
    @mousemove="handlePointerMove"
    @mousedown="handlePointerDown"
    @mouseup="handlePointerUp"
    @click="pickCoordinate"
    @contextmenu.prevent="closePicker(true)"
  >
    <div class="crosshair horizontal" :style="{ top: `${pointer.y}px` }" />
    <div class="crosshair vertical" :style="{ left: `${pointer.x}px` }" />
    <div
      v-if="pickerMode === 'region' && selectionRect"
      class="selection-box"
      :style="{
        left: `${selectionRect.left}px`,
        top: `${selectionRect.top}px`,
        width: `${selectionRect.width}px`,
        height: `${selectionRect.height}px`,
      }"
    />
    <div
      class="coordinate-badge"
      :class="{ 'color-badge': pickerMode === 'color' }"
      :style="{
        left: `${Math.min(pointer.x + 14, viewport.width - 154)}px`,
        top: `${Math.min(pointer.y + 14, viewport.height - 34)}px`,
      }"
    >
      <template v-if="pickerMode === 'region' && displayRegion">
        {{ Math.abs(displayRegion.x2 - displayRegion.x1) }} x
        {{ Math.abs(displayRegion.y2 - displayRegion.y1) }}
      </template>
      <template v-else-if="pickerMode === 'color'">
        <div
          v-if="pickedColor"
          class="color-swatch"
          :style="{ backgroundColor: pickedColor.hex }"
        />
        <span>{{ displayCoordinate.x }}, {{ displayCoordinate.y }}</span>
        <span v-if="pickedColor" class="color-hex">{{ pickedColor.hex }}</span>
      </template>
      <template v-else>
        {{ displayCoordinate.x }}, {{ displayCoordinate.y }}
      </template>
    </div>
  </main>
</template>

<style>
html,
body,
#app {
  background: transparent !important;
}
</style>

<style scoped>
.coordinate-picker {
  position: fixed;
  inset: 0;
  overflow: hidden;
  background: transparent;
  cursor: crosshair;
  user-select: none;
}

.crosshair {
  position: fixed;
  z-index: 2;
  pointer-events: none;
  background: #409EFF;
  box-shadow: 0 0 0 1px rgba(0, 0, 0, 0.45);
}

.crosshair.horizontal {
  left: 0;
  right: 0;
  height: 1px;
}

.crosshair.vertical {
  top: 0;
  bottom: 0;
  width: 1px;
}

.coordinate-badge {
  position: fixed;
  z-index: 3;
  min-width: 90px;
  padding: 5px 8px;
  color: #ffffff;
  font-size: 12px;
  font-variant-numeric: tabular-nums;
  text-align: center;
  background: rgba(0, 0, 0, 0.72);
  border: 1px solid rgba(255, 255, 255, 0.18);
  border-radius: 6px;
  pointer-events: none;
}

.selection-box {
  position: fixed;
  z-index: 2;
  pointer-events: none;
  background: rgba(64, 158, 255, 0.16);
  border: 1px solid #409EFF;
  box-shadow:
    inset 0 0 0 1px rgba(255, 255, 255, 0.45),
    0 0 0 1px rgba(0, 0, 0, 0.35);
}

.color-badge {
  display: flex;
  align-items: center;
  gap: 6px;
}

.color-swatch {
  width: 14px;
  height: 14px;
  border: 1px solid rgba(255, 255, 255, 0.5);
  border-radius: 3px;
}

.color-hex {
  font-family: ui-monospace, SFMono-Regular, Menlo, Monaco, Consolas, monospace;
}
</style>
