<script setup lang="ts">
import { pluginName } from "../plugin-labels";
import { t } from "../i18n";

import { computed, nextTick, ref, watch, useId } from "vue";
import type { KeyVisual } from "../bindings";
import { contentSelection } from "../content";
import { useDesktopAlchemy } from "../store";
import AppDialog from "./AppDialog.vue";
import AppIcon from "./AppIcon.vue";
import ContentChoicePreview from "./ContentChoicePreview.vue";
const props = defineProps<{ visual: KeyVisual }>();
const emit = defineEmits<{ close: []; select: [value: string] }>();
const w = useDesktopAlchemy();
const query = ref("");
const category = ref("all");
const selected = ref<string | null>(contentSelection(props.visual));
const results = ref<HTMLElement>();
const listId = `content-results-${useId()}`;
const plugins = computed(() =>
  w.state.plugins.filter((p) => p.manifest.widgets?.length),
);
const categories = computed(() => [
  { id: "all", name: t("ui.allContent"), icon: "device" as const },
  { id: "recent", name: t("ui.recentlyUsed"), icon: "clock" as const },
  ...plugins.value.map((p) => ({
    id: p.manifest.id,
    name: pluginName(p.manifest),
    icon: "extensions" as const,
  })),
]);
const filtered = computed(() => {
  const words = query.value
    .trim()
    .toLocaleLowerCase()
    .split(/\s+/)
    .filter(Boolean);
  const choices = w.contentChoices.filter(
    (c) =>
      (category.value === "all" ||
        (category.value === "recent"
          ? w.recentContents.includes(c.value)
          : c.pluginId === category.value)) &&
      words.every((word) =>
        `${c.name} ${c.provider} ${c.description}`
          .toLocaleLowerCase()
          .includes(word),
      ),
  );
  return category.value === "recent"
    ? choices.sort(
        (a, b) =>
          w.recentContents.indexOf(a.value) - w.recentContents.indexOf(b.value),
      )
    : choices;
});
const selectedChoice = computed(() =>
  filtered.value.find((c) => c.value === selected.value),
);
const selectedIndex = computed(() =>
  filtered.value.findIndex((c) => c.value === selected.value),
);
const heading = computed(
  () =>
    categories.value.find((c) => c.id === category.value)?.name ??
    t("ui.allContent"),
);
watch(
  filtered,
  (choices) => {
    if (!choices.some((c) => c.value === selected.value))
      selected.value = choices[0]?.value ?? null;
  },
  { immediate: true },
);
watch(plugins, () => {
  if (
    !["all", "recent"].includes(category.value) &&
    !plugins.value.some((p) => p.manifest.id === category.value)
  )
    category.value = "all";
});
function confirm() {
  if (selectedChoice.value) emit("select", selectedChoice.value.value);
}
function reveal() {
  void nextTick(() => {
    const list = results.value;
    const row = list?.querySelector('[aria-selected="true"]');
    if (!list || !row) return;
    const bounds = list.getBoundingClientRect(),
      target = row.getBoundingClientRect();
    if (target.top < bounds.top) list.scrollTop -= bounds.top - target.top;
    else if (target.bottom > bounds.bottom)
      list.scrollTop += target.bottom - bounds.bottom;
  });
}
function keydown(event: KeyboardEvent) {
  if (event.isComposing) return;
  if (event.key === "Enter") {
    event.preventDefault();
    confirm();
    return;
  }
  if (!["ArrowUp", "ArrowDown", "Home", "End"].includes(event.key)) return;
  event.preventDefault();
  const choices = filtered.value;
  if (!choices.length) return;
  const index = choices.findIndex((c) => c.value === selected.value);
  const next =
    event.key === "Home"
      ? 0
      : event.key === "End"
        ? choices.length - 1
        : Math.max(
            0,
            Math.min(
              choices.length - 1,
              index + (event.key === "ArrowDown" ? 1 : -1),
            ),
          );
  selected.value = choices[next].value;
  reveal();
}
function searchKeydown(event: KeyboardEvent) {
  if (event.isComposing) return;
  if (event.key === "ArrowDown") {
    results.value?.focus();
    keydown(event);
  } else if (event.key === "Enter") keydown(event);
}
function escape(event: KeyboardEvent) {
  if (event.isComposing) return;
  event.preventDefault();
  event.stopPropagation();
  emit("close");
}
</script>
<template>
  <AppDialog
    :title="t('ui.chooseDisplayContent')"
    wide
    class="content-picker"
    @keydown.esc.capture="escape"
    @close="emit('close')"
  >
    <p class="content-picker-subtitle">
      {{ t("phrases.keyPicker", { key: String(visual.key).padStart(2, "0") }) }}
    </p>
    <div class="content-search">
      <AppIcon name="search" :size="19" />
      <input
        v-model="query"
        type="search"
        :aria-label="t('ui.searchPluginsOrContent')"
        :placeholder="t('ui.searchPluginsOrContent2')"
        @keydown="searchKeydown"
      />
    </div>
    <div class="content-picker-layout">
      <nav class="content-categories" :aria-label="t('ui.contentCategories')">
        <template v-for="(item, index) in categories" :key="item.id">
          <p v-if="index === 2" class="content-category-caption">
            {{ t("ui.installedPlugins") }}
          </p>
          <button
            :aria-pressed="category === item.id"
            @click="category = item.id"
          >
            <AppIcon :name="item.icon" :size="18" /><span>{{ item.name }}</span>
          </button>
        </template>
        <p v-if="!plugins.length" class="content-category-caption">
          {{ t("ui.noPluginContentAvailable") }}
        </p>
      </nav>
      <section class="content-results-section" :aria-label="heading">
        <div class="content-results-heading">
          <h3>{{ heading }}</h3>
          <span>{{
            t("phrases.contentCount", { count: filtered.length })
          }}</span>
        </div>
        <div
          ref="results"
          role="listbox"
          :aria-label="t('ui.availableDisplayContent')"
          :aria-activedescendant="
            selectedIndex >= 0 ? `${listId}-${selectedIndex}` : undefined
          "
          tabindex="0"
          class="content-results"
          @keydown="keydown"
        >
          <button
            v-for="(choice, index) in filtered"
            :key="choice.value"
            :id="`${listId}-${index}`"
            type="button"
            role="option"
            :aria-label="
              choice.definition
                ? `${choice.name} · ${choice.provider}`
                : choice.name
            "
            :aria-description="choice.description"
            :aria-selected="selected === choice.value"
            tabindex="-1"
            class="content-choice"
            @click="selected = choice.value"
          >
            <ContentChoicePreview :choice="choice" :visual="visual" />
            <span class="content-choice-copy"
              ><strong>{{ choice.name }}</strong
              ><span class="content-choice-provider"
                >{{ choice.provider
                }}<small
                  v-if="choice.definition && choice.builtIn"
                  class="badge"
                  >{{ t("ui.builtIn") }}</small
                ><small v-if="choice.status === 'error'" class="error-text">{{
                  t("ui.error")
                }}</small></span
              ><span class="content-choice-description">{{
                choice.description
              }}</span></span
            >
            <span class="content-choice-check" aria-hidden="true"
              ><AppIcon
                v-if="selected === choice.value"
                name="check"
                :size="17"
            /></span>
          </button>
          <div v-if="!filtered.length" class="content-picker-empty">
            <AppIcon name="search" :size="30" /><strong>{{
              category === "recent" && !query
                ? t("ui.noRecentlyUsedContent")
                : t("ui.noMatchingContent")
            }}</strong>
            <p>{{ t("ui.tryADifferentSearchOrCategory") }}</p>
          </div>
        </div>
      </section>
    </div>
    <footer class="content-picker-footer">
      <span>{{ t("ui.chooseContentThenAdjustItsFormat") }}</span>
      <div class="button-row">
        <button data-cancel @click="emit('close')">{{ t("ui.cancel") }}</button
        ><button class="primary" :disabled="!selectedChoice" @click="confirm">
          {{ t("ui.useThisContent") }}
        </button>
      </div>
    </footer>
  </AppDialog>
</template>
