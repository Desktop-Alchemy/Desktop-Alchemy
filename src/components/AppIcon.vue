<script setup lang="ts">
import { icons, type IconName } from "../icons";
withDefaults(
  defineProps<{
    name: IconName;
    size?: number;
    weight?: "regular" | "bold" | "fill" | "duotone";
  }>(),
  { size: 18, weight: "regular" },
);
</script>

<template>
  <component
    :is="icons[name]"
    class="app-icon"
    :size="size"
    :weight="weight"
    aria-hidden="true"
    focusable="false"
  />
</template>
