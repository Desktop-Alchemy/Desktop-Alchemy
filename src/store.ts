import {
  t,
  language,
  applyLanguage,
  browserLanguage,
  errorText,
  errorDetail,
  messageText,
} from "./i18n";
import { validateSceneSettings } from "./scenes";
import { pluginName, presentationName } from "./plugin-labels";
import { defineStore } from "pinia";
import { ref, computed, watch, onScopeDispose } from "vue";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  contentCatalog,
  contentSelection,
  contentVisual,
  isGifPattern,
  applyApplicationVisual,
  readRecentContents,
  writeRecentContents,
} from "./content";
import type {
  SceneSettings,
  SceneStatus,
  SceneCondition,
  ConditionTruth,
  RunningProgram,
  AppConfig,
  HardwareCommand,
  DeviceSettings,
  KeyVisual,
  KeyAction,
  WidgetSample,
  WidgetCatalog,
  Page,
  Snapshot,
  WorkspaceSnapshot,
  DeviceState,
  AppSettings,
  ModelAppearance,
  LibraryCatalog,
  LibraryEdit,
  LibraryImportResult,
  ApplicationEntry,
  RegisteredApplication,
  LaunchTarget,
  ShortcutTarget,
  ApplicationLibrary,
  ApplicationEdit,
  PluginPackageSummary,
  BackupSummary,
  BackupRestoreNotice,
  TrayAction,
  TrayState,
  UiLanguageState,
  UiError,
  LanguagePreference,
} from "./bindings";

let languageRevision = 0;
let languageInitialization: Promise<void> | undefined;
export function initializeLanguage(): Promise<void> {
  return (languageInitialization ??= (async () => {
    if (!isTauri()) {
      applyLanguage(browserLanguage());
      return;
    }
    let timeout: ReturnType<typeof setTimeout> | undefined;
    const load = async () => {
      await listen<UiLanguageState>("ui-language-changed", (e) => {
        languageRevision++;
        applyLanguage(e.payload);
      });
      const before = languageRevision;
      const state = await invoke<UiLanguageState>("ui_language");
      if (before === languageRevision) applyLanguage(state);
    };
    try {
      await Promise.race([
        load(),
        new Promise<never>((_, reject) => {
          timeout = setTimeout(
            () => reject(new Error("Language initialization timed out")),
            5000,
          );
        }),
      ]);
    } catch (error) {
      console.error("Language initialization failed", error);
    } finally {
      clearTimeout(timeout);
    }
  })());
}

// This client deliberately does not instantiate useDesktopAlchemy or its editor/save lifecycle.
export function useTrayPanel() {
  const state = ref<TrayState>({ connection: "disconnected", theme: "system" });
  const error = ref("");
  const busy = ref(false);
  const opened = ref(0);
  let disposed = false;
  let initialized = false;
  let revision = 0;
  const unlisten: (() => void)[] = [];
  onScopeDispose(() => {
    disposed = true;
    unlisten.splice(0).forEach((stop) => stop());
  });
  async function subscribe(
    event: string,
    callback: (state: TrayState) => void,
  ) {
    const stop = await listen<TrayState>(event, (e) => callback(e.payload));
    if (disposed) stop();
    else unlisten.push(stop);
  }
  function receive(next: TrayState) {
    revision++;
    state.value = next;
  }
  async function init() {
    if (initialized || disposed || !isTauri()) return;
    initialized = true;
    try {
      await subscribe("tray-state", receive);
      await subscribe("tray-opened", (next) => {
        receive(next);
        error.value = "";
        opened.value++;
      });
      if (disposed) return;
      const before = revision;
      const next = await invoke<TrayState>("tray_panel_ready");
      if (!disposed && before === revision) receive(next);
    } catch (e) {
      error.value = errorText(e);
      unlisten.splice(0).forEach((stop) => stop());
      initialized = false;
    }
  }
  async function action(action: TrayAction) {
    if (busy.value) return;
    busy.value = true;
    error.value = "";
    try {
      if (!isTauri()) throw new Error(t("ui.useTheTrayMenuInThe"));
      await invoke("tray_action", { action });
    } catch (e) {
      error.value = errorText(e);
    } finally {
      busy.value = false;
    }
  }
  async function dismiss() {
    if (!isTauri()) return;
    try {
      await invoke("dismiss_tray");
    } catch (e) {
      error.value = errorText(e);
    }
  }
  return { state, error, busy, opened, init, action, dismiss };
}

