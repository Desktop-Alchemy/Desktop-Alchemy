<script setup lang="ts" generic="T extends string | null">
import { t } from "../i18n";

import {
  computed,
  nextTick,
  onBeforeUnmount,
  onMounted,
  ref,
  useAttrs,
  useId,
  watch,
} from "vue";
import AppIcon from "./AppIcon.vue";

defineOptions({ inheritAttrs: false });
type Option = { value: T; label: string; disabled?: boolean };
const props = withDefaults(
  defineProps<{
    modelValue: T;
    options: readonly Option[];
    disabled?: boolean;
    placeholder?: string;
  }>(),
  { disabled: false, placeholder: "" },
);
const emit = defineEmits<{
  "update:modelValue": [value: T];
  change: [value: T];
}>();
const attrs = useAttrs();
const id = `select-${useId()}`;
const trigger = ref<HTMLButtonElement>();
const menu = ref<HTMLDivElement>();
const opened = ref(false);
const active = ref(-1);
const position = ref<Record<string, string>>({});
const selected = computed(() =>
  props.options.find((o) => o.value === props.modelValue),
);
const lastEnabled = () =>
  props.options
    .map((o, index) => (o.disabled ? -1 : index))
    .filter((index) => index >= 0)
    .at(-1) ?? -1;
let search = "";
let searchTimer: ReturnType<typeof setTimeout> | undefined;
let resize: ResizeObserver | undefined;

function place() {
  if (!opened.value || !trigger.value || !menu.value) return;
  const rect = trigger.value.getBoundingClientRect();
  const gap = 6,
    edge = 10;
  const below = innerHeight - rect.bottom - gap - edge;
  const above = rect.top - gap - edge;
  const desired = Math.min(288, menu.value.scrollHeight);
  const upward = below < desired && above > below;
  const height = Math.max(0, Math.min(288, upward ? above : below));
  const width = Math.min(Math.max(rect.width, 180), innerWidth - edge * 2);
  position.value = {
    left: `${Math.min(Math.max(edge, rect.left), innerWidth - width - edge)}px`,
    top: upward ? "auto" : `${rect.bottom + gap}px`,
    bottom: upward ? `${innerHeight - rect.top + gap}px` : "auto",
    width: `${width}px`,
    maxHeight: `${height}px`,
  };
}
function revealActive() {
  void nextTick(() => {
    const list = menu.value;
    const item = list?.querySelector(`#${id}-option-${active.value}`);
    if (!list || !item) return;
    const bounds = list.getBoundingClientRect();
    const option = item.getBoundingClientRect();
    if (option.top < bounds.top + 5)
      list.scrollTop -= bounds.top + 5 - option.top;
    else if (option.bottom > bounds.bottom - 5)
      list.scrollTop += option.bottom - bounds.bottom + 5;
  });
}
function close(restoreFocus = false) {
  opened.value = false;
  menu.value?.hidePopover();
  search = "";
  clearTimeout(searchTimer);
  if (restoreFocus) trigger.value?.focus({ preventScroll: true });
}
async function open(last = false) {
  if (props.disabled || opened.value) return;
  opened.value = true;
  const index = props.options.findIndex(
    (o) => o.value === props.modelValue && !o.disabled,
  );
  active.value =
    index >= 0
      ? index
      : last
        ? lastEnabled()
        : props.options.findIndex((o) => !o.disabled);
  await nextTick();
  menu.value?.showPopover();
  place();
  revealActive();
}
function choose(index: number) {
  const option = props.options[index];
  if (!option || option.disabled) return;
  emit("update:modelValue", option.value);
  emit("change", option.value);
  close(true);
}
function step(direction: number) {
  for (let n = 1; n <= props.options.length; n++) {
    const index =
      (active.value + direction * n + props.options.length) %
      props.options.length;
    if (!props.options[index].disabled) {
      active.value = index;
      revealActive();
      return;
    }
  }
}
function keydown(event: KeyboardEvent) {
  if (props.disabled || event.isComposing) return;
  if (event.key === "Tab") {
    close();
    return;
  }
  if (event.key === "Escape") {
    if (opened.value) {
      event.preventDefault();
      event.stopPropagation();
      close(true);
    }
    return;
  }
  if (
    ["ArrowDown", "ArrowUp", "Home", "End", "Enter", " "].includes(event.key)
  ) {
    event.preventDefault();
    if (!opened.value) {
      void open(event.key === "ArrowUp" || event.key === "End");
      return;
    }
    if (event.key === "ArrowDown" || event.key === "ArrowUp")
      step(event.key === "ArrowDown" ? 1 : -1);
    else if (event.key === "Home" || event.key === "End") {
      active.value =
        event.key === "Home"
          ? props.options.findIndex((o) => !o.disabled)
          : lastEnabled();
      revealActive();
    } else choose(active.value);
    return;
  }
  if (
    event.key.length === 1 &&
    !event.ctrlKey &&
    !event.metaKey &&
    !event.altKey
  ) {
    event.preventDefault();
    if (!opened.value) void open();
    search += event.key.toLocaleLowerCase();
    clearTimeout(searchTimer);
    searchTimer = setTimeout(() => {
      search = "";
    }, 700);
    const index = props.options.findIndex(
      (o) => !o.disabled && o.label.toLocaleLowerCase().startsWith(search),
    );
    if (index >= 0) {
      active.value = index;
      revealActive();
    }
  }
}
function outside(event: PointerEvent) {
  if (
    !trigger.value?.contains(event.target as Node) &&
    !menu.value?.contains(event.target as Node)
  )
    close();
}
watch(
  () => props.disabled,
  (disabled) => {
    if (disabled) close();
  },
);
watch(
  () => JSON.stringify([props.options, props.modelValue]),
  () => {
    if (!opened.value) return;
    active.value = props.options.findIndex(
      (o) => o.value === props.modelValue && !o.disabled,
    );
    void nextTick(place);
  },
);
onMounted(() => {
  document.addEventListener("pointerdown", outside, true);
  window.addEventListener("resize", place);
  window.addEventListener("scroll", place, true);
  resize = new ResizeObserver(place);
  if (trigger.value) resize.observe(trigger.value);
});
onBeforeUnmount(() => {
  close();
  resize?.disconnect();
  document.removeEventListener("pointerdown", outside, true);
  window.removeEventListener("resize", place);
  window.removeEventListener("scroll", place, true);
});
</script>

