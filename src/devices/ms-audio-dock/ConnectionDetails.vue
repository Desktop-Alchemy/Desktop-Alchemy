<script setup lang="ts">
import UiFailure from "../../components/UiFailure.vue";
import { t } from "../../i18n";

import { useAudioDock } from "./useAudioDock";
const w = useAudioDock();
const labels: Record<string, string> = {
  get model() {
    return t("ui.model");
  },
  get serial() {
    return t("ui.serialNumber");
  },
  get hardware_version() {
    return t("ui.hardwareVersion");
  },
  get color_code() {
    return t("ui.colorCode");
  },
  get firmware_version() {
    return t("ui.firmwareVersion");
  },
};
async function copy() {
  await navigator.clipboard.writeText(
    JSON.stringify(w.audioDockState?.info ?? {}, null, 2),
  );
  w.notify(t("ui.deviceInformationCopied"), "success");
}
</script>
<template>
  <UiFailure
    v-if="w.audioDockError"
    :summary="w.audioDockError"
    :detail="w.audioDockErrorDetail"
  />
  <dl class="connection-details">
    <template v-for="(value, key) in w.audioDockState?.info" :key="key"
      ><dt>{{ labels[key] ?? key }}</dt>
      <dd>{{ value ?? t("ui.notRead") }}</dd></template
    >
  </dl>
  <button :disabled="!w.audioDockState?.info.serial" @click="copy">
    {{ t("ui.copyDeviceInformation") }}</button
  ><button
    :disabled="!w.ready || w.audioDockPending"
    @click="w.refreshAudioDock()"
  >
    {{ t("ui.readAgain") }}
  </button>
</template>
