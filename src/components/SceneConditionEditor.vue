<script setup lang="ts">
import { t } from "../i18n";

import { computed, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import type { SceneCondition, RunningProgram } from "../bindings";
import { useDesktopAlchemy } from "../store";
import AppSelect from "./AppSelect.vue";
import AppDialog from "./AppDialog.vue";
import AppIcon from "./AppIcon.vue";
import SceneDays from "./SceneDays.vue";
const props = withDefaults(
  defineProps<{ modelValue: SceneCondition; depth?: number; total: number }>(),
  { depth: 1 },
);
const emit = defineEmits<{ "update:modelValue": [value: SceneCondition] }>();
const w = useDesktopAlchemy();
const options = [
  {
    value: "all",
    get label() {
      return t("ui.allMatch");
    },
  },
  {
    value: "any",
    get label() {
      return t("ui.anyMatch");
    },
  },
  {
    value: "not",
    get label() {
      return t("ui.not");
    },
  },
  {
    value: "process",
    get label() {
      return t("ui.processRunning");
    },
  },
  {
    value: "foreground",
    get label() {
      return t("ui.foregroundApp");
    },
  },
  {
    value: "media_playing",
    get label() {
      return t("ui.systemMediaPlayback");
    },
  },
  {
    value: "time_range",
    get label() {
      return t("ui.timeRange");
    },
  },
];
const leaf = (): SceneCondition => ({ type: "media_playing" });
const group = computed(() =>
  props.modelValue.type === "all" || props.modelValue.type === "any"
    ? props.modelValue
    : null,
);
const program = computed(() =>
  props.modelValue.type === "process" || props.modelValue.type === "foreground"
    ? props.modelValue
    : null,
);
const picker = ref(false),
  loading = ref(false),
  loadError = ref("");
const programs = ref<RunningProgram[]>([]),
  search = ref("");
const filtered = computed(() =>
  programs.value.filter((p) =>
    `${p.name} ${p.path ?? ""}`
      .toLowerCase()
      .includes(search.value.toLowerCase()),
  ),
);
function patch(value: Record<string, unknown>) {
  emit("update:modelValue", {
    ...props.modelValue,
    ...value,
  } as SceneCondition);
}
function changeKind(type: string) {
  let c: SceneCondition;
  if (type === "all" || type === "any")
    c = {
      type,
      children: props.depth < 8 && props.total < 128 ? [leaf()] : [],
    };
  else if (type === "not") c = { type, child: leaf() };
  else if (type === "process" || type === "foreground")
    c = { type, matcher: { type: "name", value: "" } };
  else if (type === "time_range")
    c = { type, days: { type: "every_day" }, start: "09:00", end: "18:00" };
  else c = leaf();
  emit("update:modelValue", c);
}
function child(index: number, value?: SceneCondition) {
  if (group.value)
    patch({
      children: group.value.children.flatMap((c, i) =>
        i !== index ? [c] : value ? [value] : [],
      ),
    });
}
function matcher(value: string, type = program.value?.matcher.type ?? "name") {
  patch({ matcher: { type, value } });
}
async function chooseFile() {
  if (!w.desktop) return;
  const path = await open({
    multiple: false,
    filters: [{ name: t("ui.programOrShortcut"), extensions: ["exe", "lnk"] }],
  });
  if (typeof path !== "string") return;
  if (/\.lnk$/i.test(path)) {
    const target = await w.resolveShortcut(path);
    if (target) matcher(target.path, "path");
  } else matcher(path, "path");
}
async function load() {
  loading.value = true;
  loadError.value = "";
  const result = await w.runningPrograms();
  if (picker.value) {
    if (result)
      programs.value = result
        .filter((p) => /\.exe$/i.test(p.name))
        .sort((a, b) => a.name.localeCompare(b.name));
    else loadError.value = t("ui.couldNotReadRunningProgramsTry");
  }
  loading.value = false;
}
function useProgram(p: RunningProgram) {
  matcher(p.path || p.name, p.path ? "path" : "name");
  picker.value = false;
}
</script>
<template>
  <div class="scene-condition">
    <AppSelect
      :aria-label="t('ui.conditionType')"
      :model-value="modelValue.type"
      :options="
        options.map((o) => ({
          ...o,
          disabled: depth >= 8 && ['not', 'any', 'all'].includes(o.value),
        }))
      "
      @update:model-value="changeKind"
    />
    <template v-if="group">
      <p v-if="!group.children.length" class="hint">
        {{
          group.type === "all"
            ? t("ui.noAdditionalConditionsAlwaysMatches")
            : t("ui.addAtLeastOneCondition")
        }}
      </p>
      <div v-for="(c, i) in group.children" :key="i" class="scene-child">
        <SceneConditionEditor
          :model-value="c"
          :depth="depth + 1"
          :total="total"
          @update:model-value="child(i, $event)"
        />
        <button
          class="quiet"
          :aria-label="t('ui.removeCondition', { arg0: i + 1 })"
          @click="child(i)"
        >
          <AppIcon name="close" :size="14" />
        </button>
      </div>
      <button
        :disabled="depth >= 8 || total >= 128"
        @click="patch({ children: [...group.children, leaf()] })"
      >
        <AppIcon name="plus" :size="14" />{{ t("ui.addCondition") }}
      </button>
    </template>
    <SceneConditionEditor
      v-else-if="modelValue.type === 'not'"
      :model-value="modelValue.child"
      :depth="depth + 1"
      :total="total"
      @update:model-value="patch({ child: $event })"
    />
    <template v-else-if="program">
      <AppSelect
        :aria-label="t('ui.matchBy')"
        :model-value="program.matcher.type"
        :options="[
          { value: 'name', label: t('ui.processName') },
          { value: 'path', label: t('ui.fullPath') },
        ]"
        @update:model-value="matcher('', $event)"
      />
      <label
        >{{
          program.matcher.type === "name"
            ? t("ui.processName")
            : t("ui.programPath")
        }}<input
          :value="program.matcher.value"
          :placeholder="
            program.matcher.type === 'name'
              ? t('ui.forExampleObs64Exe')
              : t('ui.chooseProgramFile')
          "
          @input="matcher(($event.target as HTMLInputElement).value)"
      /></label>
      <div class="button-row">
        <button :disabled="!w.desktop" @click="chooseFile">
          {{ t("ui.chooseProgram") }}</button
        ><button
          :disabled="!w.desktop"
          @click="
            picker = true;
            load();
          "
        >
          {{ t("ui.runningPrograms") }}
        </button>
      </div>
    </template>
    <template v-else-if="modelValue.type === 'time_range'">
      <SceneDays
        :model-value="modelValue.days"
        @update:model-value="patch({ days: $event })"
      />
      <div class="scene-time-row">
        <label
          >{{ t("ui.start2")
          }}<input
            type="time"
            :value="modelValue.start"
            @input="
              patch({ start: ($event.target as HTMLInputElement).value })
            " /></label
        ><label
          >{{ t("ui.end")
          }}<input
            type="time"
            :value="modelValue.end"
            @input="patch({ end: ($event.target as HTMLInputElement).value })"
        /></label>
      </div>
      <p v-if="modelValue.start > modelValue.end" class="hint">
        {{ t("ui.crossesMidnightUsesTheStartDate") }}
      </p>
    </template>
    <p v-else class="hint">
      {{ t("ui.usesWindowsMediaSessionsPlayersWithout") }}
    </p>
    <AppDialog
      v-if="picker"
      :title="t('ui.runningPrograms2')"
      @close="picker = false"
    >
      <label
        >{{ t("ui.search")
        }}<input v-model="search" :placeholder="t('ui.programNameOrPath')"
      /></label>
      <p v-if="loading" role="status">{{ t("ui.reading") }}</p>
      <p v-else-if="loadError" class="error-text">{{ loadError }}</p>
      <div v-else class="scene-programs">
        <button v-for="p in filtered" :key="p.pid" @click="useProgram(p)">
          <strong>{{ p.name }}</strong
          ><small>{{ p.path || t("ui.processNameOnly") }}</small>
        </button>
        <p v-if="!filtered.length">{{ t("ui.noMatchingPrograms") }}</p>
      </div>
      <div class="dialog-actions">
        <button data-cancel @click="picker = false">{{ t("ui.cancel") }}</button
        ><button :disabled="loading" @click="load">
          {{ t("ui.refresh") }}
        </button>
      </div>
    </AppDialog>
  </div>
</template>
