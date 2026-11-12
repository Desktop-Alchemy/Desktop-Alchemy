<script setup lang="ts">
import { t } from "../i18n";

import { computed, onMounted, ref } from "vue";
import type { ApplicationEntry, RegisteredApplication } from "../bindings";
import { useDesktopAlchemy } from "../store";
import { programTarget, targetLabel } from "../launch";
import AppDialog from "./AppDialog.vue";
import AssetThumbnail from "./AssetThumbnail.vue";
import AppIcon from "./AppIcon.vue";
import LaunchTargetEditor from "./LaunchTargetEditor.vue";
import RegisteredApplicationPicker from "./RegisteredApplicationPicker.vue";
defineProps<{ pick?: boolean }>();
const emit = defineEmits<{ select: [application: ApplicationEntry] }>();
const w = useDesktopAlchemy();
const search = ref("");
const editing = ref<
  (Omit<ApplicationEntry, "id"> & { id: string | null }) | null
>(null);
const removing = ref<ApplicationEntry | null>(null);
let deleteBase: string | undefined;
function beginRemove(app: ApplicationEntry) {
  deleteBase = w.applications.revision;
  removing.value = app;
}
const registered = ref(false);
const items = computed(() =>
  w.applications.items.filter((item) =>
    `${item.name} ${targetLabel(item.target)}`
      .toLocaleLowerCase()
      .includes(search.value.trim().toLocaleLowerCase()),
  ),
);
let editBase: string | undefined;
function add() {
  editBase = w.applications.revision;
  w.error = "";
  editing.value = {
    id: null,
    name: "",
    target: programTarget(),
    icon_asset_id: null,
  };
}
function selectRegistered(app: RegisteredApplication) {
  editBase = w.applications.revision;
  registered.value = false;
  w.error = "";
  let name = Array.from(app.name).slice(0, 100).join("");
  let index = 2;
  while (w.applications.items.some((item) => item.name === name))
    name = `${app.name} (${index++})`;
  editing.value = {
    id: null,
    name,
    target: JSON.parse(JSON.stringify(app.target)),
    icon_asset_id: app.icon_asset_id,
  };
}
function edit(app: ApplicationEntry) {
  editBase = w.applications.revision;
  w.error = "";
  editing.value = JSON.parse(JSON.stringify(app));
}
async function save() {
  const editor = editing.value;
  if (!editor) return;
  const result = await w.editApplications(
    {
      type: "save",
      id: editor.id,
      name: editor.name.trim(),
      target: editor.target,
    },
    editBase,
  );
  if (result && editing.value === editor) editing.value = null;
}
async function remove() {
  const item = removing.value;
  if (
    item &&
    (await w.editApplications({ type: "delete", id: item.id }, deleteBase))
  )
    removing.value = null;
}
onMounted(() => void w.loadApplications());
</script>
<template>
  <section class="applications-browser" :aria-label="t('ui.appPresets')">
    <div class="applications-toolbar">
      <label class="application-search"
        >{{ t("ui.searchApps")
        }}<input
          v-model="search"
          type="search"
          :placeholder="t('ui.nameOrLaunchTarget')" /></label
      ><button
        class="primary"
        :disabled="!w.desktop || !!w.applicationsError || w.applicationsLoading"
        @click="add"
      >
        {{ t("ui.addApp") }}</button
      ><button
        v-if="!pick"
        :disabled="!w.desktop || !!w.applicationsError || w.applicationsLoading"
        @click="registered = true"
      >
        {{ t("ui.addRegisteredApp") }}
      </button>
    </div>
    <p class="helper applications-helper">
      {{
        pick
          ? t("ui.chooseAnAppToAssignIt")
          : t("ui.manageProgramsRegisteredAppsAndUrls")
      }}
    </p>
    <div v-if="w.applicationsError" class="notice">
      <p role="alert">{{ w.applicationsError }}</p>
      <button @click="w.loadApplications">{{ t("ui.retry") }}</button>
    </div>
    <p v-else-if="w.applicationsLoading" role="status">
      {{ t("ui.loadingApps") }}
    </p>
    <div v-else-if="!items.length" class="panel empty-state">
      <h2>{{ search ? t("ui.noMatchingApps") : t("ui.noAppsAddedYet") }}</h2>
    </div>
    <div v-else class="applications-grid">
      <article v-for="app in items" :key="app.id" class="application-card">
        <div class="application-icon-row">
          <AssetThumbnail
            v-if="app.icon_asset_id"
            :asset-id="app.icon_asset_id"
            :name="t('ui.icon', { arg0: app.name })"
          /><span v-else class="application-icon-placeholder"
            ><AppIcon name="application" :size="28"
          /></span>
        </div>
        <div class="application-heading">
          <h2>{{ app.name }}</h2>
          <span class="badge">{{
            app.target.type === "uri"
              ? t("ui.urlProtocol")
              : app.target.type === "registered_application"
                ? t("ui.registeredApp")
                : t("ui.programShortcut")
          }}</span>
        </div>
        <p class="application-path" :title="targetLabel(app.target)">
          {{ targetLabel(app.target) }}
        </p>
        <p
          v-if="app.target.type !== 'uri' && app.target.arguments"
          class="application-detail"
        >
          {{ t("ui.arguments") }}{{ app.target.arguments }}
        </p>
        <div class="button-row">
          <button
            v-if="pick"
            class="primary"
            :aria-label="t('ui.useApp', { arg0: app.name })"
            @click="emit('select', app)"
          >
            {{ t("ui.use") }}</button
          ><template v-else
            ><button
              :aria-label="t('ui.editApp', { arg0: app.name })"
              @click="edit(app)"
            >
              {{ t("ui.edit") }}</button
            ><button
              :disabled="w.pending > 0"
              :aria-label="t('ui.testApp', { arg0: app.name })"
              @click="w.testLaunchTarget(app.target)"
            >
              {{ t("ui.testLaunch") }}</button
            ><button
              v-if="app.target.type !== 'uri'"
              :disabled="w.pending > 0"
              @click="w.editApplications({ type: 'refresh_icon', id: app.id })"
            >
              {{ t("ui.refreshIcon") }}</button
            ><button class="quiet danger" @click="beginRemove(app)">
              {{ t("ui.remove") }}
            </button></template
          >
        </div>
      </article>
    </div>
    <AppDialog
      v-if="editing"
      :title="editing.id ? t('ui.editApp2') : t('ui.addApp')"
      @close="editing = null"
      ><form @submit.prevent="save">
        <label
          >{{ t("ui.appName")
          }}<input v-model="editing.name" required maxlength="120" /></label
        ><LaunchTargetEditor v-model="editing.target" />
        <p v-if="w.error" class="error-text" role="alert">{{ w.error }}</p>
        <div class="button-row dialog-actions">
          <button type="button" data-cancel @click="editing = null">
            {{ t("ui.cancel") }}</button
          ><button
            class="primary"
            :disabled="!editing.name.trim() || w.pending > 0"
          >
            {{ t("ui.saveApp") }}
          </button>
        </div>
      </form></AppDialog
    ><RegisteredApplicationPicker
      v-if="registered"
      @select="selectRegistered"
      @close="registered = false"
    /><AppDialog
      v-if="removing"
      :title="t('ui.removeApp')"
      @close="removing = null"
      ><p>{{ t("phrases.removeApp", { name: removing.name }) }}</p>
      <p v-if="w.error" class="error-text" role="alert">{{ w.error }}</p>
      <div class="button-row dialog-actions">
        <button data-cancel @click="removing = null">
          {{ t("ui.cancel") }}</button
        ><button class="primary" :disabled="w.pending > 0" @click="remove">
          {{ t("ui.remove") }}
        </button>
      </div></AppDialog
    >
  </section>
</template>
