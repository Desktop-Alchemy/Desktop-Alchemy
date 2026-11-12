<script setup lang="ts">
import { t } from "../i18n";

import type { DayFilter } from "../bindings";
import AppSelect from "./AppSelect.vue";
const props = defineProps<{ modelValue: DayFilter }>();
const emit = defineEmits<{ "update:modelValue": [value: DayFilter] }>();
function kind(value: string) {
  emit(
    "update:modelValue",
    value === "weekdays"
      ? { type: "weekdays", days: [1, 2, 3, 4, 5] }
      : value === "date"
        ? { type: "date", date: new Date().toLocaleDateString("sv-SE") }
        : { type: "every_day" },
  );
}
function toggle(day: number) {
  if (props.modelValue.type !== "weekdays") return;
  const days = props.modelValue.days;
  emit("update:modelValue", {
    type: "weekdays",
    days: days.includes(day)
      ? days.filter((v) => v !== day)
      : [...days, day].sort(),
  });
}
</script>
<template>
  <div class="scene-days">
    <AppSelect
      :aria-label="t('ui.dateRule')"
      :model-value="modelValue.type"
      :options="[
        { value: 'every_day', label: t('ui.everyDay') },
        { value: 'weekdays', label: t('ui.daysOfTheWeek') },
        { value: 'date', label: t('ui.specificDate') },
      ]"
      @update:model-value="kind"
    />
    <label v-if="modelValue.type === 'date'"
      >{{ t("ui.date")
      }}<input
        type="date"
        :value="modelValue.date"
        @input="
          emit('update:modelValue', {
            type: 'date',
            date: ($event.target as HTMLInputElement).value,
          })
        "
    /></label>
    <div
      v-if="modelValue.type === 'weekdays'"
      class="scene-weekdays"
      role="group"
      :aria-label="t('ui.weekdays')"
    >
      <label
        v-for="(name, i) in [
          t('ui.mon'),
          t('ui.tue'),
          t('ui.wed'),
          t('ui.thu'),
          t('ui.fri'),
          t('ui.sat'),
          t('ui.sun'),
        ]"
        :key="i"
        ><input
          type="checkbox"
          :checked="modelValue.days.includes(i + 1)"
          @change="toggle(i + 1)"
        />{{ name }}</label
      >
    </div>
  </div>
</template>
<style scoped>
.scene-weekdays {
  display: flex;
  flex-wrap: wrap;
  gap: 7px;
  margin-top: 12px;
}
.scene-weekdays label {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  margin: 0;
  padding: 7px 9px;
  border: 1px solid var(--border);
  border-radius: 7px;
  cursor: pointer;
}
.scene-weekdays label:has(:checked) {
  color: var(--primary-text);
  border-color: color-mix(in srgb, var(--primary) 40%, var(--border));
  background: var(--primary-soft);
}
</style>
