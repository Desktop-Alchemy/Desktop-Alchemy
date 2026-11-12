# Frontend Guide

## General design principles

### Data flow and ownership

Separate views, application state, and system interfaces. Views collect intent and display state. The application layer owns selection, drafts, request coordination, and error mapping. Backend adapters own persistence, device I/O, and permission checks. Views must not connect hardware, launch processes directly, or maintain a second business-state model.

Distinguish drafts, saved values, backend snapshots, and live device state. Define submission triggers, success criteria, and failure recovery. Shared updates must not overwrite unsaved edits.

### Asynchronous results

Capture the target, editing session, or version when issuing a request. Revalidate before applying the result. Ignore late results after unmount, target changes, or cancellation. Never reread a mutable current selection to determine where an old response belongs.

Coalesce continuous inputs to the latest value; queue operations with causal ordering. Bound loading and cancellation, and retain a recoverable state after system/API failure.

### Components and accessibility

Reuse stable semantics rather than appearance alone. Specify inputs, events, disabled states, focus, and lifecycle. Keyboard, assistive-technology, and touch behavior should be predictable. Asynchronous components need empty/loading/error/retry/cleanup behavior.

Match navigation hierarchy to data ownership. Device capabilities determine available sections; global views must not branch on model IDs. Wide layouts should become single-column layouts with independent scroll/focus scopes.

### Honest, efficient previews

Reuse production rendering semantics while identifying drafts, saved values, and actual readback. Do not infer unknown hardware state. Lazy-load 3D/image/animation work, coalesce requests, reject late results, pause expensive rendering when hidden/static, and provide a usable fallback.

### Behavioral validation

Prioritize saving, target switching, failure preservation, request isolation, and keyboard behavior. Browser mocks validate frontend logic, not native windows, filesystem permissions, or hardware. Select the smallest relevant UI scenario rather than requiring every snapshot/size combination.

## Implementation map

| Entry          | Responsibility                                                                                                        |
| -------------- | --------------------------------------------------------------------------------------------------------------------- |
| Devices        | Discovered devices and module-specific workbenches/settings                                                           |
| Pages          | Page list, 5×3 + 3 editor, visuals, navigation, and launch actions                                                    |
| Library        | Assets, application presets, and editor selection                                                                     |
| Extensions     | Local MCP configuration/token actions, plugin packages/lifecycle/permissions/errors/examples                          |
| Hardware tests | Input/location, brightness, raw image interfaces, GIF, per-LED colors, capability observations, and diagnostic export |

`App.vue` coordinates navigation, application state, editing, extensions, and shared confirmations. Module `views.ts` files register their test components or use `tests: false`; shared navigation supplies no default test content/layout. M18 `DeviceControls.vue` is reused in its workbench and tests. `HardwareTests.vue` owns test assets/observations, emits persistence-confirmation requests, and is keyed by profile ID. Static/GIF selections are separate. `DeckGrid.vue` owns layout, selection/pressed states, and preview. Shared `labels.ts` provides common state names; M18-specific capabilities/text stay in its module.

`ToastStack.vue` displays bottom-right overlays without taking workspace height. Success notices expire after four seconds, errors after eight; dismissal is available and hover/focus pauses expiry. Form errors and save failures remain in their owning forms/statuses independently of toast lifetime. Repeated operations may produce fresh notifications.

Only `src/store.ts` calls typed Tauri APIs. `bindings.ts` is generated from Rust; C ABI, HID reports, paths, and command sequencing stay in Rust adapters. Tests use existing Core APIs without a second hardware connection.

## Language and status text

`src/i18n.ts` uses the Vue I18n Composition API with Simplified Chinese/English resources in `src/locales/`. `pnpm check:i18n` validates keys, interpolation parameters, references, and leftover hardcoded Chinese. Add both languages and use complete templates rather than concatenated fragments. Display dates in the UI locale; exchange formats and identifiers remain stable.

Resources are standard nested JSON suitable for common translation platforms. See [Translation Resources](../src/locales/README.md) for import settings/placeholders. Count strings use separate `zero / one / other` branches selected by shared `t()` with `Intl.PluralRules`, avoiding platform dependence on Vue's pipe syntax.

Components/store use shared `t()`. Translate status/navigation tables when read and derive options with computed values; do not cache translated labels during module loading. Built-in plugin/widget/presentation names map by stable IDs for display only, preserving manifests, bindings, and user content.

