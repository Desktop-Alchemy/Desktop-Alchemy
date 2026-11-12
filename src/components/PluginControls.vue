<script setup lang="ts">
import { pluginName } from "../plugin-labels";
import UiFailure from "./UiFailure.vue";
import { t, errorText } from "../i18n";

import { ref, computed, nextTick } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { useDesktopAlchemy } from "../store";
import type { PluginInfo, PluginPackageSummary } from "../bindings";
import { statusLabels } from "../labels";
import AppDialog from "./AppDialog.vue";
import AppIcon from "./AppIcon.vue";
import McpControls from "./McpControls.vue";

const w = useDesktopAlchemy();
const choosing = ref(false);
const review = ref<{
  path: string | null;
  summary: PluginPackageSummary;
} | null>(null);
const uninstall = ref<{ id: string; name: string } | null>(null);
const missing = computed(() =>
  w.state.config.managed_plugin_ids.filter(
    (id) => !w.state.plugins.some((p) => p.manifest.id === id),
  ),
);
const trigger = ref<HTMLElement>();
const busy = () => choosing.value || w.pending > 0;
const disabled = (plugin: PluginInfo) =>
  w.state.config.disabled_plugins.includes(plugin.manifest.id);
function label(plugin: PluginInfo) {
  if (["running", "starting", "stopping", "error"].includes(plugin.status))
    return statusLabels[plugin.status];
  return disabled(plugin) ? t("ui.disabled") : t("ui.enabledNotRunning");
}
async function choose(example = false) {
  if (busy()) return;
  trigger.value = document.activeElement as HTMLElement;
  choosing.value = true;
  try {
    const path = example
      ? null
      : await open({
          title: t("ui.importPluginPackage"),
          multiple: false,
          filters: [
            {
              name: t("ui.desktopAlchemyPluginPackage"),
              extensions: ["m18plugin"],
            },
          ],
        });
    if (!example && typeof path !== "string") return;
    const summary = await w.inspectPluginPackage(path as string | null);
    if (summary) review.value = { path: path as string | null, summary };
  } catch (e) {
    w.error = errorText(e);
  } finally {
    choosing.value = false;
  }
}
async function close() {
  if (busy()) return;
  review.value = null;
  uninstall.value = null;
  await nextTick();
  trigger.value?.focus({ preventScroll: true });
}
async function install() {
  if (!review.value || busy()) return;
  const result = await w.installPluginPackage(
    review.value.path,
    review.value.summary.fingerprint,
  );
  if (result) await close();
}
function askUninstall(id: string, name: string) {
  trigger.value = document.activeElement as HTMLElement;
  uninstall.value = { id, name };
}
async function remove() {
  if (!uninstall.value || busy()) return;
  // call returns null on successful unit replies, undefined on failure.
  const result = await w.removePlugin(uninstall.value.id);
  if (result !== undefined) await close();
}
</script>

