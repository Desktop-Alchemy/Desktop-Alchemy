<script setup lang="ts">
import { t } from "../../i18n";

import { defineAsyncComponent, nextTick, ref, watch } from "vue";
import type { Page } from "../../bindings";
import { useDeviceContext } from "../context";
import DeckGrid from "./DeckGrid.vue";
import AppIcon from "../../components/AppIcon.vue";
import ModelAppearanceSettings from "./ModelAppearanceSettings.vue";
import { useModelImage } from "./appearance";

const props = defineProps<{
  page: Page;
  selected: number;
  pressed: number[];
  editor?: boolean;
}>();
defineEmits<{ select: [key: number] }>();
const w = useDeviceContext();
const Preview3D = defineAsyncComponent(() => import("./DevicePreview3D.vue"));
const mode = ref<"2d" | "3d">(props.editor ? "2d" : "3d");
const settingsOpen = ref(false);
const settingsTrigger = ref<HTMLButtonElement>();
const { source, error } = useModelImage(() =>
  w.modelAppearance.type === "image" ? w.modelAppearance.image : null,
);
watch(
  () => props.editor,
  (editor) => (mode.value = editor ? "2d" : "3d"),
);
async function closeSettings() {
  settingsOpen.value = false;
  await nextTick();
  settingsTrigger.value?.focus({ preventScroll: true });
}
</script>

<template>
  <div class="m18-device-preview" :class="{ 'editor-model-preview': editor }">
    <div class="device-preview-toolbar">
      <div
        class="device-preview-switch"
        role="group"
        :aria-label="t('ui.configurationPreviewMode')"
      >
        <button :aria-pressed="mode === '3d'" @click="mode = '3d'">3D</button>
        <button :aria-pressed="mode === '2d'" @click="mode = '2d'">2D</button>
      </div>
      <button
        ref="settingsTrigger"
        class="quiet model-mark-button"
        @click="settingsOpen = true"
      >
        <AppIcon name="settings" :size="14" />{{ t("ui.modelBranding") }}
      </button>
    </div>
    <component
      :is="mode === '3d' ? Preview3D : DeckGrid"
      :page="page"
      :selected="selected"
      :pressed="pressed"
      :appearance="w.modelAppearance"
      :label-image="source"
      @select="$emit('select', $event)"
    />
    <p v-if="error" class="helper" role="status">{{ error }}</p>
    <ModelAppearanceSettings
      v-if="settingsOpen"
      :profile-id="w.selectedProfileId"
      @close="closeSettings"
    />
  </div>
</template>
