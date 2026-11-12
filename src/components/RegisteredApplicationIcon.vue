<script setup lang="ts">
import { t } from "../i18n";

import { onMounted, onBeforeUnmount, ref } from "vue";
import type { RegisteredApplication } from "../bindings";
import { useDesktopAlchemy } from "../store";
import AssetThumbnail from "./AssetThumbnail.vue";
import AppIcon from "./AppIcon.vue";
const props = defineProps<{ application: RegisteredApplication }>();
const w = useDesktopAlchemy();
const element = ref<HTMLElement>();
const asset = ref(props.application.icon_asset_id);
let live = true;
let observer: IntersectionObserver | undefined;
onMounted(() => {
  observer = new IntersectionObserver((entries) => {
    if (entries.some((entry) => entry.isIntersecting)) {
      observer?.disconnect();
      void w.registeredApplicationIcon(props.application.target).then((id) => {
        if (live && id) asset.value = id;
      });
    }
  });
  if (element.value) observer.observe(element.value);
});
onBeforeUnmount(() => {
  live = false;
  observer?.disconnect();
});
</script>
<template>
  <span ref="element" class="registered-icon"
    ><AssetThumbnail
      v-if="asset"
      :asset-id="asset"
      :name="t('ui.icon', { arg0: application.name })" /><AppIcon
      v-else
      name="application"
      :size="26"
  /></span>
</template>
