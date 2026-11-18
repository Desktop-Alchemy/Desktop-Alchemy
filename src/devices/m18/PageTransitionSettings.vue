<script setup lang="ts">
import { computed, onUnmounted, ref, watch } from "vue";
import { t, errorText } from "../../i18n";
import type { M18PageTransition, M18TransitionEdit } from "../../bindings";
import AppSelect from "../../components/AppSelect.vue";
import { useDeviceContext } from "../context";
import { transitionDraft, type SlideDirection } from "./transition-drafts";

const w = useDeviceContext();
const { draft, saved, error, lastSlideDirection } = transitionDraft(
  w.selectedProfileId,
);
const isSlide = computed(() => draft.value.mode.startsWith("slide_"));
watch(
  () => draft.value.mode,
  (mode) => {
    if (mode.startsWith("slide_"))
      lastSlideDirection.value = mode as SlideDirection;
  },
  { immediate: true, flush: "sync" },
);
const animation = computed<"none" | "fade_in" | "slide">({
  get: () =>
    isSlide.value ? "slide" : (draft.value.mode as "none" | "fade_in"),
  set: (mode) => {
    draft.value.mode = mode === "slide" ? lastSlideDirection.value : mode;
  },
});
const direction = computed<SlideDirection>({
  get: () => lastSlideDirection.value,
  set: (mode) => {
    draft.value.mode = mode;
  },
});
const loading = ref(false);
const saving = ref(false);
const dirty = computed(
  () =>
    !!saved.value &&
    JSON.stringify(draft.value) !== JSON.stringify(saved.value),
);
let generation = 0;
onUnmounted(() => generation++);
async function load(acceptLatest = false) {
  const deviceId = w.currentProfile?.device_id;
  const ticket = ++generation;
  loading.value = false;
  saving.value = false;
  if (!deviceId || !w.ready) return;
  loading.value = true;
  error.value = "";
  try {
    const settings = await w.moduleRequest<M18PageTransition>(
      deviceId,
      "page_transition.get",
    );
    if (ticket !== generation) return;
    if (
      dirty.value &&
      !acceptLatest &&
      JSON.stringify(saved.value) !== JSON.stringify(settings) &&
      JSON.stringify(draft.value) !== JSON.stringify(settings)
    ) {
      error.value = t("errors.savedResourceChanged");
    } else {
      if (!dirty.value) draft.value = { ...settings };
      saved.value = settings;
    }
  } catch (e) {
    if (ticket === generation) error.value = errorText(e);
  } finally {
    if (ticket === generation) loading.value = false;
  }
}
watch(
  () => [w.currentProfile?.device_id, w.ready] as const,
  () => void load(),
  {
    immediate: true,
  },
);

async function save() {
  const deviceId = w.currentProfile?.device_id;
  if (!deviceId || !w.ready || saving.value || loading.value || !saved.value)
    return;
  const ticket = generation;
  saving.value = true;
  error.value = "";
  try {
    const edit: M18TransitionEdit = {
      settings: { ...draft.value },
      expected: { ...saved.value },
    };
    const result = await w.moduleRequest<M18PageTransition>(
      deviceId,
      "page_transition.set",
      edit,
    );
    if (ticket === generation) saved.value = result;
  } catch (e) {
    if (ticket === generation) error.value = errorText(e);
  } finally {
    if (ticket === generation) saving.value = false;
  }
}
const disabled = computed(
  () => !w.ready || loading.value || saving.value || !saved.value,
);
</script>

<template>
  <div class="page-transition-settings">
    <label class="range-label">
      {{ t("ui.pageTransition") }}
      <AppSelect
        v-model="animation"
        :aria-label="t('ui.pageTransition')"
        :disabled="disabled"
        :options="[
          { value: 'none', label: t('ui.transitionNone') },
          { value: 'fade_in', label: t('ui.transitionFadeIn') },
          { value: 'slide', label: t('ui.transitionSlide') },
        ]"
      />
    </label>
    <label v-if="isSlide" class="range-label">
      {{ t("ui.transitionDirection") }}
      <AppSelect
        v-model="direction"
        :aria-label="t('ui.transitionDirection')"
        :disabled="disabled"
        :options="[
          { value: 'slide_left', label: t('ui.transitionSlideLeft') },
          { value: 'slide_right', label: t('ui.transitionSlideRight') },
          { value: 'slide_up', label: t('ui.transitionSlideUp') },
          { value: 'slide_down', label: t('ui.transitionSlideDown') },
          { value: 'slide_random', label: t('ui.transitionSlideRandom') },
        ]"
      />
    </label>
    <label v-if="draft.mode !== 'none'" class="range-label">
      {{ t("ui.transitionDuration") }}
      <strong>{{ draft.duration_ms }} ms</strong>
      <input
        v-model.number="draft.duration_ms"
        type="range"
        min="150"
        max="1000"
        step="50"
        :disabled="disabled"
      />
    </label>
    <p class="helper">
      {{ isSlide ? t("ui.pageSlideHint") : t("ui.pageTransitionHint") }}
    </p>
    <p v-if="error" role="alert" class="helper error">{{ error }}</p>
    <button
      v-if="error && !saved"
      class="secondary"
      :disabled="!w.ready || loading"
      @click="load()"
    >
      {{ t("ui.retry") }}
    </button>
    <button
      v-if="error && saved"
      class="secondary"
      :disabled="!w.ready || loading || saving"
      @click="load(true)"
    >
      {{ t("ui.readLatestSettings") }}
    </button>
    <button class="secondary" :disabled="disabled || !dirty" @click="save">
      {{ saving ? t("ui.saving") : t("ui.saveTransition") }}
    </button>
  </div>
</template>

<style scoped>
.page-transition-settings {
  border-top: 1px solid var(--border);
  padding-top: 12px;
  margin-top: 12px;
}
</style>
