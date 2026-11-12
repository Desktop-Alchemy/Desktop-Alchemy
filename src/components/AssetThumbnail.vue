<script setup lang="ts">
import { t } from "../i18n";

import { computed, onMounted, onBeforeUnmount, ref, watch } from "vue";
import { useDesktopAlchemy } from "../store";
import AppIcon from "./AppIcon.vue";
import type { ImageRef } from "../bindings";
const props = defineProps<{
  assetId?: string;
  image?: ImageRef;
  revision?: string;
  name: string;
}>();
const reference = computed<ImageRef>(
  () => props.image ?? { type: "asset", asset_id: props.assetId ?? "" },
);
const gif = computed(
  () =>
    reference.value.type === "asset" &&
    reference.value.asset_id.endsWith(".gif"),
);
const w = useDesktopAlchemy();
const element = ref<HTMLElement>();
const source = ref("");
const failed = ref(false);
let observer: IntersectionObserver | undefined;
let generation = 0;
async function load() {
  const current = ++generation;
  const url = await w.call<string>(
    "image_thumbnail",
    { image: reference.value },
    undefined,
    true,
  );
  if (current === generation) {
    source.value = url ?? "";
    failed.value = !url;
  }
}
watch(
  () => JSON.stringify([reference.value, props.revision]),
  () => {
    source.value = "";
    failed.value = false;
    void load();
  },
);
onMounted(() => {
  observer = new IntersectionObserver((entries) => {
    if (entries.some((e) => e.isIntersecting)) {
      observer?.disconnect();
      void load();
    }
  });
  if (element.value) observer.observe(element.value);
});
onBeforeUnmount(() => {
  generation++;
  observer?.disconnect();
});
</script>
<template>
  <span ref="element" class="asset-thumbnail"
    ><img v-if="source" :src="source" :alt="name" /><span
      v-else
      :title="failed ? t('ui.previewUnavailable') : t('ui.loadingPreview')"
      ><AppIcon :name="failed ? 'image-broken' : 'image'" :size="26" /><span
        v-if="failed"
        class="sr-only"
        >{{ t("ui.previewUnavailable") }}</span
      ></span
    ><small v-if="gif" class="gif-tag">GIF</small></span
  >
</template>
