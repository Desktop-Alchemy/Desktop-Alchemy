<script setup lang="ts">
import UiFailure from "./components/UiFailure.vue";
import DraftRecovery from "./components/DraftRecovery.vue";
import { language, t, errorText } from "./i18n";

import { ref, computed, watch, onMounted, nextTick } from "vue";
import { open } from "@tauri-apps/plugin-dialog";
import { useDesktopAlchemy } from "./store";
import DeviceScope from "./devices/DeviceScope.vue";
import GenericBindings from "./devices/GenericBindings.vue";
import SceneAutomation from "./components/SceneAutomation.vue";
import { viewsFor } from "./devices";
import WidgetSourceSettings from "./components/WidgetSourceSettings.vue";
import ContentPicker from "./components/ContentPicker.vue";
import LaunchTargetEditor from "./components/LaunchTargetEditor.vue";
import { programTarget } from "./launch";
import { basicContentMode, isGifPattern } from "./content";
import type {
  DeviceDescriptor,
  DeviceState,
  KeyAction,
  LanguagePreference,
  TextPosition,
} from "./bindings";
import { statusLabels, commandLabels, logLevelLabels } from "./labels";

import PluginControls from "./components/PluginControls.vue";
import AppDialog from "./components/AppDialog.vue";
import ColorField from "./components/ColorField.vue";
import type { ImageRef } from "./bindings";
import LibraryBrowser from "./components/LibraryBrowser.vue";
import ApplicationsBrowser from "./components/ApplicationsBrowser.vue";
import BackupControls from "./components/BackupControls.vue";
import ToastStack from "./components/ToastStack.vue";
import AppSelect from "./components/AppSelect.vue";
import AppIcon from "./components/AppIcon.vue";
import AppSwitch from "./components/AppSwitch.vue";
import TitleBar from "./components/TitleBar.vue";
import type { IconName } from "./icons";
import logo from "./assets/brand/logo.png";
type NavigationItem = { id: string; icon: IconName; label: string };
const navigation: NavigationItem[] = [
  {
    id: "home",
    icon: "device",
    get label() {
      return t("ui.devices");
    },
  },
  {
    id: "library",
    icon: "library",
    get label() {
      return t("ui.library");
    },
  },
  {
    id: "plugins",
    icon: "extensions",
    get label() {
      return t("ui.extensions");
    },
  },
];
const w = useDesktopAlchemy();
const baseMode = computed(() => basicContentMode(w.visual));
const gifPattern = computed(() => isGifPattern(w.visual));
const showText = computed({
  get: () => !gifPattern.value && w.visual.show_text !== false,
  set: (value: boolean) => {
    w.visual.show_text = value;
  },
});
const textPosition = computed({
  get: () => w.visual.text_position ?? "center",
  set: (value: TextPosition) => {
    w.visual.text_position = value;
  },
});
const textPositions = computed(() => [
  { value: "top" as const, label: t("ui.textTop") },
  { value: "center" as const, label: t("ui.textCenter") },
  { value: "bottom" as const, label: t("ui.textBottom") },
]);

