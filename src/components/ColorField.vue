<script setup lang="ts">
import { t } from "../i18n";

import { ref, watch } from "vue";
const props = defineProps<{ modelValue: string; label: string }>();
const emit = defineEmits<{
  "update:modelValue": [value: string];
  change: [];
}>();
const text = ref(props.modelValue);
const invalid = ref(false);
const palette = [
  "#252936",
  "#000000",
  "#FFFFFF",
  "#F7F3EC",
  "#5966F3",
  "#2F6B5F",
  "#C48B45",
  "#B75B4B",
];
watch(
  () => props.modelValue,
  (value) => {
    text.value = value;
    invalid.value = false;
  },
);
function choose(value: string) {
  text.value = value.toUpperCase();
  invalid.value = false;
  emit("update:modelValue", text.value);
  emit("change");
}
function commit() {
  const value = text.value.trim().replace(/^#/, "");
  invalid.value = !/^[0-9a-f]{6}$/i.test(value);
  if (!invalid.value) choose(`#${value}`);
}
</script>
<template>
  <fieldset class="color-field">
    <legend>{{ label }}</legend>
    <div class="color-field-inputs">
      <input
        type="color"
        :value="modelValue"
        :aria-label="label"
        @input="choose(($event.target as HTMLInputElement).value)"
      />
      <input
        v-model="text"
        class="color-hex"
        maxlength="7"
        spellcheck="false"
        :aria-label="`${label} HEX`"
        :aria-invalid="invalid"
        @change="commit"
        @blur="commit"
        @keydown.enter.prevent="commit"
      />
    </div>
    <div
      class="color-palette"
      :aria-label="t('ui.colorPresets', { arg0: label })"
    >
      <button
        v-for="color in palette"
        :key="color"
        type="button"
        :style="{ backgroundColor: color }"
        :aria-label="`${label} ${color}`"
        :aria-pressed="modelValue.toUpperCase() === color"
        :title="color"
        @click="choose(color)"
      />
    </div>
    <small v-if="invalid" class="error-text" role="alert">{{
      t("ui.enterASixDigitHexColor")
    }}</small>
  </fieldset>
</template>
