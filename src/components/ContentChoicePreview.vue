<script setup lang="ts">
import { ref, watch, onMounted, onBeforeUnmount } from "vue";
import type { KeyVisual } from "../bindings";
import { contentVisual, type ContentChoice } from "../content";
import { useDesktopAlchemy } from "../store";
import AppIcon from "./AppIcon.vue";
const props = defineProps<{ choice: ContentChoice; visual: KeyVisual }>();
const w = useDesktopAlchemy();
const element = ref<HTMLElement>();
const source = ref("");
const visible = ref(false);
let observer: IntersectionObserver | undefined;
let generation = 0;
watch(
  [
    () => JSON.stringify([props.choice, props.visual]),
    () => w.widgetRevision,
    visible,
  ],
  async () => {
    const current = ++generation;
    if (!visible.value) return;
    if (props.choice.definition && props.choice.status !== "running") {
      source.value = "";
      return;
    }
    const url = await w.preview(contentVisual(props.visual, props.choice));
    if (current === generation) source.value = url ?? "";
  },
);
onMounted(() => {
  observer = new IntersectionObserver(([entry]) => {
    visible.value = entry.isIntersecting;
  });
  if (element.value) observer.observe(element.value);
});
onBeforeUnmount(() => {
  generation++;
  observer?.disconnect();
});
</script>
<template>
  <span ref="element" class="content-choice-preview" aria-hidden="true">
    <img v-if="source" :src="source" alt="" />
    <span v-else
      ><AppIcon
        :name="
          choice.definition
            ? 'extensions'
            : choice.value === 'basic:text'
              ? 'text'
              : choice.value === 'basic:none'
                ? 'none'
                : 'image'
        "
        :size="23"
      /><small>{{ choice.name }}</small></span
    >
  </span>
</template>