`LanguagePreference` is `system / zh-CN / en`; effective `UiLocale` is `zh-CN / en`. DesktopState owns atomic field-level updates in app settings. `ui_language` returns language state only; `set_ui_language` is main-window-only. Initialize both entries through the store before mounting: subscribe to `ui-language-changed`, then read state, rejecting late responses. The tray uses lightweight language initialization without creating the editor store. System mode checks only the first Windows preferred UI language at startup/reselection; Chinese tags use Simplified Chinese, other/failed results use English. Keep `html.lang`, titles, tray text, and native fallback menus synchronized after successful persistence.

Expected backend errors carry `UiFailure`/`UiMessage { key, args }` at their source and retain original Display text. Tauri returns `UiError { message, detail }`; snapshots/built-in statuses add optional `ui_message` metadata without replacing diagnostics. Unknown keys use a localized operation-failure summary and original details. Existing MCP/plugin error semantics remain unchanged.

Keep ordinary UI focused on labels, essential help, empty/loading states, disabled reasons, errors, retry, and save feedback. Put implementation notes, protocol investigations, and future plans in module documentation. Label previews as configuration previews. Queue/send/SDK details belong in logs/tests. Explain unavailable brightness beside the control without exposing RGB implementation details. Persistent boot-image writes must retain their independent confirmation and power-cycle consequences. Plugin trust/permission information and example key instructions support actual user decisions.

## Visuals and navigation

Follow [DESIGN.md](../DESIGN.md): cool blue-gray canvas, indigo accents, semantic states, compact navigation, and restrained corners. Variables live in `src/style.css`; light/dark mode follows the system by default with manual selection. Tests reuse the same controls/style. Primary-button hover must define both accent background and `--on-primary` foreground, rather than inheriting a pale generic hover or relying on brightness filters.

Keep keyboard focus visible and distinguish selected/physically pressed keys. Confirmations initially focus Cancel and trap Tab. The workspace scrolls independently. Choose affected layout/scale checks using [Testing](TESTING.md); reference sizes are 1180×800 and 900×650, with Windows 100%/150% scaling. Record browser and real native evidence separately.

The device home lists currently discovered devices; Saved profiles exposes offline/unavailable configurations. Show unavailable reasons and read-only editors. Cards include name/model/available serial; selection changes the editing target without stopping other devices. An unbound profile requires explicit claiming. The toolbar opens connection details with identity, firmware, serial, errors, and reconnect. Avoid repeating identity in controls/footer. M18 `DeckGrid.vue` uses a matte case, glass square caps, three round physical keys, decorative light strips, and logical keys 1–18. Neither strips nor configuration images imply readback.

The sidebar contains global entries. M18 secondary navigation orders workbench, pages, scenes, and hardware tests; other modules expose only supported sections. Device breadcrumbs follow `Desktop Alchemy / Device list / Device name / Current page`; global Library/Extensions remain global.

The Vue title bar uses Tauri window APIs for minimize/maximize/restore/close. Empty title-bar space drags; double-click toggles maximization. Closing uses existing save/tray behavior. User-facing Library, Assets, and Applications names stay consistent across navigation, pickers, deletion, and errors without renaming internal APIs. An idle footer retains only the log entry; Processing appears only during work.

### M18 preview and model labels

The device registry supports optional `poster` and `preview`. M18 cards use transparent model-derived posters; `DevicePreview.vue` owns 2D/3D switching and labels. Workbenches default to top-view 3D, editors to 2D; both can switch. Lazy-load the engine/GLB. Support drag rotation, wheel zoom, click selection, arrow-key selection, and distinct pressed/selected colors.

Model label offers default M18, custom single-line text of 1–32 characters, or a static library image. Cancel does not save. Synchronize 2D/3D only after atomic persistence. Preferences are per profile; missing images show the default plus an indication. This does not change page drafts or issue hardware commands. See [Configuration](PAGES-AND-ASSETS.md).

`DevicePreview3D.vue` requests the same Rust key renderer through the store and combines textures into an atlas. Reject results after cancellation/profile changes. `scene3d.ts` owns presentation/selection only. Static scenes draw on input/texture/size changes; animated lights draw at most 20 FPS. Pause hidden/offscreen scenes and release resources/WebGL on unmount. Initialization/context loss supports retry and 2D fallback. Pixel ratio is capped at 1.5; no autorotation, dynamic shadows, or postprocessing bloom.

