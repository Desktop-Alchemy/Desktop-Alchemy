<script setup lang="ts">
import { t } from "../i18n";

import { computed } from "vue";
import { useDeviceContext } from "./context";
import type { ControlBinding, InputTrigger, KeyAction } from "../bindings";
import LaunchTargetEditor from "../components/LaunchTargetEditor.vue";
import { programTarget } from "../launch";
const w = useDeviceContext();
const controls = computed(
  () =>
    w.currentProfile?.definition.inputs.filter(
      (control) =>
        !w.currentProfile?.definition.supports_pages ||
        control.kind.type !== "button" ||
        (!w.currentProfile.definition.displays.some(
          (display) => display.id === control.id,
        ) &&
          !w.currentProfile.layout.input_keys.some(
            (key) => String(key) === control.id,
          )),
    ) ?? [],
);
const bindings = computed(() =>
  w.currentProfile?.definition.supports_pages
    ? w.page.bindings
    : w.draft.bindings,
);
function add(id: string, type: string) {
  let trigger: InputTrigger =
    type === "rotary"
      ? { type: "clockwise" }
      : type === "analog"
        ? { type: "above", threshold: 0.5, hysteresis: 0.02 }
        : { type: "press" };
  bindings.value.push({
    control_id: id,
    enabled: true,
    trigger,
    action: { type: "none" },
  });
  w.edit();
}
function trigger(binding: ControlBinding, value: string) {
  binding.trigger =
    value === "above" || value === "below"
      ? { type: value, threshold: 0.5, hysteresis: 0.02 }
      : ({ type: value } as InputTrigger);
  w.edit();
}
function action(binding: ControlBinding, type: string) {
  binding.action =
    type === "launch"
      ? { type: "launch", target: programTarget() }
      : type === "page"
        ? { type: "page", page_id: w.page.id }
        : ({ type } as KeyAction);
  w.edit();
}
</script>
<template>
  <section v-if="controls.length" class="panel">
    <h2>{{ t("ui.inputActions") }}</h2>
    <fieldset :disabled="!!w.currentProfile?.config_error">
      <div v-for="control in controls" :key="control.id" class="input-control">
        <div class="panel-heading">
          <strong>{{ control.name }}</strong
          ><button @click="add(control.id, control.kind.type)">
            {{ t("ui.addAction") }}
          </button>
        </div>
        <div
          v-for="binding in bindings.filter((b) => b.control_id === control.id)"
          :key="bindings.indexOf(binding)"
          class="binding-row"
        >
          <input
            v-model="binding.enabled"
            type="checkbox"
            :aria-label="t('ui.enableAction')"
            @change="w.edit"
          /><select
            :value="binding.trigger.type"
            :aria-label="t('ui.triggerCondition')"
            @change="
              trigger(binding, ($event.target as HTMLSelectElement).value)
            "
          >
            <option v-if="control.kind.type === 'button'" value="press">
              {{ t("ui.press") }}
            </option>
            <template v-if="control.kind.type === 'rotary'"
              ><option value="clockwise">{{ t("ui.clockwise") }}</option>
              <option value="counterclockwise">
                {{ t("ui.counterclockwise") }}
              </option></template
            ><template v-if="control.kind.type === 'analog'"
              ><option value="above">{{ t("ui.crossAboveThreshold") }}</option>
              <option value="below">
                {{ t("ui.crossBelowThreshold") }}
              </option></template
            ></select
          ><template
            v-if="
              binding.trigger.type === 'above' ||
              binding.trigger.type === 'below'
            "
            ><label
              >{{ t("ui.threshold")
              }}<input
                v-model.number="binding.trigger.threshold"
                type="number"
                min="0.01"
                max="0.99"
                step="0.01"
                @change="w.edit" /></label
            ><label
              >{{ t("ui.hysteresis")
              }}<input
                v-model.number="binding.trigger.hysteresis"
                type="number"
                min="0"
                max="0.49"
                step="0.01"
                @change="w.edit" /></label></template
          ><select
            :value="binding.action.type"
            :aria-label="t('ui.action')"
            @change="
              action(binding, ($event.target as HTMLSelectElement).value)
            "
          >
            <option value="none">{{ t("ui.noAction") }}</option>
            <option value="launch">{{ t("ui.launchAppOrUrl") }}</option>
            <template v-if="w.currentProfile?.definition.supports_pages"
              ><option value="previous">{{ t("ui.previousPage") }}</option>
              <option value="home">{{ t("ui.homePage") }}</option>
              <option value="next">{{ t("ui.nextPage") }}</option>
              <option value="page">{{ t("ui.specificPage") }}</option></template
            ></select
          ><select
            v-if="binding.action.type === 'page'"
            v-model="binding.action.page_id"
            @change="w.edit"
          >
            <option
              v-for="page in w.draft.pages"
              :key="page.id"
              :value="page.id"
            >
              {{ page.name }}
            </option></select
          ><LaunchTargetEditor
            v-if="binding.action.type === 'launch'"
            v-model="binding.action.target"
            @update:model-value="w.edit"
          /><button
            @click="
              bindings.splice(bindings.indexOf(binding), 1);
              w.edit();
            "
          >
            {{ t("ui.delete2") }}
          </button>
        </div>
      </div>
    </fieldset>
  </section>
</template>
<style scoped>
fieldset {
  border: 0;
  padding: 0;
  min-width: 0;
}
.input-control {
  padding: 12px 0;
  border-bottom: 1px solid var(--border);
}
.binding-row {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
  align-items: center;
  padding: 8px 0;
}
.binding-row > select {
  width: auto;
  flex: 1 1 150px;
  margin-top: 0;
}
.binding-row > input[type="checkbox"] {
  flex: 0 0 auto;
}
.binding-row label {
  margin: 0;
}
.binding-row :deep(.launch-target-editor) {
  flex: 1 0 100%;
}
.binding-row input[type="number"] {
  width: 80px;
}
</style>
