# Pages, Assets, and Configuration

Implementation: `model.rs`, `core.rs`, `storage.rs`, and `render.rs`. Frontend drafts belong to `src/store.ts`.

## Configuration and pages

Production configuration lives in `platform-v1/config.json` under the Tauri application-data directory. The workspace format is `desktop-alchemy.workspace`, version 1. Common and module configuration versions are independent. Older data directories are neither read, migrated, nor deleted. Profiles are parsed and validated individually; unknown modules, unsupported versions, and invalid profiles remain read-only with their original JSON preserved. Unknown nested fields survive round trips. See [Device Modules and Multiple Devices](DEVICES.md). Validate common fields, module fields, and asset references before atomically writing configuration and publishing memory state.

Each M18 `Page` contains an ID, name, 18 `KeyVisual` entries, and an optional temporary background. A visual stores text, foreground/background colors, an optional personal asset or plugin-icon reference, and an optional widget binding. Actions live separately in `Page.bindings`; devices without pages use `AppConfig.bindings`. Colors use `#RRGGBB`; key text is limited to 80 characters. Keep 1–200 pages. Key numbers must be unique and cover 1–18, and navigation targets must exist. Launch actions retain a program/shortcut path, arguments, and optional working directory. Loading validates structure; target existence is checked when invoked.

`background_asset` is an optional temporary background included in persistence, restoration, and backups. It addresses the underlying 480×272 RAM image; subsequent uploads of 15 opaque key images cover it. It does not participate in `KeyVisual` rendering or split an image across keys. The page editor does not expose this setting or clear existing values. Hardware tests, MCP, and plugins retain temporary-background operations.

Pages support creation, duplication, ordering, renaming, and deletion. The frontend removes navigation references to deleted pages; the last page cannot be deleted. Screen brightness and original LED colors, brightness, and modes belong to device settings rather than individual pages.

Autosave begins 650 ms after editing stops. Failure preserves the draft and allows retry. Activate this page waits for persistence before activation. `save_config` submits only pages; `Core::update_pages` merges them under the configuration lock while retaining the latest active page, device settings, theme, capability observations, and plugin registration. Deleting the active page falls back to the first page. Editing another page does not refresh the device; changing or deleting the active page does. Report device-update failure separately from successful persistence. Background snapshots must not overwrite pending or in-flight drafts. Page changes and plugin stops clear or restore temporary overrides; plugins do not edit persistent pages.

## Assets and rendering

| Directory or field | Purpose                                                               |
| ------------------ | --------------------------------------------------------------------- |
| `assets/`          | Original PNG/JPEG/GIF bytes named by SHA-256 and normalized extension |
| `cache/`           | Rebuildable Rust composites, default backgrounds, and test images     |
| `asset_id`         | Validated internal asset ID, never the original external path         |

Import copies bytes into application storage, so moving the source file does not affect pages. Encoded files and decoded data are limited to 20 MiB, 8192 pixels per edge, and 32 million pixels. Base64 inputs also have an encoded-length limit. Reject path traversal and missing assets. Plugin transport additionally limits each message line to 1 MiB.

Base content has three modes: `none` draws the background only, `text` draws text without an image, and `pattern` draws the selected image with optional text. `KeyVisual.content_mode` is nullable: missing/null on existing configuration infers the mode from the retained image/text without changing its appearance. `show_text` defaults to true and `text_position` defaults to `center` (`top` and `bottom` are also supported). Changing mode or hiding text retains all text, asset references, colors, and actions, including in copies and backups. New mode selections enable text for Text and disable it for Pattern; reselecting the same content preserves its settings.

Rust scales static images to fill 64×64, then uses resvg to compose the requested background, image, and text layers. Text uses at most seven characters per line and four lines; the complete text block aligns at the top, center, or bottom while remaining horizontally centered. The system font database is reused. Cache keys derive from `KeyVisual`; frontend and MCP previews request the same Rust output. GIF assets retain animation without text overlays only in Pattern mode. A retained GIF in None or Text mode is not played. A preview is not hardware readback. Plugin widgets render their own content independently of the retained base content settings.

Page activation advances the page generation and discards obsolete display tasks. Prepare all assets in the background, then enqueue one batch: stop old GIFs, send the temporary background, upload static/GIF keys, refresh once, start new GIFs, and restore saved device settings. The native worker prepares all encoding before continuous `transport.dll` uploads and calls `transport_refresh` once at the end. Ordinary MCP/plugin writes cannot interleave. Publish plugin page events only after the base page is queued. Recheck page/source generations after preparation and before submission. Stopping MCP/plugins cancels removed-but-unsubmitted work; work already inside the C SDK queue cannot reliably be withdrawn. Reconnection obtains a fresh path and activates the saved current page.

