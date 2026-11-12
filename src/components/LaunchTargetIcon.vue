<script setup lang="ts">
import { t } from "../i18n";

import { computed, onBeforeUnmount, ref, watch } from "vue";
import type { LaunchTarget } from "../bindings";
import { targetLabel } from "../launch";
import { useDesktopAlchemy } from "../store";
import AssetThumbnail from "./AssetThumbnail.vue";
import AppIcon from "./AppIcon.vue";

const props = defineProps<{ target: LaunchTarget; name: string }>();
const w = useDesktopAlchemy();
const iconAsset = ref<string | null>(null);
const libraryAsset = computed(
  () =>
    w.applications.items.find(
      (item) => targetLabel(item.target) === targetLabel(props.target),
    )?.icon_asset_id ?? null,
);
let revision = 0;
watch(
  () => JSON.stringify(props.target),
  async () => {
    const ticket = ++revision;
    iconAsset.value = null;
    if (props.target.type !== "registered_application") return;
    const icon = await w.registeredApplicationIcon(props.target);
    if (ticket === revision && icon) iconAsset.value = icon;
  },
  { immediate: true },
);
onBeforeUnmount(() => revision++);
const asset = computed(() => libraryAsset.value || iconAsset.value);
</script>

<template>
  <span class="launch-target-icon" aria-hidden="true">
    <AssetThumbnail
      v-if="asset"
      :asset-id="asset"
      :name="t('ui.icon', { arg0: name })"
    />
    <AppIcon
      v-else
      :name="target.type === 'uri' ? 'export' : 'application'"
      :size="22"
    />
  </span>
</template>
