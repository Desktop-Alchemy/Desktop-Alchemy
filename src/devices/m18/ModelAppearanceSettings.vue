<script setup lang="ts">
import { t } from "../../i18n";

import { ref, computed, nextTick } from "vue";
import { useDeviceContext } from "../context";
import type { ImageRef, ModelAppearance } from "../../bindings";
import AppDialog from "../../components/AppDialog.vue";
import AppSelect from "../../components/AppSelect.vue";
import LibraryBrowser from "../../components/LibraryBrowser.vue";
import { useModelImage } from "./appearance";

const props = defineProps<{ profileId: string }>();
const emit = defineEmits<{ close: [] }>();
const w = useDeviceContext();
const initial = w.settings.model_appearances[props.profileId] ?? {
  type: "default" as const,
};
const mode = ref<ModelAppearance["type"]>(initial.type);
const text = ref(initial.type === "text" ? initial.text : "");
const image = ref<ImageRef | null>(
  initial.type === "image" ? JSON.parse(JSON.stringify(initial.image)) : null,
);
const picker = ref(false);
const pickerTrigger = ref<HTMLButtonElement>();
const saving = ref(false);
const saveError = ref("");
const { source, error: imageError } = useModelImage(() =>
  mode.value === "image" ? image.value : null,
);
const textError = computed(() =>
  mode.value === "text" &&
  (!text.value.trim() ||
    [...text.value].length > 32 ||
    /\p{Cc}/u.test(text.value))
    ? t("ui.enter132CharactersWithoutLine")
    : "",
);
const invalid = computed(
  () =>
    !!textError.value ||
    (mode.value === "image" && (!image.value || !source.value)),
);
function close() {
  if (!saving.value) emit("close");
}
async function closePicker() {
  picker.value = false;
  await nextTick();
  pickerTrigger.value?.focus({ preventScroll: true });
}
async function save() {
  if (saving.value || invalid.value) return;
  const appearance: ModelAppearance =
    mode.value === "default"
      ? { type: "default" }
      : mode.value === "text"
        ? { type: "text", text: text.value.trim() }
        : { type: "image", image: JSON.parse(JSON.stringify(image.value)) };
  saving.value = true;
  saveError.value = "";
  const success = await w.setModelAppearance(props.profileId, appearance);
  saving.value = false;
  if (success) emit("close");
  else saveError.value = w.error || t("ui.couldNotSaveModelBrandingTry");
}
</script>

<template>
  <AppDialog :title="t('ui.modelBranding')" @close="close">
    <p>{{ t("ui.usedInTheDeviceWorkspaceS") }}</p>
    <div class="model-mark-sample" :aria-label="t('ui.brandingPreview')">
      <img
        v-if="mode === 'image' && source"
        :src="source"
        :alt="t('ui.customBrandingPreview')"
      />
      <span v-else>{{
        mode === "text" ? text || t("ui.previewText") : "M18"
      }}</span>
    </div>
    <label
      >{{ t("ui.brandingContent") }}
      <AppSelect
        v-model="mode"
        :aria-label="t('ui.brandingContent')"
        :disabled="saving"
        :options="[
          { value: 'default', label: t('ui.defaultM18') },
          { value: 'text', label: t('ui.customText') },
          { value: 'image', label: t('ui.customImage') },
        ]"
      />
    </label>
    <label v-if="mode === 'text'"
      >{{ t("ui.brandingText") }}
      <input
        v-model="text"
        :aria-label="t('ui.brandingText')"
        :disabled="saving"
        :aria-invalid="!!textError"
        :placeholder="t('ui.enterText')"
      />
    </label>
    <p v-if="textError" class="error-text" role="alert">{{ textError }}</p>
    <div v-if="mode === 'image'">
      <button ref="pickerTrigger" :disabled="saving" @click="picker = true">
        {{ t("ui.chooseImage") }}
      </button>
      <p class="helper">
        {{ t("ui.importPngOrJpegFilesIn") }}
      </p>
      <p v-if="imageError" class="error-text" role="alert">{{ imageError }}</p>
    </div>
    <p v-if="saveError" class="error-text" role="alert">{{ saveError }}</p>
    <div class="button-row dialog-actions">
      <button data-cancel :disabled="saving" @click="close">
        {{ t("ui.cancel") }}
      </button>
      <button
        class="primary"
        :disabled="!w.desktop || saving || invalid"
        @click="save"
      >
        {{ saving ? t("ui.saving") : t("ui.saveBranding") }}
      </button>
    </div>
  </AppDialog>
  <AppDialog
    v-if="picker"
    :title="t('ui.chooseBrandingImage')"
    wide
    @close="closePicker"
  >
    <LibraryBrowser
      pick
      static-only
      @select="
        image = $event;
        closePicker();
      "
    />
  </AppDialog>
</template>