## Built-in brightness page

`render::brightness_page` generates grayscale, solid-color, gradient, checkerboard, and background assets. `show_brightness_test` creates the independent page `builtin-brightness-test-v1` on first use and reopens it thereafter. It changes the current page without rewriting the original page. Return to the original page or select another page in the editor. The test page follows ordinary editing/deletion rules.

## Settings export and import

Implementation: `backup.rs`, `app.rs`, `desktop.rs`, and `BackupControls.vue`. Export settings creates a `.m18backup` ZIP containing:

| File                           | Contents                                                                                                                                                                      |
| ------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `manifest.json`                | Format `desktop-alchemy.platform.backup`, `format_version: 1`, client version, Unix creation time in milliseconds, and each data file's length/SHA-256                        |
| `config.json`                  | Saved profiles, pages, visuals, actions/navigation, launch arguments/directories, active pages, themes, screen/LED settings, capability observations, and plugin registration |
| `library.json`                 | Personal asset folders, names, stable IDs, and references                                                                                                                     |
| `applications.json`            | Original launch targets, names, arguments/directories, and cached icon references                                                                                             |
| `app-settings.json`            | Close behavior, language preference, and per-profile model labels/images                                                                                                      |
| `assets/<SHA-256>.<extension>` | All imported PNG/JPEG/GIF originals, including application icons and retained unreferenced assets                                                                             |

The frontend waits for autosave and stops export on save failure. The backend reads persistent state only: drafts are excluded, no applications are launched, and no extra Shell icons are extracted for backup. Write and synchronize a temporary archive in the destination directory before atomic replacement. The destination cannot overwrite files inside the application-data or asset directories. Exclude MCP connection settings/tokens, logs, rebuildable caches, application/shortcut/plugin binaries, and the device's persistent boot image. Restore preserves local MCP settings. Launch targets and plugins may need attention on another computer.

Import fully extracts into isolated staging and validates the manifest, exact file set, SHA-256 values, image formats/dimensions/content IDs, versioned data models, and image references before showing a summary. Display creation time, client version, page/image/folder/application/plugin counts, and data size. Accept only format version 1; compatibility is not inferred from the client version string. Reject extra files/directories, symlinks, encryption, duplicate paths, and traversal. Limits: 10,000 data files, 16 MiB per JSON file, 4 MiB manifest, 20 MiB per image, and 2 GiB total data. Enforce limits during actual decompression, not only against ZIP declarations.

Import and restart explicitly replaces configuration and discards unsaved drafts. Recheck the archive SHA-256 against the reviewed fingerprint, synchronize staging, and publish `backup-pending/`. Use normal shutdown to stop MCP, plugins, and the SDK before restarting. Restore runs before any Core, plugin, MCP, or SDK initialization, never mixes new files with old memory, and never writes the device's persistent boot image.

Before replacement, save the current state to `backups/before-import-<UUID>.m18backup`. If damaged metadata cannot be archived normally, preserve its raw directory; underlying assets remain. Restore uses synchronized original-file copies and a rollback journal. Any metadata replacement failure rolls back all metadata; interrupted transactions are rolled back before startup. Mark a successful transaction committed, atomically rename its directory, then clean up so interrupted cleanup cannot look uncommitted. Newly added immutable assets survive failure/interruption; original images are not deleted. Corrupt pending imports are quarantined as `backup-rejected-<UUID>/`, allowing startup with the existing configuration. Rollback failure aborts startup and preserves repair material. Startup notifications and runtime logs expose the outcome and automatic-backup path.

A validated backup image can repair damaged local bytes with the same content ID. Stream the original bytes into an undo directory before replacement, and retain them in the original-state backup. Later failure/interruption restores both images and metadata. Identical healthy content is not rewritten. Export ignores internal interrupted-write files named `<valid asset ID>.tmp`.

## Runtime diagnostics

`runtime.log` is bounded; `events.jsonl` records input/device/page events, `commands.jsonl` command states, and `session-health.jsonl` health samples. Diagnostic export creates `acceptance-report.json`. These runtime artifacts and full serial numbers are local troubleshooting data and should not enter the public repository. They are distinct from source documentation.