const tab = ref("home");
const hasDeviceContext = ref(false);
const deviceViews = computed(() =>
  w.currentProfile?.module_available === false
    ? undefined
    : viewsFor(w.currentProfile?.adapter_id),
);
const deviceNavigation = computed<NavigationItem[]>(() => {
  const items: NavigationItem[] = [
    {
      id: "device",
      icon: deviceViews.value?.icon ?? "device",
      label: deviceViews.value?.controlLabel ?? t("ui.deviceControls"),
    },
  ];
  items.push(...(deviceViews.value?.sections ?? []));
  if (w.hasDeck) {
    items.push(
      {
        id: "pages",
        icon: "pages",
        get label() {
          return t("ui.pages");
        },
      },
      {
        id: "scenes",
        icon: "clock",
        get label() {
          return t("ui.scenes");
        },
      },
    );
  }
  if (deviceViews.value?.tests)
    items.push({
      id: "tests",
      icon: "test",
      get label() {
        return t("ui.hardwareTests");
      },
    });
  return items;
});
watch(
  () => w.selectedProfileId,
  () => {
    contentTarget.value = null;
    assetPicker.value = false;
    applicationPicker.value = false;
    connectionDetails.value = false;
    if (
      isDeviceRoute.value &&
      !deviceNavigation.value.some((item) => item.id === tab.value)
    )
      tab.value = "device";
  },
);
const showLogs = ref(false);
const showSettings = ref(false);
watch(
  () => w.trayNavigation,
  (request) => {
    // Restoring the window must never dismiss an existing confirmation or picker.
    if (!request || document.querySelector("dialog[open]")) return;
    if (request.action === "settings") showSettings.value = true;
    else if (request.action === "device") tab.value = "home";
    else if (["pages", "plugins"].includes(request.action)) {
      if (request.action === "pages") hasDeviceContext.value = true;
      tab.value = request.action;
    }
  },
);
const assetPicker = ref(false);
const librarySection = ref<"assets" | "applications">("assets");
const applicationPicker = ref(false);
const connectionDetails = ref(false);
const connectionTrigger = ref<HTMLButtonElement>();
async function closeConnectionDetails() {
  connectionDetails.value = false;
  await nextTick();
  connectionTrigger.value?.focus({ preventScroll: true });
}
const contentTarget = ref<{ pageId: string; key: number } | null>(null);
const contentTrigger = ref<HTMLButtonElement>();
const contentName = computed(
  () =>
    w.widgetOptions.find((o) => o.value === w.widgetSelection)?.label ??
    t("ui.pluginUnavailable"),
);
function openContentPicker() {
  contentTarget.value = { pageId: w.page.id, key: w.selectedKey };
}
async function closeContentPicker() {
  contentTarget.value = null;
  await nextTick();
  contentTrigger.value?.focus({ preventScroll: true });
}
function chooseContent(value: string) {
  if (
    contentTarget.value?.pageId === w.page.id &&
    contentTarget.value.key === w.selectedKey
  )
    w.selectContent(value);
  void closeContentPicker();
}
watch([() => w.selectedPage, () => w.selectedKey], () => {
  contentTarget.value = null;
});
function libraryTabKey(event: KeyboardEvent) {
  if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
  event.preventDefault();
  librarySection.value =
    event.key === "Home"
      ? "assets"
      : event.key === "End"
        ? "applications"
        : librarySection.value === "assets"
          ? "applications"
          : "assets";
  void nextTick(() =>
    document.getElementById(`library-${librarySection.value}-tab`)?.focus(),
  );
}
const confirm = ref<{
  title: string;
  description: string;
  run: () => void;
  buttonLabel?: string;
} | null>(null);
const modal = ref<HTMLDialogElement | null>(null);
function trapDialog(event: KeyboardEvent) {
  if (event.key !== "Tab" || !modal.value) return;
  const controls = [
    ...modal.value.querySelectorAll<HTMLButtonElement>("button:not(:disabled)"),
  ];
  const first = controls[0],
    last = controls.at(-1);
  if (
    event.shiftKey &&
    (document.activeElement === first ||
      !modal.value.contains(document.activeElement))
  ) {
    event.preventDefault();
    last?.focus();
  } else if (
    !event.shiftKey &&
    (document.activeElement === last ||
      !modal.value.contains(document.activeElement))
  ) {
    event.preventDefault();
    first?.focus();
  }
}
watch(confirm, async (value) => {
  if (value) {
    await nextTick();
    modal.value?.showModal();
    modal.value?.querySelector("button")?.focus();
  }
});
watch(
  () => w.exitRequested,
  async () => {
    showSettings.value = false;
    assetPicker.value = false;
    applicationPicker.value = false;
    contentTarget.value = null;
    connectionDetails.value = false;
    if (await w.flushSave()) {
      void w.call("confirm_exit");
      return;
    }
    void w.call("show_window", {}, undefined, true);
    confirm.value = {
      title: t("ui.quitDesktopAlchemy"),
      description: t("ui.pageOrSceneChangesCouldNot"),
      buttonLabel: t("ui.discardChangesAndQuit"),
      run: () => {
        void w.call("confirm_exit");
      },
    };
  },
);
const connectionLabels: Record<string, string> = {
  get disconnected() {
    return t("ui.disconnected");
  },
  get connecting() {
    return t("ui.connecting");
  },
  get ready() {
    return t("ui.connected");
  },
  get reconnecting() {
    return t("ui.reconnecting");
  },
  get error() {
    return t("ui.connectionError");
  },
  get busy() {
    return t("ui.deviceInUse");
  },
};
const connectedDevices = computed(() =>
  w.workspace.discovered.map((descriptor) => ({
    descriptor,
    profile: w.workspace.devices.find(
      (profile) => profile.device_id === descriptor.id,
    ),
  })),
);
const deviceRouteTabs = ["device", "pages", "scenes", "tests"];
const isDeviceRoute = computed(
  () => deviceRouteTabs.includes(tab.value) || tab.value.startsWith("device:"),
);
const showDeviceNavigation = computed(
  () =>
    hasDeviceContext.value &&
    isDeviceRoute.value &&
    deviceNavigation.value.length > 1,
);
const breadcrumbLabels: Record<string, string> = {
  get home() {
    return t("ui.deviceList");
  },
  get device() {
    return t("ui.deviceWorkspace");
  },
  get pages() {
    return t("ui.pageEditor");
  },
  get scenes() {
    return t("ui.sceneAutomation");
  },
  get library() {
    return t("ui.library");
  },
  get plugins() {
    return t("ui.localExtensions");
  },
  get tests() {
    return t("ui.hardwareTests");
  },
};
const breadcrumbLabel = computed(
  () =>
    deviceNavigation.value.find((item) => item.id === tab.value)?.label ??
    breadcrumbLabels[tab.value] ??
    t("ui.deviceList"),
);
const selectedDeviceName = computed(
  () => w.currentProfile?.name || t("ui.devices"),
);
const bindingTarget = ref<DeviceDescriptor | null>(null);
const bindingProfiles = computed(() =>
  bindingTarget.value
    ? w.workspace.devices.filter(
        (profile) =>
          profile.adapter_id === bindingTarget.value?.adapter_id &&
          !profile.device_id,
      )
    : [],
);
const savedProfiles = computed(() =>
  w.workspace.devices.filter(
    (profile) =>
      !connectedDevices.value.some(
        (entry) => entry.profile?.profile_id === profile.profile_id,
      ),
  ),
);
async function openSavedProfile(profileId: string) {
  if (w.switchingDevice) return;
  try {
    await w.selectDevice(profileId);
    hasDeviceContext.value = true;
    tab.value = "device";
  } catch (error) {
    w.error = errorText(error);
  }
}
async function openConnectedDevice(entry: {
  descriptor: DeviceDescriptor;
  profile: DeviceState | undefined;
}) {
  if (w.switchingDevice) return;
  if (!entry.profile) {
    w.error = "";
    bindingTarget.value = entry.descriptor;
    return;
  }
  try {
    await w.selectDevice(entry.profile.profile_id);
    hasDeviceContext.value = true;
    tab.value = "device";
  } catch (error) {
    w.error = errorText(error);
  }
}
async function bindAndOpenDevice(profileId: string) {
  const target = bindingTarget.value;
  if (!target || w.switchingDevice || w.pending) return;
  try {
    await w.selectDevice(profileId);
    await w.bindDevice(target.id);
    if (w.error) return;
    bindingTarget.value = null;
    hasDeviceContext.value = true;
    tab.value = "device";
  } catch (error) {
    w.error = errorText(error);
  }
}
const activeName = computed(
  () =>
    w.state.config.pages.find((p) => p.id === w.state.active_page)?.name ??
    t("ui.home"),
);
const actionType = computed({
  get: () => w.keyAction.type,
  set: (type: string) => {
    w.keyAction =
      type === "page"
        ? { type: "page", page_id: w.draft.pages[0].id }
        : type === "launch"
          ? {
              type: "launch",
              target: programTarget(),
            }
          : ({ type } as KeyAction);
    w.edit();
  },
});
const keyLabel = computed(() =>
  w.hasDisplay(w.selectedKey)
    ? t("ui.displayKey", { arg0: String(w.selectedKey).padStart(2, "0") })
    : t("ui.physicalKey", { arg0: w.selectedKey }),
);
const systemDark = matchMedia("(prefers-color-scheme: dark)");
const systemIsDark = ref(systemDark.matches);
systemDark.addEventListener("change", (e) => (systemIsDark.value = e.matches));
watch(
  [() => w.draft.theme, systemIsDark],
  () => {
    document.documentElement.dataset.theme =
      w.draft.theme === "system"
        ? systemIsDark.value
          ? "dark"
          : "light"
        : w.draft.theme;
  },
  { immediate: true },
);
async function theme() {
  const values = ["system", "light", "dark"];
  w.draft.theme = values[(values.indexOf(w.draft.theme) + 1) % values.length];
  if (w.desktop) await w.call("set_theme", { theme: w.draft.theme });
}
async function chooseImage() {
  const editorTarget = `${w.selectedProfileId}/${w.selectedPage}/${w.selectedKey}`;
  if (!w.desktop) {
    w.error = t("ui.selectImagesInTheDesktopApp");
    return;
  }
  const path = await open({
    multiple: false,
    filters: [
      {
        name: t("ui.images"),
        extensions: ["png", "jpg", "jpeg", "gif"],
      },
    ],
  });
  if (
    `${w.selectedProfileId}/${w.selectedPage}/${w.selectedKey}` !== editorTarget
  )
    return;
  if (typeof path !== "string") return;
  const results = await w.importLibrary([path]);
  const id = results?.[0]?.item?.asset_id;
  if (
    `${w.selectedProfileId}/${w.selectedPage}/${w.selectedKey}` !== editorTarget
  )
    return;
  if (results?.[0]?.error) w.error = results[0].error;
  if (!id) return;
  useAsset(id);
}
function useAsset(value: string | ImageRef) {
  const image: ImageRef =
    typeof value === "string" ? { type: "asset", asset_id: value } : value;
  w.visual.asset_id = image.type === "asset" ? image.asset_id : null;
  w.visual.plugin_icon =
    image.type === "plugin_icon"
      ? { plugin_id: image.plugin_id, icon_id: image.icon_id }
      : null;
  w.visual.default_icon_asset_id = null;
  w.edit();
}
function deletePage() {
  confirm.value = {
    title: t("ui.delete", { arg0: w.page.name }),
    description: t("ui.thePageWillBeDeletedAnd"),
    run: w.deletePage,
  };
}
onMounted(() => void w.init());
</script>

