<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref, watch } from "vue";
import { AudioDockScene } from "./scene3d";
const props = defineProps<{ pressed: boolean; focused: boolean }>();
const emit = defineEmits<{ failed: []; loaded: [] }>();
const host = ref<HTMLDivElement>();
let scene: AudioDockScene | undefined;
let disposed = false;
onMounted(async () => {
  try {
    if (!host.value) return;
    scene = new AudioDockScene(host.value, () => emit("failed"));
    await scene.load();
    if (disposed) return;
    scene.setState(props.pressed, props.focused);
    emit("loaded");
  } catch {
    if (!disposed) emit("failed");
  }
});
watch([() => props.pressed, () => props.focused], () =>
  scene?.setState(props.pressed, props.focused),
);
onBeforeUnmount(() => {
  disposed = true;
  scene?.dispose();
});
defineExpose({
  setView: (view: "front" | "top" | "back") => scene?.setView(view),
  stats: () => scene?.stats(),
});
</script>
<template><div ref="host" class="dock-model-canvas" /></template>