## Library and application settings

The built-in M18 set contains 23 warm-white PNG icons with dark-brown symbols for navigation, media, audio, capture, lighting, timers, lock, settings, refresh, application switching, and brightness. The Back symbol uses the cached `arrow_back` vector. Seed updates replace an older generated image only when it still matches the original built-in asset; user replacements survive. The independent M18 common keys folder leaves existing pages/folders intact. See [Icon Resources](../src-tauri/resources/icon-library/README.md) for sources and licensing.

Social-media assets contain 26 brands in color and reversed-white variants: 52 local 64×64 PNGs in separate folders. Seed markers are local and excluded from backups; images and library entries follow normal backup rules. Users may rename, move, or delete entries; deleted seeds are not re-added at startup.

`library.json` remains version 1. Folders store `id/parent_id/name`; entries store `id/folder_id/name/asset_id`. Folders are logical and do not move originals. Names contain 1–120 characters, with no slashes, control characters, or leading/trailing whitespace. Sibling folder and entry names are separately unique without case sensitivity. Moves retain asset IDs, reject cycles, and fail atomically on naming/content conflicts.

When the index is absent, deduplicate saved-page references into an Existing page assets folder, excluding the brightness-test page. Use short hashes when original filenames cannot be recovered. Do not rescan or re-add deleted entries later. The original MCP asset-import tool, plugin imports and hardware-test imports do not automatically enter this index. Explicit MCP library uploads do enter the selected personal folder and use the same owner, validation and atomic metadata publication. A damaged index is retained and disables library management only; existing pages still render their assets.

Batch import validates/saves/reports each item. Identical content in the same folder reuses an entry; different content with the same name gets a numeric suffix. Each metadata mutation atomically saves under its own lock before publishing memory. Deletion removes index entries only, retaining underlying assets for saved pages and drafts. `cache/library-*.png` contains rebuildable, on-demand 160-pixel thumbnails.

`app-settings.json` independently stores `close_behavior` (`tray` by default, or `exit`), `language` (`system` by default, or `zh-CN`/`en`), and per-profile `model_appearances` (empty by default). Older settings/backups without language use the default. Page drafts cannot overwrite these preferences. Retain damaged settings and report an error; close behavior falls back to direct exit until an explicit user change writes settings.

Model appearance is default, 1–32 characters of non-control text, or a stable `ImageRef`. Personal images must exist and be static; plugin icons retain stable references. `set_model_appearance` validates the profile/image and merges only that profile's entry; restoring the default removes its override. Close behavior and language likewise update only their own fields through the settings owner. Publish changes after atomic persistence; failure retains previous values. Backups include preferences and referenced personal images; plugin icons are references only. Model labels affect software previews, never the device queue.

Version 2 `applications.json` stores stable IDs, names, typed `LaunchTarget` values, and optional cached icons. The platform data root accepts this typed index, not legacy v1 data. Initial creation deduplicates saved action targets across available profiles. Selecting a preset copies its target to `KeyAction::Launch`; later preset edits/deletion do not modify existing actions. M18 copies default titles/icons only for display keys and preserves custom content. Audio Dock uses `AppConfig.bindings` without creating pages or display assets. Missing/uninstalled targets remain structurally valid and fail explicitly at launch. See [Applications and Launching](APPLICATIONS.md).

## Registered applications and cached icons

`ApplicationCatalog` enumerates desktop and Store entries through AppsFolder, returning stable Shell identities, names, and typed targets. Do not filter to package identities containing `!`, launch by name/list position, or merge same-name entries. Extract icons on demand from the original `IShellItem`, using placeholders on failure. Enumeration and extraction run on blocking workers; file pickers retain `.exe`/`.lnk` support. `LaunchTarget` separates Program, RegisteredApplication, and Uri. Re-resolve registered Shell items at invocation and fail clearly when unavailable.

