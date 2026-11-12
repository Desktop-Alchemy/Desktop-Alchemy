import { markRaw } from "vue";
import type { DeviceViews } from "../types";
import DevicePreview from "./DevicePreview.vue";
import DeviceControls from "./DeviceControls.vue";
import HardwareTests from "./HardwareTests.vue";
import poster from "../../assets/devices/m18/m18-poster.png";

export const m18Views: DeviceViews = {
  preview: markRaw(DevicePreview),
  poster,
  controls: markRaw(DeviceControls),
  tests: markRaw(HardwareTests),
};
