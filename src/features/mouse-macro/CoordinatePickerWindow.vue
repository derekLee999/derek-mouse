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

const currentWindow = getCurrentWindow();
const screenBounds = ref<ScreenBounds | null>(null);
const pointer = ref({ x: 0, y: 0 });
const viewport = ref({ width: window.innerWidth, height: window.innerHeight });
const closing = ref(false);

const displayCoordinate = computed(() => {
  if (!screenBounds.value) return { x: 0, y: 0 };

  return {
    x: Math.max(0, Math.round(screenBounds.value.left + pointer.value.x * scaleX())),
    y: Math.max(0, Math.round(screenBounds.value.top + pointer.value.y * scaleY())),
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
});

onBeforeUnmount(() => {
  document.removeEventListener("keydown", handleKeydown);
  window.removeEventListener("resize", updateViewport);
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

function updateViewport() {
  viewport.value = {
    width: window.innerWidth,
    height: window.innerHeight,
  };
}

async function pickCoordinate(event: MouseEvent) {
  if (!screenBounds.value) return;

  pointer.value = {
    x: event.clientX,
    y: event.clientY,
  };

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
</script>

<template>
  <main class="coordinate-picker" @mousemove="handlePointerMove" @click="pickCoordinate" @contextmenu.prevent="closePicker(true)">
    <div class="crosshair horizontal" :style="{ top: `${pointer.y}px` }" />
    <div class="crosshair vertical" :style="{ left: `${pointer.x}px` }" />
    <div
      class="coordinate-badge"
      :style="{
        left: `${Math.min(pointer.x + 14, viewport.width - 104)}px`,
        top: `${Math.min(pointer.y + 14, viewport.height - 34)}px`,
      }"
    >
      {{ displayCoordinate.x }}, {{ displayCoordinate.y }}
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
</style>