function clone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T;
}
export { newPage } from "./devices/models";
import { targetLabel } from "./launch";
import { newPage, initial, initialConfig, emptyVisual } from "./devices/models";
export const useDesktopAlchemy = defineStore("desktop-alchemy", () => {
  const desktop = ref(isTauri());
  const workspace = ref<WorkspaceSnapshot>({
    global: { theme: "system", managed_plugin_ids: [], disabled_plugins: [] },
    devices: [],
    discovered: [],
    plugins: [],
    logs: [],
  });
  const selectedProfileId = ref("");
  const switchingDevice = ref(false);
  const currentProfile = computed(() =>
    workspace.value.devices.find(
      (d) => d.profile_id === selectedProfileId.value,
    ),
  );
  const hasDeck = computed(
    () => !!currentProfile.value?.definition.supports_pages,
  );
  const hasDisplay = (key: number) =>
    !!currentProfile.value?.layout.display_keys.includes(key);
  type EditorCache = {
    draft: AppConfig;
    sceneDraft: SceneSettings;
    dirty: boolean;
    sceneDirty: boolean;
    saveError: string;
    sceneSaveError: string;
    editRevision: number;
    sceneRevision: number;
    selectedPage: string;
    selectedKey: number;
  };
  const editors = new Map<string, EditorCache>();
  // The revision belongs to the draft's base, never to the latest background snapshot.
  const draftBases = new Map<string, Record<string, string>>();
  function acceptBase(profileId: string, resource: string, revision?: string) {
    if (!revision) return;
    const bases = draftBases.get(profileId) ?? {};
    bases[resource] = revision;
    draftBases.set(profileId, bases);
  }
  function expectedBase(profileId: string, resource: string) {
    return draftBases.get(profileId)?.[resource];
  }
  const state = ref<Snapshot>({
    revisions: {},
    input_values: {},
    active_page: initial.id,
    connection: "disconnected",
    error: null,
    devices: [],
    selected_device: null,
    config: initialConfig,
    pressed: [],
    last_key_press: null,
    logs: [],
    receipts: [],
    plugins: [],
    sdk_pid: null,
    gif_status: null,
  });
  const sceneDraft = ref<SceneSettings>(clone(initialConfig.scene_settings));
  const sceneDirty = ref(false);
  const sceneSaving = ref(false);
  const sceneSaveError = ref("");
  const sceneStatus = ref<SceneStatus>({
    states: [],
    winner_id: null,
    retained_id: null,
    error: null,
  });
  let sceneRevision = 0;
  let sceneTimer: ReturnType<typeof setTimeout> | undefined;
  let scenePromise: Promise<boolean> | undefined;
  const widgetSamples = ref<WidgetSample[]>([]);
  const widgetRevision = ref(0);
  const contentChoices = computed(() => contentCatalog(state.value.plugins));
  const recentContents = ref(readRecentContents());
  const widgetOptions = computed(() => {
    const options = contentChoices.value.map((choice) => ({
      value: choice.value,
      label: choice.definition
        ? `${choice.name} · ${choice.provider}`
        : choice.name,
    }));
    const selected = visual.value?.widget;
    if (
      selected &&
      !options.some(
        (o) => o.value === `${selected.plugin_id}/${selected.widget_id}`,
      )
    )
      options.push({
        value: `${selected.plugin_id}/${selected.widget_id}`,
        get label() {
          return t("ui.pluginUnavailable");
        },
      });
    return options;
  });
  const widgetSelection = computed({
    get: () => contentSelection(visual.value),
    set: (selection: string) => {
      const choice = contentChoices.value.find((c) => c.value === selection);
      if (choice)
        Object.assign(visual.value, contentVisual(visual.value, choice));
    },
  });
  const widgetPluginName = computed(() =>
    ((manifest) => (manifest ? pluginName(manifest) : t("ui.pluginSettings")))(
      state.value.plugins.find(
        (p) => p.manifest.id === visual.value.widget?.plugin_id,
      )?.manifest,
    ),
  );
  function selectContent(value: string) {
    if (!contentChoices.value.some((c) => c.value === value)) return false;
    recentContents.value = [
      value,
      ...recentContents.value.filter((v) => v !== value),
    ].slice(0, 12);
    writeRecentContents(recentContents.value);
    if (widgetSelection.value === value) return false;
    widgetSelection.value = value;
    edit();
    return true;
  }
  const widgetPresentationOptions = computed(() => {
    const binding = visual.value.widget;
    if (!binding) return [];
    const definition = state.value.plugins
      .find((p) => p.manifest.id === binding.plugin_id)
      ?.manifest.widgets.find((w) => w.id === binding.widget_id);
    const options = (definition?.presentations ?? []).map((p) => ({
      value: p.id,
      label: presentationName(binding.plugin_id, p.id, p.name),
    }));
    if (
      binding.presentation_id &&
      !options.some((p) => p.value === binding.presentation_id)
    )
      options.push({
        value: binding.presentation_id,
        get label() {
          return t("ui.displayFormatUnavailable");
        },
      });
    return options;
  });
  const widgetPresentation = computed({
    get: () =>
      visual.value.widget?.presentation_id ??
      widgetPresentationOptions.value[0]?.value ??
      "",
    set: (id: string) => {
      if (visual.value.widget) visual.value.widget.presentation_id = id;
    },
  });
  const widgetDisplayStatus = computed(() => {
    const binding = visual.value.widget;
    if (!binding) return "";
    const plugin = state.value.plugins.find(
      (p) => p.manifest.id === binding.plugin_id,
    );
    if (!plugin) return t("ui.pluginUnavailableShowingTheButtonS");
    if (state.value.config.disabled_plugins.includes(binding.plugin_id))
      return t("ui.pluginDisabledEnableItInExtensions2");
    if (plugin.status === "error") return t("ui.pluginErrorRetryInExtensions");
    if (plugin.status === "starting") return t("ui.startingCollection");
    if (plugin.status === "stopping") return t("ui.stoppingCollection");
    if (plugin.status !== "running")
      return t("ui.collectionStartsAutomaticallyAfterSavingThe");
    const sample = widgetSamples.value.find(
      (s) =>
        s.plugin_id === binding.plugin_id &&
        s.data.widget_id === binding.widget_id &&
        (s.data.source_id ?? null) === (binding.source_id ?? null),
    );
    const valid = sample && Date.now() / 1000 - sample.updated_at < 5;
    if (valid && !sample?.data.valid && sample?.data.detail)
      return sample.data.detail;
    const definition = plugin.manifest.widgets.find(
      (d) => d.id === binding.widget_id,
    );
    if (definition?.select_source && !binding.source_id)
      return t("ui.chooseADataSource2");
    const chosen =
      binding.presentation_id &&
      binding.presentation_id !== definition?.presentations[0]?.id
        ? sample?.data.presentations[binding.presentation_id]
        : sample?.data;
    return valid && chosen?.valid
      ? t("ui.liveDisplayDisablingThePluginShows")
      : t("ui.waitingForValidData");
  });
  const draft = ref<AppConfig>(clone(initialConfig));
  const dirty = ref(false);
  const pending = ref(0);
  const notifications = ref<
    {
      id: number;
      kind: "success" | "error";
      message: string;
      detail?: string;
    }[]
  >([]);
  let notificationId = 0;
  function notify(
    message: string,
    kind: "success" | "error" = "success",
    detail?: string,
  ) {
    notifications.value.push({ id: ++notificationId, kind, message, detail });
    notifications.value = notifications.value.slice(-4);
  }
  function dismissNotification(id: number) {
    notifications.value = notifications.value.filter((n) => n.id !== id);
  }
  const lastToast = ref("");
  const lastError = ref("");
  const toast = computed({
    get: () => lastToast.value,
    set: (message: string) => {
      lastToast.value = message;
      if (message) notify(message);
    },
  });
  const error = computed({
    get: () => lastError.value,
    set: (message: string) => {
      lastError.value = message;
      if (message) notify(message, "error");
    },
  });
  const saving = ref(false);
  const activating = ref(false);
  const saveError = ref("");
  let editRevision = 0;
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let savePromise: Promise<boolean> | undefined;
  const saveStatus = computed(() =>
    saveError.value
      ? t("ui.automaticSaveFailed")
      : saving.value
        ? t("ui.saving")
        : dirty.value
          ? desktop.value
            ? t("ui.waitingToSaveAutomatically")
            : t("ui.previewChangesAreNotSaved")
          : t("ui.allChangesSaved"),
  );
  watch(
    () => state.value.error,
    (message) => {
      if (message)
        notify(
          state.value.ui_message
            ? messageText(state.value.ui_message)
            : t("errors.deviceFailed"),
          "error",
          message,
        );
    },
  );
  onScopeDispose(() => clearTimeout(saveTimer));
  const settings = ref<AppSettings>({
    language: "system",
    close_behavior: "tray",
    model_appearances: {},
  });
  const modelAppearance = computed<ModelAppearance>(
    () =>
      settings.value.model_appearances[selectedProfileId.value] ?? {
        type: "default",
      },
  );
  const library = ref<LibraryCatalog>({ revision: 0, folders: [], items: [] });
  let libraryLoadGeneration = 0;
  const libraryError = ref("");
  const applications = ref<ApplicationLibrary & { revision?: string }>({
    version: 2,
    items: [],
  });
  const applicationsError = ref("");
  const applicationsLoading = ref(false);
  const exitRequested = ref(0);
  const trayNavigation = ref<{ action: TrayAction } | null>(null);
  const selectedPage = ref(initial.id);
  const selectedKey = ref(1);
  const device = computed(() =>
    state.value.devices.find((d) => d.id === state.value.selected_device),
  );
  const page = computed(
    () =>
      draft.value.pages.find((p) => p.id === selectedPage.value) ??
      draft.value.pages[0] ??
      initial,
  );
  const visual = computed(
    () =>
      page.value.keys.find((k) => k.key === selectedKey.value) ?? emptyVisual,
  );
  const selectedInputId = computed(() => {
    const definition = currentProfile.value?.definition;
    const display = definition?.displays.find(
      (d) => d.key === selectedKey.value,
    );
    return definition?.inputs.find(
      (control) =>
        control.kind.type === "button" &&
        (control.id === display?.id ||
          control.id === String(selectedKey.value)),
    )?.id;
  });
  const keyAction = computed<KeyAction>({
    get: () =>
      page.value.bindings.find(
        (b) =>
          b.control_id === selectedInputId.value && b.trigger.type === "press",
      )?.action ?? { type: "none" },
    set: (action) => {
      if (!selectedInputId.value) return;
      const bindings = page.value.bindings;
      const binding = bindings.find(
        (b) =>
          b.control_id === selectedInputId.value && b.trigger.type === "press",
      );
      if (binding) binding.action = action;
      else
        bindings.push({
          control_id: selectedInputId.value,
          enabled: true,
          trigger: { type: "press" },
          action,
        });
    },
  });
  const ready = computed(
    () =>
      state.value.connection === "ready" &&
      !!device.value &&
      !currentProfile.value?.config_error,
  );
  async function call<T>(
    command: string,
    args: Record<string, unknown> = {},
    success?: string,
    background = false,
  ): Promise<T | undefined> {
    if (!background) {
      pending.value++;
      error.value = "";
    }
    try {
      if (!desktop.value) throw new Error(t("ui.useTheDesktopAppForThis"));
      const profileId = selectedProfileId.value;
      const deviceId = currentProfile.value?.device_id;
      if (
        [
          "hardware",
          "activate_page",
          "set_led_settings",
          "show_brightness_test",
          "verify_capability",
          "set_capability_enabled",
          "export_report",
          "retry_connection",
        ].includes(command)
      ) {
        if (!deviceId) throw new Error(t("ui.connectAndLinkADeviceFirst"));
        args = { deviceId, ...args };
      }
      if (
        [
          "save_config",
          "save_scenes",
          "pause_scenes",
          "preview_key",
          "scene_status",
          "retry_scene",
        ].includes(command)
      )
        args = { ...args, profileId };
      if (command === "snapshot") {
        command = "device_snapshot";
        args = { profileId };
      }
      if (command === "set_led_settings")
        args = { expectedRevision: state.value.revisions?.settings, ...args };
      const result = await invoke<T>(command, args);
      if (success) toast.value = success;
      return result;
    } catch (e) {
      if (!background) {
        lastError.value = errorText(e);
        notify(errorText(e), "error", errorDetail(e));
      }
      return undefined;
    } finally {
      if (!background) pending.value--;
    }
  }
  async function targetCall<T>(
    profileId: string,
    deviceId: string | null | undefined,
    command: string,
    args: Record<string, unknown> = {},
  ): Promise<T | undefined> {
    try {
      if (!desktop.value) throw new Error(t("ui.useTheDesktopAppForThis"));
      if (command === "snapshot") command = "device_snapshot";
      if (command === "set_led_settings") {
        const profile = workspace.value.devices.find(
          (p) => p.profile_id === profileId,
        );
        args = {
          expectedRevision: profile?.state.revisions?.settings,
          ...args,
        };
      }
      return await invoke<T>(command, { ...args, profileId, deviceId });
    } catch (e) {
      notify(errorText(e), "error");
      return undefined;
    }
  }
  async function moduleRequest<T>(
    deviceId: string,
    operation: string,
    payload: unknown = null,
  ): Promise<T> {
    if (!desktop.value) throw new Error(t("ui.useTheDesktopAppForThis"));
    return invoke<T>("module_request", { deviceId, operation, payload });
  }
  function receive(s: Snapshot) {
    // Older isolated UI fixtures may omit the additive fields.
    s.active_page ??= s.config.active_page;

    s.config.bindings ??= [];
    s.config.scene_settings ??= clone(initialConfig.scene_settings);
    state.value = s;
    if (!sceneDirty.value && !sceneSaving.value) {
      sceneDraft.value = clone(s.config.scene_settings);
      acceptBase(selectedProfileId.value, "scenes", s.revisions?.scenes);
    } else sceneDraft.value.paused = s.config.scene_settings.paused;
    if (!dirty.value && !saving.value) {
      draft.value = clone(s.config);
      acceptBase(selectedProfileId.value, "pages", s.revisions?.pages);
      acceptBase(selectedProfileId.value, "bindings", s.revisions?.bindings);
    } else {
      draft.value.device_settings = clone(s.config.device_settings);
      draft.value.theme = s.config.theme;
    }
    if (!draft.value.pages.some((p) => p.id === selectedPage.value))
      selectedPage.value = draft.value.pages[0]?.id ?? "";
  }
  function cacheEditor() {
    if (!selectedProfileId.value) return;
    editors.set(selectedProfileId.value, {
      draft: clone(draft.value),
      sceneDraft: clone(sceneDraft.value),
      dirty: dirty.value,
      sceneDirty: sceneDirty.value,
      saveError: saveError.value,
      sceneSaveError: sceneSaveError.value,
      editRevision,
      sceneRevision,
      selectedPage: selectedPage.value,
      selectedKey: selectedKey.value,
    });
  }
  function openProfile(profile: DeviceState) {
    selectedProfileId.value = profile.profile_id;
    state.value = clone(profile.state);
    sceneStatus.value = clone(profile.scene_status);
    const cached = editors.get(profile.profile_id);
    if (!cached?.dirty) {
      acceptBase(profile.profile_id, "pages", profile.state.revisions?.pages);
      acceptBase(
        profile.profile_id,
        "bindings",
        profile.state.revisions?.bindings,
      );
    }
    if (!cached?.sceneDirty)
      acceptBase(profile.profile_id, "scenes", profile.state.revisions?.scenes);
    draft.value = clone(cached?.draft ?? profile.state.config);
    sceneDraft.value = clone(
      cached?.sceneDraft ?? profile.state.config.scene_settings,
    );
    dirty.value = cached?.dirty ?? false;
    sceneDirty.value = cached?.sceneDirty ?? false;
    saveError.value = cached?.saveError ?? "";
    sceneSaveError.value = cached?.sceneSaveError ?? "";
    editRevision = cached?.editRevision ?? 0;
    sceneRevision = cached?.sceneRevision ?? 0;
    selectedPage.value = cached?.selectedPage ?? profile.state.active_page;
    selectedKey.value =
      cached?.selectedKey ??
      profile.layout.display_keys[0] ??
      profile.layout.input_keys[0] ??
      1;
    receive(profile.state);
  }
  async function selectDevice(profileId: string) {
    if (profileId === selectedProfileId.value || switchingDevice.value) return;
    switchingDevice.value = true;
    try {
      clearTimeout(saveTimer);
      clearTimeout(sceneTimer);
      // Settle existing writes against their original profile; failed drafts stay cached.
      await flushCurrent();
      cacheEditor();
      const profile = workspace.value.devices.find(
        (d) => d.profile_id === profileId,
      );
      if (profile) openProfile(profile);
    } finally {
      switchingDevice.value = false;
    }
  }
  function receiveWorkspace(next: WorkspaceSnapshot) {
    workspace.value = next;
    const profile = next.devices.find(
      (d) => d.profile_id === selectedProfileId.value,
    );
    if (profile) {
      receive(profile.state);
      sceneStatus.value = profile.scene_status;
    } else if (!selectedProfileId.value && next.devices[0])
      openProfile(next.devices[0]);
    state.value.plugins = next.plugins;
    state.value.logs = next.logs;
    state.value.config.theme = next.global.theme;
    state.value.config.disabled_plugins = next.global.disabled_plugins;
    draft.value.theme = next.global.theme;
  }
  async function bindDevice(deviceId: string) {
    await call("bind_device", { profileId: selectedProfileId.value, deviceId });
  }
  async function init() {
    if (desktop.value) {
      await listen<WidgetSample[]>("plugin-widgets", (event) => {
        widgetSamples.value = event.payload;
        widgetRevision.value++;
      });
      const beforeSamples = widgetRevision.value;
      const currentSamples = await call<WidgetSample[]>(
        "widget_samples",
        {},
        undefined,
        true,
      );
      if (currentSamples && beforeSamples === widgetRevision.value) {
        widgetSamples.value = currentSamples;
        widgetRevision.value++;
      }
      await listen("exit-requested", () => exitRequested.value++);
      await listen<TrayAction>("tray-navigate", (e) => {
        trayNavigation.value = { action: e.payload };
      });
      await listen<UiError>("action-error", (e) => {
        lastError.value = errorText(e.payload);
        notify(errorText(e.payload), "error", errorDetail(e.payload));
      });
      await listen<WorkspaceSnapshot>("workspace-state", (e) =>
        receiveWorkspace(e.payload),
      );
      await listen<AppSettings>("app-settings-changed", (event) => {
        settings.value = event.payload;
      });
      await listen("applications-changed", () => {
        void loadApplications();
      });
      await listen<number>("library-catalog-changed", () => {
        void loadLibrary();
      });
      await loadLibrary();
      await call("frontend_ready", {}, undefined, true);
      const snapshot = await invoke<WorkspaceSnapshot>("snapshot");
      receiveWorkspace(snapshot);
      const appSettings = await call<AppSettings>("app_settings");
      if (appSettings)
        settings.value = {
          ...appSettings,
          model_appearances: appSettings.model_appearances ?? {},
        };
      const notice = await call<BackupRestoreNotice | null>(
        "backup_restore_notice",
        {},
        undefined,
        true,
      );
      if (notice?.restored)
        toast.value = notice.ui_message
          ? messageText(notice.ui_message)
          : t("app.backupRestored");
      else if (notice)
        notify(
          notice.ui_message
            ? messageText(notice.ui_message)
            : t("errors.restoreFailed"),
          "error",
          notice.message,
        );
    }
  }
  async function setAppSettings(next: Pick<AppSettings, "close_behavior">) {
    if (
      (await call<null>(
        "set_app_settings",
        { settings: { ...settings.value, ...next } },
        t("ui.appSettingsSaved"),
      )) !== undefined
    )
      settings.value.close_behavior = next.close_behavior;
  }
  const languageSaving = ref(false);
  async function setLanguage(preference: LanguagePreference) {
    if (languageSaving.value) return;
    if (!desktop.value) {
      applyLanguage(browserLanguage(preference));
      return;
    }
    languageSaving.value = true;
    try {
      const before = languageRevision;
      const next = await invoke<UiLanguageState>("set_ui_language", {
        preference,
      });
      if (before === languageRevision) {
        languageRevision++;
        applyLanguage(next);
      }
      settings.value.language = language.preference;
    } catch (e) {
      notify(t("errors.languageSaveFailed"), "error", errorDetail(e));
    } finally {
      languageSaving.value = false;
    }
  }
  async function setModelAppearance(
    profileId: string,
    appearance: ModelAppearance,
  ) {
    if (
      (await call<null>(
        "set_model_appearance",
        { profileId, appearance: clone(appearance) },
        t("ui.modelBrandingSaved"),
      )) === undefined
    )
      return false;
    if (appearance.type === "default")
      delete settings.value.model_appearances[profileId];
    else settings.value.model_appearances[profileId] = clone(appearance);
    return true;
  }
  async function exportBackup(path: string) {
    if (!(await flushSave())) return false;
    return (
      (await call<null>(
        "export_backup",
        { path },
        t("ui.settingsBackupExported"),
      )) !== undefined
    );
  }
  async function inspectBackup(path: string) {
    return call<BackupSummary>("inspect_backup", { path });
  }
  async function importBackup(path: string, fingerprint: string) {
    clearTimeout(saveTimer);
    clearTimeout(sceneTimer);
    if (savePromise) await savePromise;
    if (scenePromise) await scenePromise;
    return (
      (await call<null>(
        "import_backup",
        { path, fingerprint },
        t("ui.backupVerifiedRestartingAndRestoringSettings"),
      )) !== undefined
    );
  }
  async function loadLibrary() {
    const generation = ++libraryLoadGeneration;
    libraryError.value = "";
    if (!desktop.value) {
      libraryError.value = t("ui.manageLocalAssetsInTheDesktop");
      return;
    }
    try {
      const catalog = await invoke<LibraryCatalog>("library_snapshot");
      if (generation === libraryLoadGeneration) library.value = catalog;
    } catch (e) {
      libraryError.value = errorText(e);
    }
  }
  async function editLibrary(
    edit: LibraryEdit,
    expectedRevision = library.value.personal_revision,
  ) {
    ++libraryLoadGeneration;
    const result = await call<LibraryCatalog>("library_edit", {
      edit,
      expectedRevision,
    });
    ++libraryLoadGeneration;
    if (result && result.revision >= library.value.revision)
      library.value = result;
    else if (result) await loadLibrary();
    return result;
  }
  async function loadApplications() {
    applicationsError.value = "";
    if (!desktop.value) {
      applicationsError.value = t("ui.manageAppPresetsInTheDesktop");
      return;
    }
    applicationsLoading.value = true;
    try {
      applications.value = await invoke<
        ApplicationLibrary & { revision?: string }
      >("applications_snapshot");
    } catch (e) {
      applicationsError.value = errorText(e);
    } finally {
      applicationsLoading.value = false;
    }
  }
  async function editApplications(
    edit: ApplicationEdit,
    expectedRevision = applications.value.revision,
  ) {
    const result = await call<ApplicationLibrary & { revision?: string }>(
      "applications_edit",
      {
        edit,
        expectedRevision,
      },
    );
    if (result) applications.value = result;
    return result;
  }
  function registeredApplications(background = false) {
    return call<RegisteredApplication[]>(
      "registered_applications",
      {},
      undefined,
      background,
    );
  }
  const registeredIcons = new Map<string, Promise<string | undefined>>();
  let activeIcons = 0;
  const iconWaiters: (() => void)[] = [];
  function registeredApplicationIcon(target: LaunchTarget) {
    const key = JSON.stringify(target);
    const existing = registeredIcons.get(key);
    if (existing) return existing;
    const task = (async () => {
      if (activeIcons >= 3)
        await new Promise<void>((resolve) => iconWaiters.push(resolve));
      activeIcons++;
      try {
        return await call<string>(
          "registered_application_icon",
          { target },
          undefined,
          true,
        );
      } finally {
        activeIcons--;
        iconWaiters.shift()?.();
      }
    })();
    registeredIcons.set(key, task);
    return task;
  }
  function testLaunchTarget(target: LaunchTarget) {
    return call<null>(
      "test_launch_target",
      { target: clone(target) },
      t("ui.windowsAcceptedTheLaunchRequest"),
    );
  }
  function resolveShortcut(path: string) {
    return call<ShortcutTarget>("resolve_shortcut", { path });
  }
  function useApplication(application: ApplicationEntry) {
    if (!hasDeck.value) {
      const key = currentProfile.value?.layout.input_keys[0];
      if (key === undefined) return;
      const binding = draft.value.bindings.find(
        (b) => b.control_id === String(key) && b.trigger.type === "press",
      );
      const action: KeyAction = {
        type: "launch",
        target: clone(application.target),
      };
      if (binding) binding.action = action;
      else
        draft.value.bindings.push({
          control_id: String(key),
          enabled: false,
          trigger: { type: "press" },
          action,
        });
      edit();
      return;
    }
    keyAction.value = { type: "launch", target: clone(application.target) };
    const beforeApplication = clone(visual.value);
    if (hasDisplay(selectedKey.value))
      applyApplicationVisual(visual.value, application);
    if (
      application.target.type === "registered_application" &&
      !application.icon_asset_id &&
      hasDisplay(selectedKey.value)
    ) {
      const profileId = selectedProfileId.value;
      const pageId = selectedPage.value;
      const key = selectedKey.value;
      const signature = JSON.stringify(application.target);
      void registeredApplicationIcon(application.target).then((icon) => {
        if (
          !icon ||
          selectedProfileId.value !== profileId ||
          selectedPage.value !== pageId ||
          selectedKey.value !== key
        )
          return;
        const current = visual.value;
        if (
          keyAction.value.type === "launch" &&
          JSON.stringify(keyAction.value.target) === signature &&
          !current.plugin_icon &&
          (!current.asset_id ||
            current.asset_id === current.default_icon_asset_id)
        ) {
          // Only apply default mode changes if the user has not edited the visual while resolving the icon.
          const pendingVisual = clone(beforeApplication);
          applyApplicationVisual(pendingVisual, application);
          if (JSON.stringify(current) !== JSON.stringify(pendingVisual)) return;
          const resolved = clone(beforeApplication);
          applyApplicationVisual(resolved, {
            ...application,
            icon_asset_id: icon,
          });
          Object.assign(current, resolved);
          edit();
        }
      });
    }
    edit();
  }
  async function importLibrary(
    paths: string[],
    folderId: string | null = null,
  ) {
    const result = await call<LibraryImportResult[]>("library_import", {
      paths,
      folderId,
    });
    if (result) await loadLibrary();
    return result;
  }
  function flushPages(): Promise<boolean> {
    clearTimeout(saveTimer);
    if (savePromise) return savePromise;
    if (!dirty.value) return Promise.resolve(true);
    if (!desktop.value) return Promise.resolve(false);
    saving.value = true;
    saveError.value = "";
    savePromise = (async () => {
      while (dirty.value) {
        const revision = editRevision;
        try {
          const profileId = selectedProfileId.value;
          const resource = hasDeck.value ? "pages" : "bindings";
          let saved: { revision?: string } | undefined;
          if (hasDeck.value)
            saved = await invoke("save_config", {
              profileId,
              expectedRevision: expectedBase(profileId, resource),
              config: clone(draft.value),
            });
          else
            saved = await invoke("save_input_bindings", {
              profileId,
              expectedRevision: expectedBase(profileId, resource),
              bindings: clone(draft.value.bindings),
            });
          acceptBase(profileId, resource, saved?.revision);
          // Keep newer edits separate from snapshots received during this save.
          const s = await call<Snapshot>("snapshot", {}, undefined, true);
          if (s) receive(s);
          if (revision === editRevision) dirty.value = false;
        } catch (e) {
          saveError.value = errorText(e);
          notify(
            t("ui.couldNotSaveConfigurationAutomatically", {
              arg0: saveError.value,
            }),
            "error",
          );
          return false;
        }
      }
      return true;
    })().finally(() => {
      saving.value = false;
      savePromise = undefined;
    });
    return savePromise;
  }
  function editScenes() {
    sceneDirty.value = true;
    sceneRevision++;
    sceneSaveError.value = "";
    clearTimeout(sceneTimer);
    if (desktop.value) sceneTimer = setTimeout(() => void flushScenes(), 650);
  }
  function flushScenes(): Promise<boolean> {
    clearTimeout(sceneTimer);
    if (scenePromise) return scenePromise;
    if (!sceneDirty.value) return Promise.resolve(true);
    if (!desktop.value) return Promise.resolve(false);
    sceneSaving.value = true;
    scenePromise = (async () => {
      if (!(await flushPages())) return false;
      while (sceneDirty.value) {
        const revision = sceneRevision;
        try {
          const validation = validateSceneSettings(
            sceneDraft.value,
            state.value.config.pages,
          );
          if (validation) throw new Error(validation);
          const profileId = selectedProfileId.value;
          const saved = await invoke<{ revision?: string }>("save_scenes", {
            profileId,
            expectedRevision: expectedBase(profileId, "scenes"),
            settings: clone(sceneDraft.value),
          });
          acceptBase(profileId, "scenes", saved?.revision);
          if (revision === sceneRevision) sceneDirty.value = false;
          const snapshot = await call<Snapshot>(
            "snapshot",
            {},
            undefined,
            true,
          );
          if (snapshot) receive(snapshot);
          sceneSaveError.value = "";
        } catch (e) {
          sceneSaveError.value = errorText(e);
          notify(
            t("ui.couldNotSaveScenesAutomatically", {
              arg0: sceneSaveError.value,
            }),
            "error",
          );
          return false;
        }
      }
      return true;
    })().finally(() => {
      sceneSaving.value = false;
      scenePromise = undefined;
    });
    return scenePromise;
  }
  async function flushCurrent(): Promise<boolean> {
    if (!(await flushPages())) return false;
    return flushScenes();
  }
  async function flushSave(): Promise<boolean> {
    if (!(await flushCurrent())) return false;
    for (const [profileId, cached] of editors) {
      if (profileId === selectedProfileId.value) continue;
      try {
        if (cached.dirty) {
          const profile = workspace.value.devices.find(
            (profile) => profile.profile_id === profileId,
          );
          if (profile?.definition.supports_pages)
            await invoke("save_config", {
              profileId,
              config: clone(cached.draft),
              expectedRevision: expectedBase(profileId, "pages"),
            });
          else
            await invoke("save_input_bindings", {
              profileId,
              bindings: clone(cached.draft.bindings),
              expectedRevision: expectedBase(profileId, "bindings"),
            });
          cached.dirty = false;
        }
        if (cached.sceneDirty) {
          await invoke("save_scenes", {
            profileId,
            settings: clone(cached.sceneDraft),
            expectedRevision: expectedBase(profileId, "scenes"),
          });
          cached.sceneDirty = false;
        }
        cached.saveError = "";
        cached.sceneSaveError = "";
      } catch (e) {
        cached.saveError = errorText(e);
        notify(
          t("ui.couldNotSaveDeviceConfiguration", { arg0: errorText(e) }),
          "error",
        );
        return false;
      }
    }
    return true;
  }
  async function reloadSavedDrafts() {
    if (saving.value || sceneSaving.value) return false;
    const profileId = selectedProfileId.value;
    clearTimeout(saveTimer);
    clearTimeout(sceneTimer);
    const snapshot = await invoke<Snapshot>("device_snapshot", { profileId });
    if (
      profileId !== selectedProfileId.value ||
      saving.value ||
      sceneSaving.value
    )
      return false;
    dirty.value = false;
    sceneDirty.value = false;
    saveError.value = "";
    sceneSaveError.value = "";
    editors.delete(profileId);
    receive(snapshot);
    return true;
  }
  async function pauseScenes(paused: boolean) {
    if (!(await flushScenes())) return;
    await call("pause_scenes", { paused });
  }
  function runningPrograms() {
    return call<RunningProgram[]>("running_programs");
  }
  function checkSceneCondition(condition: SceneCondition) {
    return call<ConditionTruth>("check_scene_condition", {
      condition: clone(condition),
    });
  }
  onScopeDispose(() => clearTimeout(sceneTimer));
  async function activatePage() {
    if (activating.value) return;
    const profileId = selectedProfileId.value;
    const id = selectedPage.value;
    activating.value = true;
    try {
      if (!(await flushSave())) return;
      if (
        selectedProfileId.value !== profileId ||
        !draft.value.pages.some((p) => p.id === id)
      )
        return;
      const result = await call<null>(
        "activate_page",
        { pageId: id },
        t("ui.pageActivated"),
      );
      if (result !== undefined) {
        const s = await call<Snapshot>("snapshot", {}, undefined, true);
        if (s) receive(s);
      }
    } finally {
      activating.value = false;
    }
  }
  async function hardware(command: HardwareCommand, allowPersistent = false) {
    return call<string>(
      "hardware",
      { command, allowPersistent },
      t("ui.deviceOperationSubmitted"),
    );
  }
  async function setLedSettings(settings: DeviceSettings) {
    return call<void>(
      "set_led_settings",
      { settings: clone(settings) },
      t("ui.ledSettingsSaved"),
    );
  }
  function edit() {
    if (currentProfile.value?.config_error) {
      saveError.value = currentProfile.value.config_error;
      return;
    }
    editRevision++;
    dirty.value = true;
    saveError.value = "";
    clearTimeout(saveTimer);
    const actions = draft.value.pages
      .flatMap((page) => page.bindings.map((binding) => binding.action))
      .concat(draft.value.bindings.map((binding) => binding.action));
    const complete = actions.every(
      (action) =>
        action.type !== "launch" || !!targetLabel(action.target).trim(),
    );
    if (desktop.value && complete)
      saveTimer = setTimeout(() => void flushSave(), 650);
  }
  async function addPage() {
    const profileId = selectedProfileId.value;
    const p = desktop.value
      ? await targetCall<Page>(
          profileId,
          currentProfile.value?.device_id,
          "create_page",
          { name: t("ui.page", { arg0: draft.value.pages.length + 1 }) },
        )
      : newPage(
          t("ui.page", { arg0: draft.value.pages.length + 1 }),
          currentProfile.value?.definition,
        );
    if (!p || selectedProfileId.value !== profileId) return;
    draft.value.pages.push(p);
    selectedPage.value = p.id;
    edit();
  }
  function duplicatePage() {
    const p = clone(page.value);
    p.id = crypto.randomUUID();
    p.name = t("ui.copy", { arg0: p.name });
    draft.value.pages.push(p);
    selectedPage.value = p.id;
    edit();
  }
  function movePage(delta: number) {
    const pages = draft.value.pages;
    const i = pages.findIndex((p) => p.id === page.value.id);
    const j = i + delta;
    if (j >= 0 && j < pages.length) {
      [pages[i], pages[j]] = [pages[j], pages[i]];
      edit();
    }
  }
  function deletePage() {
    if (draft.value.pages.length < 2) return;
    const id = page.value.id;
    draft.value.pages = draft.value.pages.filter((p) => p.id !== id);
    for (const binding of [
      ...draft.value.bindings,
      ...draft.value.pages.flatMap((p) => p.bindings),
    ])
      if (binding.action.type === "page" && binding.action.page_id === id)
        binding.action = { type: "none" };
    if (draft.value.active_page === id)
      draft.value.active_page = draft.value.pages[0]?.id ?? "";
    selectedPage.value = draft.value.pages[0]?.id ?? "";
    edit();
  }
  async function inspectPluginPackage(path: string | null) {
    return path === null
      ? call<PluginPackageSummary>("inspect_example")
      : call<PluginPackageSummary>("inspect_plugin_package", { path });
  }
  async function installPluginPackage(
    path: string | null,
    fingerprint: string,
  ) {
    return path === null
      ? call<string>(
          "install_example",
          { fingerprint },
          t("ui.pluginInstalled"),
        )
      : call<string>(
          "install_plugin_package",
          { path, fingerprint },
          t("ui.pluginInstalled"),
        );
  }
  async function removePlugin(id: string) {
    return call<null>("remove_plugin", { id }, t("ui.pluginUninstalled"));
  }
  async function setPluginEnabled(id: string, enabled: boolean) {
    return call<null>("set_plugin_enabled", { id, enabled });
  }
  async function preview(v: KeyVisual) {
    if (!desktop.value) return undefined;
    return isGifPattern(v) && !v.widget
      ? call<string>("preview_asset", { assetId: v.asset_id }, undefined, true)
      : call<string>("preview_key", { visual: clone(v) }, undefined, true);
  }
  return {
    targetCall,
    moduleRequest,
    workspace,
    selectedProfileId,
    currentProfile,
    switchingDevice,
    hasDeck,
    hasDisplay,
    selectDevice,
    bindDevice,
    desktop,
    sceneDraft,
    sceneDirty,
    sceneSaving,
    sceneSaveError,
    sceneStatus,
    editScenes,
    flushScenes,
    pauseScenes,
    runningPrograms,
    checkSceneCondition,
    state,
    discoverWidgetSources: (id: string, lease: string) =>
      invoke<WidgetCatalog>("discover_widget_sources", { id, lease }),
    releaseWidgetDiscovery: (id: string, lease: string) =>
      invoke<void>("release_widget_discovery", { id, lease }),
    enableHardwareSensors: () => invoke<void>("enable_hardware_sensors"),
    widgetSamples,
    widgetRevision,
    contentChoices,
    recentContents,
    selectContent,
    widgetOptions,
    widgetSelection,
    widgetPluginName,
    widgetPresentationOptions,
    widgetPresentation,
    widgetDisplayStatus,
    setPluginEnabled,
    inspectPluginPackage,
    installPluginPackage,
    removePlugin,
    draft,
    dirty,
    pending,
    toast,
    error,
    notifications,
    notify,
    dismissNotification,
    saving,
    activating,
    saveError,
    saveStatus,
    settings,
    library,
    libraryError,
    applications,
    applicationsError,
    applicationsLoading,
    loadApplications,
    editApplications,
    registeredApplications,
    registeredApplicationIcon,
    testLaunchTarget,
    resolveShortcut,
    useApplication,
    exitRequested,
    trayNavigation,
    setAppSettings,
    language,
    languageSaving,
    setLanguage,
    modelAppearance,
    setModelAppearance,
    exportBackup,
    inspectBackup,
    importBackup,
    loadLibrary,
    editLibrary,
    importLibrary,
    selectedPage,
    selectedKey,
    device,
    page,
    visual,
    keyAction,
    selectedInputId,
    ready,
    call,
    init,
    flushSave,
    reloadSavedDrafts,
    activatePage,
    hardware,
    setLedSettings,
    edit,
    addPage,
    duplicatePage,
    movePage,
    deletePage,
    preview,
  };
});
