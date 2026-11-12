<script setup lang="ts">
import UiFailure from "../../components/UiFailure.vue";
import { t } from "../../i18n";

import { computed, ref, watch } from "vue";
import { useAudioDock } from "./useAudioDock";
import AppDialog from "../../components/AppDialog.vue";
import AppSelect from "../../components/AppSelect.vue";
import AppIcon from "../../components/AppIcon.vue";
import type {
  AudioDockSettingsUpdate,
  AudioDockEqualizerPreset,
} from "../../bindings";
const emit = defineEmits<{ applied: [message: string] }>();
const w = useAudioDock();
const settings = computed(() => w.audioDockState?.settings);
const enabled = computed(
  () =>
    w.ready &&
    w.desktop &&
    !w.audioDockPending &&
    w.audioDockState?.connection === "ready",
);
const active = computed(() =>
  settings.value?.equalizer_presets.find(
    (p) => p.id === settings.value?.active_equalizer,
  ),
);
const factory = computed(
  () => settings.value?.equalizer_presets.filter((p) => p.factory) ?? [],
);
const custom = computed(
  () => settings.value?.equalizer_presets.filter((p) => !p.factory) ?? [],
);
const factoryNames: Record<number, string> = {
  get 0() {
    return t("ui.flat");
  },
  get 1() {
    return t("ui.classical");
  },
  get 2() {
    return t("ui.jazz");
  },
  get 3() {
    return t("ui.pop");
  },
  get 4() {
    return t("ui.rock");
  },
};
const bands = computed(() => [
  t("ui.bass"),
  t("ui.lowMids"),
  t("ui.mids"),
  t("ui.highMids"),
  t("ui.treble"),
]);
const gains = ref([0, 0, 0, 0, 0]);
const dirty = ref(false);
const saving = ref(false);
const destination = ref("new");
const name = ref("");
const creating = ref(false);
const modalGains = ref([0, 0, 0, 0, 0]);
const deleting = ref<AudioDockEqualizerPreset | null>(null);
const switching = ref<AudioDockEqualizerPreset | null>(null);
const free = computed(() => settings.value?.free_equalizer_slots.length ?? 0);
const destinationOptions = computed(() => [
  {
    value: "new",
    get label() {
      return t("ui.newCustomPreset");
    },
    disabled: !free.value,
  },
  ...custom.value.map((p) => ({
    value: String(p.id),
    get label() {
      return t("ui.replace", { arg0: p.name });
    },
  })),
]);
const nameError = computed(() => {
  const value = name.value.trim();
  if (!value) return t("ui.enterAPresetName");
  if (
    new TextEncoder().encode(value).length > 19 ||
    /[\u0000-\u001f\u007f-\u009f]/.test(value)
  )
    return t("ui.theNameIsTooLongOr");
  if (
    settings.value?.equalizer_presets.some(
      (p) => p.name === value && String(p.id) !== destination.value,
    )
  )
    return t("ui.thisNameIsAlreadyInUse");
  return "";
});
watch(
  settings,
  () => {
    if (!dirty.value && active.value) gains.value = [...active.value.gains];
  },
  { immediate: true },
);
watch(
  () => w.currentProfile?.device_id,
  () => {
    dirty.value = false;
    saving.value = false;
    deleting.value = null;
    switching.value = null;
    gains.value = [...(active.value?.gains ?? [0, 0, 0, 0, 0])];
  },
);
watch(destination, (value) => {
  name.value =
    value === "new"
      ? ""
      : (custom.value.find((p) => p.id === Number(value))?.name ?? "");
});
async function apply(
  update: AudioDockSettingsUpdate,
  message = t("ui.applied"),
) {
  const id = w.currentProfile?.device_id;
  const value = await w.updateAudioDock(update);
  if (id !== w.currentProfile?.device_id || value?.connection !== "ready")
    return false;
  emit("applied", message);
  return true;
}
function editBand(index: number, event: Event) {
  gains.value[index] = Number((event.target as HTMLInputElement).value);
  dirty.value = gains.value.some((v, i) => v !== active.value?.gains[i]);
}
function reset() {
  gains.value = [...(active.value?.gains ?? [0, 0, 0, 0, 0])];
  dirty.value = false;
}
async function selectPreset(preset: AudioDockEqualizerPreset) {
  if (dirty.value) {
    switching.value = preset;
    return;
  }
  if (await apply({ type: "select_equalizer", id: preset.id })) reset();
}
async function confirmSwitch() {
  const preset = switching.value;
  if (preset && (await apply({ type: "select_equalizer", id: preset.id }))) {
    reset();
    switching.value = null;
  }
}
function openSave(create = false) {
  creating.value = create;
  destination.value =
    !create && active.value && !active.value.factory
      ? String(active.value.id)
      : "new";
  name.value = destination.value === "new" ? "" : (active.value?.name ?? "");
  modalGains.value = [...gains.value];
  saving.value = true;
}
async function save() {
  if (
    await apply(
      {
        type: "save_equalizer",
        id: destination.value === "new" ? null : Number(destination.value),
        name: name.value.trim(),
        gains: modalGains.value as [number, number, number, number, number],
      },
      t("ui.presetSavedAndApplied"),
    )
  ) {
    reset();
    saving.value = false;
  }
}
async function remove() {
  const preset = deleting.value;
  if (
    preset &&
    (await apply(
      { type: "delete_equalizer", id: preset.id, name: preset.name },
      t("ui.presetDeleted"),
    ))
  ) {
    reset();
    deleting.value = null;
  }
}
</script>
<template>
  <div class="dock-equalizer">
    <section
      class="panel dock-preset-panel"
      :aria-label="t('ui.equalizerPresets')"
    >
      <div class="dock-section-heading">
        <div>
          <h2>{{ t("ui.soundPresets") }}</h2>
          <p>{{ t("ui.appliesImmediatelyWhenSelected") }}</p>
        </div>
        <button :disabled="!enabled || !free" @click="openSave(true)">
          <AppIcon name="plus" :size="16" />{{ t("ui.newPreset") }}
        </button>
      </div>
      <div
        class="dock-preset-grid"
        role="group"
        :aria-label="t('ui.factoryPresets')"
      >
        <button
          v-for="preset in factory"
          :key="preset.id"
          class="dock-preset"
          :aria-pressed="active?.id === preset.id"
          :disabled="!enabled"
          @click="selectPreset(preset)"
        >
          <span class="dock-preset-name">{{
            factoryNames[preset.id] ?? preset.name
          }}</span
          ><small>{{ preset.name }}</small
          ><AppIcon v-if="active?.id === preset.id" name="check" :size="16" />
        </button>
      </div>
      <div class="dock-custom-heading">
        <h3>{{ t("ui.custom") }}</h3>
        <span class="helper">{{ custom.length }} / 11</span>
      </div>
      <div
        v-if="custom.length"
        class="dock-custom-presets"
        role="group"
        :aria-label="t('ui.customPresets')"
      >
        <button
          v-for="preset in custom"
          :key="preset.id"
          class="dock-custom-preset"
          :aria-pressed="active?.id === preset.id"
          :disabled="!enabled"
          @click="selectPreset(preset)"
        >
          <AppIcon name="equalizer" :size="16" /><span>{{ preset.name }}</span
          ><AppIcon v-if="active?.id === preset.id" name="check" :size="16" />
        </button>
      </div>
      <p v-else class="dock-empty-presets">
        {{ t("ui.noCustomPresetsYetAdjustA") }}
      </p>
      <p v-if="!free && enabled" class="helper">
        {{ t("ui.presetSlotsAreFullReplaceOr") }}
      </p>
    </section>
    <section class="panel dock-band-panel" :aria-label="t('ui.equalizerBands')">
      <div class="dock-section-heading">
        <div>
          <h2>{{ t("ui.adjustSound") }}</h2>
          <p>
            {{ active?.name ?? t("ui.currentPresetNotRead")
            }}<span v-if="dirty" class="dock-draft-label">{{
              t("ui.unsavedAdjustments")
            }}</span>
          </p>
        </div>
        <button
          v-if="active && !active.factory"
          class="quiet"
          :disabled="!enabled || dirty"
          @click="deleting = { ...active }"
        >
          <AppIcon name="trash" :size="16" />{{ t("ui.deleteThisPreset") }}
        </button>
      </div>
      <div class="dock-eq-bands">
        <label v-for="(band, index) in bands" :key="band"
          ><span>{{ band }}</span
          ><output>{{
            active
              ? (gains[index] > 0 ? "+" : "") + gains[index] + " dB"
              : t("ui.notRead")
          }}</output
          ><span class="dock-eq-rail"
            ><span class="dock-eq-zero" aria-hidden="true"></span
            ><input
              type="range"
              min="-12"
              max="12"
              step="1"
              :value="gains[index]"
              :aria-label="t('ui.gain', { arg0: band })"
              :disabled="!enabled || !active"
              :aria-description="t('ui.range12To12DbSave')"
              @input="editBand(index, $event)" /></span
        ></label>
      </div>
      <div class="dock-eq-footer">
        <p class="helper">
          {{
            dirty
              ? t("ui.adjustmentsHaveNotBeenAppliedTo")
              : t("ui.dragTheBandsToAdjustThe")
          }}
        </p>
        <div class="button-row">
          <button v-if="dirty" :disabled="!enabled" @click="reset">
            {{ t("ui.resetAdjustments") }}</button
          ><button
            class="primary"
            :disabled="!enabled || !dirty"
            @click="openSave()"
          >
            {{ t("ui.saveAndApply") }}
          </button>
        </div>
      </div>
    </section>
    <AppDialog
      v-if="saving"
      :title="creating ? t('ui.newCustomEqualizer') : t('ui.saveEqualizer')"
      @close="!w.audioDockPending && (saving = false)"
    >
      <label v-if="!creating"
        >{{ t("ui.saveTo")
        }}<AppSelect
          v-model="destination"
          :options="destinationOptions"
          :aria-label="t('ui.equalizerSaveDestination')"
          :disabled="w.audioDockPending"
      /></label>
      <label
        >{{ t("ui.presetName")
        }}<input
          v-model="name"
          :readonly="destination !== 'new'"
          :disabled="w.audioDockPending"
          :placeholder="t('ui.forExampleEverydaySound')"
          :aria-invalid="!!name && !!nameError"
          aria-describedby="dock-preset-name-help"
      /></label>
      <p id="dock-preset-name-help" class="helper">
        {{ t("ui.useAShortNameUpTo") }}
      </p>
      <div class="dock-modal-bands">
        <label v-for="(band, index) in bands" :key="band"
          >{{ band
          }}<input
            v-model.number="modalGains[index]"
            type="range"
            min="-12"
            max="12"
            step="1"
            :aria-label="t('ui.presetGain', { arg0: band })"
            :disabled="w.audioDockPending"
          /><output>{{ modalGains[index] }} dB</output></label
        >
      </div>
      <p class="helper">
        {{
          destination === "new"
            ? t("ui.saveAsANewPresetAnd")
            : t("ui.replaceTheBandsInThisCustom")
        }}
      </p>
      <p v-if="name && nameError" class="error-text" role="alert">
        {{ nameError }}
      </p>
      <UiFailure
        v-if="w.audioDockError"
        :summary="w.audioDockError"
        :detail="w.audioDockErrorDetail"
      />
      <div class="button-row dialog-actions">
        <button
          data-cancel
          :disabled="w.audioDockPending"
          @click="saving = false"
        >
          {{ t("ui.cancel") }}</button
        ><button
          class="primary"
          :disabled="
            !enabled || !!nameError || (destination === 'new' && !free)
          "
          @click="save"
        >
          {{ t("ui.saveAndApply2") }}
        </button>
      </div>
    </AppDialog>
    <AppDialog
      v-if="deleting"
      :title="t('ui.deleteCustomEqualizer')"
      @close="!w.audioDockPending && (deleting = null)"
    >
      <p>{{ t("phrases.deletePreset", { name: deleting.name }) }}</p>
      <p class="helper">{{ t("ui.anActivePresetWillSwitchBack") }}</p>
      <UiFailure
        v-if="w.audioDockError"
        :summary="w.audioDockError"
        :detail="w.audioDockErrorDetail"
      />
      <div class="button-row dialog-actions">
        <button
          data-cancel
          :disabled="w.audioDockPending"
          @click="deleting = null"
        >
          {{ t("ui.cancel") }}</button
        ><button class="danger" :disabled="!enabled" @click="remove">
          {{ t("ui.deletePreset") }}
        </button>
      </div>
    </AppDialog>
    <AppDialog
      v-if="switching"
      :title="t('ui.discardBandAdjustments')"
      @close="!w.audioDockPending && (switching = null)"
    >
      <p>{{ t("phrases.switchPreset", { name: switching.name }) }}</p>
      <UiFailure
        v-if="w.audioDockError"
        :summary="w.audioDockError"
        :detail="w.audioDockErrorDetail"
      />
      <div class="button-row dialog-actions">
        <button
          data-cancel
          :disabled="w.audioDockPending"
          @click="switching = null"
        >
          {{ t("ui.keepEditing") }}</button
        ><button class="primary" :disabled="!enabled" @click="confirmSwitch">
          {{ t("ui.discardAndSwitch") }}
        </button>
      </div>
    </AppDialog>
  </div>
</template>