The strips/top indicators follow the current device draft's LED mode, color, brightness, speed, and per-LED colors. `led-preview.ts` mirrors `src-tauri/src/led.rs`: RGB proportional dimming and five software animations, with speed 1–10 mapping to periods of 9–1.8 seconds. Keep algorithms synchronized. Interpolate 24 colors over the existing strips; emissive surfaces plus three soft glow planes add three draws without fullscreen effects/dynamic lights. Zero brightness hides glow but retains unlit housings. The visual is not photometrically calibrated, the estimated positions are not a calibrated physical LED order, and its independent clock does not track hardware phase or MCP/plugin overrides. Firmware-default effects use a static illustration. Zero brightness stops animation; reduced motion uses a static quarter-cycle frame. 2D strips remain decorative. See [M18 Visual Model](../src/assets/devices/m18/README.md) for geometry/regeneration.

Audio Dock cards use model-derived posters. Its shared `DevicePreview.vue` defaults to top-view 3D with image switching and three viewpoints. Lazy-load engine/GLB, place preview above controls in narrow windows, and retain static fallback/retry. It visualizes Teams input/selection only, without device actions. See [Audio Dock](hardware/microsoft-audio-dock/README.md#appearance-and-3d-model).

## Shared controls and assets

Use `AppSwitch.vue` for settings booleans: grooved track, textured knob, symbols, motion, semantic themes, `role="switch"`, required reactive translated `label`, native button keyboard behavior, and visible focus. Unknown device values use null, centered/disabled, rather than appearing off. Scenes, widget progress, and Audio Dock share this control. Library/weekday multiselects retain checkbox semantics; ordinary input styling must exclude checkboxes.

Use `AppSelect.vue` for ordinary single-choice settings, not new native `<select>` controls. `options` contains `value / label / disabled` and supports strings/empty folder values. Emit `change` after updating `v-model`. The labeled combobox opens listbox/options in a manual popover inside its owning modal, avoiding workspace clipping. Support arrows, Home/End, Enter/Space, Escape, Tab, prefix search, and disabled-item skipping. Keep focus on the trigger with `aria-activedescendant`. Flip at window edges, bound height, and scroll only the list. Synchronize dynamic labels/options and remove listeners on unmount.

`AppIcon.vue` imports local SVGs from `icons.ts` with consistent size, `currentColor`, and regular/duotone states. Hide decorative icons from assistive technology; icon-only buttons require translated accessible names. Do not substitute emoji/Unicode geometry for UI icons. User-authored key text remains unrestricted by this UI convention.

Bundle locked Manrope Variable for Latin/numbers and Noto Sans SC Variable for Chinese. No remote font CDN. Ship font/icon licenses in `licenses/`; these fonts do not replace Rust device-rendering fonts. Desktop Alchemy branding uses `src/assets/brand/logo.png` in navigation/settings/favicon. Preserve the transparent crop, lower-left probe, and tilt instead of completing the portrait. Generate native/taskbar/tray/installer icons from the same original through Tauri CLI. See [Brand Assets](../src/assets/brand/README.md). Application identifier: `dev.masterain.desktopalchemy`.

## Autosave and logs

Saved pages, bindings and scenes include content revisions. Capture the revision when a draft starts and keep it while dirty/in flight; a background snapshot must not advance that base. Successful saves return the new revision. The shared owner rejects stale desktop/MCP edits atomically, retaining the draft and error. Reload saved configuration explicitly confirms discarding unsaved pages, bindings and scenes. Asset/application dialogs likewise capture their opening revision.

Page autosave waits 650 ms, with one in-flight request. Edits during saving retain the draft and queue its latest revision. Success quietly updates save status; failure retains draft/error/retry. Snapshots cannot overwrite pending/in-flight pages. Browser previews explicitly do not persist.

Selecting a page changes the editing target only. Activate this page waits for save, then invokes `activate_page`; failed saving must not activate stale state. Exit/export wait for all saves. Only failed exit saving offers discard confirmation. The backend merges pages under lock with current active page/theme/settings/capability observations/plugin IDs. Update the device only when the active page changes or is deleted. Persistence and device-update outcomes are distinct.

Localize log operation names, levels, and concise statuses; retain technical bodies. Do not show raw command IDs or a permanent SDK PID in everyday UI. `acknowledged` means accepted by the interface, not physically drawn; structured diagnostics retain raw IDs/events.

## Library, settings, and tray

Library has Assets and Applications tabs with arrow-key switching and preserved browsing state. Reuse `LibraryBrowser.vue` in image pickers. Management uses trees/breadcrumbs/thumbnails; checkboxes select batches and folder buttons navigate. Confirm recursive folder/asset counts before deletion; Cancel sends nothing. Rust caches resized thumbnails and the frontend loads visible items only. GIFs show a first-frame thumbnail/badge.

Choose from assets supports static/GIF files and enters autosave; direct import also indexes the asset at the root. Library changes persist independently. Ordinary page editing does not expose temporary device backgrounds or erase stored background values.

App settings independently saves close behavior and language. True-exit confirmation focuses Cancel. Launch forms accept `.exe`/`.lnk`, arguments, and working directory; Test run shares Core dispatch with physical input.

The tray uses `tray.html`, `src/tray.ts`, and `TrayPanel.vue`, without mounting App or the editor store. Its 260-DIP panel reuses themes/fonts/SVGs, 14-DIP corners, and an 8-DIP transparent shadow margin. It shows app/connection state, Devices/Pages/Extensions, and Show window/App settings/Exit. Focus the first item on open; cycle arrows/Tab, activate Enter/Space, and dismiss on Escape, transparent-margin click, or native blur.

Lightweight `useTrayPanel` subscribes to tray state/open events, dispatches the three tray commands, and initializes language. It does not initialize pages, sampling, backup, or autosave. Rust generates `TrayState`/`TrayAction`; compact tray state is pushed while visible when connection/theme changes. Navigation/exit target the main window; an existing modal is preserved while restoring it. Exit reuses normal save checks. Main/tray close handling is separate. Capabilities permit event subscription/unsubscription and language reads; command dispatch is further restricted to panel ready/action/close, never full snapshots/tokens/device APIs. Keep the native fallback menu if the panel is unavailable.

`ApplicationsBrowser.vue` manages typed v2 presets. Shared `LaunchTargetEditor.vue` supports program/shortcut, registered application, and URL/protocol. `RegisteredApplicationPicker.vue` returns a Shell target/display data only. Every backend call uses the store. File/directory/icon/catalog results check their captured object/session and cannot modify another form after cancellation. M18-only title/icon copying stays in key selection; screenless inputs copy actions only.

Audio Dock `DeviceControls.vue` owns Audio settings, Equalizer, and Teams key sections. `devices/index.ts` declares `controlLabel / sections`; App builds navigation/routes/breadcrumbs. Custom routes use `device:`; switching to an unsupported device returns to its workbench. Section changes retain the editor/EQ draft. Settings use shared refresh/error state; Teams has independent input/autosave feedback, a target summary, explicit editing, and collapsible advanced arguments/directory.

Input bindings use per-profile drafts/autosave/retry. `save_input_bindings(profile_id, bindings)` atomically updates only that target. Incomplete launch targets remain drafts until valid; switching/exit still checks unsaved input. Name resolution validates page/target/profile generations. Do not expose raw Shell launch identities in everyday UI.

`BackupControls.vue` uses native `.m18backup` dialogs and typed store methods. Show counts before import; focus Cancel and explicitly explain replacement, draft loss, and restart. Failure keeps the dialog for retry/cancel; processing blocks duplicate submission. Export includes saved state only. Startup loads restoration notices through a dedicated API. Tokens never enter backup types. Save dialogs require `dialog:allow-save`.

Preset cards/pickers reuse `AssetThumbnail.vue`, with placeholders for missing icons. Refresh uses the typed index mutation. `useApplication` tracks `default_icon_asset_id` to distinguish automatic icons from overrides; manual image/removal clears it. Bottom keys receive actions only. Rust supplies defaults for absent optional fields.

Add Windows application calls `installed_windows_apps` through store `installedWindowsApps`. Search the cached-name/icon list with loading/empty/error/retry/cancel states. Late results cannot reopen a cancelled picker. Selection opens the existing edit form; only Save indexes the target, without launching it. Display a type marker rather than raw Shell identities. Registered applications use their default working directory; arguments remain provider-dependent.

## Widget editing and preview

Display content opens `ContentPicker.vue`: search above, All/Recent/installed plugins on the left, preview/name/source/presentations on the right. Multiword search covers names/plugins/presentations. Results support arrows/Home/End/Enter; Escape/Cancel preserves the button and restores focus. Categories/results scroll independently and confirmation remains visible in narrow windows. Only Use this content changes a binding; browsing/search/samples do not. Reselecting identical content preserves presentation/colors/source.

`content.ts` builds the catalog from None, Text, Pattern, and plugin manifests without hardcoded plugin forms. None exposes the background controls; Text adds a Show text switch, an editable text field, color, and Top/Center/Bottom radio options; Pattern adds image selection and the same optional text controls. Hidden text remains editable; its color and position controls are disabled. GIF patterns disable text-overlay controls with a short explanation. Switching base modes retains text and images; selecting Text enables text, selecting Pattern disables it, and reselecting the current content preserves settings. Existing configuration keeps its previous image/text appearance. New application presets applied to untouched default keys choose Pattern with text off, while custom visuals retain their mode and text settings.

Recent content retains at most 12 IDs in WebView localStorage, excluded from configuration/backups; obsolete empty Ordinary-content IDs are discarded. `ContentChoicePreview.vue` requests the shared Rust renderer only for visible entries and discards late results. Stopped plugins use placeholders; opening the picker does not start collection. Bindings have independent colors/progress. Base content retains original text/assets and actions remain separate. Screenless keys have no display binding. Unknown plugins preserve bindings until explicitly replaced.

Store receives `plugin-widgets` and queries `widget_samples`, separately from `draft`. `DeckGrid.vue` calls `preview_key / preview_asset` through store. Preview uses the backend renderer with 140-ms request coalescing and stale-result rejection. Samples/statuses never trigger saving.

Extensions offers enable/disable/error retry and explains automatic running with saved bindings. Built-ins cannot be removed; the counter example retains manual start/stop. Properties separate plugin settings from Button appearance. Presentation options come from manifest `presentations`; omit empty selectors. Choices update only that binding's `presentation_id`; the first item remains the default and unknown IDs remain unavailable rather than being silently replaced.

`PluginControls.vue` selects `.m18plugin`, checks through typed store APIs, shows a review, then installs. Failed inspection opens no review; Cancel installs nothing. Failed installation retains review/error; pending operations prevent duplicate actions/closing. Replacement shows old/new versions, focuses Cancel, and restores trigger focus afterward. Uninstall separately confirms. Distinguish disabled, enabled-idle, starting, running, stopping, and error. A disabled-state persistence failure offers retry. Built-in examples use the same package flow.

`WidgetSourceSettings.vue` adds search/source selection for source-aware widgets. Per-instance discovery leases renew/read about every two seconds and release on unmount/content changes. Discard old-session results. Disabled plugins do not request discovery/authorization. Browsing catalogs never changes drafts; explicit selection writes `source_id/source_name`. Hardware authorization is an explicit action; samples match plugin/widget/source.

## Transparent images and plugin catalogs

`ColorField` combines native color selection, six-digit HEX, and common colors. Keep invalid HEX in the input with an error until corrected, without modifying drafts. Use checkerboard thumbnails and explain that backgrounds show through transparent areas.

`LibraryBrowser` combines personal and read-only plugin folders from `LibraryCatalog` with current-folder name search. Show plugin name/ID; hide personal mutation controls for plugin entries. Image selection returns typed `ImageRef`. On unmounting a folder, return to root and clear selection. `library-catalog-changed` updates catalog/previews, never drafts. Missing icons retain their references and source indication.

## Multiple devices and module context

M18 previews/controls/tests/default pages live in `src/devices/m18/`, registered in `src/devices/index.ts`. Devices without key layouts do not render the M18 editor. Store caches drafts/page-key selections/save errors by `profile_id`. Actual operations capture `device_id`; responses cannot redirect to the current UI selection. Exit/export check all drafts. See [Devices](DEVICES.md).

Modules register optional previews, settings, tests, and extra pages. The fallback provides generic 2D display and button/rotary/analog bindings. Avoid adapter-ID branches in App/global store. `DeviceScope` supplies explicit context; responses and saves belong to their original target. Module state/composables remain inside module directories; Audio Dock settings use `module_request` and its own crate. The backend `create_page` factory creates defaults, and `KeyVisual` excludes actions. Unknown/invalid module configuration is explicitly read-only.