<template>
  <div class="page-heading">
    <div>
      <p class="eyebrow">{{ t("ui.localServices") }}</p>
      <h1>{{ t("ui.localExtensions") }}</h1>
    </div>
    <div class="button-row">
      <button
        class="secondary"
        :disabled="!w.desktop || busy()"
        @click="choose(true)"
      >
        {{ t("ui.addBuiltInExample") }}
      </button>
      <button
        class="primary"
        :disabled="!w.desktop || busy()"
        @click="choose()"
      >
        {{ t("ui.importPluginPackage") }}
      </button>
    </div>
  </div>
  <McpControls />
  <div class="notice">{{ t("ui.pluginsRunWithYourUserPermissions") }}</div>
  <p v-if="choosing" role="status" class="helper">
    {{ t("ui.checkingPluginPackage") }}
  </p>
  <section
    v-if="!w.state.plugins.length && !missing.length"
    class="panel empty-state"
  >
    <div class="empty-icon"><AppIcon name="extensions" :size="44" /></div>
    <h2>{{ t("ui.noPluginsAdded") }}</h2>
    <p>{{ t("ui.importAnM18pluginPackageOrAdd") }}</p>
  </section>
  <section
    v-for="plugin in w.state.plugins"
    :key="plugin.manifest.id"
    class="panel plugin-panel"
  >
    <div class="panel-heading">
      <div>
        <h2>
          {{ pluginName(plugin.manifest) }}
          <span class="version">{{ plugin.manifest.version }}</span>
        </h2>
        <p class="helper">
          {{ plugin.built_in ? t("ui.builtInPlugin") : t("ui.installed") }} ·
          {{ plugin.manifest.id }}
        </p>
      </div>
      <span class="badge" :class="plugin.status">{{
        plugin.manifest.executable ? label(plugin) : t("ui.iconPack")
      }}</span>
    </div>
    <p v-if="plugin.manifest.id === 'dev.masterain.counter'" class="helper">
      {{ t("ui.key1CountKey2Next") }}
    </p>
    <div class="permission-list">
      <span
        v-for="permission in plugin.manifest.permissions"
        :key="permission"
        class="tag"
        >{{ permission }}</span
      >
    </div>
    <p v-if="plugin.manifest.icons?.length" class="helper">
      {{ t("phrases.includedIcons", { count: plugin.manifest.icons.length }) }}
    </p>
    <UiFailure
      v-if="plugin.error"
      :detail="plugin.error"
      :message="plugin.ui_message"
    />
    <p v-if="plugin.manifest.widgets.length" class="helper">
      {{ t("ui.runsWhenEnabledAndReferencedBy") }}
    </p>
    <div class="button-row">
      <button
        v-if="plugin.manifest.executable"
        class="primary"
        :disabled="
          busy() ||
          ['starting', 'stopping', 'running'].includes(plugin.status) ||
          (!disabled(plugin) &&
            plugin.manifest.widgets.length > 0 &&
            plugin.status !== 'error')
        "
        @click="w.setPluginEnabled(plugin.manifest.id, true)"
      >
        {{
          plugin.status === "error" && !disabled(plugin)
            ? t("ui.retry")
            : disabled(plugin)
              ? t("ui.enable")
              : plugin.manifest.widgets.length
                ? t("ui.enabled")
                : t("ui.start")
        }}
      </button>
      <button
        v-if="plugin.manifest.executable"
        :disabled="
          busy() ||
          plugin.status === 'stopping' ||
          (disabled(plugin) && plugin.status !== 'error')
        "
        @click="w.setPluginEnabled(plugin.manifest.id, false)"
      >
        {{
          disabled(plugin) && plugin.status === "error"
            ? t("ui.retryDisabling")
            : t("ui.disable")
        }}
      </button>
      <button
        v-if="!plugin.built_in"
        class="quiet danger"
        :disabled="busy() || plugin.status === 'stopping'"
        @click="askUninstall(plugin.manifest.id, pluginName(plugin.manifest))"
      >
        {{ t("ui.uninstall") }}
      </button>
    </div>
  </section>
  <section v-for="id in missing" :key="id" class="panel plugin-panel">
    <div class="panel-heading">
      <h2>{{ id }}</h2>
      <span class="badge error">{{ t("ui.pluginUnavailable") }}</span>
    </div>
    <p class="helper">{{ t("ui.pluginFilesAreMissingOrDamaged") }}</p>
    <button
      class="quiet danger"
      :disabled="busy()"
      @click="askUninstall(id, id)"
    >
      {{ t("ui.uninstall") }}
    </button>
  </section>
  <AppDialog
    v-if="review"
    :title="
      review.summary.installed_version === null
        ? t('ui.installPlugin')
        : t('ui.replacePlugin')
    "
    @close="close"
  >
    <dl class="plugin-summary">
      <dt>{{ t("ui.plugin") }}</dt>
      <dd>{{ pluginName(review.summary.manifest) }}</dd>
      <dt>{{ t("ui.identifier") }}</dt>
      <dd>{{ review.summary.manifest.id }}</dd>
      <dt>{{ t("ui.version") }}</dt>
      <dd>
        {{
          review.summary.installed_version
            ? `${review.summary.installed_version} → `
            : ""
        }}{{ review.summary.manifest.version }}
      </dd>
      <dt>{{ t("ui.permissions") }}</dt>
      <dd>
        {{
          review.summary.manifest.permissions.join(t("app.listSeparator")) ||
          t("ui.none")
        }}
      </dd>
      <dt>{{ t("ui.installedSize") }}</dt>
      <dd>{{ (review.summary.total_bytes / 1024 / 1024).toFixed(1) }} MiB</dd>
    </dl>
    <p>
      {{
        review.summary.installed_version === null
          ? t("ui.disabledByDefaultAfterInstallationEnable")
          : t("ui.replacementStopsTheOldProcessAnd")
      }}
    </p>
    <p v-if="w.error" role="alert" class="error-text">{{ w.error }}</p>
    <div class="button-row dialog-actions">
      <button data-cancel :disabled="busy()" @click="close">
        {{ t("ui.cancel") }}</button
      ><button class="primary" :disabled="busy()" @click="install">
        {{
          w.pending
            ? t("ui.installing")
            : review.summary.installed_version === null
              ? t("ui.install")
              : t("ui.confirmReplacement")
        }}
      </button>
    </div>
  </AppDialog>
  <AppDialog v-if="uninstall" :title="t('ui.uninstallPlugin')" @close="close">
    <p>{{ t("phrases.uninstallPlugin", { name: uninstall.name }) }}</p>
    <p class="helper">{{ t("ui.pageBindingsAreRetainedAndCan") }}</p>
    <p v-if="w.error" role="alert" class="error-text">{{ w.error }}</p>
    <div class="button-row dialog-actions">
      <button data-cancel :disabled="busy()" @click="close">
        {{ t("ui.cancel") }}</button
      ><button class="danger" :disabled="busy()" @click="remove">
        {{ t("ui.confirmUninstall") }}
      </button>
    </div>
  </AppDialog>
</template>

<style scoped>
.plugin-summary {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  gap: 10px 20px;
  font-size: 13px;
}
.plugin-summary dt {
  color: var(--subtle);
}
.plugin-summary dd {
  margin: 0;
  overflow-wrap: anywhere;
}
</style>
