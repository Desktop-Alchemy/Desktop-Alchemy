<script setup lang="ts">
import UiFailure from "../../components/UiFailure.vue";
import { t } from "../../i18n";

import { computed, ref, watch, onBeforeUnmount } from "vue";
import { useDeviceContext } from "../context";
import type {
  ControlBinding,
  KeyAction,
  LaunchTarget,
  RegisteredApplication,
} from "../../bindings";
import { programTarget, targetLabel } from "../../launch";
import LaunchTargetEditor from "../../components/LaunchTargetEditor.vue";
import ApplicationsBrowser from "../../components/ApplicationsBrowser.vue";
import AppDialog from "../../components/AppDialog.vue";
import AppSwitch from "../../components/AppSwitch.vue";
import AppIcon from "../../components/AppIcon.vue";
import RegisteredApplicationPicker from "../../components/RegisteredApplicationPicker.vue";
import LaunchTargetIcon from "../../components/LaunchTargetIcon.vue";
const w = useDeviceContext();
const props = defineProps<{ active: boolean }>();
const presets = ref(false);
const registeredPicker = ref(false);
const editing = ref(false);
const resolvedName = ref("");
const loadingName = ref(false);
let applicationsPromise:
  | Promise<RegisteredApplication[] | undefined>
  | undefined;