<template>
  <div class="app-select" :class="attrs.class">
    <button
      ref="trigger"
      v-bind="{ ...attrs, class: undefined }"
      type="button"
      class="select-trigger"
      role="combobox"
      :disabled="disabled"
      aria-haspopup="listbox"
      :aria-expanded="opened"
      :aria-controls="`${id}-list`"
      :aria-activedescendant="
        opened && active >= 0 ? `${id}-option-${active}` : undefined
      "
      @click="opened ? close() : open()"
      @keydown="keydown"
      @blur="close()"
    >
      <span class="select-value" :class="{ placeholder: !selected }">{{
        selected?.label ?? (placeholder || t("ui.selectAnOption"))
      }}</span>
      <AppIcon name="caret-down" :size="16" class="select-chevron" />
    </button>
    <div
      :id="`${id}-list`"
      ref="menu"
      class="select-menu"
      popover="manual"
      role="listbox"
      :aria-label="String(attrs['aria-label'] ?? t('ui.options'))"
      :style="position"
      @pointerdown.prevent
      @click.stop.prevent
      @keydown="keydown"
    >
      <div
        v-for="(option, index) in options"
        :id="`${id}-option-${index}`"
        :key="`${option.value}`"
        role="option"
        class="select-option"
        :aria-selected="option.value === modelValue"
        :aria-disabled="option.disabled || undefined"
        :class="{ highlighted: active === index, disabled: option.disabled }"
        @pointermove="!option.disabled && (active = index)"
        @click.stop.prevent="choose(index)"
      >
        <span>{{ option.label }}</span
        ><AppIcon v-if="option.value === modelValue" name="check" :size="16" />
      </div>
      <p v-if="!options.length" class="select-empty">{{ t("ui.noOptions") }}</p>
    </div>
  </div>
</template>
