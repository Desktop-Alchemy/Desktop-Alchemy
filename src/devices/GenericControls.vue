<script setup lang="ts">
import { t } from "../i18n";

import GenericBindings from "./GenericBindings.vue";
import { useDeviceContext } from "./context";
const w = useDeviceContext();
const supported = (id: string) =>
  w.device?.capabilities[id]?.available === true &&
  !w.state.config.disabled_capabilities.includes(id);
</script>
<template>
  <section class="panel">
    <h2>{{ w.currentProfile?.name }}</h2>
    <p>{{ w.ready ? t("ui.deviceConnected") : t("ui.deviceOffline") }}</p>
    <fieldset :disabled="!w.ready || !!w.currentProfile?.config_error">
      <label v-if="supported('screen_brightness')" class="range-label"
        >{{ t("ui.screenBrightness") }}
        {{ w.draft.device_settings.screen_brightness }}%<input
          v-model.number="w.draft.device_settings.screen_brightness"
          type="range"
          min="0"
          max="100"
          @change="
            w.hardware({
              type: 'screen_brightness',
              value: w.draft.device_settings.screen_brightness,
            })
          "
      /></label>
      <template v-if="supported('led_color')">
        <label class="range-label"
          >{{ t("ui.ledColor")
          }}<input
            v-model="w.draft.device_settings.led_color"
            type="color"
            @change="
              w.draft.device_settings.led_mode = 'solid';
              w.setLedSettings(w.draft.device_settings);
            "
        /></label>
        <label class="range-label"
          >{{ t("ui.ledBrightness")
          }}<input
            v-model.number="w.draft.device_settings.led_brightness"
            type="range"
            min="0"
            max="255"
            @change="w.setLedSettings(w.draft.device_settings)"
        /></label>
      </template>
    </fieldset>
    <GenericBindings v-if="!w.currentProfile?.definition.supports_pages" />
  </section>
</template>
<style scoped>
fieldset {
  border: 0;
  padding: 0;
  min-width: 0;
}
</style>