<template>
  <div class="app-frame">
    <TitleBar />
    <div class="app-shell">
      <aside class="rail">
        <nav :aria-label="t('ui.mainNavigation')">
          <button
            v-for="item in navigation"
            :key="item.id"
            :class="{
              active:
                item.id === 'home'
                  ? tab === 'home' || isDeviceRoute
                  : tab === item.id,
            }"
            :aria-current="
              (item.id === 'home' && (tab === 'home' || isDeviceRoute)) ||
              tab === item.id
                ? 'page'
                : undefined
            "
            :title="item.label"
            @click="tab = item.id === 'home' ? 'home' : item.id"
          >
            <AppIcon
              :name="item.icon"
              class="nav-icon"
              :size="24"
              :weight="
                (item.id === 'home' && (tab === 'home' || isDeviceRoute)) ||
                tab === item.id
                  ? 'duotone'
                  : 'regular'
              "
            /><small>{{ item.label }}</small>
          </button>
        </nav>
        <div class="rail-bottom">
          <button
            :title="t('ui.appSettings')"
            :aria-label="t('ui.appSettings')"
            @click="showSettings = true"
          >
            <AppIcon name="settings" :size="22" />
          </button>
          <button
            :title="t('ui.theme', { arg0: t('app.themes.' + w.draft.theme) })"
            :aria-label="t('ui.changeTheme')"
            @click="theme"
          >
            <AppIcon name="theme" :size="22" /></button
          ><button
            :class="{ active: showLogs }"
            :title="t('ui.activityLog')"
            :aria-label="t('ui.activityLog')"
            @click="showLogs = !showLogs"
          >
            <AppIcon name="logs" :size="22" />
          </button>
        </div>
      </aside>
      <main class="workspace">
        <DeviceScope
          :key="w.selectedProfileId"
          :profile-id="w.selectedProfileId"
        >
          <header class="toolbar">
            <div class="breadcrumb">
              <button class="breadcrumb-home" @click="tab = 'home'">
                Desktop Alchemy
              </button>
              <span>/</span>
              <template v-if="isDeviceRoute">
                <button class="breadcrumb-link" @click="tab = 'home'">
                  {{ t("ui.deviceList") }}
                </button>
                <span>/</span>
                <button
                  v-if="tab !== 'device'"
                  class="breadcrumb-link breadcrumb-device"
                  @click="tab = 'device'"
                >
                  {{ selectedDeviceName }}
                </button>
                <span v-else class="breadcrumb-current breadcrumb-device">
                  {{ selectedDeviceName }}
                </span>
                <template v-if="tab !== 'device'">
                  <span>/</span>
                  <span class="breadcrumb-current">{{ breadcrumbLabel }}</span>
                </template>
              </template>
              <span v-else class="breadcrumb-current">{{
                breadcrumbLabel
              }}</span>
            </div>
            <div class="toolbar-right">
              <span
                v-if="(w.sceneDirty || w.sceneSaveError) && tab !== 'scenes'"
                class="unsaved"
                :class="{ 'error-text': w.sceneSaveError }"
                >{{
                  w.sceneSaveError
                    ? t("ui.couldNotSaveScenes")
                    : w.sceneSaving
                      ? t("ui.savingScenes")
                      : t("ui.waitingToSaveScenes")
                }}</span
              >
              <span v-if="w.dirty && tab !== 'pages'" class="unsaved">{{
                w.saveStatus
              }}</span>
              <button
                v-if="hasDeviceContext && tab !== 'home' && w.currentProfile"
                ref="connectionTrigger"
                class="connection"
                :class="w.state.connection"
                :aria-label="
                  t('ui.deviceConnection', {
                    arg0: w.currentProfile?.name ?? t('ui.noDeviceSelected'),
                    arg1:
                      connectionLabels[w.state.connection] ??
                      t('ui.unknownStatus'),
                  })
                "
                :title="t('ui.viewDeviceConnection')"
                @click="connectionDetails = true"
              >
                <i aria-hidden="true"></i
                >{{ w.currentProfile?.name ?? t("ui.noDeviceSelected") }}
                {{
                  connectionLabels[w.state.connection] ?? t("ui.unknownStatus")
                }}
              </button>
            </div>
          </header>
          <nav
            v-if="showDeviceNavigation"
            class="device-subnav"
            :aria-label="t('ui.deviceFeatures')"
          >
            <span class="device-subnav-label">{{
              t("ui.deviceFeatures")
            }}</span>
            <button
              v-for="item in deviceNavigation"
              :key="item.id"
              type="button"
              :aria-current="tab === item.id ? 'page' : undefined"
              @click="tab = item.id"
            >
              <AppIcon
                :name="item.icon"
                :size="17"
                :weight="tab === item.id ? 'duotone' : 'regular'"
              />
              {{ item.label }}
            </button>
          </nav>
          <DraftRecovery
            v-if="isDeviceRoute && (w.saveError || w.sceneSaveError)"
          />
          <div v-if="!w.desktop" class="notice">
            {{ t("ui.browserPreviewUseTheDesktopApp") }}
          </div>
          <div class="content" :class="{ 'editor-content': tab === 'pages' }">
            <fieldset
              class="device-route-fields"
              :disabled="isDeviceRoute && !!w.currentProfile?.config_error"
            >
              <section v-if="tab === 'home'" class="device-home">
                <div class="device-home-heading">
                  <div>
                    <p class="eyebrow">{{ t("ui.devices") }}</p>
                    <h1>{{ t("ui.connectedDevices") }}</h1>
                    <p>{{ t("ui.selectADeviceToOpenIts") }}</p>
                  </div>
                  <div class="device-home-count" aria-live="polite">
                    <strong>{{ connectedDevices.length }}</strong>
                    <span>{{ t("ui.connected2") }}</span>
                  </div>
                </div>
                <div v-if="connectedDevices.length" class="device-card-grid">
                  <button
                    v-for="entry in connectedDevices"
                    :key="entry.descriptor.id"
                    class="device-card"
                    :disabled="w.switchingDevice"
                    :aria-label="
                      t('ui.manage', {
                        arg0: entry.profile?.name ?? entry.descriptor.model,
                      })
                    "
                    @click="openConnectedDevice(entry)"
                  >
                    <span class="device-card-visual" aria-hidden="true">
                      <img
                        v-if="viewsFor(entry.descriptor.adapter_id)?.poster"
                        :src="viewsFor(entry.descriptor.adapter_id)?.poster"
                        class="device-card-product"
                        alt=""
                      />
                      <span v-else class="device-card-icon">
                        <AppIcon
                          :name="
                            viewsFor(entry.descriptor.adapter_id)?.icon ??
                            'device'
                          "
                          :size="42"
                          weight="duotone"
                        />
                      </span>
                      <span class="device-card-accent"></span>
                    </span>
                    <span class="device-card-heading">
                      <span>
                        <strong>{{
                          entry.profile?.name ?? entry.descriptor.model
                        }}</strong>
                        <small
                          v-if="
                            entry.profile &&
                            entry.profile.name !== entry.descriptor.model
                          "
                        >
                          {{ entry.descriptor.model }}
                        </small>
                      </span>
                      <span
                        class="device-card-status"
                        :class="entry.profile?.state.connection ?? 'pending'"
                      >
                        <i aria-hidden="true"></i>
                        {{
                          entry.profile
                            ? (connectionLabels[
                                entry.profile.state.connection
                              ] ?? t("ui.statusUnknown"))
                            : t("ui.configurationRequired")
                        }}
                      </span>
                    </span>
                    <span class="device-card-serial">
                      <span>{{ t("ui.serialNumber") }}</span>
                      <strong class="mono">{{
                        entry.descriptor.serial || t("ui.notProvided")
                      }}</strong>
                    </span>
                    <span class="device-card-action">
                      {{
                        entry.profile
                          ? t("ui.manageDevice")
                          : t("ui.chooseConfiguration")
                      }}
                      <AppIcon name="arrow-right" :size="17" />
                    </span>
                  </button>
                </div>
                <div v-else class="device-home-empty">
                  <span class="device-home-empty-icon" aria-hidden="true">
                    <AppIcon name="device" :size="32" weight="duotone" />
                  </span>
                  <h2>{{ t("ui.noConnectedDevices") }}</h2>
                  <p>{{ t("ui.connectASupportedDeviceToSee") }}</p>
                </div>
                <details
                  v-if="savedProfiles.length"
                  class="panel saved-profiles"
                >
                  <summary>
                    {{
                      t("app.savedConfigurations", {
                        count: savedProfiles.length,
                      })
                    }}
                  </summary>
                  <div class="button-row wrap">
                    <button
                      v-for="profile in savedProfiles"
                      :key="profile.profile_id"
                      :disabled="w.switchingDevice"
                      :aria-label="
                        t('ui.openConfiguration', { arg0: profile.name })
                      "
                      @click="openSavedProfile(profile.profile_id)"
                    >
                      {{ profile.name }} ·
                      {{
                        profile.config_error
                          ? t("ui.configurationUnavailable")
                          : t("ui.offline")
                      }}
                    </button>
                  </div>
                </details>
              </section>
              <p
                v-if="w.currentProfile?.config_error"
                class="error-text"
                role="alert"
              >
                <UiFailure
                  :detail="w.currentProfile.config_error"
                  :message="w.currentProfile.ui_message"
                />{{ t("ui.readOnlyConfiguration") }}
              </p>
              <section v-if="!deviceViews && isDeviceRoute" class="panel">
                <h2>{{ w.currentProfile?.name ?? t("ui.noDevices") }}</h2>
                <p>
                  {{ t("ui.connectASupportedDeviceToManage") }}
                </p>
              </section>
              <section
                v-if="w.currentProfile && !w.currentProfile.device_id"
                class="panel"
              >
                <h2>{{ t("ui.linkDeviceConfiguration") }}</h2>
                <p>{{ t("ui.selectTheDeviceThatBelongsTo") }}</p>
                <button
                  v-for="device in w.workspace.discovered.filter(
                    (d) =>
                      d.adapter_id === w.currentProfile?.adapter_id &&
                      !w.workspace.devices.some((p) => p.device_id === d.id),
                  )"
                  :key="device.id"
                  @click="w.bindDevice(device.id)"
                >
                  {{ device.model }} · {{ device.serial || device.id }}
                </button>
              </section>
              <component
                v-if="
                  !w.hasDeck &&
                  tab !== 'tests' &&
                  deviceViews &&
                  deviceNavigation.some((item) => item.id === tab)
                "
                :is="deviceViews.controls"
                :key="w.selectedProfileId"
                :page="tab"
              />
              <template v-if="tab === 'device' && w.hasDeck">
                <div class="page-heading">
                  <div>
                    <p class="eyebrow">{{ t("ui.deviceControls") }}</p>
                    <h1>{{ t("ui.deviceWorkspace") }}</h1>
                  </div>
                </div>
                <div class="device-layout">
                  <section class="panel live-panel">
                    <div class="panel-heading">
                      <div>
                        <h2>{{ t("ui.keysAndDisplay") }}</h2>
                        <p>{{ t("ui.currentPage") }}{{ activeName }}</p>
                      </div>
                      <span class="badge">{{
                        w.state.pressed.length
                          ? t("ui.pressed", {
                              arg0: w.state.pressed.join(", "),
                            })
                          : t("ui.waitingForKeyInput")
                      }}</span>
                    </div>
                    <component
                      :is="deviceViews?.preview"
                      :key="w.selectedProfileId"
                      :page="
                        w.state.config.pages.find(
                          (p) => p.id === w.state.active_page,
                        ) ?? w.page
                      "
                      :selected="w.selectedKey"
                      :pressed="w.state.pressed"
                      @select="w.selectedKey = $event"
                    />
                    <div class="deck-caption">
                      <span>{{ t("ui.configurationPreview") }}</span
                      ><button class="quiet" @click="tab = 'pages'">
                        {{ t("ui.editPage") }}
                        <AppIcon name="arrow-right" :size="14" />
                      </button>
                    </div>
                  </section>
                  <component
                    :is="deviceViews?.controls"
                    :key="w.selectedProfileId"
                  />
                </div>
              </template>

              <SceneAutomation v-else-if="tab === 'scenes' && w.hasDeck" />
              <template v-else-if="tab === 'pages' && w.hasDeck">
                <div class="page-heading">
                  <div>
                    <p class="eyebrow">{{ t("ui.pageConfiguration") }}</p>
                    <h1>{{ t("ui.pageEditor") }}</h1>
                  </div>
                  <div class="button-row">
                    <span
                      class="save-status"
                      :class="{ 'error-text': w.saveError }"
                      role="status"
                      :title="w.saveError || undefined"
                      >{{ w.saveStatus }}</span
                    >
                    <button
                      v-if="w.saveError"
                      class="secondary"
                      :disabled="w.saving"
                      @click="w.flushSave()"
                    >
                      {{ t("ui.retrySave") }}</button
                    ><button
                      class="primary"
                      :disabled="!w.desktop || w.pending > 0 || w.activating"
                      @click="w.activatePage()"
                    >
                      {{
                        w.activating ? t("ui.activating") : t("ui.activatePage")
                      }}
                    </button>
                  </div>
                </div>
                <div class="editor-layout">
                  <aside class="page-list panel">
                    <div class="panel-heading">
                      <h2>{{ t("ui.pages") }}</h2>
                      <button
                        class="quiet"
                        :title="t('ui.createPage')"
                        :aria-label="t('ui.createPage')"
                        @click="w.addPage"
                      >
                        <AppIcon name="plus" :size="16" />
                      </button>
                    </div>
                    <button
                      v-for="p in w.draft.pages"
                      :key="p.id"
                      class="page-item"
                      :class="{ selected: p.id === w.page.id }"
                      @click="w.selectedPage = p.id"
                    >
                      <AppIcon name="pages" :size="16" /><span
                        >{{ p.name
                        }}<small v-if="p.id === w.state.active_page">{{
                          t("ui.activePage")
                        }}</small></span
                      >
                    </button>
                    <div class="page-list-actions">
                      <button class="quiet" @click="w.duplicatePage">
                        {{ t("ui.duplicate") }}</button
                      ><button
                        class="quiet"
                        :disabled="w.draft.pages[0].id === w.page.id"
                        :aria-label="t('ui.movePageUp')"
                        @click="w.movePage(-1)"
                      >
                        <AppIcon name="up" :size="14" /></button
                      ><button
                        class="quiet"
                        :disabled="w.draft.pages.at(-1)?.id === w.page.id"
                        :aria-label="t('ui.movePageDown')"
                        @click="w.movePage(1)"
                      >
                        <AppIcon name="down" :size="14" /></button
                      ><button
                        class="quiet danger"
                        :disabled="w.draft.pages.length < 2"
                        @click="deletePage"
                      >
                        {{ t("ui.delete2") }}
                      </button>
                    </div>
                  </aside>
                  <section class="panel editor-deck">
                    <div class="panel-heading">
                      <div>
                        <input
                          v-model="w.page.name"
                          class="page-name"
                          :aria-label="t('ui.pageName')"
                          maxlength="60"
                          @input="w.edit"
                        />
                        <p>{{ t("ui.clickAKeyToEditChanges") }}</p>
                      </div>
                      <span class="badge">{{
                        t("phrases.keyCounts", {
                          displays:
                            w.currentProfile?.layout.display_keys.length ?? 0,
                          physical:
                            w.currentProfile?.layout.input_keys.filter(
                              (key) => !w.hasDisplay(key),
                            ).length ?? 0,
                        })
                      }}</span>
                    </div>
                    <component
                      :is="deviceViews?.preview"
                      :key="w.selectedProfileId"
                      :page="w.page"
                      editor
                      :selected="w.selectedKey"
                      :pressed="w.state.pressed"
                      @select="w.selectedKey = $event"
                    />
                    <p class="editor-preview-label">
                      {{ t("ui.configurationPreview") }}
                    </p>
                  </section>
                  <aside class="panel inspector">
                    <div class="panel-heading">
                      <h2>{{ keyLabel }}</h2>
                      <span class="badge">{{
                        w.hasDisplay(w.selectedKey) ? "LCD" : t("ui.noDisplay")
                      }}</span>
                    </div>
                    <template v-if="w.hasDisplay(w.selectedKey)">
                      <div class="content-field">
                        <span id="content-field-label">{{
                          t("ui.displayContent")
                        }}</span>
                        <button
                          ref="contentTrigger"
                          class="content-trigger"
                          :aria-label="t('ui.chooseDisplayContent')"
                          aria-haspopup="dialog"
                          :aria-expanded="!!contentTarget"
                          @click="openContentPicker"
                        >
                          <AppIcon
                            :name="
                              w.visual.widget
                                ? 'extensions'
                                : baseMode === 'text'
                                  ? 'text'
                                  : baseMode === 'none'
                                    ? 'none'
                                    : 'image'
                            "
                            :size="19"
                          />
                          <span>{{ contentName }}</span
                          ><AppIcon name="caret-down" :size="15" />
                        </button>
                      </div>
                      <template v-if="w.visual.widget">
                        <section
                          class="widget-settings"
                          :aria-label="
                            t('ui.settings', { arg0: w.widgetPluginName })
                          "
                        >
                          <h3>{{ w.widgetPluginName }}</h3>
                          <WidgetSourceSettings />
                          <label v-if="w.widgetPresentationOptions.length"
                            >{{ t("ui.displayFormat")
                            }}<AppSelect
                              v-model="w.widgetPresentation"
                              :aria-label="t('ui.displayFormat')"
                              :options="w.widgetPresentationOptions"
                              @change="w.edit"
                          /></label>
                          <p class="helper">{{ w.widgetDisplayStatus }}</p>
                        </section>
                        <h3>{{ t("ui.buttonAppearance") }}</h3>
                        <div class="split">
                          <label
                            >{{ t("ui.textColor")
                            }}<input
                              v-model="w.visual.widget.style.foreground"
                              type="color"
                              @input="w.edit"
                          /></label>
                          <label
                            >{{ t("ui.backgroundColor")
                            }}<input
                              v-model="w.visual.widget.style.background"
                              type="color"
                              @input="w.edit"
                          /></label>
                        </div>
                        <label
                          >{{ t("ui.accentColor")
                          }}<input
                            v-model="w.visual.widget.style.accent"
                            type="color"
                            @input="w.edit"
                        /></label>
                        <AppSwitch
                          v-model="w.visual.widget.style.show_progress"
                          :label="t('ui.showProgressBar')"
                          @update:model-value="w.edit"
                        />
                      </template>
                      <template v-else>
                        <template v-if="baseMode !== 'none'">
                          <AppSwitch
                            v-model="showText"
                            :label="t('ui.showText')"
                            :disabled="gifPattern"
                            @update:model-value="w.edit"
                          />
                          <p v-if="gifPattern" class="helper">
                            {{ t("ui.gifNoTextOverlay") }}
                          </p>
                          <label
                            >{{ t("ui.textContent")
                            }}<textarea
                              v-model="w.visual.text"
                              rows="2"
                              maxlength="80"
                              @input="w.edit"
                            />
                          </label>
                          <fieldset
                            class="text-position-field"
                            :disabled="!showText || gifPattern"
                          >
                            <legend>{{ t("ui.textPosition") }}</legend>
                            <div class="text-position-options">
                              <label
                                v-for="position in textPositions"
                                :key="position.value"
                              >
                                <input
                                  v-model="textPosition"
                                  type="radio"
                                  name="key-text-position"
                                  :value="position.value"
                                  @change="w.edit"
                                />
                                <span>{{ position.label }}</span>
                              </label>
                            </div>
                          </fieldset>
                          <label class="foreground-color-row"
                            >{{ t("ui.textColor")
                            }}<input
                              v-model="w.visual.foreground"
                              :disabled="!showText || gifPattern"
                              type="color"
                              @input="w.edit"
                          /></label>
                        </template>
                        <ColorField
                          :label="t('ui.backgroundColor')"
                          v-model="w.visual.background"
                          @change="w.edit"
                        />
                        <template v-if="baseMode === 'pattern'">
                          <p class="helper">
                            {{
                              t(
                                "ui.theBackgroundColorAppearsThroughTransparent",
                              )
                            }}
                          </p>
                          <p
                            v-if="
                              w.visual.plugin_icon &&
                              !w.library.items.some(
                                (item) =>
                                  item.image.type === 'plugin_icon' &&
                                  item.image.plugin_id ===
                                    w.visual.plugin_icon?.plugin_id &&
                                  item.image.icon_id ===
                                    w.visual.plugin_icon?.icon_id,
                              )
                            "
                            class="notice"
                            role="status"
                          >
                            {{ t("ui.pluginIconUnavailableReferenceRetained")
                            }}{{ w.visual.plugin_icon.plugin_id }}
                            /
                            {{ w.visual.plugin_icon.icon_id
                            }}{{ t("ui.reinstallThePluginToRestoreIt") }}
                          </p>
                          <label>{{ t("ui.imageGif") }}</label>
                          <div class="button-row">
                            <button :disabled="!w.desktop" @click="chooseImage">
                              {{ t("ui.importAsset") }}</button
                            ><button
                              :disabled="!w.desktop"
                              @click="assetPicker = true"
                            >
                              {{ t("ui.chooseFromImageAssets") }}</button
                            ><button
                              :disabled="
                                !w.visual.asset_id && !w.visual.plugin_icon
                              "
                              @click="
                                w.visual.asset_id = null;
                                w.visual.plugin_icon = null;
                                w.visual.default_icon_asset_id = null;
                                w.edit();
                              "
                            >
                              {{ t("ui.remove") }}
                            </button>
                          </div>
                        </template>
                      </template>
                    </template>
                    <p v-else class="helper">
                      {{ t("ui.theBottomPhysicalKeysSupportPage") }}
                    </p>
                    <template v-if="w.selectedInputId"
                      ><label
                        >{{ t("ui.onPress")
                        }}<AppSelect
                          v-model="actionType"
                          :aria-label="t('ui.onPress')"
                          :options="[
                            { value: 'none', label: t('ui.noAction') },
                            { value: 'previous', label: t('ui.previousPage') },
                            { value: 'home', label: t('ui.homePage') },
                            { value: 'next', label: t('ui.nextPage') },
                            { value: 'page', label: t('ui.goToPage') },
                            { value: 'launch', label: t('ui.launchAppUrl') },
                          ]"
                      /></label>
                      <button
                        :disabled="!w.desktop || w.pending > 0"
                        @click="applicationPicker = true"
                      >
                        {{ t("ui.chooseFromAppPresets") }}
                      </button>
                      <label v-if="w.keyAction.type === 'page'"
                        >{{ t("ui.targetPage")
                        }}<AppSelect
                          v-model="w.keyAction.page_id"
                          :aria-label="t('ui.targetPage')"
                          :options="
                            w.draft.pages.map((p) => ({
                              value: p.id,
                              label: p.name,
                            }))
                          "
                          @change="w.edit"
                      /></label>
                      <LaunchTargetEditor
                        v-if="w.keyAction.type === 'launch'"
                        :key="`${w.selectedProfileId}/${w.selectedPage}/${w.selectedKey}`"
                        v-model="w.keyAction.target"
                        @update:model-value="w.edit"
                        @select="w.useApplication"
                    /></template>
                  </aside>
                </div>
                <GenericBindings />
              </template>

              <template v-else-if="tab === 'library'">
                <div class="page-heading">
                  <div>
                    <p class="eyebrow">{{ t("ui.localResources") }}</p>
                    <h1>{{ t("ui.library") }}</h1>
                    <p>{{ t("ui.manageImagesAndAppsForReuse") }}</p>
                  </div>
                </div>
                <div
                  class="library-tabs"
                  role="tablist"
                  :aria-label="t('ui.libraryCategories')"
                  @keydown="libraryTabKey"
                >
                  <button
                    id="library-assets-tab"
                    role="tab"
                    :aria-selected="librarySection === 'assets'"
                    :tabindex="librarySection === 'assets' ? 0 : -1"
                    aria-controls="library-assets-panel"
                    @click="librarySection = 'assets'"
                  >
                    {{ t("ui.imageAssets") }}
                  </button>
                  <button
                    id="library-applications-tab"
                    role="tab"
                    :aria-selected="librarySection === 'applications'"
                    :tabindex="librarySection === 'applications' ? 0 : -1"
                    aria-controls="library-applications-panel"
                    @click="librarySection = 'applications'"
                  >
                    {{ t("ui.appPresets") }}
                  </button>
                </div>
                <section
                  :id="`library-${librarySection}-panel`"
                  role="tabpanel"
                  :aria-labelledby="`library-${librarySection}-tab`"
                >
                  <KeepAlive
                    ><LibraryBrowser
                      v-if="librarySection === 'assets'" /><ApplicationsBrowser
                      v-else
                  /></KeepAlive>
                </section>
              </template>
              <PluginControls v-else-if="tab === 'plugins'" />
              <KeepAlive>
                <component
                  :is="deviceViews?.tests"
                  :key="w.selectedProfileId"
                  v-if="tab === 'tests' && deviceViews?.tests"
                  @confirm="confirm = $event"
                />
              </KeepAlive>
            </fieldset>
          </div>
          <footer class="statusbar">
            <span v-if="w.pending" role="status">{{ t("ui.processing") }}</span>
            <button class="quiet" @click="showLogs = !showLogs">
              {{ showLogs ? t("app.hideLog") : t("app.openLog") }}
              <AppIcon
                :name="showLogs ? 'caret-down' : 'caret-up'"
                :size="14"
              />
            </button>
          </footer>
        </DeviceScope>
      </main>
      <aside
        v-if="showLogs"
        class="log-drawer"
        :aria-label="t('ui.activityLog')"
      >
        <div class="panel-heading">
          <div>
            <h2>{{ t("ui.activityLog") }}</h2>
          </div>
          <button
            class="quiet"
            :aria-label="t('ui.closeLog')"
            @click="showLogs = false"
          >
            <AppIcon name="close" />
          </button>
        </div>
        <h3>{{ t("ui.recentOperations") }}</h3>
        <p v-if="!w.state.receipts.length" class="helper">
          {{ t("ui.noDeviceOperationsYet") }}
        </p>
        <div
          class="receipt"
          v-for="r in [...w.state.receipts].reverse().slice(0, 15)"
          :key="r.id"
        >
          <span>{{ commandLabels[r.command] ?? r.command }}</span
          ><span class="badge" :class="r.status">{{
            statusLabels[r.status]
          }}</span
          ><small>{{ r.detail }}</small>
        </div>
        <h3>{{ t("ui.appsAndPlugins") }}</h3>
        <p v-if="!w.state.logs.length" class="helper">
          {{ t("ui.noActivityYet") }}
        </p>
        <div
          v-for="(entry, n) in [...w.state.logs].reverse()"
          :key="n"
          class="log-entry"
        >
          <time>{{
            new Date(entry.at).toLocaleTimeString(language.locale)
          }}</time
          ><span
            class="log-level"
            :class="{ 'error-text': entry.level === 'error' }"
            >{{ logLevelLabels[entry.level] ?? entry.level }}</span
          ><span>{{ entry.message }}</span>
        </div>
      </aside>
    </div>
    <dialog
      v-if="confirm"
      ref="modal"
      class="dialog"
      role="alertdialog"
      @keydown="trapDialog"
      :aria-label="confirm.title"
      @cancel.prevent="confirm = null"
    >
      <h2>{{ confirm.title }}</h2>
      <p>{{ confirm.description }}</p>
      <div class="button-row">
        <button @click="confirm = null">{{ t("ui.cancel") }}</button
        ><button
          class="primary"
          @click="
            confirm.run();
            confirm = null;
          "
        >
          {{ confirm.buttonLabel ?? t("ui.confirmAction") }}
        </button>
      </div>
    </dialog>
    <AppDialog
      v-if="bindingTarget"
      :title="t('ui.chooseConfiguration')"
      @close="bindingTarget = null"
    >
      <p>
        {{
          t("phrases.bindConfiguration", {
            model: bindingTarget.model,
            serial: bindingTarget.serial || t("ui.serialNumberNotProvided"),
          })
        }}
      </p>
      <div v-if="bindingProfiles.length" class="device-profile-list">
        <button
          v-for="profile in bindingProfiles"
          :key="profile.profile_id"
          class="device-profile-option"
          :disabled="w.switchingDevice || w.pending > 0"
          @click="bindAndOpenDevice(profile.profile_id)"
        >
          <span>
            <strong>{{ profile.name }}</strong>
            <small>{{ profile.definition.name }}</small>
          </span>
          <AppIcon name="arrow-right" :size="16" />
        </button>
      </div>
      <p v-else class="helper">
        {{ t("ui.noConfigurationsAvailableToLinkWait") }}
      </p>
      <p v-if="w.error" class="error-text" role="alert">{{ w.error }}</p>
      <div class="button-row dialog-actions">
        <button data-cancel @click="bindingTarget = null">
          {{ t("ui.cancel") }}
        </button>
      </div>
    </AppDialog>
    <AppDialog
      v-if="connectionDetails"
      :title="t('ui.deviceConnection2')"
      @close="closeConnectionDetails"
    >
      <p class="connection-detail-status">
        {{ w.currentProfile?.name ?? t("ui.devices") }} ·
        {{ connectionLabels[w.state.connection] ?? t("ui.unknownStatus") }}
      </p>
      <DeviceScope
        v-if="deviceViews?.details"
        :key="w.selectedProfileId"
        :profile-id="w.selectedProfileId"
        ><component :is="deviceViews.details"
      /></DeviceScope>
      <dl v-else-if="w.device" class="connection-details">
        <dt>{{ t("ui.model") }}</dt>
        <dd>{{ w.device.model }}</dd>
        <dt>{{ t("ui.firmware") }}</dt>
        <dd>{{ w.device.firmware }}</dd>
        <dt>{{ t("ui.serialNumber") }}</dt>
        <dd>{{ w.device.serial || t("ui.notProvided") }}</dd>
      </dl>
      <p v-else class="helper">{{ t("ui.connectTheDeviceViaUsbAnd") }}</p>
      <UiFailure
        v-if="w.state.error"
        :detail="w.state.error"
        :message="w.state.ui_message"
      />
      <div class="button-row dialog-actions">
        <button data-cancel @click="closeConnectionDetails">
          {{ t("ui.close") }}
        </button>
        <button
          :disabled="!w.desktop || w.pending > 0"
          @click="w.call('retry_connection')"
        >
          {{ t("ui.reconnectDevice") }}
        </button>
      </div>
    </AppDialog>
    <ContentPicker
      v-if="contentTarget"
      :visual="w.visual"
      @close="closeContentPicker"
      @select="chooseContent"
    />
    <AppDialog
      v-if="showSettings"
      :title="t('ui.appSettings')"
      @close="showSettings = false"
    >
      <div class="settings-brand">
        <img :src="logo" alt="" /><strong>Desktop Alchemy</strong>
      </div>
      <label>
        {{ t("app.language") }}
        <AppSelect
          :aria-label="t('app.language')"
          :model-value="w.language.preference"
          :options="[
            { value: 'system', label: t('app.system') },
            { value: 'zh-CN', label: t('app.chinese') },
            { value: 'en', label: 'English' },
          ]"
          :disabled="w.languageSaving"
          @update:model-value="w.setLanguage($event as LanguagePreference)"
        />
      </label>
      <p v-if="!w.desktop" class="helper">{{ t("app.previewLanguage") }}</p>
      <label
        >{{ t("ui.whenClosingTheMainWindow")
        }}<AppSelect
          :model-value="w.settings.close_behavior"
          :aria-label="t('ui.whenClosingTheMainWindow')"
          :options="[
            { value: 'tray', label: t('ui.minimizeToTray') },
            { value: 'exit', label: t('ui.quitApp') },
          ]"
          :disabled="!w.desktop || w.pending > 0"
          @change="
            w.setAppSettings({ close_behavior: $event as 'tray' | 'exit' })
          "
      /></label>
      <p class="helper">
        {{ t("ui.keyActionsAndDeviceControlsKeep") }}
      </p>
      <BackupControls />
      <p v-if="w.error" class="error-text" role="alert">{{ w.error }}</p>
      <div class="button-row dialog-actions">
        <button data-cancel @click="showSettings = false">
          {{ t("ui.done") }}
        </button>
      </div>
    </AppDialog>
    <AppDialog
      v-if="assetPicker"
      :title="t('ui.chooseImageAsset')"
      wide
      @close="assetPicker = false"
    >
      <LibraryBrowser
        pick
        @select="
          useAsset($event);
          assetPicker = false;
        "
      />
    </AppDialog>
    <AppDialog
      v-if="applicationPicker"
      :title="t('ui.chooseAppPreset')"
      wide
      @close="applicationPicker = false"
      ><ApplicationsBrowser
        pick
        @select="
          w.useApplication($event);
          applicationPicker = false;
        "
    /></AppDialog>
  </div>
  <ToastStack />
</template>

<style scoped>
.device-route-fields {
  border: 0;
  padding: 0;
  margin: 0;
  min-width: 0;
}
.saved-profiles summary {
  cursor: pointer;
}
.saved-profiles .button-row {
  margin-top: 12px;
}
</style>
