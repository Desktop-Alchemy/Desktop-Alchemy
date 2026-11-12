<script setup lang="ts">
import { t } from "../i18n";

import { computed, onBeforeUnmount, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import type { LaunchTarget, RegisteredApplication } from "../bindings";
import { programTarget, targetLabel } from "../launch";
import { useDesktopAlchemy } from "../store";
import AppSelect from "./AppSelect.vue";
import AppDialog from "./AppDialog.vue";
import RegisteredApplicationPicker from "./RegisteredApplicationPicker.vue";
import LaunchTargetIcon from "./LaunchTargetIcon.vue";
const props = defineProps<{
  modelValue: LaunchTarget;
  compact?: boolean;
  displayName?: string;
  hideRegisteredPicker?: boolean;
  showTest?: boolean;
}>();
const emit = defineEmits<{
  "update:modelValue": [target: LaunchTarget];
  select: [application: RegisteredApplication];
}>();
const w = useDesktopAlchemy();
const picking = ref(false);
const shortcut = ref<string | null>(null);
const shortcutError = ref("");
const selectedApplication = ref<{ key: string; name: string } | null>(null);
const registeredName = computed(
  () =>
    props.displayName ||
    (selectedApplication.value?.key === targetLabel(props.modelValue)
      ? selectedApplication.value.name
      : undefined) ||
    w.applications.items.find(
      (a) => targetLabel(a.target) === targetLabel(props.modelValue),
    )?.name ||
    (targetLabel(props.modelValue)
      ? t("ui.selectedWindowsApp")
      : t("ui.noAppSelected")),
);
let live = true;
onBeforeUnmount(() => (live = false));
const kind = computed({
  get: () => props.modelValue.type,
  set: (type: string) => {
    picking.value = false;
    emit(
      "update:modelValue",
      type === "uri"
        ? { type: "uri", uri: "" }
        : type === "registered_application"
          ? {
              type: "registered_application",
              identity: { type: "apps_folder", parsing_name: "" },
              arguments: "",
            }
          : programTarget(),
    );
  },
});
function patch(fields: Record<string, unknown>) {
  emit("update:modelValue", { ...props.modelValue, ...fields } as LaunchTarget);
}
function select(app: RegisteredApplication) {
  selectedApplication.value = { key: targetLabel(app.target), name: app.name };
  emit("update:modelValue", JSON.parse(JSON.stringify(app.target)));
  emit("select", app);
  picking.value = false;
}
async function chooseProgram() {
  const original = props.modelValue;
  const path = await w.call<string | null>("choose_program");
  if (!live || props.modelValue !== original || !path) return;
  if (path.toLowerCase().endsWith(".lnk")) {
    shortcut.value = path;
    shortcutError.value = "";
  } else
    emit("update:modelValue", { ...programTarget(), path } as LaunchTarget);
}
async function chooseShortcut(original: boolean) {
  const path = shortcut.value;
  if (!path) return;
  const target = props.modelValue;
  const resolved = original ? await w.resolveShortcut(path) : undefined;
  if (!live || shortcut.value !== path || props.modelValue !== target) return;
  if (original && !resolved) {
    shortcutError.value = w.error || t("ui.couldNotReadShortcut");
    return;
  }
  emit(
    "update:modelValue",
    resolved
      ? { type: "program", ...resolved }
      : { type: "program", path, arguments: "", working_directory: null },
  );
  shortcut.value = null;
}
async function directory() {
  const target = props.modelValue;
  const path = await open({ directory: true, multiple: false });
  if (live && props.modelValue === target && typeof path === "string")
    patch({ working_directory: path });
}
</script>
<template>
  <div class="launch-target-editor">
    <label
      >{{ t("ui.launchType")
      }}<AppSelect
        v-model="kind"
        :aria-label="t('ui.launchType')"
        :options="[
          { value: 'program', label: t('ui.programShortcut') },
          { value: 'registered_application', label: t('ui.registeredApp') },
          { value: 'uri', label: t('ui.urlCustomProtocol') },
        ]" /></label
    ><template v-if="modelValue.type === 'program'"
      ><label
        >{{ t("ui.programShortcut")
        }}<input
          :value="modelValue.path"
          :placeholder="t('ui.fullPathToAnExeOr')"
          @input="
            patch({ path: ($event.target as HTMLInputElement).value })
          " /></label
      ><button type="button" :disabled="!w.desktop" @click="chooseProgram">
        {{ t("ui.chooseProgram") }}
      </button></template
    ><template v-else-if="modelValue.type === 'registered_application'"
      ><div class="launch-target-selected">
        <LaunchTargetIcon :target="modelValue" :name="registeredName" />
        <strong>{{ registeredName }}</strong>
      </div>
      <button
        v-if="!hideRegisteredPicker"
        type="button"
        :disabled="!w.desktop"
        @click="picking = true"
      >
        {{ t("ui.chooseRegisteredApp") }}
      </button></template
    ><label v-else
      >{{ t("ui.urlProtocolAddress")
      }}<input
        :value="modelValue.uri"
        :placeholder="t('ui.httpsExampleComOrMsteams')"
        @input="patch({ uri: ($event.target as HTMLInputElement).value })"
      />
    </label>
    <component
      v-if="modelValue.type !== 'uri'"
      :is="compact ? 'details' : 'div'"
      class="launch-options"
    >
      <summary v-if="compact">
        {{ t("ui.advancedOptions")
        }}<span
          v-if="
            modelValue.arguments ||
            (modelValue.type === 'program' && modelValue.working_directory)
          "
        >
          {{ t("ui.configured") }}</span
        >
      </summary>
      <label
        >{{ t("ui.launchArguments")
        }}<textarea
          :value="modelValue.arguments"
          rows="2"
          :placeholder="t('ui.optionalEncloseArgumentsContainingSpacesIn')"
          @input="
            patch({ arguments: ($event.target as HTMLTextAreaElement).value })
          "
        />
      </label>
      <template v-if="modelValue.type === 'program'"
        ><label
          >{{ t("ui.workingDirectory")
          }}<input
            :value="modelValue.working_directory ?? ''"
            :placeholder="t('ui.leaveEmptyToUseTheDefault')"
            @input="
              patch({
                working_directory:
                  ($event.target as HTMLInputElement).value || null,
              })
            " /></label
        ><button type="button" :disabled="!w.desktop" @click="directory">
          {{ t("ui.chooseFolder") }}
        </button></template
      >
      <p v-else class="helper">
        {{ t("ui.someAppsMayIgnoreLaunchArguments") }}
      </p>
    </component>
    <button
      v-if="showTest !== false"
      type="button"
      :disabled="!w.desktop || w.pending > 0 || !targetLabel(modelValue)"
      @click="w.testLaunchTarget(modelValue)"
    >
      {{ t("ui.testLaunch") }}
    </button>
  </div>
  <RegisteredApplicationPicker
    v-if="picking"
    @select="select"
    @close="picking = false"
  /><AppDialog
    v-if="shortcut"
    :title="t('ui.chooseHowToAddTheShortcut')"
    @close="shortcut = null"
    ><p>{{ t("ui.addTheShortcutTargetOrThe") }}</p>
    <p class="application-path">{{ shortcut }}</p>
    <p v-if="shortcutError" class="error-text" role="alert">
      {{ shortcutError }}
    </p>
    <div class="button-row dialog-actions">
      <button data-cancel @click="shortcut = null">{{ t("ui.cancel") }}</button
      ><button :disabled="w.pending > 0" @click="chooseShortcut(false)">
        {{ t("ui.addShortcut") }}</button
      ><button :disabled="w.pending > 0" @click="chooseShortcut(true)">
        {{ t("ui.addTargetFile") }}
      </button>
    </div></AppDialog
  >
</template>
