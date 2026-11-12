<script setup lang="ts">
import UiFailure from "./UiFailure.vue";
import { t } from "../i18n";

import { computed, ref, watch, nextTick } from "vue";
import { useDesktopAlchemy } from "../store";
import type { Scene, SceneFallback, SceneTrigger } from "../bindings";
import {
  conditionSize,
  conditionSummary,
  newScene,
  sceneStatusLabels,
} from "../scenes";
import AppSelect from "./AppSelect.vue";
import AppSwitch from "./AppSwitch.vue";
import AppDialog from "./AppDialog.vue";
import AppIcon from "./AppIcon.vue";
import SceneDays from "./SceneDays.vue";
import SceneConditionEditor from "./SceneConditionEditor.vue";
const w = useDesktopAlchemy();
const selected = ref("");
const scene = computed(() =>
  w.sceneDraft.scenes.find((s) => s.id === selected.value),
);
const pages = computed(() =>
  w.draft.pages.map((p) => ({ value: p.id, label: p.name })),
);
const drag = ref<string | null>(null);
const deleting = ref<Scene | null>(null);
const checking = ref(false),
  checkResult = ref("");
watch(
  () => w.sceneDraft.scenes.map((s) => s.id),
  (ids) => {
    if (!ids.includes(selected.value)) selected.value = ids[0] ?? "";
  },
  { immediate: true },
);
watch(
  () => scene.value?.condition,
  () => {
    checkResult.value = "";
  },
  { deep: true },
);
function add() {
  const s = newScene(w.draft.pages[0].id);
  w.sceneDraft.scenes.push(s);
  selected.value = s.id;
  w.editScenes();
}
function duplicate() {
  if (!scene.value) return;
  const s: Scene = JSON.parse(JSON.stringify(scene.value));
  s.id = crypto.randomUUID();
  s.name = t("ui.copy", { arg0: s.name.slice(0, 116) });
  s.enabled = false;
  w.sceneDraft.scenes.splice(
    w.sceneDraft.scenes.indexOf(scene.value) + 1,
    0,
    s,
  );
  selected.value = s.id;
  w.editScenes();
}
function move(id: string, offset: number) {
  const list = w.sceneDraft.scenes,
    index = list.findIndex((s) => s.id === id),
    target = index + offset;
  if (target < 0 || target >= list.length) return;
  const [item] = list.splice(index, 1);
  list.splice(target, 0, item);
  w.editScenes();
  void nextTick(() => document.getElementById(`scene-${id}`)?.focus());
}
function drop(id: string) {
  if (drag.value) {
    const list = w.sceneDraft.scenes;
    const from = list.findIndex((s) => s.id === drag.value),
      to = list.findIndex((s) => s.id === id);
    if (from >= 0 && to >= 0 && from !== to) move(drag.value, to - from);
  }
  drag.value = null;
}
function trigger(type: string) {
  if (!scene.value) return;
  const value: SceneTrigger =
    type === "scheduled"
      ? { type, schedule: { days: { type: "every_day" }, time: "09:00" } }
      : { type: "continuous" };
  scene.value.trigger = value;
  w.editScenes();
}
function fallback(type: string) {
  const v: SceneFallback =
    type === "page"
      ? { type, page_id: w.draft.pages[0].id }
      : type === "keep"
        ? { type }
        : { type: "return" };
  w.sceneDraft.fallback = v;
  w.editScenes();
}
function status(s: Scene) {
  if (!pages.value.some((p) => p.value === s.page_id))
    return t("ui.targetPageMissing");
  if (!w.sceneDraft.enabled) return t("ui.automationOff");
  if (w.sceneDraft.paused) return t("ui.automationPaused");
  return (
    sceneStatusLabels[
      w.sceneStatus.states.find((r) => r.id === s.id)?.status ?? ""
    ] ?? (s.enabled ? t("ui.waitingToCheck") : t("ui.disabled"))
  );
}
async function check() {
  if (!scene.value) return;
  const id = scene.value.id,
    before = JSON.stringify(scene.value.condition);
  checking.value = true;
  const result = await w.checkSceneCondition(scene.value.condition);
  if (
    scene.value?.id === id &&
    before === JSON.stringify(scene.value.condition)
  )
    checkResult.value =
      result === "true"
        ? t("ui.conditionsCurrentlyMatch")
        : result === "false"
          ? t("ui.conditionsDoNotCurrentlyMatch")
          : t("ui.conditionsUnavailableCheckTheProgramOr");
  checking.value = false;
}
</script>
<template>
  <div class="page-heading">
    <div>
      <p class="eyebrow">{{ t("ui.automaticPageSwitching") }}</p>
      <h1>{{ t("ui.sceneAutomation") }}</h1>
    </div>
    <div class="button-row">
      <span
        class="save-status"
        :class="{ 'error-text': w.sceneSaveError }"
        role="status"
        >{{
          !w.desktop
            ? t("ui.browserPreviewChangesAreNotSaved")
            : w.sceneSaveError
              ? t("ui.couldNotSave")
              : w.sceneSaving
                ? t("ui.saving")
                : w.sceneDirty
                  ? t("ui.waitingToSave")
                  : t("ui.allChangesSaved")
        }}</span
      >
      <button v-if="w.sceneSaveError" @click="w.flushScenes()">
        {{ t("ui.retrySave") }}
      </button>
      <button
        v-if="w.sceneDraft.enabled"
        :disabled="!w.desktop"
        @click="w.pauseScenes(!w.sceneDraft.paused)"
      >
        {{
          w.sceneDraft.paused
            ? t("ui.resumeAutomation")
            : t("ui.pauseAutomation")
        }}
      </button>
    </div>
  </div>
  <section class="panel scene-global">
    <AppSwitch
      v-model="w.sceneDraft.enabled"
      :label="t('ui.enableSceneAutomation')"
      @update:model-value="w.editScenes"
    />
    <span class="hint">{{
      w.sceneDraft.paused
        ? t("ui.pausedResumeAutomationAfterSwitchingPages")
        : t("ui.selectsAPageByListPriority")
    }}</span>
    <details>
      <summary>{{ t("ui.fallbackAndPerformance") }}</summary>
      <div class="scene-global-fields">
        <label
          >{{ t("ui.whenNoScenesMatch")
          }}<AppSelect
            :model-value="w.sceneDraft.fallback.type"
            :options="[
              { value: 'return', label: t('ui.returnToThePreviousPage') },
              { value: 'keep', label: t('ui.keepCurrentPage') },
              { value: 'page', label: t('ui.useADefaultPage') },
            ]"
            @update:model-value="fallback"
        /></label>
        <label v-if="w.sceneDraft.fallback.type === 'page'"
          >{{ t("ui.defaultPage")
          }}<AppSelect
            v-model="w.sceneDraft.fallback.page_id"
            :options="pages"
            @change="w.editScenes"
        /></label>
        <label
          >{{ t("ui.processScanIntervalMs")
          }}<input
            v-model.number="w.sceneDraft.process_interval_ms"
            type="number"
            min="500"
            max="10000"
            step="100"
            @input="w.editScenes"
        /></label>
      </div>
    </details>
  </section>
  <div v-if="w.sceneStatus.error" class="scene-error" role="alert">
    {{ w.sceneStatus.error }}
    <button @click="w.call('retry_scene')">{{ t("ui.retrySwitch") }}</button>
  </div>
  <div class="scene-layout">
    <section class="panel scene-list">
      <div class="panel-heading">
        <h2>{{ t("ui.priority") }}</h2>
        <button :disabled="w.sceneDraft.scenes.length >= 200" @click="add">
          <AppIcon name="plus" :size="15" />{{ t("ui.new") }}
        </button>
      </div>
      <p class="hint">{{ t("ui.higherEntriesTakePriorityDragTo") }}</p>
      <div
        v-for="(s, i) in w.sceneDraft.scenes"
        :key="s.id"
        class="scene-list-row"
        :class="{ selected: s.id === selected }"
        draggable="true"
        @dragstart="drag = s.id"
        @dragend="drag = null"
        @dragover.prevent
        @drop.prevent="drop(s.id)"
      >
        <button
          :id="`scene-${s.id}`"
          class="scene-select"
          :aria-pressed="selected === s.id"
          @click="selected = s.id"
          @keydown.alt.up.prevent="move(s.id, -1)"
          @keydown.alt.down.prevent="move(s.id, 1)"
        >
          <span class="scene-rank">{{ i + 1 }}</span
          ><span
            ><strong>{{ s.name }}</strong
            ><small
              >{{
                pages.find((p) => p.value === s.page_id)?.label ||
                t("ui.pageMissing")
              }}
              · {{ status(s) }}</small
            ></span
          >
        </button>
        <div class="scene-order">
          <button
            class="quiet"
            :disabled="i === 0"
            :aria-label="t('ui.moveUp', { arg0: s.name })"
            @click="move(s.id, -1)"
          >
            <AppIcon name="up" :size="14" /></button
          ><button
            class="quiet"
            :disabled="i === w.sceneDraft.scenes.length - 1"
            :aria-label="t('ui.moveDown', { arg0: s.name })"
            @click="move(s.id, 1)"
          >
            <AppIcon name="down" :size="14" />
          </button>
        </div>
      </div>
      <p v-if="!w.sceneDraft.scenes.length" class="hint">
        {{ t("ui.addYourFirstSceneAndChoose") }}
      </p>
    </section>
    <section
      v-if="scene"
      class="panel scene-editor"
      :aria-label="t('ui.editScene', { arg0: scene.name })"
    >
      <div class="panel-heading">
        <h2>{{ t("ui.sceneSettings") }}</h2>
        <div class="button-row">
          <button
            :disabled="w.sceneDraft.scenes.length >= 200"
            @click="duplicate"
          >
            {{ t("ui.duplicate") }}</button
          ><button class="quiet" @click="deleting = scene">
            {{ t("ui.delete2") }}
          </button>
        </div>
      </div>
      <label
        >{{ t("ui.sceneName")
        }}<input v-model="scene.name" maxlength="120" @input="w.editScenes"
      /></label>
      <div class="scene-form-row">
        <label
          >{{ t("ui.targetPage")
          }}<AppSelect
            v-model="scene.page_id"
            :options="pages"
            :placeholder="t('ui.targetPageMissingSelectAnotherPage')"
            @change="w.editScenes" /></label
        ><AppSwitch
          v-model="scene.enabled"
          :label="t('ui.enableThisScene')"
          @update:model-value="w.editScenes"
        />
      </div>
      <label
        >{{ t("ui.trigger")
        }}<AppSelect
          :model-value="scene.trigger.type"
          :options="[
            { value: 'continuous', label: t('ui.whileConditionsMatch') },
            { value: 'scheduled', label: t('ui.atAScheduledTime') },
          ]"
          @update:model-value="trigger"
      /></label>
      <div v-if="scene.trigger.type === 'scheduled'" class="scene-schedule">
        <SceneDays
          v-model="scene.trigger.schedule.days"
          @update:model-value="w.editScenes"
        />
        <label
          >{{ t("ui.triggerTime")
          }}<input
            v-model="scene.trigger.schedule.time"
            type="time"
            @input="w.editScenes"
        /></label>
        <p class="hint">
          {{ t("ui.atTheScheduledTimeAdditionalConditions") }}
        </p>
      </div>
      <div class="scene-condition-heading">
        <h3>
          {{
            scene.trigger.type === "scheduled"
              ? t("ui.additionalConditions")
              : t("ui.conditions")
          }}
        </h3>
        <small>{{ conditionSize(scene.condition) }} / 128</small>
      </div>
      <SceneConditionEditor
        v-model="scene.condition"
        :total="conditionSize(scene.condition)"
        @update:model-value="w.editScenes"
      />
      <p class="scene-summary">{{ conditionSummary(scene.condition) }}</p>
      <div class="button-row">
        <button :disabled="!w.desktop || checking" @click="check">
          {{
            checking ? t("ui.checking") : t("ui.checkCurrentConditions")
          }}</button
        ><span role="status" class="hint">{{ checkResult }}</span>
      </div>
      <details v-if="scene.trigger.type === 'continuous'">
        <summary>{{ t("ui.switchDelay") }}</summary>
        <div class="scene-form-row">
          <label
            >{{ t("ui.enterAfterMatchingContinuouslyMs")
            }}<input
              v-model.number="scene.enter_delay_ms"
              type="number"
              min="0"
              max="60000"
              step="100"
              @input="w.editScenes" /></label
          ><label
            >{{ t("ui.leaveAfterConditionsStopMatchingMs")
            }}<input
              v-model.number="scene.exit_delay_ms"
              type="number"
              min="0"
              max="60000"
              step="100"
              @input="w.editScenes"
          /></label>
        </div>
      </details>
      <p v-if="w.sceneSaveError" class="error-text" role="alert">
        {{ w.sceneSaveError }}
      </p>
    </section>
    <section v-else class="panel scene-empty">
      <AppIcon name="clock" :size="34" />
      <h2>{{ t("ui.letPagesFollowYourWork") }}</h2>
      <p>{{ t("ui.switchAutomaticallyByRunningAppForeground") }}</p>
      <button class="primary" @click="add">{{ t("ui.createScene") }}</button>
    </section>
  </div>
  <AppDialog
    v-if="deleting"
    :title="t('ui.deleteScene')"
    @close="deleting = null"
    ><p>{{ t("phrases.deleteScene", { name: deleting.name }) }}</p>
    <div class="dialog-actions">
      <button data-cancel @click="deleting = null">{{ t("ui.cancel") }}</button
      ><button
        class="danger"
        @click="
          w.sceneDraft.scenes = w.sceneDraft.scenes.filter(
            (s) => s.id !== deleting?.id,
          );
          deleting = null;
          w.editScenes();
        "
      >
        {{ t("ui.deleteScene") }}
      </button>
    </div></AppDialog
  >
