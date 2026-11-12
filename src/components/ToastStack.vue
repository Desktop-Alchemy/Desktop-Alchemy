<script setup lang="ts">
import { t } from "../i18n";

import { ref, watch, nextTick, onMounted, onBeforeUnmount } from "vue";
import { useDesktopAlchemy } from "../store";
import AppIcon from "./AppIcon.vue";

const w = useDesktopAlchemy();
const stack = ref<HTMLElement>();
const target = ref<HTMLElement>(document.body);
const timers = new Map<number, ReturnType<typeof setTimeout>>();
const known = new Set<number>();
function raiseStack() {
  if (!w.notifications.length) return;
  stack.value?.hidePopover?.();
  stack.value?.showPopover?.();
}
const dialogs = new MutationObserver(async () => {
  const active =
    [...document.querySelectorAll<HTMLDialogElement>("dialog[open]")].at(-1) ??
    document.body;
  if (target.value === active) return;
  // Content outside a modal is inert, even in the top layer. Keep the toast
  // inside the active dialog so its close button remains accessible.
  stack.value?.hidePopover?.();
  target.value = active;
  await nextTick();
  raiseStack();
});
onMounted(() =>
  dialogs.observe(document.body, {
    subtree: true,
    attributes: true,
    attributeFilter: ["open"],
    childList: true,
  }),
);
function pause(id: number) {
  clearTimeout(timers.get(id));
  timers.delete(id);
}
function resume(id: number, kind: string) {
  pause(id);
  timers.set(
    id,
    setTimeout(() => w.dismissNotification(id), kind === "error" ? 8000 : 4000),
  );
}
watch(
  () => [...w.notifications],
  async (items) => {
    for (const id of known) {
      if (!items.some((item) => item.id === id)) {
        pause(id);
        known.delete(id);
      }
    }
    for (const item of items) {
      if (!known.has(item.id)) {
        known.add(item.id);
        resume(item.id, item.kind);
      }
    }
    await nextTick();
    // A manual popover keeps notifications visible above modal dialogs without
    // moving focus or dismissing the dialog underneath.
    if (items.length) raiseStack();
    else stack.value?.hidePopover?.();
  },
  { immediate: true },
);
onBeforeUnmount(() => {
  dialogs.disconnect();
  for (const id of timers.keys()) pause(id);
});
</script>

<template>
  <Teleport :to="target">
    <div
      ref="stack"
      class="toast-stack"
      popover="manual"
      :aria-label="t('ui.notifications')"
    >
      <TransitionGroup name="toast">
        <div
          v-for="item in w.notifications"
          :key="item.id"
          class="toast-message"
          :class="item.kind"
          :role="item.kind === 'error' ? 'alert' : 'status'"
          aria-atomic="true"
          @mouseenter="pause(item.id)"
          @mouseleave="resume(item.id, item.kind)"
          @focusin="pause(item.id)"
          @focusout="resume(item.id, item.kind)"
        >
          <span class="toast-icon"
            ><AppIcon
              :name="item.kind === 'error' ? 'warning' : 'check'"
              :size="16"
          /></span>
          <div class="toast-content">
            <span>{{ item.message }}</span>
            <details v-if="item.detail">
              <summary>{{ t("errors.details") }}</summary>
              <pre>{{ item.detail }}</pre>
            </details>
          </div>
          <button
            class="quiet"
            :aria-label="t('ui.dismissNotification')"
            @click="w.dismissNotification(item.id)"
          >
            <AppIcon name="close" :size="16" />
          </button>
        </div>
      </TransitionGroup>
    </div>
  </Teleport>
</template>
