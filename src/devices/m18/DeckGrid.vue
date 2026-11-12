<script setup lang="ts">
import { t } from "../../i18n";

import { ref, watch, onBeforeUnmount } from "vue";
import { useDeviceContext } from "../context";
import type { Page, ModelAppearance } from "../../bindings";
import AppIcon from "../../components/AppIcon.vue";
import { showsText } from "../../content";
const w = useDeviceContext();
const props = defineProps<{
  page: Page;
  selected: number;
  pressed: number[];
  appearance?: ModelAppearance;
  labelImage?: string;
}>();
defineEmits<{ select: [key: number] }>();
const images = ref<Record<number, string>>({});
let timer: ReturnType<typeof setTimeout>;
let generation = 0;
watch(
  [
    () => JSON.stringify(props.page),
    () => (props.page.keys.some((k) => k.plugin_icon) ? w.library.revision : 0),
    () => (props.page.keys.some((k) => k.widget) ? w.widgetRevision : 0),
    () =>
      props.page.keys.some((k) => k.widget)
        ? JSON.stringify(
            w.state.plugins.map((p) => [p.manifest.id, p.status]),
          ) + JSON.stringify(w.state.config.disabled_plugins)
        : "",
  ],
  () => {
    clearTimeout(timer);
    const n = ++generation;
    timer = setTimeout(async () => {
      if (!w.desktop) return;
      const result: Record<number, string> = {};
      await Promise.all(
        props.page.keys
          .filter((k) => k.key <= 15)
          .map(async (k) => {
            try {
              const image = await w.preview(k);
              if (image) result[k.key] = image;
            } catch {
              /* Missing assets retain the editable fallback label. */
            }
          }),
      );
      if (n === generation) images.value = result;
    }, 140);
  },
  { immediate: true },
);
onBeforeUnmount(() => {
  clearTimeout(timer);
  generation++;
});
</script>
<template>
  <div class="deck-shell">
    <span class="deck-wordmark" aria-hidden="true">
      <img
        v-if="appearance?.type === 'image' && labelImage"
        :src="labelImage"
        alt=""
      />
      <template v-else>{{
        appearance?.type === "text" ? appearance.text : "M18"
      }}</template>
    </span>
    <div class="deck-grid" role="group" :aria-label="t('ui.15DisplayKeys')">
      <button
        v-for="key in page.keys.filter((k) => k.key <= 15)"
        :key="key.key"
        class="deck-key"
        :class="{
          selected: selected === key.key,
          pressed: pressed.includes(key.key),
        }"
        :aria-label="t('ui.key', { arg0: key.key, arg1: key.text })"
        :aria-pressed="selected === key.key"
        @click="$emit('select', key.key)"
      >
        <span
          class="deck-screen"
          :style="{ background: key.background, color: key.foreground }"
        >
          <img v-if="images[key.key]" :src="images[key.key]" alt="" /><span
            v-else
            class="deck-fallback"
            ><template v-if="showsText(key)">{{ key.text }}</template></span
          >
        </span>
        <small>{{ String(key.key).padStart(2, "0") }}</small>
      </button>
    </div>
    <div
      class="deck-bottom"
      role="group"
      :aria-label="t('ui.3PhysicalKeysWithoutDisplays')"
    >
      <span class="deck-led" aria-hidden="true"></span>
      <button
        v-for="key in page.keys.filter((k) => k.key > 15)"
        :key="key.key"
        :aria-label="t('ui.physicalKey', { arg0: key.key })"
        :class="{
          selected: selected === key.key,
          pressed: pressed.includes(key.key),
        }"
        :aria-pressed="selected === key.key"
        @click="$emit('select', key.key)"
      >
        <AppIcon
          :name="key.key === 16 ? 'previous' : key.key === 17 ? 'home' : 'next'"
          :size="17"
        />
        <small>{{ key.key }}</small>
      </button>
      <span class="deck-led" aria-hidden="true"></span>
    </div>
  </div>
</template>
