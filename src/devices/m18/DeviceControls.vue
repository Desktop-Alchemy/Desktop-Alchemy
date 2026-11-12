<script setup lang="ts">
import { t } from "../../i18n";

import { computed } from "vue";
import { useDeviceContext } from "../context";
import type { LedMode } from "../../bindings";
import AppSelect from "../../components/AppSelect.vue";

const w = useDeviceContext();
const disabled = (feature: string) =>
  !w.ready || w.device?.capabilities[feature]?.available === false;
const animatedModes: LedMode[] = [
  "breathing",
  "color_cycle",
  "rainbow",
  "wave",
  "comet",
];
const animated = computed(() =>
  animatedModes.includes(w.draft.device_settings.led_mode),
);
const usesPrimaryColor = computed(() =>
  ["solid", "breathing", "wave", "comet"].includes(
    w.draft.device_settings.led_mode,
  ),
);
const individualModeDisabled = computed(() => disabled("individual_leds"));
const modes: Array<{
  value: LedMode;
  label: string;
  needsIndividual?: boolean;
}> = [
  {
    value: "solid",
    get label() {
      return t("ui.solid");
    },
  },
  {
    value: "breathing",
    get label() {
      return t("ui.breathing");
    },
    needsIndividual: true,
  },
  {
    value: "color_cycle",
    get label() {
      return t("ui.colorCycle");
    },
    needsIndividual: true,
  },
  {
    value: "rainbow",
    get label() {
      return t("ui.rainbow");
    },
    needsIndividual: true,
  },
  {
    value: "wave",
    get label() {
      return t("ui.wave");
    },
    needsIndividual: true,
  },
  {
    value: "comet",
    get label() {
      return t("ui.comet");
    },
    needsIndividual: true,
  },
  {
    value: "individual",
    get label() {
      return t("ui.staticPerLed");
    },
    needsIndividual: true,
  },
  {
    value: "device_default",
    get label() {
      return t("ui.deviceDefaultEffect");
    },
  },
];

function applyLedSettings() {
  void w.setLedSettings(w.draft.device_settings);
}

function setLedMode(mode: LedMode) {
  w.draft.device_settings.led_mode = mode;
  applyLedSettings();
}
</script>

<template>
  <section class="panel control-panel">
    <div class="panel-heading">
      <h2>{{ t("ui.screenAndLighting") }}</h2>
    </div>
    <label class="range-label">
      {{ t("ui.screenBrightness") }}
      <strong>{{ w.draft.device_settings.screen_brightness }}%</strong>
      <input
        v-model.number="w.draft.device_settings.screen_brightness"
        type="range"
        min="0"
        max="100"
        :disabled="disabled('screen_brightness')"
        @change="
          w.hardware({
            type: 'screen_brightness',
            value: w.draft.device_settings.screen_brightness,
          })
        "
      />
    </label>
    <label class="range-label">
      {{ t("ui.ledEffect") }}
      <AppSelect
        v-model="w.draft.device_settings.led_mode"
        :aria-label="t('ui.ledEffect')"
        :options="
          modes.map((mode) => ({
            value: mode.value,
            label: mode.label,
            disabled: mode.needsIndividual && individualModeDisabled,
          }))
        "
        :disabled="disabled('led_color')"
        @change="applyLedSettings"
      />
    </label>
    <label class="range-label">
      {{ t("ui.ledBrightness2") }}
      <strong>{{ w.draft.device_settings.led_brightness }} / 255</strong>
      <input
        v-model.number="w.draft.device_settings.led_brightness"
        type="range"
        min="0"
        max="255"
        :disabled="
          disabled('led_brightness') ||
          w.draft.device_settings.led_mode === 'device_default'
        "
        @change="applyLedSettings"
      />
    </label>
    <p
      v-if="w.draft.device_settings.led_mode === 'device_default'"
      class="helper"
    >
      {{ t("ui.theDefaultEffectIsPlayedBy") }}
    </p>
    <p
      v-else-if="w.draft.device_settings.led_mode === 'individual'"
      class="helper"
    >
      {{ t("ui.editTheColorsOfAll24") }}
    </p>
    <div v-if="usesPrimaryColor" class="color-control">
      <label>
        {{ animated ? t("ui.primaryColor") : t("ui.overallLedColor") }}
        <input
          v-model="w.draft.device_settings.led_color"
          type="color"
          :disabled="disabled('led_color')"
          @change="applyLedSettings"
        />
      </label>
      <span class="mono">{{ w.draft.device_settings.led_color }}</span>
    </div>
    <label v-if="animated" class="range-label">
      {{ t("ui.effectSpeed") }}
      <strong>{{ w.draft.device_settings.led_speed }} / 10</strong>
      <input
        v-model.number="w.draft.device_settings.led_speed"
        type="range"
        min="1"
        max="10"
        step="1"
        :disabled="disabled('individual_leds')"
        @change="applyLedSettings"
      />
    </label>
    <div class="button-row wrap">
      <button
        v-if="w.draft.device_settings.led_mode !== 'device_default'"
        class="secondary"
        :disabled="disabled('led_color')"
        @click="setLedMode('device_default')"
      >
        {{ t("ui.restoreDeviceDefaultEffect") }}
      </button>
      <button
        v-else
        class="secondary"
        :disabled="disabled('led_color')"
        @click="setLedMode('solid')"
      >
        {{ t("ui.useSelectedStaticColor") }}
      </button>
    </div>
  </section>
</template>
