<script setup lang="ts">
import { t } from "../../i18n";

import { computed, ref, watch, onBeforeUnmount } from "vue";
import { useAudioDock } from "./useAudioDock";
import AppSwitch from "../../components/AppSwitch.vue";
import AppSelect from "../../components/AppSelect.vue";
import AppIcon from "../../components/AppIcon.vue";
import type {
  AudioDockSettingsUpdate,
  AudioDockNoiseSuppression,
} from "../../bindings";
const emit = defineEmits<{ applied: [] }>();
const w = useAudioDock();
const settings = computed(() => w.audioDockState?.settings);
const enabled = computed(
  () =>
    w.ready &&
    w.desktop &&
    !w.audioDockPending &&
    w.audioDockState?.connection === "ready",
);
const volume = ref<number | null>(null);
let timer: ReturnType<typeof setTimeout>;
let dragging = false;
const keyboard = ref(false);
const volumeKeys = [
  "ArrowLeft",
  "ArrowRight",
  "ArrowUp",
  "ArrowDown",
  "Home",
  "End",
  "PageUp",
  "PageDown",
];
watch(
  () => settings.value?.prompt_volume,
  (v) => {
    if (v != null) volume.value = v;
  },
  { immediate: true },
);
watch(
  () => w.currentProfile?.device_id,
  () => {
    clearTimeout(timer);
    volume.value = settings.value?.prompt_volume ?? null;
  },
);
onBeforeUnmount(() => clearTimeout(timer));
async function apply(update: AudioDockSettingsUpdate) {
  const id = w.currentProfile?.device_id;
  const value = await w.updateAudioDock(update);
  if (id === w.currentProfile?.device_id && value?.connection === "ready")
    emit("applied");
}
function volumeInput(event: Event) {
  volume.value = Number((event.target as HTMLInputElement).value);
  clearTimeout(timer);
  if (!dragging) {
    const id = w.currentProfile?.device_id;
    timer = setTimeout(() => {
      if (id === w.currentProfile?.device_id) void commitVolume();
    }, 450);
  }
}
async function commitVolume() {
  clearTimeout(timer);
  dragging = false;
  if (
    enabled.value &&
    volume.value != null &&
    settings.value?.prompt_volume !== volume.value
  )
    await apply({ type: "prompt_volume", value: volume.value });
}
</script>
<template>
  <div class="dock-audio">
    <section
      class="panel dock-settings-group"
      :aria-label="t('ui.promptToneSettings')"
    >
      <div class="dock-setting">
        <span class="dock-setting-icon"
          ><AppIcon name="speaker" :size="22"
        /></span>
        <div class="dock-setting-copy">
          <h2>{{ t("ui.promptTone") }}</h2>
          <p>{{ t("ui.playAudioPromptsForDeviceOperations") }}</p>
        </div>
        <AppSwitch
          :model-value="settings?.prompt_tone ?? null"
          :disabled="!enabled"
          :label="t('ui.promptTone')"
          hide-label
          @update:model-value="apply({ type: 'prompt_tone', enabled: $event })"
        />
      </div>
      <div class="dock-volume">
        <div class="dock-volume-label">
          <label for="dock-prompt-volume">{{ t("ui.promptVolume") }}</label
          ><output for="dock-prompt-volume">{{
            settings?.prompt_volume == null ? t("ui.notRead") : volume + "%"
          }}</output>
        </div>
        <input
          id="dock-prompt-volume"
          :value="volume ?? 0"
          type="range"
          min="0"
          max="100"
          step="1"
          :disabled="!enabled || volume == null"
          :aria-label="t('ui.promptVolume')"
          @input="volumeInput"
          @pointerdown="
            dragging = true;
            keyboard = false;
          "
          @pointerup="dragging = false"
          @pointercancel="dragging = false"
          @keydown="keyboard = volumeKeys.includes($event.key)"
          @keyup="keyboard = false"
          @change="!keyboard && commitVolume()"
          @blur="commitVolume"
        />
        <p class="helper">{{ t("ui.changingTheSwitchOrVolumePlays") }}</p>
      </div>
    </section>
    <section
      class="panel dock-settings-group"
      :aria-label="t('ui.callSettings')"
    >
      <div class="dock-setting">
        <span class="dock-setting-icon"
          ><AppIcon name="microphone" :size="22"
        /></span>
        <div class="dock-setting-copy">
          <h2>{{ t("ui.pushToTalk") }}</h2>
          <p>{{ t("ui.holdTheMuteKeyToUnmute") }}</p>
        </div>
        <AppSwitch
          :model-value="settings?.push_to_talk ?? null"
          :disabled="!enabled"
          :label="t('ui.pushToTalk')"
          hide-label
          @update:model-value="apply({ type: 'push_to_talk', enabled: $event })"
        />
      </div>
      <div class="dock-setting dock-noise">
        <span class="dock-setting-icon"
          ><AppIcon name="equalizer" :size="22"
        /></span>
        <div class="dock-setting-copy">
          <h2>{{ t("ui.microphoneNoiseSuppression") }}</h2>
          <p>{{ t("ui.useStandardInNoisyEnvironmentsOr") }}</p>
        </div>
        <AppSelect
          :disabled="!enabled"
          :model-value="settings?.noise_suppression ?? ''"
          :options="[
            { value: '', label: t('ui.notRead'), disabled: true },
            { value: 'standard', label: t('ui.standardDefault') },
            { value: 'low', label: t('ui.lowVolume') },
          ]"
          :aria-label="t('ui.microphoneNoiseSuppressionMode')"
          @update:model-value="
            apply({
              type: 'noise_suppression',
              mode: $event as AudioDockNoiseSuppression,
            })
          "
        />
      </div>
    </section>
  </div>
</template>
