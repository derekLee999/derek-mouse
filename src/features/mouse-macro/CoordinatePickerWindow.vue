<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue";
import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { ElMessage } from "element-plus";

type CoordinatePickSnapshot = {
  imagePath: string;
  left: number;
  top: number;
  width: number;
  height: number;
};

const currentWindow = getCurrentWindow();
const snapshot = ref<CoordinatePickSnapshot | null>(null);
const imageSrc = computed(() => snapshot.value ? convertFileSrc(snapshot.value.imagePath) : "");
const imageError = ref("");
const pointer = ref({ x: 0, y: 0 });
const viewport = ref({ width: window.innerWidth, height: window.innerHeight });
const closing = ref(false);

const displayCoordinate = computed(() => {
  if (!snapshot.value) return { x: 0, y: 0 };

  return {
    x: Math.max(0, Math.round(snapshot.value.left + pointer.value.x * scaleX())),
    y: Math.max(0, Math.round(snapshot.value.top + pointer.value.y * scaleY())),
  };
});

onMounted(async () => {
  try {
    snapshot.value = await invoke<CoordinatePickSnapshot>("get_mouse_coordinate_pick_snapshot");
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

function handleImageError() {
  imageError.value = "截图加载失败，请按 Esc 退出后重试。";
}

function updateViewport() {
  viewport.value = {
    width: window.innerWidth,
    height: window.innerHeight,
  };
}

async function pickCoordinate(event: MouseEvent) {
  if (!snapshot.value) return;

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
  return snapshot.value ? snapshot.value.width / Math.max(viewport.value.width, 1) : 1;
}

function scaleY() {
  return snapshot.value ? snapshot.value.height / Math.max(viewport.value.height, 1) : 1;
}
</script>

<template>
  <main class="coordinate-picker" @mousemove="handlePointerMove" @click="pickCoordinate">
    <img
      v-if="snapshot"
      class="screen-image"
      :src="imageSrc"
      alt=""
      draggable="false"
      @error="handleImageError"
    />
    <div v-if="imageError" class="error-panel" @click.stop>
      {{ imageError }}
    </div>
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

<style scoped>
.coordinate-picker {
  position: fixed;
  inset: 0;
  overflow: hidden;
  background: #000000;
  cursor: crosshair;
  user-select: none;
}

.screen-image {
  position: absolute;
  inset: 0;
  width: 100%;
  height: 100%;
  object-fit: fill;
  pointer-events: none;
}

.crosshair {
  position: fixed;
  z-index: 2;
  pointer-events: none;
  background: rgba(45, 212, 191, 0.9);
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

.error-panel {
  position: fixed;
  left: 50%;
  top: 50%;
  z-index: 4;
  max-width: 320px;
  padding: 12px 14px;
  color: #ffffff;
  font-size: 13px;
  line-height: 1.6;
  text-align: center;
  background: rgba(0, 0, 0, 0.78);
  border: 1px solid rgba(255, 255, 255, 0.2);
  border-radius: 8px;
  transform: translate(-50%, -50%);
}
</style>
