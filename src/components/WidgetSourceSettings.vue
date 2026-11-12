<script setup lang="ts">
import { t, errorText, messageText } from "../i18n";

import { computed, onBeforeUnmount, ref, watch } from "vue";
import { useDesktopAlchemy } from "../store";
import type { WidgetCatalog } from "../bindings";
import AppSelect from "./AppSelect.vue";

const w = useDesktopAlchemy();
const search = ref("");
const error = ref("");
const busy = ref(false);
const catalog = ref<WidgetCatalog>({
  sources: [],
  hardware_status: "",
  message: null,
});
const lease = crypto.randomUUID();
let generation = 0;
let timer: ReturnType<typeof setTimeout> | undefined;
let activePlugin: string | undefined;
const binding = computed(() => w.visual.widget);
const plugin = computed(() =>
  w.state.plugins.find((p) => p.manifest.id === binding.value?.plugin_id),
);
const needsSource = computed(
  () =>
    plugin.value?.manifest.widgets.find(
      (d) => d.id === binding.value?.widget_id,
    )?.select_source ?? false,
);
const disabled = computed(
  () =>
    !!binding.value &&
    w.state.config.disabled_plugins.includes(binding.value.plugin_id),
);
const hardware = computed(
  () =>
    binding.value?.plugin_id === "dev.masterain.performance" &&
    ["temperature", "fan", "power", "voltage"].includes(
      binding.value.widget_id,
    ),
);
const source = computed(() =>
  catalog.value.sources.find(
    (s) =>
      s.widget_id === binding.value?.widget_id &&
      s.id === binding.value?.source_id,
  ),
);
const options = computed(() => {
  const words = search.value
    .toLocaleLowerCase()
    .trim()
    .split(/\s+/)
    .filter(Boolean);
  const choices = catalog.value.sources
    .filter(
      (s) =>
        s.widget_id === binding.value?.widget_id &&
        words.every((word) => s.name.toLocaleLowerCase().includes(word)),
    )
    .map((s) => ({
      value: s.id,
      label: s.name + (s.available ? "" : t("ui.unavailable")),
    }));
  if (
    binding.value?.source_id &&
    !choices.some((s) => s.value === binding.value?.source_id)
  ) {
    choices.unshift({
      value: binding.value.source_id,
      label: `${binding.value.source_name ?? t("ui.selectedSource")}${source.value ? "" : t("ui.disconnected2")}`,
    });
  }
  return [
    {
      value: "",
      get label() {
        return t("ui.chooseADataSource");
      },
    },
    ...choices,
  ];
});
const selection = computed({
  get: () => binding.value?.source_id ?? "",
  set: (id: string) => {
    if (!binding.value) return;
    binding.value.source_id = id || null;
    binding.value.source_name =
      catalog.value.sources.find(
        (s) => s.widget_id === binding.value?.widget_id && s.id === id,
      )?.name ?? null;
    w.edit();
  },
});
const message = computed(() => {
  if (disabled.value) return t("ui.pluginDisabledEnableItInExtensions");
  if (error.value) return error.value;
  if (source.value?.error)
    return source.value.ui_message
      ? messageText(source.value.ui_message)
      : binding.value?.plugin_id === "dev.masterain.performance"
        ? t("phrases.sourceFailed")
        : source.value.error;
  if (hardware.value && catalog.value.message)
    return catalog.value.ui_message
      ? messageText(catalog.value.ui_message)
      : t("phrases.sourceFailed");
  if (binding.value?.source_id && !source.value)
    return t("ui.selectedSourceDisconnectedTheBindingIs");
  if (
    !catalog.value.sources.some((s) => s.widget_id === binding.value?.widget_id)
  )
    return busy.value
      ? t("ui.findingDataSources")
      : t("ui.noAvailableSourcesFound");
  return "";
});
watch(
  () =>
    `${needsSource.value ? binding.value?.plugin_id : ""}/${disabled.value}`,
  () => {
    const n = ++generation;
    clearTimeout(timer);
    if (activePlugin)
      void w.releaseWidgetDiscovery(activePlugin, lease).catch(() => {});
    activePlugin =
      needsSource.value && !disabled.value
        ? binding.value?.plugin_id
        : undefined;
    catalog.value = { sources: [], hardware_status: "", message: null };
    error.value = "";
    const id = activePlugin;
    if (!id) return;
    async function refresh() {
      busy.value = true;
      try {
        const next = await w.discoverWidgetSources(id!, lease);
        if (n !== generation) return;
        catalog.value = next;
        error.value = "";
      } catch (e) {
        if (n === generation) error.value = errorText(e);
      } finally {
        if (n === generation) {
          busy.value = false;
          timer = setTimeout(refresh, 2000);
        }
      }
    }
    void refresh();
  },
  { immediate: true },
);
watch(
  () => binding.value?.widget_id,
  () => {
    search.value = "";
  },
);
async function enable() {
  busy.value = true;
  error.value = "";
  catalog.value.hardware_status = "starting";
  try {
    await w.enableHardwareSensors();
  } catch (e) {
    error.value = errorText(e);
  } finally {
    busy.value = false;
  }
}
onBeforeUnmount(() => {
  generation++;
  clearTimeout(timer);
  if (activePlugin)
    void w.releaseWidgetDiscovery(activePlugin, lease).catch(() => {});
});
</script>
<template>
  <div v-if="needsSource" class="widget-sources">
    <label
      >{{ t("ui.dataSource") }}
      <input
        v-model="search"
        type="search"
        :placeholder="t('ui.searchDevicesOrSensors')"
        :aria-label="t('ui.searchDataSources')"
        :disabled="disabled"
      />
      <AppSelect
        v-model="selection"
        :options="options"
        :disabled="disabled"
        :aria-label="t('ui.dataSource')"
      />
    </label>
    <p v-if="message" class="helper" role="status">{{ message }}</p>
    <details
      v-if="source?.error || (hardware && catalog.message)"
      class="helper"
    >
      <summary>{{ t("errors.details") }}</summary>
      <p>{{ source?.error || catalog.message }}</p>
    </details>
    <button
      v-if="hardware && catalog.hardware_status !== 'ready'"
      class="secondary"
      :disabled="disabled || busy || catalog.hardware_status === 'starting'"
      @click="enable"
    >
      {{
        catalog.hardware_status === "starting"
          ? t("ui.waitingForSystemAuthorization")
          : t("ui.enableHardwareSensors")
      }}
    </button>
    <p
      v-if="
        hardware &&
        ['driver_required', 'permission_required'].includes(
          catalog.hardware_status,
        )
      "
      class="helper"
    >
      {{ t("ui.firstUseMayRequireInstallingA") }}
    </p>
  </div>
</template>
<style scoped>
.widget-sources {
  display: grid;
  gap: 8px;
  min-width: 0;
}
.widget-sources label {
  display: grid;
  gap: 6px;
  min-width: 0;
}
.widget-sources input {
  min-width: 0;
  width: 100%;
}
</style>
