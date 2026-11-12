<script setup lang="ts">
import { t } from "../../i18n";

import { computed, ref, watch, onBeforeUnmount } from "vue";
import { useAudioDock } from "./useAudioDock";
import AppIcon from "../../components/AppIcon.vue";
import AudioSettings from "./AudioSettings.vue";
import EqualizerSettings from "./EqualizerSettings.vue";
import TeamsSettings from "./TeamsSettings.vue";
import DevicePreview from "./DevicePreview.vue";
const props = withDefaults(defineProps<{ page?: string }>(), {
  page: "device",
});
const w = useAudioDock();
const title = computed(() =>
  props.page === "device:equalizer"
    ? t("ui.equalizer")
    : props.page === "device:teams"
      ? t("ui.teamsKey")
      : t("ui.audioSettings"),
);
const subtitle = computed(() =>
  props.page === "device:equalizer"
    ? t("ui.chooseASoundPresetOrCreate")
    : props.page === "device:teams"
      ? t("ui.useTheTeamsKeyToOpen")
      : t("ui.adjustPromptTonesAndMicrophoneBehavior"),
);
const message = ref("");
let messageTimer: ReturnType<typeof setTimeout>;
function applied(value = t("ui.applied")) {
  clearTimeout(messageTimer);
  message.value = value;
  messageTimer = setTimeout(() => (message.value = ""), 3500);
}
watch(
  () => props.page,
  () => {
    clearTimeout(messageTimer);
    message.value = "";
    if (props.page !== "device:teams" && w.ready && !w.audioDockPending)
      void w.refreshAudioDock();
  },
);
onBeforeUnmount(() => clearTimeout(messageTimer));
</script>
<template>
  <div class="dock-page">
    <div class="page-heading dock-heading">
      <div>
        <h1>{{ title }}</h1>
        <p>{{ subtitle }}</p>
      </div>
      <div v-if="page !== 'device:teams'" class="dock-sync">
        <span v-if="w.audioDockPending" class="helper" role="status">{{
          t("ui.syncing")
        }}</span>
        <span
          v-else-if="message && !w.audioDockError"
          class="dock-applied"
          role="status"
          ><AppIcon name="check" :size="15" />{{ message }}</span
        >
        <button
          class="quiet"
          :disabled="!w.ready || !w.desktop || w.audioDockPending"
          @click="w.refreshAudioDock()"
        >
          <AppIcon name="reconnect" :size="16" />{{ t("ui.refresh") }}
        </button>
      </div>
    </div>
    <div class="dock-workspace">
      <DevicePreview :teams-page="page === 'device:teams'" />
      <div class="dock-settings">
        <template v-if="page !== 'device:teams'">
          <div v-if="!w.ready" class="dock-feedback" role="status">
            <AppIcon name="warning" :size="18" /><span>{{
              t("ui.deviceOfflineReconnectToAdjustAudio")
            }}</span>
          </div>
          <div
            v-else-if="w.audioDockError"
            class="dock-feedback error-text"
            role="alert"
          >
            <AppIcon name="warning" :size="18" /><span>{{
              w.audioDockError
            }}</span
            ><button
              :disabled="w.audioDockPending"
              @click="w.refreshAudioDock()"
            >
              {{ t("ui.readAgain") }}
            </button>
          </div>
          <p v-else-if="!w.audioDockState" class="helper" role="status">
            {{ t("ui.readingDeviceSettings") }}
          </p>
        </template>
        <!-- Keep the editors mounted across device subpages so EQ drafts survive navigation. -->
        <AudioSettings v-show="page === 'device'" @applied="applied()" />
        <EqualizerSettings
          v-show="page === 'device:equalizer'"
          @applied="applied($event)"
        />
        <TeamsSettings
          v-show="page === 'device:teams'"
          :active="page === 'device:teams'"
        />
      </div>
    </div>
  </div>
</template>
<style src="./audio-dock.css"></style>
