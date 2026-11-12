<script setup lang="ts">
import { useDesktopAlchemy } from "../store";
import { provideDeviceContext } from "./context";
const props = defineProps<{ profileId: string }>();
provideDeviceContext(useDesktopAlchemy(), props.profileId);
</script>
<template><slot /></template>
