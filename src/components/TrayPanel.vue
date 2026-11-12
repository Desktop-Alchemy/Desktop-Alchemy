<script setup lang="ts">
import { t } from "../i18n";

import { computed, nextTick, onMounted, onUnmounted, ref, watch } from "vue";
import { useTrayPanel } from "../store";
import AppIcon from "./AppIcon.vue";
import type { TrayAction } from "../bindings";
import type { IconName } from "../icons";

const tray = useTrayPanel();
const panel = ref<HTMLElement>();
const keyboardNavigation = ref(false);
const system = matchMedia("(prefers-color-scheme: dark)");
const systemDark = ref(system.matches);
const updateSystem = () => (systemDark.value = system.matches);
watch(
  [() => tray.state.value.theme, systemDark],
  ([theme, dark]) => {
    document.documentElement.dataset.theme =
      theme === "system" ? (dark ? "dark" : "light") : theme;
  },
  { immediate: true },
);
const connection = computed(
  () =>
    ({
      ready: t("ui.connected"),
      connecting: t("ui.connecting"),
      reconnecting: t("ui.reconnecting"),
      disconnected: t("ui.disconnected"),
      error: t("ui.connectionError"),
    })[tray.state.value.connection] ?? t("ui.disconnected"),
);
const entries: { action: TrayAction; label: string; icon: IconName }[] = [
  {
    action: "device",
    get label() {
      return t("ui.devices");
    },
    icon: "device",
  },
  {
    action: "pages",
    get label() {
      return t("ui.pages");
    },
    icon: "pages",
  },
  {
    action: "plugins",
    get label() {
      return t("ui.extensions");
    },
    icon: "extensions",
  },
];
const shortcuts: { action: TrayAction; label: string; icon: IconName }[] = [
  {
    action: "show",
    get label() {
      return t("ui.showWindow");
    },
    icon: "application",
  },
  {
    action: "settings",
    get label() {
      return t("ui.appSettings");
    },
    icon: "settings",
  },
  {
    action: "exit",
    get label() {
      return t("ui.quit");
    },
    icon: "power",
  },
];
async function focusFirst() {
  await nextTick();
  panel.value
    ?.querySelector<HTMLButtonElement>("button:not(:disabled)")
    ?.focus({ preventScroll: true });
}
watch(tray.opened, focusFirst);
function keyboard(event: KeyboardEvent) {
  if (event.key === "Escape") {
    event.preventDefault();
    void tray.dismiss();
    return;
  }
  if (
    event.key.startsWith("Arrow") ||
    event.key === "Tab" ||
    event.key === "Home" ||
    event.key === "End"
  )
    keyboardNavigation.value = true;
  const buttons = [
    ...(panel.value?.querySelectorAll<HTMLButtonElement>(
      "button:not(:disabled)",
    ) ?? []),
  ];
  const current = buttons.indexOf(document.activeElement as HTMLButtonElement);
  let index: number;
  switch (event.key) {
    case "ArrowDown":
    case "ArrowRight":
      index = (current + 1) % buttons.length;
      break;
    case "ArrowUp":
    case "ArrowLeft":
      index = (current - 1 + buttons.length) % buttons.length;
      break;
    case "Home":
      index = 0;
      break;
    case "End":
      index = buttons.length - 1;
      break;
    case "Tab":
      index =
        (current + (event.shiftKey ? -1 : 1) + buttons.length) % buttons.length;
      break;
    default:
      return;
  }
  event.preventDefault();
  buttons[index]?.focus();
}
onMounted(() => {
  system.addEventListener("change", updateSystem);
  window.addEventListener("focus", focusFirst);
  void tray.init();
  void focusFirst();
});
onUnmounted(() => {
  system.removeEventListener("change", updateSystem);
  window.removeEventListener("focus", focusFirst);
});
</script>

