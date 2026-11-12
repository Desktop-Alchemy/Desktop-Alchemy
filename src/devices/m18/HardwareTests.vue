<script setup lang="ts">
import { t } from "../../i18n";

import { computed, ref } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import type { HardwareCommand } from "../../bindings";
import { useDeviceContext } from "../context";
import DeckGrid from "./DeckGrid.vue";
import DeviceControls from "./DeviceControls.vue";
import CapabilityRecords from "./CapabilityRecords.vue";
import AppSelect from "../../components/AppSelect.vue";
import AppIcon from "../../components/AppIcon.vue";

const emit = defineEmits<{
  confirm: [request: { title: string; description: string; run: () => void }];
}>();
const w = useDeviceContext();
const imageAsset = ref("");
const gifAsset = ref("");
const imageName = ref("");
const gifName = ref("");
const bulkKeys = ref("1,2,3");
const dataInput = ref("");
const ledColors = ref(Array.from({ length: 24 }, () => "#5966F3"));
const returnPage = ref("");
const activePage = computed(
  () =>
    w.state.config.pages.find((p) => p.id === w.state.active_page) ?? w.page,
);
const disabled = (name: string) =>
  !w.ready || w.device?.capabilities[name]?.available === false;
const selectedIsPhysical = computed(() => w.selectedKey > 15);
const canRestorePage = computed(() =>
  w.state.config.pages.some((p) => p.id === returnPage.value),
);

async function chooseAsset(gif = false) {
  if (!w.desktop) return;
  const path = await open({
    multiple: false,
    filters: [
      {
        name: gif ? "GIF" : t("ui.images"),
        extensions: gif ? ["gif"] : ["png", "jpg", "jpeg"],
      },
    ],
  });
  if (typeof path !== "string") return;
  const id = await w.call<string>("import_asset", { path });
  if (!id) return;
  const name = path.split(/[\\/]/).at(-1) ?? path;
  if (gif) {
    gifAsset.value = id;
    gifName.value = name;
  } else {
    imageAsset.value = id;
    imageName.value = name;
  }
}
async function send(command: HardwareCommand) {
  await w.hardware(command);
}
async function sendImageData(background = false) {
  const data = await w.call<string>("preview_asset", {
    assetId: imageAsset.value,
  });
  if (data)
    await send(
      background
        ? { type: "background_data", data, persistent: false }
        : { type: "key_data", key: w.selectedKey, data },
    );
}
function writeStartup() {
  emit("confirm", {
    title: t("ui.writeDeviceBootBackground"),
    description: t("ui.thisImagePersistsAfterPowerIs"),
    run: () => {
      void w.hardware(
        { type: "background", asset_id: imageAsset.value, persistent: true },
        true,
      );
    },
  });
}
async function showCalibration() {
  const previous = w.state.active_page;
  const result = await w.call<null>(
    "show_brightness_test",
    {},
    t("ui.testImageQueued"),
  );
  if (result !== undefined && previous !== "builtin-brightness-test-v1")
    returnPage.value = previous;
}
async function exportReport() {
  const path = await w.call<string>("export_report");
  if (path) w.toast = t("ui.acceptanceReportSaved", { arg0: path });
}
function sendAllLeds() {
  const colors = ledColors.value.map(
    (hex) =>
      [
        parseInt(hex.slice(1, 3), 16),
        parseInt(hex.slice(3, 5), 16),
        parseInt(hex.slice(5, 7), 16),
      ] as [number, number, number],
  );
  void send({ type: "led_colors", colors });
}
</script>

