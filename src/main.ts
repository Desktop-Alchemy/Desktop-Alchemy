import { createApp } from "vue";
import { createPinia } from "pinia";
import App from "./App.vue";
import "@fontsource-variable/noto-sans-sc";
import "./fonts.css";
import "./style.css";
import { i18n, observeDocumentLanguage } from "./i18n";
import { initializeLanguage } from "./store";
void initializeLanguage().then(() => {
  observeDocumentLanguage();
  createApp(App).use(createPinia()).use(i18n).mount("#app");
});