<template>
  <div
    class="tray-shell"
    :class="{ 'tray-keyboard': keyboardNavigation }"
    @pointerdown="keyboardNavigation = false"
    @pointerdown.self="tray.dismiss"
    @contextmenu.prevent
    @keydown="keyboard"
  >
    <section
      ref="panel"
      class="tray-panel"
      :aria-label="t('ui.desktopAlchemyTrayMenu')"
    >
      <header class="tray-heading">
        <strong>Desktop Alchemy</strong>
        <span
          class="tray-connection"
          role="status"
          :data-connection="tray.state.value.connection"
        >
          <i aria-hidden="true" />M18 {{ connection }}
        </span>
      </header>
      <nav class="tray-entries" :aria-label="t('ui.quickActions')">
        <button
          v-for="entry in entries"
          :key="entry.action"
          :disabled="tray.busy.value"
          @click="tray.action(entry.action)"
        >
          <AppIcon :name="entry.icon" :size="23" /><span>{{
            entry.label
          }}</span>
        </button>
      </nav>
      <footer class="tray-shortcuts">
        <button
          v-for="entry in shortcuts"
          :key="entry.action"
          :disabled="tray.busy.value"
          @click="tray.action(entry.action)"
        >
          <AppIcon :name="entry.icon" :size="23" /><span>{{
            entry.label
          }}</span>
        </button>
      </footer>
      <p v-if="tray.error.value" class="tray-error" role="alert">
        {{ tray.error.value }}
      </p>
    </section>
  </div>
</template>

<style>
html.tray-surface,
.tray-surface body,
.tray-surface #app {
  min-width: 0;
  width: 100%;
  height: 100%;
  overflow: hidden;
  background: transparent;
}
.tray-shell {
  width: 100%;
  height: 100%;
  padding: 8px;
}
.tray-panel {
  position: relative;
  width: 100%;
  max-width: 260px;
  border: 1px solid var(--border);
  border-radius: 14px;
  background: var(--card);
  color: var(--fg);
  box-shadow: 0 4px 12px #00000026;
  overflow: hidden;
  user-select: none;
}
.tray-heading {
  height: 64px;
  padding: 11px 16px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.tray-heading strong {
  font-size: 15px;
  line-height: 22px;
  font-weight: 700;
}
.tray-connection {
  display: flex;
  align-items: center;
  gap: 7px;
  color: var(--subtle);
  font-size: 12px;
  line-height: 18px;
}
.tray-connection i {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: var(--subtle);
}
.tray-connection[data-connection="ready"] i {
  background: var(--success);
}
.tray-connection[data-connection="connecting"] i,
.tray-connection[data-connection="reconnecting"] i {
  background: var(--warning);
}
.tray-connection[data-connection="error"] i {
  background: var(--error);
}
.tray-entries {
  height: 128px;
  margin: 0 10px;
  padding: 6px 0;
  border-top: 1px solid var(--border);
  display: flex;
  flex-direction: column;
}
.tray-panel button {
  border: 0;
  background: transparent;
  box-shadow: none;
  color: var(--subtle);
  font-size: 14px;
  font-weight: 500;
}
.tray-entries button {
  display: flex;
  align-items: center;
  gap: 8px;
  text-align: left;
  padding: 0 8px;
  min-height: 38px;
  flex: 1;
  border-radius: 8px;
}
.tray-shortcuts {
  height: 64px;
  width: fit-content;
  margin: 0 auto;
  padding: 5px 0 7px;
  border-top: 1px solid var(--border);
  display: grid;
  grid-template-columns: repeat(3, 68px);
  gap: 4px;
}
.tray-shortcuts button {
  display: flex;
  align-items: center;
  justify-content: center;
  flex-direction: column;
  gap: 3px;
  padding: 5px 2px;
  font-size: 12px;
  border-radius: 8px;
}
.tray-panel button:hover,
.tray-keyboard .tray-panel button:focus-visible {
  background: var(--primary-soft);
  color: var(--primary-text);
}
.tray-panel button:focus-visible {
  outline: 2px solid var(--primary);
  outline-offset: -2px;
}
.tray-panel button:active {
  transform: none;
  background: var(--secondary);
}
.tray-panel button:disabled {
  opacity: 0.5;
  cursor: default;
}
.tray-error {
  position: absolute;
  inset: 6px;
  margin: 0;
  padding: 10px;
  border: 1px solid var(--error);
  border-radius: 10px;
  color: var(--error);
  background: var(--card);
  font-size: 12px;
  overflow-wrap: anywhere;
  pointer-events: none;
}
@media (prefers-reduced-motion: reduce) {
  .tray-panel button {
    transition: none;
  }
}
</style>
