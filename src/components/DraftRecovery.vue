<script setup lang="ts">
import { ref } from "vue";
import { useDesktopAlchemy } from "../store";
import { t, errorText } from "../i18n";
import AppDialog from "./AppDialog.vue";
const w = useDesktopAlchemy();
const confirming = ref(false);
const busy = ref(false);
const failure = ref("");
async function reload() {
  busy.value = true;
  try {
    if (await w.reloadSavedDrafts()) confirming.value = false;
  } catch (error) {
    failure.value = errorText(error);
  } finally {
    busy.value = false;
  }
}
</script>
<template>
  <div class="notice">
    <button class="secondary" @click="confirming = true">
      {{ t("ui.reloadSavedDrafts") }}
    </button>
  </div>
  <AppDialog
    v-if="confirming"
    :title="t('ui.reloadSavedDrafts')"
    @close="!busy && (confirming = false)"
  >
    <p>{{ t("ui.reloadSavedDraftsDescription") }}</p>
    <p v-if="failure" class="error-text" role="alert">{{ failure }}</p>
    <div class="button-row">
      <button data-cancel :disabled="busy" @click="confirming = false">
        {{ t("ui.cancel") }}
      </button>
      <button class="primary" :disabled="busy" @click="reload">
        {{ t("ui.reloadSavedDrafts") }}
      </button>
    </div>
  </AppDialog>
</template>
