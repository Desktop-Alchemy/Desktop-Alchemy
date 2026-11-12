<script setup lang="ts">
import { t } from "../../i18n";

import { computed, defineAsyncComponent, onErrorCaptured, ref } from "vue";
import { useDeviceContext } from "../context";
import poster from "../../assets/devices/ms-audio-dock/audio-dock-poster.png";
import AppIcon from "../../components/AppIcon.vue";
const Model = defineAsyncComponent({
  loader: () => import("./DevicePreview3D.vue"),
  onError: (_error, _retry, fail) => {
    failed();
    fail();
  },
});
const props = defineProps<{ teamsPage: boolean }>();
const w = useDeviceContext();
const mode = ref<"image" | "3d">("3d");
const loading = ref(true),
  error = ref(false),
  revision = ref(0);
const model = ref<{
  setView: (view: "front" | "top" | "back") => void;
  stats: () => unknown;
}>();
const pressed = computed(() => w.ready && w.state.pressed.includes(1));
const description = computed(() =>
  !w.ready
    ? t("ui.deviceOffline")
    : pressed.value
      ? t("ui.teamsKeyPressed")
      : props.teamsPage
        ? t("ui.teamsKeyInputPreview")
        : t("ui.deviceAppearance"),
);
function failed() {
  error.value = true;
  loading.value = false;
}
onErrorCaptured(() => {
  failed();
  return false;
});
function retry() {
  error.value = false;
  loading.value = true;
  revision.value++;
  mode.value = "3d";
}
function changeMode(value: "image" | "3d") {
  mode.value = value;
  if (value === "3d") retry();
}
defineExpose({ stats: () => model.value?.stats() });
</script>
<template>
  <section
    class="panel dock-visual-panel"
    :aria-label="t('ui.audioDockAppearancePreview')"
  >
    <div class="dock-visual-heading">
      <h2>{{ t("ui.appearancePreview") }}</h2>
      <div class="segmented" role="group" :aria-label="t('ui.previewMode')">
        <button :aria-pressed="mode === 'image'" @click="changeMode('image')">
          {{ t("ui.images") }}
        </button>
        <button :aria-pressed="mode === '3d'" @click="changeMode('3d')">
          3D
        </button>
      </div>
    </div>
    <div
      class="dock-model-stage"
      :class="{ 'is-pressed': pressed }"
      role="group"
      :aria-label="`Microsoft Audio Dock, ${description}`"
    >
      <img
        v-if="mode === 'image' || loading || error"
        :src="poster"
        class="dock-model-poster"
        :alt="t('ui.microsoftAudioDockAppearance')"
      />
      <Model
        v-if="mode === '3d' && !error"
        ref="model"
        :key="revision"
        :pressed="pressed"
        :focused="teamsPage"
        @loaded="loading = false"
        @failed="failed"
      />
      <span
        v-if="mode === '3d' && loading"
        class="dock-model-message"
        role="status"
        >{{ t("ui.loading3dPreview") }}</span
      >
    </div>
    <div
      v-if="mode === '3d' && error"
      class="dock-model-fallback"
      role="status"
    >
      <span>{{ t("ui.3dUnavailableShowingTheDeviceImage") }}</span
      ><button class="quiet" @click="retry">{{ t("ui.retry") }}</button>
    </div>
    <div v-else-if="mode === '3d'" class="dock-model-tools">
      <span>{{ t("ui.dragToRotateScrollToZoom2") }}</span>
      <div role="group" :aria-label="t('ui.modelView')">
        <button
          class="quiet"
          :disabled="loading"
          @click="model?.setView('front')"
          :aria-label="t('ui.frontAngle')"
        >
          <AppIcon name="reconnect" :size="15" />
        </button>
        <button
          class="quiet"
          :disabled="loading"
          @click="model?.setView('top')"
        >
          {{ t("ui.top") }}
        </button>
        <button
          class="quiet"
          :disabled="loading"
          @click="model?.setView('back')"
        >
          {{ t("ui.rear") }}
        </button>
      </div>
    </div>
    <div class="dock-model-caption" :class="{ 'is-active': pressed }">
      <i aria-hidden="true"></i><span>{{ description }}</span>
    </div>
  </section>
</template>
