<script setup lang="ts">
import { language, t, errorText } from "../i18n";

import { ref } from "vue";
import { open, save } from "@tauri-apps/plugin-dialog";
import { useDesktopAlchemy } from "../store";
import type { BackupSummary } from "../bindings";
import AppDialog from "./AppDialog.vue";

const w = useDesktopAlchemy();
const choosing = ref(false);
const restarting = ref(false);
const review = ref<{ path: string; summary: BackupSummary } | null>(null);
const filters = [
  {
    get name() {
      return t("ui.desktopAlchemySettingsBackup");
    },
    extensions: ["m18backup"],
  },
];
async function exportSettings() {
  choosing.value = true;
  try {
    const date = new Date();
    const stamp = `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
    const path = await save({
      title: t("ui.exportSettingsBackup"),
      defaultPath: `Desktop-Alchemy-${stamp}.m18backup`,
      filters,
    });
    if (path) await w.exportBackup(path);
  } catch (e) {
    w.error = errorText(e);
  } finally {
    choosing.value = false;
  }
}
async function chooseBackup() {
  choosing.value = true;
  try {
    const path = await open({
      title: t("ui.importSettingsBackup"),
      multiple: false,
      filters,
    });
    if (typeof path !== "string") return;
    const summary = await w.inspectBackup(path);
    if (summary) review.value = { path, summary };
  } catch (e) {
    w.error = errorText(e);
  } finally {
    choosing.value = false;
  }
}
async function restore() {
  if (!review.value || restarting.value) return;
  restarting.value = true;
  if (
    !(await w.importBackup(review.value.path, review.value.summary.fingerprint))
  )
    restarting.value = false;
}
function closeReview() {
  if (!restarting.value && w.pending === 0) review.value = null;
}
</script>

<template>
  <section class="backup-controls" aria-labelledby="backup-heading">
    <h3 id="backup-heading">{{ t("ui.configurationBackup") }}</h3>
    <p class="helper">
      {{ t("ui.backUpImportedImagesImageFolders") }}
    </p>
    <p v-if="w.dirty || w.sceneDirty" class="helper">
      {{ w.saveStatus }}{{ t("ui.pageAndSceneChangesAreSaved") }}
    </p>
    <div class="button-row">
      <button
        :disabled="!w.desktop || w.pending > 0 || choosing || restarting"
        @click="exportSettings"
      >
        {{ t("ui.exportSettings") }}
      </button>
      <button
        :disabled="!w.desktop || w.pending > 0 || choosing || restarting"
        @click="chooseBackup"
      >
        {{ t("ui.importSettings") }}
      </button>
    </div>
    <p v-if="choosing" class="helper" role="status">
      {{ t("ui.processingBackupPleaseWait") }}
    </p>
  </section>
  <AppDialog
    v-if="review"
    :title="t('ui.restoreSettingsBackup')"
    @close="closeReview"
  >
    <dl class="backup-summary">
      <dt>{{ t("ui.backupDate") }}</dt>
      <dd>
        {{
          new Date(review.summary.created_at).toLocaleString(language.locale)
        }}
      </dd>
      <dt>{{ t("ui.appVersion") }}</dt>
      <dd>{{ review.summary.app_version }}</dd>
      <dt>{{ t("ui.pagesAndImages") }}</dt>
      <dd>
        {{
          t("phrases.pageImageCounts", {
            pages: review.summary.pages,
            images: review.summary.images,
          })
        }}
      </dd>
      <dt>{{ t("ui.imageFolders") }}</dt>
      <dd>
        {{
          t("phrases.folderItemCounts", {
            folders: review.summary.folders,
            items: review.summary.library_items,
          })
        }}
      </dd>
      <dt>{{ t("ui.appsAndPlugins") }}</dt>
      <dd>
        {{
          t("phrases.appPluginCounts", {
            apps: review.summary.applications,
            plugins: review.summary.plugins,
          })
        }}
      </dd>
      <dt>{{ t("ui.sceneAutomation") }}</dt>
      <dd>
        {{ t("phrases.sceneCount", { count: review.summary.scenes ?? 0 }) }}
      </dd>
      <dt>{{ t("ui.dataSize") }}</dt>
      <dd>{{ (review.summary.total_bytes / 1024 / 1024).toFixed(1) }} MiB</dd>
    </dl>
    <p>
      {{ t("ui.importReplacesCurrentPagesLibraryAnd") }}
    </p>
    <p v-if="w.dirty || w.sceneDirty" class="error-text">
      {{ t("ui.unsavedPageAndSceneChangesWill") }}
    </p>
    <p class="helper">
      {{ t("ui.appsShortcutsAndPluginBinariesAre") }}
    </p>
    <p v-if="w.error" class="error-text" role="alert">{{ w.error }}</p>
    <p v-if="restarting" role="status">
      {{ t("ui.restartingAndRestoringSettings") }}
    </p>
    <div class="button-row dialog-actions">
      <button
        data-cancel
        :disabled="restarting || w.pending > 0"
        @click="closeReview"
      >
        {{ t("ui.cancel") }}
      </button>
      <button
        class="primary"
        :disabled="restarting || w.pending > 0"
        @click="restore"
      >
        {{ t("ui.importAndRestart") }}
      </button>
    </div>
  </AppDialog>
</template>

<style scoped>
.backup-controls {
  border-top: 1px solid var(--border);
  padding-top: 16px;
  margin-top: 18px;
}
.backup-controls .button-row {
  justify-content: flex-start;
  flex-wrap: wrap;
}
.backup-summary {
  display: grid;
  grid-template-columns: auto minmax(0, 1fr);
  gap: 10px 20px;
  font-size: 13px;
}
.backup-summary dt {
  color: var(--subtle);
}
.backup-summary dd {
  margin: 0;
  overflow-wrap: anywhere;
}
</style>