<template>
  <div class="page-heading">
    <div>
      <p class="eyebrow">HARDWARE TESTS</p>
      <h1>{{ t("ui.hardwareTests") }}</h1>
    </div>
    <button
      class="secondary"
      :disabled="!w.desktop || w.pending > 0"
      @click="exportReport"
    >
      {{ t("ui.exportAcceptanceReport") }} <AppIcon name="export" :size="16" />
    </button>
  </div>
  <div class="notice">
    {{ t("ui.testsChangeDeviceImagesOrLighting") }}
  </div>
  <div class="device-layout">
    <section class="panel live-panel">
      <div class="panel-heading">
        <div>
          <h2>{{ t("ui.keysAndDisplay2") }}</h2>
          <p>{{ t("ui.currentPage") }}{{ activePage.name }}</p>
        </div>
        <span class="badge">{{
          w.state.pressed.length
            ? t("ui.pressed", { arg0: w.state.pressed.join(", ") })
            : t("ui.waitingForKeyInput")
        }}</span>
      </div>
      <DeckGrid
        :page="activePage"
        :selected="w.selectedKey"
        :pressed="w.state.pressed"
        @select="w.selectedKey = $event"
      />
      <div class="deck-caption">
        <span>{{ t("ui.configurationPreview") }}</span
        ><span
          >{{ t("ui.selected2")
          }}{{
            selectedIsPhysical ? t("ui.physicalKey2") : t("ui.displayKey2")
          }}
          {{ w.selectedKey }}</span
        >
      </div>
      <div class="button-row wrap">
        <button
          class="secondary"
          :disabled="!w.ready || w.pending > 0"
          @click="w.call('activate_page', { pageId: w.state.active_page })"
        >
          {{ t("ui.restoreCurrentPageDisplay") }}
        </button>
        <button
          class="secondary"
          :disabled="!w.ready"
          @click="send({ type: 'refresh' })"
        >
          {{ t("ui.refreshDisplay") }}
        </button>
        <button
          class="secondary"
          :disabled="!w.ready"
          @click="send({ type: 'firmware' })"
        >
          {{ t("ui.queryFirmware") }}
        </button>
      </div>
    </section>
    <div class="test-control-stack">
      <section class="panel">
        <div class="panel-heading">
          <h2>{{ t("ui.brightnessTestPattern") }}</h2>
        </div>
        <div class="button-row wrap">
          <button
            class="secondary"
            :disabled="!w.ready || w.dirty || w.pending > 0"
            @click="showCalibration"
          >
            {{ t("ui.openBrightnessTestPage") }}
          </button>
          <button
            class="secondary"
            :disabled="!w.ready || !canRestorePage || w.pending > 0"
            @click="w.call('activate_page', { pageId: returnPage })"
          >
            {{ t("ui.returnToOriginalPage") }}
          </button>
        </div>
        <p v-if="w.dirty" class="helper">
          {{ w.saveStatus }}{{ t("ui.testsCanStartAfterSavingFinishes") }}
        </p>
        <p class="helper">
          {{ t("ui.grayscaleSolidColorsGradientsAndCheckerboards") }}
        </p>
      </section>
      <DeviceControls />
    </div>
  </div>
  <section class="panel">
    <div class="panel-heading">
      <h2>{{ t("ui.imageInterfaces") }}</h2>
    </div>
    <div class="hardware-tools">
      <div>
        <h3>{{ t("ui.staticImagesAndBackgrounds") }}</h3>
        <button class="secondary" :disabled="!w.desktop" @click="chooseAsset()">
          {{ t("ui.choosePngJpeg") }}
        </button>
        <p class="helper asset-name">
          {{ imageName || t("ui.noImageSelected") }}
        </p>
        <div class="button-row wrap">
          <button
            :disabled="
              disabled('key_images') || !imageAsset || selectedIsPhysical
            "
            @click="
              send({
                type: 'key_image',
                key: w.selectedKey,
                asset_id: imageAsset,
              })
            "
          >
            {{ t("ui.setSelectedKey") }}
          </button>
          <button
            :disabled="
              disabled('key_images') || !imageAsset || selectedIsPhysical
            "
            @click="sendImageData()"
          >
            {{ t("ui.base64KeyImage") }}
          </button>
          <button
            :disabled="disabled('temporary_background') || !imageAsset"
            @click="
              send({
                type: 'background',
                asset_id: imageAsset,
                persistent: false,
              })
            "
          >
            {{ t("ui.temporaryBackground") }}
          </button>
          <button
            :disabled="disabled('temporary_background') || !imageAsset"
            @click="sendImageData(true)"
          >
            {{ t("ui.base64TemporaryBackground") }}
          </button>
          <button
            :disabled="disabled('startup_background') || !imageAsset"
            @click="writeStartup"
          >
            {{ t("ui.writeBootBackground") }}
          </button>
        </div>
        <label
          >{{ t("ui.keyNumbersForBatchImage")
          }}<input v-model="bulkKeys" placeholder="1,2,3"
        /></label>
        <button
          :disabled="disabled('key_images') || !imageAsset"
          @click="
            send({
              type: 'bulk_image',
              keys: bulkKeys.split(',').map(Number),
              asset_id: imageAsset,
            })
          "
        >
          {{ t("ui.setSpecifiedKeys") }}
        </button>
        <div class="button-row wrap">
          <button
            :disabled="disabled('key_images') || selectedIsPhysical"
            @click="send({ type: 'clear_key', key: w.selectedKey })"
          >
            {{ t("ui.clearSelectedKey") }}
          </button>
          <button
            :disabled="disabled('key_images')"
            @click="send({ type: 'clear_all' })"
          >
            {{ t("ui.clearAllKeys") }}
          </button>
        </div>
        <details>
          <summary>{{ t("ui.enterBase64ImageDirectly") }}</summary>
          <label
            >{{ t("ui.imageData")
            }}<textarea
              v-model="dataInput"
              rows="3"
              placeholder="data:image/png;base64,…"
            />
          </label>
          <button
            :disabled="
              disabled('key_images') || !dataInput || selectedIsPhysical
            "
            @click="
              send({ type: 'key_data', key: w.selectedKey, data: dataInput })
            "
          >
            {{ t("ui.sendToSelectedKey") }}
          </button>
        </details>
      </div>
      <div>
        <h3>{{ t("ui.keyGif") }}</h3>
        <button
          class="secondary"
          :disabled="!w.desktop"
          @click="chooseAsset(true)"
        >
          {{ t("ui.chooseGifFile") }}
        </button>
        <p class="helper asset-name">{{ gifName || t("ui.noGifSelected") }}</p>
        <div class="button-row wrap">
          <button
            :disabled="disabled('key_gif') || !gifAsset || selectedIsPhysical"
            @click="
              send({ type: 'key_gif', key: w.selectedKey, asset_id: gifAsset })
            "
          >
            {{ t("ui.assignGif") }}
          </button>
          <button
            :disabled="disabled('key_gif') || selectedIsPhysical"
            @click="send({ type: 'clear_gif', key: w.selectedKey })"
          >
            {{ t("ui.clearCurrentGif") }}
          </button>
          <button
            :disabled="disabled('key_gif')"
            @click="send({ type: 'start_gif' })"
          >
            {{ t("ui.play") }}
          </button>
          <button
            :disabled="disabled('key_gif')"
            @click="send({ type: 'stop_gif' })"
          >
            {{ t("ui.stop") }}
          </button>
          <button
            :disabled="disabled('key_gif')"
            @click="send({ type: 'gif_status' })"
          >
            {{ t("ui.queryLoopStatus") }}
          </button>
        </div>
        <p class="mono helper">
          {{ w.state.gif_status ?? t("ui.playbackStatusNotQueried") }}
        </p>
      </div>
    </div>
  </section>
  <section class="panel">
    <div class="panel-heading">
      <h2>{{ t("ui.perLedColors") }}</h2>
      <span class="badge">24 RGB</span>
    </div>
    <div class="led-grid">
      <input
        v-for="(_, n) in ledColors"
        :key="n"
        v-model="ledColors[n]"
        type="color"
        :aria-label="`LED ${n + 1}`"
      />
    </div>
    <button
      class="secondary"
      :disabled="disabled('individual_leds')"
      @click="sendAllLeds"
    >
      {{ t("ui.sendPerLedColors") }}
    </button>
  </section>
  <CapabilityRecords />
</template>
