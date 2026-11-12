<script setup lang="ts">
import { t } from "../../i18n";

import { ref, watch, onMounted, onBeforeUnmount } from "vue";
import type { Page, ModelAppearance } from "../../bindings";
import { useDeviceContext } from "../context";
import AppIcon from "../../components/AppIcon.vue";
import { M18Scene } from "./scene3d";
import type { LedPreviewSettings } from "./led-preview";

const props = defineProps<{
  page: Page;
  selected: number;
  pressed: number[];
  appearance?: ModelAppearance;
  labelImage?: string;
}>();
const emit = defineEmits<{ select: [key: number] }>();
const w = useDeviceContext();
const host = ref<HTMLDivElement>();
const loading = ref(true);
const error = ref("");
let scene: M18Scene | undefined;
let disposed = false;
let generation = 0;
let timer: ReturnType<typeof setTimeout>;
let previews: Record<number, string> = {};
let pageSignature = "";
const ledSettings = (): LedPreviewSettings => {
  const settings = w.draft.device_settings;
  return {
    led_mode: settings.led_mode,
    led_color: settings.led_color,
    led_brightness: settings.led_brightness,
    led_speed: settings.led_speed,
    led_colors: settings.led_colors,
  };
};

async function mountScene() {
  if (loading.value && scene) return;
  scene?.dispose();
  scene = undefined;
  loading.value = true;
  error.value = "";
  try {
    if (!host.value || disposed) return;
    const current = new M18Scene(
      host.value,
      (key) => emit("select", key),
      (message) => {
        error.value = message;
        current.dispose();
      },
    );
    scene = current;
    await current.load();
    if (disposed || current !== scene) return;
    current.setPage(props.page, previews);
    current.setSelection(props.selected, props.pressed);
    current.setAppearance(
      props.appearance ?? { type: "default" },
      props.labelImage ?? "",
    );
    current.setLedSettings(ledSettings());
  } catch {
    scene?.dispose();
    scene = undefined;
    if (!disposed) error.value = t("ui.3dPreviewUnavailableRetryOrSwitch");
  } finally {
    if (!disposed) loading.value = false;
  }
}

watch(
  [
    () => JSON.stringify(props.page),
    () => (props.page.keys.some((key) => key.widget) ? w.widgetRevision : 0),
    () =>
      props.page.keys.some((key) => key.plugin_icon) ? w.library.revision : 0,
    () =>
      props.page.keys.some((key) => key.widget)
        ? JSON.stringify(
            w.state.plugins.map((plugin) => [
              plugin.manifest.id,
              plugin.status,
            ]),
          ) + JSON.stringify(w.state.config.disabled_plugins)
        : "",
  ],
  () => {
    clearTimeout(timer);
    const current = ++generation;
    const profileId = w.selectedProfileId;
    const page = JSON.parse(JSON.stringify(props.page)) as Page;
    const signature = JSON.stringify(page);
    if (signature !== pageSignature) previews = {};
    pageSignature = signature;
    scene?.setPage(page, previews);
    timer = setTimeout(async () => {
      if (!w.desktop || disposed || profileId !== w.selectedProfileId) return;
      const images: Record<number, string> = {};
      await Promise.all(
        page.keys
          .filter((key) => key.key <= 15)
          .map(async (key) => {
            const image = await w.preview(key);
            if (image) images[key.key] = image;
          }),
      );
      if (
        disposed ||
        current !== generation ||
        profileId !== w.selectedProfileId
      )
        return;
      previews = images;
      scene?.setPage(page, images);
    }, 140);
  },
  { immediate: true },
);

watch(
  [() => props.selected, () => props.pressed],
  () => {
    scene?.setSelection(props.selected, props.pressed);
  },
  { deep: true },
);
watch([() => JSON.stringify(props.appearance), () => props.labelImage], () => {
  scene?.setAppearance(
    props.appearance ?? { type: "default" },
    props.labelImage ?? "",
  );
});
watch(
  () => JSON.stringify(ledSettings()),
  () => scene?.setLedSettings(ledSettings()),
);

function keydown(event: KeyboardEvent) {
  const delta = ["ArrowRight", "ArrowDown"].includes(event.key)
    ? 1
    : ["ArrowLeft", "ArrowUp"].includes(event.key)
      ? -1
      : 0;
  if (!delta && !["Home", "End"].includes(event.key)) return;
  event.preventDefault();
  emit(
    "select",
    event.key === "Home"
      ? 1
      : event.key === "End"
        ? 18
        : ((props.selected - 1 + delta + 18) % 18) + 1,
  );
}

onMounted(mountScene);
onBeforeUnmount(() => {
  disposed = true;
  generation++;
  clearTimeout(timer);
  scene?.dispose();
});
defineExpose({
  stats: () => scene?.stats(),
  capturePng: () => scene?.capturePng(),
});
</script>

<template>
  <div class="m18-model-preview">
    <div
      class="m18-model-viewport"
      role="group"
      tabindex="0"
      :aria-label="
        t('ui.m183dConfigurationPreviewKeySelected', { arg0: selected })
      "
      @keydown="keydown"
    >
      <div ref="host" class="m18-model-canvas" />
      <div v-if="loading" class="m18-model-overlay" role="status">
        {{ t("ui.loading3dPreview") }}
      </div>
      <div v-else-if="error" class="m18-model-overlay" role="alert">
        <p>{{ error }}</p>
        <button @click="mountScene">{{ t("ui.retryLoading") }}</button>
      </div>
    </div>
    <div class="m18-model-toolbar">
      <span>{{ t("ui.dragToRotateScrollToZoom") }}</span>
      <button
        class="quiet"
        :disabled="loading || !!error"
        @click="scene?.resetView()"
      >
        <AppIcon name="reconnect" :size="14" />{{ t("ui.resetView") }}
      </button>
    </div>
    <p
      v-if="w.draft.device_settings.led_mode === 'device_default'"
      class="helper"
    >
      {{ t("ui.theDeviceDefaultEffectIsPlayed") }}
    </p>
  </div>
</template>