</template>

<style>
.scene-global {
  margin-bottom: 20px;
  display: grid;
  gap: 12px;
}
.scene-layout .hint,
.scene-global .hint,
.scene-condition .hint {
  font-size: 12px;
  line-height: 1.7;
  color: var(--subtle);
  font-weight: 400;
  margin: 0;
}
.scene-editor label,
.scene-global-fields label {
  margin: 0;
}
.scene-editor label > input,
.scene-global-fields label > input {
  margin-top: 6px;
}
.scene-global details > summary {
  font-size: 12px;
  color: var(--subtle);
  cursor: pointer;
}
.scene-global details {
  border-top: 1px solid var(--border);
  padding-top: 12px;
}
.scene-editor .button-row {
  flex-wrap: wrap;
}
.scene-layout {
  display: grid;
  grid-template-columns: minmax(220px, 290px) minmax(0, 1fr);
  gap: 20px;
  align-items: start;
}
.scene-list,
.scene-editor {
  min-width: 0;
}
.scene-list-row {
  display: flex;
  align-items: center;
  border: 1px solid var(--border);
  border-radius: 9px;
  margin-top: 8px;
}
.scene-list-row.selected {
  border-color: var(--primary);
  background: var(--primary-soft);
}
.scene-select {
  flex: 1;
  min-width: 0;
  display: flex;
  gap: 10px;
  text-align: left;
  border: 0;
  background: transparent;
  padding: 12px 8px;
  white-space: normal;
}
.scene-select > span:last-child {
  min-width: 0;
  display: grid;
  gap: 5px;
}
.scene-select strong,
.scene-select small {
  overflow-wrap: anywhere;
}
.scene-select small,
.scene-summary {
  color: var(--subtle);
  font-size: 12px;
}
.scene-rank {
  color: var(--primary-text);
  font-variant-numeric: tabular-nums;
}
.scene-order {
  display: grid;
  padding-right: 3px;
}
.scene-order button {
  padding: 4px;
  min-height: 25px;
}
.scene-editor {
  display: grid;
  gap: 16px;
}
.scene-editor label {
  min-width: 0;
}
.scene-form-row,
.scene-global-fields {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
  gap: 14px;
  align-items: end;
}
.scene-global-fields {
  padding-top: 14px;
}
.scene-condition {
  display: grid;
  gap: 12px;
  min-width: 0;
}
.scene-child {
  display: flex;
  gap: 6px;
  align-items: start;
  border-left: 2px solid var(--border);
  padding-left: 10px;
}
.scene-child > .scene-condition {
  flex: 1;
}
.scene-condition > .scene-condition {
  padding-left: 12px;
  border-left: 2px solid var(--border);
}
.scene-condition-heading,
.scene-time-row {
  display: flex;
  gap: 14px;
  justify-content: space-between;
  align-items: center;
}
.scene-condition-heading h3 {
  margin: 0;
}
.scene-time-row {
  justify-content: start;
  flex-wrap: wrap;
}
.scene-days,
.scene-schedule {
  display: grid;
  gap: 12px;
}
.scene-weekdays {
  display: flex;
  gap: 12px;
  flex-wrap: wrap;
}
.scene-weekdays label {
  display: flex;
  flex-direction: row;
  align-items: center;
  gap: 5px;
}
.scene-weekdays input {
  width: auto;
}
.scene-summary {
  overflow-wrap: anywhere;
  margin: 0;
}
.scene-programs {
  display: grid;
  gap: 6px;
  max-height: 320px;
  overflow: auto;
  margin: 14px 0;
}
.scene-programs button {
  display: grid;
  gap: 4px;
  text-align: left;
  white-space: normal;
}
.scene-programs small {
  overflow-wrap: anywhere;
  color: var(--subtle);
}
.scene-error {
  color: var(--error);
  margin-bottom: 16px;
}
.scene-empty {
  display: grid;
  justify-items: start;
  gap: 12px;
  color: var(--subtle);
}
@media (max-width: 1000px) {
  .scene-layout {
    grid-template-columns: minmax(190px, 230px) minmax(0, 1fr);
    gap: 12px;
  }
  .scene-editor {
    padding: 16px;
  }
  .scene-child {
    padding-left: 5px;
    gap: 3px;
  }
}
@media (max-width: 780px) {
  .scene-layout {
    grid-template-columns: 1fr;
  }
  .scene-list {
    max-height: 270px;
    overflow-y: auto;
  }
}
</style>
