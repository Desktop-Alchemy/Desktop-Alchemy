<script setup lang="ts">
import { t } from "../i18n";

import { computed, onMounted, onBeforeUnmount, ref } from "vue";
import type { RegisteredApplication } from "../bindings";
import { targetLabel } from "../launch";
import { useDesktopAlchemy } from "../store";
import AppDialog from "./AppDialog.vue";
import RegisteredApplicationIcon from "./RegisteredApplicationIcon.vue";
const emit = defineEmits<{
  select: [application: RegisteredApplication];
  close: [];
}>();
const w = useDesktopAlchemy();
const search = ref("");
const apps = ref<RegisteredApplication[]>([]);
const loading = ref(false);
const error = ref("");
let revision = 0;
const filtered = computed(() => {
  const term = search.value.trim().toLocaleLowerCase();
  return apps.value.filter((app) =>
    `${app.name} ${targetLabel(app.target)}`.toLocaleLowerCase().includes(term),
  );
});
async function load() {
  const ticket = ++revision;
  loading.value = true;
  error.value = "";
  const result = await w.registeredApplications();
  if (ticket !== revision) return;
  if (result) apps.value = result;
  else error.value = w.error || t("ui.couldNotReadRegisteredApps");
  loading.value = false;
}
onMounted(load);
onBeforeUnmount(() => revision++);
</script>
<template>
  <AppDialog :title="t('ui.chooseRegisteredApp2')" @close="emit('close')"
    ><label
      >{{ t("ui.searchRegisteredApps")
      }}<input
        v-model="search"
        type="search"
        :placeholder="t('ui.searchAppNames')"
    /></label>
    <p v-if="loading" role="status">{{ t("ui.readingRegisteredApps") }}</p>
    <div v-else-if="error">
      <p role="alert" class="error-text">{{ error }}</p>
      <button @click="load">{{ t("ui.retry") }}</button>
    </div>
    <template v-else
      ><p class="helper">
        {{ t("phrases.appCount", { count: filtered.length }) }}
      </p>
      <p v-if="!filtered.length" role="status">
        {{ t("ui.noMatchingRegisteredApps") }}
      </p>
      <div class="windows-app-list">
        <button
          v-for="app in filtered"
          :key="app.id"
          :aria-label="t('ui.chooseRegisteredApp3', { arg0: app.name })"
          @click="emit('select', app)"
        >
          <RegisteredApplicationIcon :application="app" /><strong>{{
            app.name
          }}</strong>
        </button>
      </div></template
    >
    <div class="button-row dialog-actions">
      <button data-cancel @click="emit('close')">{{ t("ui.cancel") }}</button>
    </div></AppDialog
  >
</template>