const applicationNames = new Map<string, string>();
let nameGeneration = 0;
const binding = computed<ControlBinding>(
  () =>
    w.draft.bindings.find((b) => b.control_id === "1") ?? {
      control_id: "1",
      trigger: { type: "press" },
      enabled: false,
      action: { type: "none" },
    },
);
function update(fields: Partial<ControlBinding>) {
  const index = w.draft.bindings.findIndex((b) => b.control_id === "1");
  if (index < 0) w.draft.bindings.push({ ...binding.value, ...fields });
  else w.draft.bindings[index] = { ...binding.value, ...fields };
  w.edit();
}
function target(value: LaunchTarget) {
  editing.value = true;
  update({ action: { type: "launch", target: value } as KeyAction });
}
function selectRegistered(app: RegisteredApplication) {
  resolvedName.value = app.name;
  registeredPicker.value = false;
  target(app.target);
}
const configured = computed(
  () =>
    binding.value.action.type === "launch" &&
    !!targetLabel(binding.value.action.target),
);
const launchTarget = computed(() =>
  binding.value.action.type === "launch" ? binding.value.action.target : null,
);
const targetKey = computed(() =>
  launchTarget.value ? targetLabel(launchTarget.value) : "",
);
const targetName = computed(() => {
  const target = launchTarget.value;
  if (!target) return "";
  if (target.type === "uri") return target.uri;
  if (target.type === "program")
    return target.path.split(/[\\/]/).pop() || t("ui.programShortcut");
  return (
    resolvedName.value ||
    w.applications.items.find((a) => targetLabel(a.target) === targetKey.value)
      ?.name ||
    (loadingName.value
      ? t("ui.readingAppName")
      : t("ui.windowsAppNameUnavailable"))
  );
});
const inputStatus = computed(() => {
  if (w.state.pressed.includes(1)) return t("ui.teamsKeyPressed");
  const at = w.state.last_key_press;
  if (typeof at !== "number") return t("ui.waitingForKeyPress");
  const time = new Date(at).toLocaleTimeString(undefined, {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });
  return t("ui.lastPressed", { arg0: time });
});
watch(
  [() => props.active, targetKey, () => w.currentProfile?.device_id],
  async () => {
    const ticket = ++nameGeneration;
    resolvedName.value = "";
    loadingName.value = false;
    if (!props.active || launchTarget.value?.type !== "registered_application")
      return;
    const key = targetKey.value;
    if (applicationNames.has(key)) {
      resolvedName.value = applicationNames.get(key)!;
      return;
    }
    const library = w.applications.items.find(
      (a) => targetLabel(a.target) === key,
    );
    if (library) {
      resolvedName.value = library.name;
      return;
    }
    loadingName.value = true;
    applicationsPromise ??= w.registeredApplications(true);
    const apps = await applicationsPromise;
    if (apps)
      for (const app of apps)
        applicationNames.set(targetLabel(app.target), app.name);
    else applicationsPromise = undefined;
    if (ticket === nameGeneration && key === targetKey.value) {
      resolvedName.value = applicationNames.get(key) ?? "";
      loadingName.value = false;
    }
  },
  { immediate: true },
);
watch(
  () => w.currentProfile?.device_id,
  () => {
    editing.value = false;
    presets.value = false;
  },
);
onBeforeUnmount(() => nameGeneration++);
</script>
<template>
  <div class="dock-teams">
    <div
      v-if="w.state.connection === 'busy'"
      class="dock-feedback"
      role="status"
    >
      <AppIcon name="warning" :size="18" />{{
        t("ui.anotherAppIsUsingThisDevice")
      }}
    </div>
    <div v-else-if="!w.ready" class="dock-feedback" role="status">
      <AppIcon name="warning" :size="18" />{{
        t("ui.deviceOfflineYouCanStillEdit")
      }}
    </div>
    <UiFailure
      v-if="w.state.error"
      :detail="w.state.error"
      :message="w.state.ui_message"
    />
    <section class="panel dock-settings-group">
      <div class="dock-setting">
        <span class="dock-setting-icon"
          ><AppIcon name="keyboard" :size="22"
        /></span>
        <div class="dock-setting-copy">
          <h2>{{ t("ui.customizeTeamsKey") }}</h2>
          <p>{{ t("ui.whenEnabledPressingTheTeamsKey") }}</p>
        </div>
        <AppSwitch
          :model-value="binding.enabled"
          :label="t('ui.enableTeamsKeyMapping')"
          hide-label
          @update:model-value="update({ enabled: $event })"
        />
      </div>
      <div class="dock-mapping-body">
        <div class="dock-section-heading">
          <h3>{{ t("ui.onPress") }}</h3>
          <div v-if="editing" class="dock-target-choices">
            <button :disabled="!w.desktop" @click="registeredPicker = true">
              <AppIcon name="application" :size="16" />{{
                t("ui.chooseRegisteredApp2")
              }}
            </button>
            <button :disabled="!w.desktop" @click="presets = true">
              <AppIcon name="library" :size="16" />{{
                t("ui.chooseFromAppPresets")
              }}
            </button>
          </div>
        </div>
        <div v-if="binding.action.type !== 'launch'" class="dock-mapping-empty">
          <p>{{ t("ui.chooseAnAppOrUrlFor") }}</p>
          <button :disabled="!w.desktop" @click="target(programTarget())">
            {{ t("ui.setAppOrUrl") }}
          </button>
        </div>
        <template v-else>
          <div v-if="configured && !editing" class="dock-target-summary">
            <LaunchTargetIcon :target="launchTarget!" :name="targetName" />
            <div>
              <strong>{{ targetName }}</strong>
              <p>
                {{
                  launchTarget?.type === "uri"
                    ? t("ui.openUrlOrProtocol")
                    : launchTarget?.type === "program"
                      ? t("ui.launchProgram")
                      : t("ui.launchWindowsApp")
                }}
              </p>
            </div>
            <button @click="editing = true">{{ t("ui.changeTarget") }}</button>
          </div>
          <LaunchTargetEditor
            v-else
            :model-value="binding.action.target"
            :display-name="targetName"
            compact
            hide-registered-picker
            :show-test="false"
            @update:model-value="target"
            @select="resolvedName = $event.name"
          />
          <div class="button-row dock-target-actions">
            <button
              v-if="configured"
              :disabled="!w.desktop || w.pending > 0"
              @click="w.testLaunchTarget(binding.action.target)"
            >
              {{ t("ui.testLaunch") }}</button
            ><button v-if="editing" class="primary" @click="editing = false">
              {{ t("ui.finishEditing") }}</button
            ><button
              class="quiet"
              @click="
                update({ action: { type: 'none' } });
                editing = false;
              "
            >
              {{ t("ui.clearAction") }}
            </button>
          </div></template
        >
        <p v-if="binding.enabled && !configured" class="helper">
          {{ t("ui.chooseATargetBeforeTheKey") }}
        </p>
      </div>
      <div class="dock-mapping-footer">
        <span
          class="save-status"
          :class="{ 'error-text': w.saveError }"
          role="status"
          >{{ w.saveStatus }}</span
        ><button v-if="w.saveError" :disabled="w.saving" @click="w.flushSave">
          {{ t("ui.retrySave") }}</button
        ><span class="helper" role="status">{{ inputStatus }}</span>
      </div>
    </section>
    <p class="dock-footnote">
      {{ t("ui.mappingsSaveAutomaticallyAndWorkWhile") }}
    </p>
    <AppDialog
      v-if="presets"
      :title="t('ui.chooseAppPreset')"
      @close="presets = false"
      ><ApplicationsBrowser
        pick
        @select="
          w.useApplication($event);
          presets = false;
        "
    /></AppDialog>
    <RegisteredApplicationPicker
      v-if="registeredPicker"
      @select="selectRegistered"
      @close="registeredPicker = false"
    />
  </div>
</template>
