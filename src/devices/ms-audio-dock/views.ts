import { t } from "../../i18n";
import { markRaw } from "vue";
import type { DeviceViews } from "../types";
import ConnectionDetails from "./ConnectionDetails.vue";
import DeviceControls from "./DeviceControls.vue";
import poster from "../../assets/devices/ms-audio-dock/audio-dock-poster.png";

export const audioDockViews: DeviceViews = {
  details: markRaw(ConnectionDetails),
  icon: "speaker",
  poster,
  controls: markRaw(DeviceControls),
  tests: false,
  get controlLabel() {
    return t("ui.audioSettings");
  },
  sections: [
    {
      id: "device:equalizer",
      icon: "equalizer",
      get label() {
        return t("ui.equalizer");
      },
    },
    {
      id: "device:teams",
      icon: "keyboard",
      get label() {
        return t("ui.teamsKey");
      },
    },
  ],
};
