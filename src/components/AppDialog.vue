<script setup lang="ts">
import { t } from "../i18n";

import { onMounted, ref } from "vue";
import AppIcon from "./AppIcon.vue";
defineProps<{ title: string; wide?: boolean }>();
const emit = defineEmits<{ close: [] }>();
const dialog = ref<HTMLDialogElement>();
function trapFocus(event: KeyboardEvent) {
  if (event.key !== "Tab" || event.defaultPrevented) return;
  const items = [
    ...(dialog.value?.querySelectorAll<HTMLElement>(
      "button, input, select, textarea, a[href], [tabindex]",
    ) ?? []),
  ].filter(
    (item) =>
      item.tabIndex >= 0 &&
      !item.matches(":disabled") &&
      !item.closest("[inert]") &&
      item.getClientRects().length > 0 &&
      getComputedStyle(item).visibility !== "hidden",
  );
  const first = items[0],
    last = items.at(-1);
  if (!first || !last) {
    event.preventDefault();
    return;
  }
  if (event.shiftKey && document.activeElement === first) {
    event.preventDefault();
    last.focus();
  } else if (!event.shiftKey && document.activeElement === last) {
    event.preventDefault();
    first.focus();
  }
}
onMounted(() => {
  dialog.value?.showModal();
  dialog.value
    ?.querySelector<HTMLButtonElement>("[data-cancel]")
    ?.focus({ preventScroll: true });
  // Long translations must open at the heading while retaining the safe focus target.
  if (dialog.value) dialog.value.scrollTop = 0;
});
</script>
<template>
  <dialog
    ref="dialog"
    class="dialog"
    :class="{ 'wide-dialog': wide }"
    :aria-label="title"
    @keydown="trapFocus"
    @cancel.prevent="emit('close')"
  >
    <div class="panel-heading">
      <h2>{{ title }}</h2>
      <button
        class="quiet"
        :aria-label="t('ui.closeDialog')"
        @click="emit('close')"
      >
        <AppIcon name="close" />
      </button>
    </div>
    <slot />
  </dialog>
</template>
