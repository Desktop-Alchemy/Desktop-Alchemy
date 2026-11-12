import { createApp } from "vue";
import TrayPanel from "./components/TrayPanel.vue";
import "@fontsource-variable/noto-sans-sc";
import "./fonts.css";
import "./style.css";

import { i18n, observeDocumentLanguage } from "./i18n";
import { initializeLanguage } from "./store";
void initializeLanguage().then(() => {
  observeDocumentLanguage(true);
  createApp(TrayPanel).use(i18n).mount("#app");
});
