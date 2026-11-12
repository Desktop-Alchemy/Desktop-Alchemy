<script setup lang="ts">
import { t } from "../../i18n";

import { computed, ref, watch } from "vue";
import { useDeviceContext } from "../context";
import { statusLabels } from "../../labels";
import { featureLabels } from "./labels";
import type { Verification } from "../../bindings";
const w = useDeviceContext();
const note = ref("");
const feature = ref("");
const capabilities = computed(() =>
  Object.entries(w.device?.capabilities ?? {}).flatMap(([id, cap]) =>
    cap ? [{ id, cap }] : [],
  ),
);
watch(
  capabilities,
  (entries) => {
    if (!entries.some(({ id }) => id === feature.value))
      feature.value = entries[0]?.id ?? "";
  },
  { immediate: true },
);
async function verify(status: Verification) {
  await w.call(
    "verify_capability",
    { feature: feature.value, status, note: note.value },
    t("ui.observationRecorded"),
  );
}
</script>
<template>
  <section class="panel">
    <div class="panel-heading">
      <h2>{{ t("ui.hardwareCapabilityRecords") }}</h2>
    </div>
    <div v-for="{ id, cap } in capabilities" :key="id" class="capability">
      <span>{{ featureLabels[id] ?? id }}</span>
      <span class="badge" :class="cap.verification" :title="cap.reason">{{
        statusLabels[cap.verification]
      }}</span>
      <label
        ><input
          type="checkbox"
          :checked="w.state.config.disabled_capabilities.includes(id)"
          :disabled="!w.ready || w.pending > 0"
          @change="
            w.call('set_capability_enabled', {
              feature: id,
              enabled: !($event.target as HTMLInputElement).checked,
            })
          "
        />{{ t("ui.manuallyDisabled") }}</label
      >
    </div>
    <p v-if="!capabilities.length" class="empty">
      {{ t("ui.connectADeviceToViewIts") }}
    </p>
    <div v-if="capabilities.length" class="verification-form">
      <select v-model="feature" :aria-label="t('ui.verifyCapability')">
        <option v-for="{ id } in capabilities" :key="id" :value="id">
          {{ featureLabels[id] ?? id }}
        </option>
      </select>
      <input
        v-model="note"
        :placeholder="t('ui.enterHardwareObservations')"
        :aria-label="t('ui.hardwareObservations')"
      />
      <button
        :disabled="!w.ready || !note.trim() || w.pending > 0"
        @click="verify('verified')"
      >
        {{ t("ui.hardwareVerified") }}
      </button>
      <button
        :disabled="!w.ready || !note.trim() || w.pending > 0"
        @click="verify('failed')"
      >
        {{ t("ui.failed") }}
      </button>
    </div>
  </section>
</template>
