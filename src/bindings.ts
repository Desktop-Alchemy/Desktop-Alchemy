// Generated from Rust model.rs. Run pnpm bindings.
export type JsonValue = unknown;
export type DeviceCapabilities = Record<string, Capability>;
export type DayFilter = { "type": "every_day" } | { "type": "weekdays", days: Array<number>, } | { "type": "date", date: string, };
export type SceneTime = { days: DayFilter, time: string, };
export type SceneTrigger = { "type": "continuous" } | { "type": "scheduled", schedule: SceneTime, };
export type ProcessMatcher = { "type": "name", value: string, } | { "type": "path", value: string, };
export type SceneCondition = { "type": "all", children: Array<SceneCondition>, } | { "type": "any", children: Array<SceneCondition>, } | { "type": "not", child: SceneCondition, } | { "type": "process", matcher: ProcessMatcher, } | { "type": "foreground", matcher: ProcessMatcher, } | { "type": "media_playing" } | { "type": "time_range", days: DayFilter, start: string, end: string, };
export type Scene = { id: string, name: string, enabled: boolean, page_id: string, trigger: SceneTrigger, condition: SceneCondition, enter_delay_ms: number, exit_delay_ms: number, };
export type SceneFallback = { "type": "keep" } | { "type": "return" } | { "type": "page", page_id: string, };
export type SceneSettings = { enabled: boolean, paused: boolean, process_interval_ms: number, fallback: SceneFallback, scenes: Array<Scene>, };
export type ConditionTruth = "true" | "false" | "unknown";
export type RunningProgram = { pid: number, name: string, path: string | null, };
export type SceneState = { id: string, truth: ConditionTruth, status: string, };
export type SceneStatus = { ui_message?: UiMessage, states: Array<SceneState>, winner_id: string | null, retained_id: string | null, error: string | null, };
export type InputValue = { "type": "button", control_id: string, pressed: boolean, } | { "type": "rotary", control_id: string, delta: number, } | { "type": "analog", control_id: string, value: number, };
export type InputKind = { "type": "button" } | { "type": "rotary" } | { "type": "analog", min: number, max: number, unit: string, };
export type InputControl = { id: string, name: string, kind: InputKind, };
export type DisplayRegion = { id: string, key: number, width: number, height: number, column: number, row: number, };
export type InputTrigger = { "type": "press" } | { "type": "clockwise" } | { "type": "counterclockwise" } | { "type": "above", threshold: number, hysteresis: number, } | { "type": "below", threshold: number, hysteresis: number, };
export type ControlBinding = { control_id: string, enabled: boolean, trigger: InputTrigger, action: KeyAction, };
export type DeviceDto = { device_id: string | null, adapter_id: string, name: string, connection: string, error: string | null, definition: DeviceDefinition, capabilities: { [key in string]?: Capability }, inputs: { [key in string]?: InputValue }, firmware: string | null, gif_status: string | null, };
export type ErrorCode = "invalid_argument" | "not_found" | "offline" | "unsupported" | "config_unavailable" | "resource_exhausted" | "stale_operation" | "version_unsupported" | "internal";
export type ApiError = { code: ErrorCode, message: string, };
export type Verification = "unverified" | "verified" | "unsupported" | "failed";
export type Capability = { available: boolean, verification: Verification, reason: string, };
export type DeviceLayout = { input_keys: Array<number>, display_keys: Array<number>, columns: number, key_width: number, key_height: number, background_width: number, background_height: number, led_count: number, };
export type DeviceProfile = { id: string, adapter_id: string, schema_version: number, name: string, device_id: string | null, config: AppConfig, config_version: number, module_data: JsonValue, };
export type GlobalConfig = { theme: string, managed_plugin_ids: Array<string>, disabled_plugins: Array<string>, } & ({ [key in string]?: number | string | boolean | Array<JsonValue> | { [key in string]?: JsonValue } | null });
export type WorkspaceConfig = { format: string, version: number, global: GlobalConfig, profiles: Array<DeviceProfile>, } & ({ [key in string]?: number | string | boolean | Array<JsonValue> | { [key in string]?: JsonValue } | null });
export type DeviceState = { ui_message?: UiMessage, module_state: JsonValue, module_available: boolean, config_error: string | null, definition: DeviceDefinition, profile_id: string, adapter_id: string, name: string, device_id: string | null, layout: DeviceLayout, state: Snapshot, scene_status: SceneStatus, };
export type WorkspaceSnapshot = { global: GlobalConfig, devices: Array<DeviceState>, discovered: Array<DeviceDescriptor>, plugins: Array<PluginInfo>, logs: Array<LogEntry>, };
export type DeviceDescriptor = { adapter_id: string, layout: DeviceLayout, id: string, model: string, serial: string, firmware: string, vendor_id: number, product_id: number, capabilities: { [key in string]?: Capability }, };
export type DeviceEvent = { input: InputValue | null, at: number, kind: string, device_id: string, key: number | null, pressed: boolean | null, page_id: string | null, };
export type HardwareCommand = { "type": "screen_brightness", value: number, } | { "type": "led_brightness", value: number, } | { "type": "led_color", color: string, } | { "type": "led_colors", colors: Array<[number, number, number]>, } | { "type": "reset_led" } | { "type": "key_image", key: number, asset_id: string, } | { "type": "key_data", key: number, data: string, } | { "type": "bulk_image", keys: Array<number>, asset_id: string, } | { "type": "background", asset_id: string, persistent: boolean, } | { "type": "background_data", data: string, persistent: boolean, } | { "type": "clear_key", key: number, } | { "type": "clear_all" } | { "type": "refresh" } | { "type": "key_gif", key: number, asset_id: string, } | { "type": "clear_gif", key: number, } | { "type": "start_gif" } | { "type": "stop_gif" } | { "type": "gif_status" } | { "type": "firmware" };
export type DeviceCommand = { "type": "screen_brightness", value: number, } | { "type": "led_brightness", value: number, } | { "type": "led_color", color: string, } | { "type": "led_colors", colors: Array<[number, number, number]>, } | { "type": "reset_led" } | { "type": "key_image", key: number, asset_id: string, } | { "type": "key_data", key: number, data: string, } | { "type": "bulk_image", keys: Array<number>, asset_id: string, } | { "type": "background", asset_id: string, } | { "type": "background_data", data: string, } | { "type": "clear_key", key: number, } | { "type": "clear_all" } | { "type": "refresh" } | { "type": "key_gif", key: number, asset_id: string, } | { "type": "clear_gif", key: number, } | { "type": "start_gif" } | { "type": "stop_gif" } | { "type": "gif_status" } | { "type": "firmware" };
export type DisplayVisual = { key: number, text: string, foreground: string, background: string, asset_id: string | null, plugin_icon: PluginIconRef | null, };
export type RegisteredIdentity = { "type": "apps_folder", parsing_name: string, } | { "type": "aumid", id: string, };
export type LaunchTarget = { "type": "program", path: string, arguments: string, working_directory: string | null, } | { "type": "registered_application", identity: RegisteredIdentity, arguments: string, } | { "type": "uri", uri: string, };
export type DeviceDefinition = { inputs: Array<InputControl>, displays: Array<DisplayRegion>, adapter_id: string, name: string, layout: DeviceLayout, supports_pages: boolean, input_names: { [key in number]?: string }, };
export type KeyAction = { "type": "launch", target: LaunchTarget, } | { "type": "none" } | { "type": "previous" } | { "type": "home" } | { "type": "next" } | { "type": "page", page_id: string, };
export type KeyContentMode = "none" | "text" | "pattern";
export type TextPosition = "top" | "center" | "bottom";
export type CloseBehavior = "tray" | "exit";
export type ModelAppearance = { "type": "default" } | { "type": "text", text: string, } | { "type": "image", image: ImageRef, };
export type AppSettings = { language: LanguagePreference, close_behavior: CloseBehavior, model_appearances: { [key in string]?: ModelAppearance }, };
export type LanguagePreference = "system" | "zh-CN" | "en";
export type UiLocale = "zh-CN" | "en";
export type UiLanguageState = { preference: LanguagePreference, locale: UiLocale, };
export type UiMessage = { key: string, args: { [key in string]?: JsonValue }, };
export type UiError = { message: UiMessage, detail: string, };
export type TrayAction = "device" | "pages" | "plugins" | "show" | "settings" | "exit";
export type TrayState = { connection: string, theme: string, };
export type BackupSummary = { format_version: number, app_version: string, created_at: number, pages: number, scenes: number, images: number, folders: number, library_items: number, applications: number, plugins: number, total_bytes: number,
/**
 * SHA-256 of the exact archive reviewed by the user.
 */
fingerprint: string, };
export type BackupRestoreNotice = { ui_message?: UiMessage, restored: boolean, message: string, };
export type PluginIconRef = { plugin_id: string, icon_id: string, };
export type ImageRef = { "type": "asset", asset_id: string, } | { "type": "plugin_icon", plugin_id: string, icon_id: string, };
export type CatalogFolder = { id: string, parent_id: string | null, name: string, plugin_id: string | null, };
export type CatalogItem = { id: string, folder_id: string | null, name: string, image: ImageRef, revision: string, };
export type LibraryCatalog = { personal_revision?: string, revision: number, folders: Array<CatalogFolder>, items: Array<CatalogItem>, };
export type PluginIconDefinition = { id: string, name: string, path: string, };
export type PluginIconInfo = { id: string, name: string, origin: string, revision: string, };
export type LibraryFolder = { id: string, parent_id: string | null, name: string, };
export type LibraryItem = { id: string, folder_id: string | null, name: string, asset_id: string, };
export type AssetLibrary = { version: number, folders: Array<LibraryFolder>, items: Array<LibraryItem>, };
export type ApplicationEntry = { id: string, name: string, target: LaunchTarget, icon_asset_id: string | null, };
export type RegisteredApplication = { id: string, name: string, target: LaunchTarget, icon_asset_id: string | null, };
export type ShortcutTarget = { path: string, arguments: string, working_directory: string | null, };
export type ApplicationLibrary = { version: number, items: Array<ApplicationEntry>, };
export type ApplicationEdit = { "type": "refresh_icon", id: string, } | { "type": "save", id: string | null, name: string, target: LaunchTarget, } | { "type": "delete", id: string, };
export type LibraryImportResult = { path: string, item: LibraryItem | null, error: string | null, };
export type LibraryEdit = { "type": "create_folder", parent_id: string | null, name: string, } | { "type": "rename_folder", id: string, name: string, } | { "type": "rename_item", id: string, name: string, } | { "type": "move", folder_ids: Array<string>, item_ids: Array<string>, parent_id: string | null, } | { "type": "delete", folder_ids: Array<string>, item_ids: Array<string>, };
export type KeyVisual = { key: number, text: string,
/**
 * Missing on existing pages: infer the base content without changing its appearance.
 */
content_mode: KeyContentMode | null, show_text: boolean, text_position: TextPosition, foreground: string, background: string, asset_id: string | null, plugin_icon: PluginIconRef | null,
/**
 * Last automatically applied app icon. Explicit image selection clears this.
 */
default_icon_asset_id: string | null, widget: WidgetBinding | null, };
export type WidgetStyle = { foreground: string, background: string, accent: string, show_progress: boolean, };
export type WidgetBinding = { plugin_id: string, widget_id: string, style: WidgetStyle, presentation_id: string | null, source_id: string | null, source_name: string | null, };
export type WidgetDefinition = { id: string, name: string, default_style: WidgetStyle, presentations: Array<WidgetPresentation>, select_source: boolean, };
export type WidgetPresentation = { id: string, name: string, };
export type WidgetContent = { title: string, value: string, secondary_value: string | null, detail: string | null, progress: number | null, valid: boolean, };
export type WidgetData = { widget_id: string, source_id: string | null, title: string, value: string, secondary_value: string | null, detail: string | null, progress: number | null, valid: boolean, presentations: { [key in string]?: WidgetContent }, };
export type WidgetSample = { plugin_id: string, data: WidgetData, updated_at: number, };
export type WidgetSource = { ui_message?: UiMessage, widget_id: string, id: string, name: string, available: boolean, error: string | null, };
export type WidgetCatalog = { ui_message?: UiMessage, sources: Array<WidgetSource>, hardware_status: string, message: string | null, };
export type Page = { bindings: Array<ControlBinding>, id: string, name: string, keys: Array<KeyVisual>, background_asset: string | null, };
export type LedMode = "solid" | "individual" | "breathing" | "color_cycle" | "rainbow" | "wave" | "comet" | "device_default";
export type DeviceSettings = { screen_brightness: number, led_brightness: number, led_color: string, led_mode: LedMode, led_speed: number, led_colors: Array<[number, number, number]>, };
export type AppConfig = { disabled_capabilities: Array<string>, bindings: Array<ControlBinding>, scene_settings: SceneSettings, version: number, pages: Array<Page>, active_page: string, theme: string, device_settings: DeviceSettings, verification: { [key in string]?: { [key in string]?: Capability } }, managed_plugin_ids: Array<string>, disabled_plugins: Array<string>, };
export type PluginManifest = { id: string, name: string, version: string, api_version: number, executable: string | null, permissions: Array<string>, widgets: Array<WidgetDefinition>, icons: Array<PluginIconDefinition>, };
export type PluginInfo = { ui_message?: UiMessage, manifest: PluginManifest, path: string, status: string, error: string | null, built_in: boolean, };
export type PluginPackageSummary = { manifest: PluginManifest, fingerprint: string, installed_version: string | null, total_bytes: number, };
export type LogEntry = { at: number, level: string, message: string, };
export type CommandReceipt = { device_id: string, id: string, command: string, status: string, at: number, detail: string, };
export type Snapshot = { revisions: { [key in string]?: string }, ui_message?: UiMessage, input_values: { [key in string]?: InputValue }, active_page: string, connection: string, error: string | null, devices: Array<DeviceDescriptor>, selected_device: string | null, config: AppConfig, pressed: Array<number>, last_key_press: number | null, logs: Array<LogEntry>, receipts: Array<CommandReceipt>, plugins: Array<PluginInfo>, sdk_pid: number | null, gif_status: string | null, };
export type AudioDockConnection = "loading" | "ready" | "offline" | "busy" | "error";
export type AudioDockNoiseSuppression = "standard" | "low";
export type AudioDockEqualizerPreset = { id: number, name: string,
/**
 * Low, mid-low, mid, mid-high, high; signed dB, -12..=12.
 */
gains: [number, number, number, number, number], factory: boolean, };
export type AudioDockDeviceInfo = { model: string | null, serial: string | null, hardware_version: string | null, color_code: string | null, firmware_version: string | null, };
export type AudioDockSettings = { prompt_tone: boolean | null, prompt_volume: number | null, push_to_talk: boolean | null, noise_suppression: AudioDockNoiseSuppression | null, active_equalizer: number | null, equalizer_presets: Array<AudioDockEqualizerPreset>, free_equalizer_slots: Array<number>, };
export type AudioDockState = { ui_message?: UiMessage, device_id: string, connection: AudioDockConnection, settings: AudioDockSettings, info: AudioDockDeviceInfo, error: string | null, };
export type AudioDockSettingsUpdate = { "type": "prompt_tone", enabled: boolean, } | { "type": "prompt_volume", value: number, } | { "type": "push_to_talk", enabled: boolean, } | { "type": "noise_suppression", mode: AudioDockNoiseSuppression, } | { "type": "select_equalizer", id: number, } | { "type": "save_equalizer", id: number | null, name: string, gains: [number, number, number, number, number], } | { "type": "delete_equalizer", id: number, name: string, };
