import { t } from "./i18n";
export const statusLabels: Record<string, string> = {
  get queued() {
    return t("ui.queued");
  },
  get sent() {
    return t("ui.sent");
  },
  get acknowledged() {
    return t("ui.accepted");
  },
  get failed() {
    return t("ui.failed");
  },
  get cancelled() {
    return t("ui.cancelled");
  },
  get superseded() {
    return t("ui.merged");
  },
  get verified() {
    return t("ui.hardwareVerified");
  },
  get unverified() {
    return t("ui.notVerified");
  },
  get unsupported() {
    return t("ui.unsupported");
  },
  get running() {
    return t("ui.running");
  },
  get stopped() {
    return t("ui.disabled");
  },
  get starting() {
    return t("ui.starting");
  },
  get stopping() {
    return t("ui.stopping");
  },
  get error() {
    return t("ui.error");
  },
};

export const commandLabels: Record<string, string> = {
  get screen_brightness() {
    return t("ui.adjustScreenBrightness");
  },
  get led_brightness() {
    return t("ui.adjustLedBrightness");
  },
  get led_color() {
    return t("ui.setLedColor");
  },
  get led_colors() {
    return t("ui.setPerLedColors");
  },
  get reset_led() {
    return t("ui.restoreDefaultLedEffect");
  },
  get key_image() {
    return t("ui.setKeyImage");
  },
  get key_data() {
    return t("ui.setKeyImage");
  },
  get bulk_image() {
    return t("ui.setKeyImagesInBulk");
  },
  get background() {
    return t("ui.setBackgroundImage");
  },
  get background_data() {
    return t("ui.setBackgroundImage");
  },
  get clear_key() {
    return t("ui.clearKeyImage");
  },
  get clear_all() {
    return t("ui.clearAllKeyImages");
  },
  get refresh() {
    return t("ui.refreshDisplay");
  },
  get key_gif() {
    return t("ui.setKeyGif");
  },
  get clear_gif() {
    return t("ui.clearKeyGif");
  },
  get start_gif() {
    return t("ui.playGif");
  },
  get stop_gif() {
    return t("ui.pauseGif");
  },
  get gif_status() {
    return t("ui.queryGifStatus");
  },
  get firmware() {
    return t("ui.queryFirmwareVersion");
  },
};

export const logLevelLabels: Record<string, string> = {
  get info() {
    return t("ui.info");
  },
  get warning() {
    return t("ui.warning");
  },
  get error() {
    return t("ui.error2");
  },
  get input() {
    return t("ui.keyInput");
  },
  get sdk() {
    return t("ui.devices");
  },
  get plugin() {
    return t("ui.plugin");
  },
};
