<script setup lang="ts">
import { t } from "../i18n";

import { onMounted, onUnmounted, ref } from "vue";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { isTauri } from "@tauri-apps/api/core";
import AppIcon from "./AppIcon.vue";
import logo from "../assets/brand/logo.png";

const isMaximized = ref(false);
let unlistenResize: (() => void) | undefined;
let appWindow: ReturnType<typeof getCurrentWindow> | undefined;

function currentWindow() {
  return (appWindow ??= getCurrentWindow());
}

async function updateMaximized() {
  if (!isTauri()) return;
  try {
    isMaximized.value = await currentWindow().isMaximized();
  } catch {
    // The titlebar remains usable if the window state query is unavailable.
  }
}

onMounted(async () => {
  if (!isTauri()) return;
  await updateMaximized();
  try {
    unlistenResize = await currentWindow().listen(
      "tauri://resize",
      updateMaximized,
    );
  } catch {
    // Native controls remain available even if resize notifications fail.
  }
});

onUnmounted(() => unlistenResize?.());

async function handleDragStart(event: MouseEvent) {
  if (
    !isTauri() ||
    event.button !== 0 ||
    (event.target as HTMLElement).closest("button")
  )
    return;
  try {
    if (event.detail === 2) await currentWindow().toggleMaximize();
    else await currentWindow().startDragging();
  } catch {
    // Native window actions are best-effort during shutdown.
  }
}

async function minimize() {
  if (!isTauri()) return;
  try {
    await currentWindow().minimize();
  } catch {
    // The window may already be closing.
  }
}

async function toggleMaximize() {
  if (!isTauri()) return;
  try {
    await currentWindow().toggleMaximize();
    await updateMaximized();
  } catch {
    // The window may already be closing.
  }
}

async function close() {
  if (!isTauri()) return;
  try {
    await currentWindow().close();
  } catch {
    // The main window's close handler owns exit and save behavior.
  }
}
</script>

<template>
  <header
    class="titlebar"
    :aria-label="t('ui.appTitleBar')"
    @mousedown.capture="handleDragStart"
  >
    <div class="titlebar-brand">
      <img :src="logo" alt="" />
      <span>Desktop Alchemy</span>
    </div>
    <div class="titlebar-drag-region" aria-hidden="true" />
    <div class="titlebar-controls">
      <button
        type="button"
        class="titlebar-button"
        :aria-label="t('ui.minimizeWindow')"
        :title="t('ui.minimize')"
        :disabled="!isTauri()"
        @mousedown.stop
        @click="minimize"
      >
        <AppIcon name="minimize" :size="15" />
      </button>
      <button
        type="button"
        class="titlebar-button"
        :aria-label="
          isMaximized ? t('ui.restoreWindow') : t('ui.maximizeWindow')
        "
        :title="isMaximized ? t('ui.restore') : t('ui.maximize')"
        :disabled="!isTauri()"
        @mousedown.stop
        @click="toggleMaximize"
      >
        <AppIcon
          :name="isMaximized ? 'restore' : 'maximize'"
          :class="{ 'titlebar-restore-icon': isMaximized }"
          :size="14"
        />
      </button>
      <button
        type="button"
        class="titlebar-button titlebar-button-close"
        :aria-label="t('ui.closeWindow')"
        :title="t('ui.close')"
        :disabled="!isTauri()"
        @mousedown.stop
        @click="close"
      >
        <AppIcon name="close" :size="16" />
      </button>
    </div>
  </header>
</template>
