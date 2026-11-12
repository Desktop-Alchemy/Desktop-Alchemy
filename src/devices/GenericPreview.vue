<script setup lang="ts">
import { ref, watch, onScopeDispose } from "vue";
import type { Page } from "../bindings";
import { useDeviceContext } from "./context";
import { showsText } from "../content";
const w = useDeviceContext();
const props = defineProps<{
  page: Page;
  selected: number;
  pressed: number[];
  editor?: boolean;
}>();
defineEmits<{ select: [key: number] }>();
const images = ref<Record<number, string>>({});
let generation = 0;
watch(
  () => JSON.stringify(props.page),
  async () => {
    const before = ++generation;
    const next: Record<number, string> = {};
    await Promise.all(
      props.page.keys.map(async (v) => {
        if (!w.currentProfile?.layout.display_keys.includes(v.key)) return;
        const image = await w.preview(v);
        if (image) next[v.key] = image;
      }),
    );
    if (before === generation) images.value = next;
  },
  { immediate: true },
);
onScopeDispose(() => generation++);
</script>
<template>
  <div
    class="generic-device-grid"
    :style="{
      gridTemplateColumns: `repeat(${w.currentProfile?.layout.columns || 5},minmax(48px,1fr))`,
    }"
  >
    <button
      v-for="display in w.currentProfile?.definition.displays"
      :key="display.id"
      :aria-pressed="selected === display.key"
      :class="{
        pressed: pressed.includes(display.key),
        selected: selected === display.key,
      }"
      :style="{
        aspectRatio: `${display.width}/${display.height}`,
        gridColumn: display.column + 1,
        gridRow: display.row + 1,
      }"
      @click="$emit('select', display.key)"
    >
      <img v-if="images[display.key]" :src="images[display.key]" alt="" /><span
        v-else
        >{{
          page.keys.find((v) => v.key === display.key && showsText(v))?.text ||
          ""
        }}</span
      >
    </button>
  </div>
</template>
<style scoped>
.generic-device-grid {
  display: grid;
  gap: 10px;
  padding: 20px;
}
.generic-device-grid button {
  padding: 4px;
  overflow: hidden;
}
.generic-device-grid img {
  width: 100%;
  height: 100%;
  object-fit: contain;
}
.generic-device-grid .selected {
  outline: 2px solid var(--primary);
}
.generic-device-grid .pressed {
  transform: translateY(2px);
}
</style>