`application_icons` obtains HICONs on the Tauri blocking pool using the [Windows Shell image list](https://learn.microsoft.com/en-us/windows/win32/api/shellapi/nf-shellapi-shgetimagelist); shortcuts retain their own/target icon rules. Convert through [WIC CreateBitmapFromHICON](https://learn.microsoft.com/en-us/windows/win32/api/wincodec/nf-wincodec-iwicimagingfactory-createbitmapfromhicon) to RGBA instead of guessing HBITMAP alpha semantics. RAII owns icon/COM lifetimes. Extraction neither launches applications nor enters device queues.

Trim transparent padding, scale with premultiplied alpha, center with padding, and validate/hash-store a 256×256 transparent PNG in `assets/`. Cached icons do not automatically enter personal library folders. A changed program path attempts extraction; name/argument/directory edits reuse the cache. Failure does not block application saving. Opening presets fills missing caches; failure to persist this automatic repair does not prevent reading the existing index. Explicit edits/refreshes still require atomic persistence.

`default_icon_asset_id` records the last automatically applied key icon, defaulting to null. Apply a new preset icon only to an empty display key or one still using its previous default. Manual import, library selection, or image removal clears this marker. Bottom keys never receive images. Stable asset IDs protect saved pages when original programs move, presets are deleted, or icons refresh.

## Widget bindings and temporary frames

`KeyVisual.widget` defaults to null; otherwise it stores plugin/widget IDs, optional presentation/source information, and independent foreground/background/accent/progress styling. Only display keys 1–15 support widgets. Bindings do not modify base text/images or press actions; missing plugins retain bindings and base content. Workspace format remains version 1. `disabled_plugins` defaults to empty; page autosave merges only pages and cannot overwrite plugin state. Backups include bindings/disabled state, excluding live samples and memory frames.

The host renders saved bindings on the current page; the editor previews drafts with current samples. Snapshots and sample events must not overwrite drafts. Dynamic frames use the same native encoding/upload path, key mapping, refresh, generations, and path checks. A single latest pending frame replaces earlier work per key; ordinary commands have priority. MCP/traditional-plugin overrides take precedence. Stopping widgets restores base content; reconnecting or returning to a page rebuilds current metrics.

Ordinary cached/imported images use independent temporary files followed by atomic publication, preventing partial reads during previews/page changes/reconnection. Dynamic metrics do not write these directories. `WidgetBinding.presentation_id` defaults to null, selecting the first manifest presentation; backups retain the choice, not runtime presentation data. See [Plugin Protocol](PLUGIN-PROTOCOL.md).

`managed_plugin_ids` and `disabled_plugins` default to empty. Managed files live in `plugins/<ID>/`, except built-in resources. Backups retain IDs/state/bindings, not binaries. Missing plugins preserve page content and bindings. Recover plugin transactions before backup restoration so an older transaction cannot overwrite restored settings. External absolute-path registration is not an import UI.

## Transparent assets and plugin icons

`KeyVisual.background` remains a `#RRGGBB` solid color, edited with the color picker, HEX input, and common colors. Compose transparent PNGs over that color and text last; opaque images cover the background. Premultiplied-alpha scaling avoids dark fringes. Preview and hardware output share the Rust compositor.

The transparent common-keys set has light/dark variants of 23 Material symbols: 46 transparent PNGs. It uses independent one-time seeding and user-deletable entries, leaving warm-white assets/page references intact. Thumbnails use checkerboards.

`LibraryCatalog` combines personal version 1 `library.json` with read-only plugin folders. `ImageRef` distinguishes personal assets from `{plugin_id,icon_id}`. Plugin folders are not written into the personal index and cannot be renamed/moved/deleted/imported into through personal-library controls.

`KeyVisual.plugin_icon` defaults to null and is mutually exclusive with personal-asset fields. Applying an application preset does not replace a plugin icon. Manual image selection/removal updates the appropriate source. Icons are available while installed, independently of plugin execution. Uninstall retains visual references, colors, text, and actions, displays a missing-source indication, and allows recovery after reinstalling the same IDs.

Packaged icons load from installation resources; runtime icons live in `plugin-icons/<ID>/`. Neither enters personal `assets/` or settings backups. Thumbnails/composites remain in memory; whole-page output uses internal frames through the existing Core/native batch queue. Catalog revisions invalidate prepared-but-unsubmitted batches and refresh the active page. Restore does not require referenced plugins to be installed first.

Temporary plugin `display.key` visuals retain in-memory icon references. Catalog changes recompose them; uninstall falls back to their background/text. Stopping the source or changing pages follows normal override restoration without adding composites to personal assets.

## Profile isolation

Global workspace settings are separate from device profiles. Pages, device settings, scenes, and capability observations belong to profiles; the library and application presets are shared. Autosave uses `profile_id`, merges only the captured target, and retains its draft on failure. Backups include all content-addressed assets to cover unknown module references. Format/version/isolation rules are in [Device Modules and Multiple Devices](DEVICES.md).
