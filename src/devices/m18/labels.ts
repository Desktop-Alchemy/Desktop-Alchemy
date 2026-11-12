import { t } from "../../i18n";
export const featureLabels: Record<string, string> = {
  get device_control() {
    return t("ui.deviceConnectionAndRefresh");
  },
  get key_input() {
    return t("ui.18KeyInput");
  },
  get key_images() {
    return t("ui.15KeyStaticDisplay");
  },
  get temporary_background() {
    return t("ui.temporaryBackground");
  },
  get startup_background() {
    return t("ui.bootBackground");
  },
  get screen_brightness() {
    return t("ui.screenBrightness");
  },
  get led_brightness() {
    return t("ui.ledBrightness2");
  },
  get led_color() {
    return t("ui.overallLedColor");
  },
  get individual_leds() {
    return t("ui.24IndependentLedColors");
  },
  get key_gif() {
    return t("ui.keyGif");
  },
  get reconnect() {
    return t("ui.disconnectRecovery");
  },
};
