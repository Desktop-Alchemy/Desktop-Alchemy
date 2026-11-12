# Desktop Alchemy Plugin Protocol v3

Executable plugins are independent EXE processes; icon-only packs need no executable. Import external plugins as `.m18plugin` packages. New executable plugins start disabled; icon-only packs mount immediately. Ordinary plugins require manual startup again after a client restart. Enabled widget plugins run automatically when saved bindings exist. Disabled state persists. The host does not execute through a shell. Native plugins have the current user's system privileges: declared permissions restrict host APIs, not the operating system.

Plugins use public host APIs only. Device operations enter Core queues and the client's official `transport.dll`. Plugins must not create separate SDK connections or load their own device libraries, bypassing ownership/restoration.

## Versions and public boundary

Manifest `api_version` and initialization `apiVersion` must be 3; earlier versions are rejected. Host initialization also supplies `hostApiVersion: 1`, `methods`, `events`, and `capabilities` describing input types, generic device descriptions, temporary control, and module-setting boundaries. `devices.list` returns independent `DeviceDto` values: identity, generic definition, capabilities, inputs, connection, and errors. It excludes saved configuration, paths, SDK messages, and internal snapshots. `input.input` events carry complete button/rotary/analog values identified by the definition's string `control_id`.

Error data contains `code/message`. Stable codes: `invalid_argument`, `not_found`, `offline`, `unsupported`, `config_unavailable`, `resource_exhausted`, `stale_operation`, `version_unsupported`, and `internal`. Within a protocol major version, additions are optional fields/capabilities; clients may ignore unknown response fields. Request fields are strictly validated. Removing fields or changing meaning requires a new major version. Module-specific settings are not part of the plugin API. The desktop and explicitly authorized MCP management tools use their owning module service.

## Packages, installation, and removal

A `.m18plugin` is a ZIP with root `plugin.json`. If it declares `executable`, include that EXE and any runtime/DLL/assets it needs:

```text
plugin.json
my-plugin.exe
assets/...
```

Run `pnpm plugin:pack -- <plugin-directory> [output.m18plugin]`. The default output is beside the source directory with its name. Output must be outside the source, which may not contain links/reparse points. The extension does not imply official `.sdPlugin` compatibility. The JSON-RPC host protocol is v3; there is no system double-click installation association.

Import plugin package in Extensions first reviews name, version, permissions, and replacement details. Inspection/installation never executes package programs. Confirmation rechecks the complete SHA-256 and rejects changed files. Replacing the same ID always requires confirmation, including equal versions and downgrades. Reject case conflicts and the built-in `dev.masterain.performance` ID. Replacement preserves bindings, disabled state, and previous run intent; failure restores old files/configuration. Icon-only packs have no process lifecycle.

External plugins live in application data at `plugins/<plugin ID>/`. Configuration stores `managed_plugin_ids`, not source archives or external absolute paths. Deleting the source archive does not affect installation. Built-in performance resources remain installation-owned. Backups contain IDs, bindings, and shared `disabled_plugins` state, never binaries. Restore does not delete local plugin files; missing plugins require reimport. Uninstall stops the process and removes managed files/registration while preserving page bindings and base visuals. Built-ins cannot be uninstalled.

Limits: 512 MiB archive, 1 GiB total decompressed content, 256 MiB per file, 4096 entries, and 256 KiB root manifest. Stream validation checks actual decompressed length and CRC. Reject encryption, links, special files, absolute paths, traversal, Windows reserved names, duplicates/case conflicts, and file/directory collisions. Validate in isolated staging; directory swaps and a transaction journal publish files/configuration. Startup recovers interrupted transactions before discovery. Staging, rollback, and committed garbage are not discoverable plugins.

## Manifest and lifecycle

```json
{
  "id": "example.counter",
  "name": "Counter",
  "version": "1.0.0",
  "api_version": 3,
  "executable": "counter.exe",
  "permissions": [
    "events",
    "devices.read",
    "pages.read",
    "pages.activate",
    "display.write"
  ]
}
```

`executable` is relative to the plugin directory. Reject `..`, absolute paths, and escaping links. Other languages may bundle their runtime into an executable plugin; the installed client itself needs neither Node nor Python. Keep each plugin/manifest in its own directory before packing.

Stdio uses UTF-8 newline-delimited JSON-RPC 2.0, with a 1-MiB maximum per line. Stdout is protocol-only; write logs to stderr. IDs are strings/numbers and responses echo them. Notifications have no ID.

The host sends initialization (the minimal fields are shown):

```json
{
  "jsonrpc": "2.0",
  "id": "initialize",
  "method": "initialize",
  "params": { "apiVersion": 3, "pluginId": "example.counter" }
}
```

Reply within five seconds:

```json
{ "jsonrpc": "2.0", "id": "initialize", "result": { "apiVersion": 3 } }
```

The host sends `ping` every ten seconds; respond before the next heartbeat. It sends `shutdown` on stop; respond and exit. Ordinary plugins have up to one second for graceful exit; the built-in performance plugin has up to seven seconds for hardware-broker cleanup. Then terminate the owned Windows Job and wait for its complete process tree. Assign the Job when creating the process.

Disable succeeds only after process collection and disabled-state persistence both succeed. Handshake, request handlers, and pipe waits are cancellable. Stopping rejects requests and clears samples/unsubmitted tasks; it performs no sampling. If persistence fails, retain the current stop, report failure, and allow retry. Device restoration failure is reported separately and does not restart collection. Pipe-write timeout is three seconds; host-method timeout is five. Unexpected exit/protocol failure becomes an error with explicit retry, never an unlimited restart loop.

## Plugin-to-host methods

| Method               | Permission       | Parameters/result                                                            |
| -------------------- | ---------------- | ---------------------------------------------------------------------------- |
| `widgets.publish`    | `widgets.write`  | `WidgetData` → `{ "accepted": true }`; runtime data only                     |
| `ping`               | None             | `{}` → `{ "ok": true }`                                                      |
| `events.subscribe`   | `events`         | `{ "kinds": ["button.down"] }`; replaces subscriptions                       |
| `devices.list`       | `devices.read`   | Device descriptions, firmware, and capability evidence                       |
| `pages.list`         | `pages.read`     | `{device_id}` → `{device_id, active_page, pages:[{id,name}]}`                |
| `pages.activate`     | `pages.activate` | `{ "device_id":"…", "page_id":"…" }`                                         |
| `assets.import`      | `display.write`  | `{ "data":"data:image/png;base64,…" }` → `{assetId}`                         |
| `display.key`        | `display.write`  | `{device_id, visual:DisplayVisual}`; temporary key visual                    |
| `display.background` | `display.write`  | `{ "device_id":"…", "asset_id":"…" }`; temporary background                  |
| `device.command`     | `device.write`   | `{device_id, command:DeviceCommand}`; persistent boot-image writes forbidden |

Import images through the validated host asset library, then reference returned IDs. There is no arbitrary external-file read API. PNG/JPEG/GIF limits are 20 MiB per image, 8192 pixels per edge, and 32 million pixels. The 1-MiB JSON-RPC line limit also applies: resize/compress before transport. There is no chunked upload.

Example `display.key`:

```json
{
  "jsonrpc": "2.0",
  "id": "count-1",
  "method": "display.key",
  "params": {
    "device_id": "<device_id>",
    "visual": {
      "key": 1,
      "text": "Count 1",
      "foreground": "#FFFFFF",
      "background": "#5966F3",
      "asset_id": null,
      "action": { "type": "none" }
    }
  }
}
```

M18 display keys are 1–15 in five columns/three rows, starting at the top left. Bottom keys 16–18 produce input only. `action` preserves the type contract; temporary visual updates never change saved press actions.

Example `device.command`:

```json
{
  "jsonrpc": "2.0",
  "id": "brightness-1",
  "method": "device.command",
  "params": {
    "device_id": "<device_id>",
    "command": { "type": "screen_brightness", "value": 50 }
  }
}
```

Public `DeviceCommand`, `DisplayVisual`, and descriptor types originate in the interface crate's `external.rs` and are generated into `src/bindings.ts`. Hardware-operation success returns `receiptId`, meaning queued rather than physically confirmed. Rejection uses JSON-RPC `error`, code `-32000`, a readable reason, and structured `data.code/message`.

M18 `led_brightness` is 0–255 proportional RGB dimming and never invokes the incompatible native `setLEDBrightness`. `led_color`/`led_colors` provide undimmed originals. Retain original RGB and derive output using current brightness; color/dimming queues coalesce complete lighting state. `reset_led` selects firmware defaults, where `led_brightness` explicitly fails. Send `led_color`/`led_colors` to resume static control before dimming. Plugin changes remain temporary.

## Host notifications

Subscriptions include `device.connected`, `device.disconnected`, `button.down`, `button.up`, and `page.changed`.

```json
{
  "jsonrpc": "2.0",
  "method": "event",
  "params": {
    "at": 1793784000000,
    "kind": "button.down",
    "device_id": "…",
    "key": 1,
    "pressed": true,
    "page_id": null
  }
}
```

Time is Unix milliseconds. SDK paths stay private; unknown paths/out-of-range inputs are ignored. Display writes are runtime overrides. Page changes clear them; stopping a plugin restores the base page and replays other active overrides. Temporary device-setting changes return to saved client values.

External application integrations, such as Discord/OBS state monitoring, belong in plugins. They may call `pages.activate` on state changes while the host retains SDK/USB ownership. Official `.sdPlugin` host compatibility is not provided.

## Examples

Counter code: `src-tauri/src/bin/example_plugin.rs`; manifest: `examples/counter/plugin.json`. `pnpm package` compiles/packages it; see [Development](DEVELOPMENT.md) for development copying. Inspect/install the example package in Extensions, then enable it. Key 1 increments the counter; key 2 activates the next page. Process integration tests cover initialization, subscriptions, page changes, and shutdown.

## Bound display widgets

The built-in Computer performance plugin uses this same protocol with ID `dev.masterain.performance`. Widget IDs include `cpu`, `memory`, `disk`, `network`, `battery`, `temperature`, `fan`, `power`, and `voltage`. Register from the current installation without persisting installation paths. Missing installed resources fail explicitly rather than falling back to development directories. The counter example remains manually started.

Optional manifest `widgets` defaults to empty, with at most 32 entries. Each has `id`, `name`, and `default_style`: `foreground`, `background`, `accent` (`#RRGGBB`) and `show_progress`. IDs are unique within a plugin and contain 1–100 ASCII letters/digits/dots/underscores/hyphens. Names contain 1–32 characters without controls.

Declare `widgets.write` and publish:

```json
{
  "jsonrpc": "2.0",
  "id": "sample-1",
  "method": "widgets.publish",
  "params": {
    "widget_id": "cpu",
    "title": "CPU",
    "value": "28%",
    "detail": null,
    "progress": 0.28,
    "valid": true
  }
}
```

`{ "accepted": true }` means validated and cached in memory, not sent/drawn. Limits: title 32 characters, main value 24, detail 48, no control characters. Progress is null or a finite number from 0 to 1. Publish only declared widgets; stopped/disabled plugins cannot publish. Existing transport/heartbeat/timeout/Job constraints apply.

Plugins own collection, formatting, and defaults. The host renders generic title/value/detail/progress. Saved `KeyVisual.widget` holds `plugin_id`, `widget_id`, and `style`, separately from actions. Plugins submit no key/page target. The host updates saved current-page bindings and allows duplicate metric bindings. Editor previews use samples with draft styling; unsaved bindings neither drive hardware nor autostart plugins.

One process runs when an enabled plugin has any saved binding; removing the last stops it. `set_plugin_enabled` atomically updates `disabled_plugins` and survives restarts. Crashes require user retry. Exit stops supervision and collects processes. Before a valid sample or after five seconds without one, render `—`. Stopping restores original button content while keeping bindings.

Priority: base content → bound widgets → MCP/traditional-plugin overrides. Submit the complete base page before dynamic frames. Removing overrides restores widgets. Each current-page key has one latest pending frame; ordinary commands win. Recheck page/source/binding/path between rendering/submission. Dynamic PNG/JPEG stays in bounded memory, never the permanent library or per-frame receipt log; identical frames are skipped. `assets.import / display.key` retains persistent-asset semantics for finished images and is unsuitable for unlimited-frequency frames.

CPU samples every second using PDH `\Processor Information(_Total)\% Processor Time` with `PdhAddEnglishCounterW`; the first valid result needs two samples. Memory uses `GlobalMemoryStatusEx`, used = total physical − available physical. Percentages are integers and GiB values have one decimal. Metrics fail independently. Compare CPU against the same PerfMon counter, not frame-for-frame against Task Manager's potentially different window/definition. Sampling is separate from protocol input and cannot block heartbeats.

The UI queries `widget_samples` and subscribes to `plugin-widgets` for typed `WidgetSample[]`, independently of configuration drafts. `preview_key` uses the same Rust renderer and is not device readback.

### Plugin-defined presentations

Optional `presentations: [{"id":"percent","name":"Percentage"}, ...]` allows up to 16 entries with widget ID/name limits. The first is default. Nullable binding `presentation_id` selects it; absent/null uses default. Optional sample `presentations` maps IDs to complete `{title,value,detail,progress,valid}` content under the same limits. Only declared IDs are accepted; default content remains in top-level fields. The host selects provided content without interpreting business data. Unknown/missing non-default content is unavailable. All presentations share sample time/expiry.

Memory declares percentage, available capacity, and used capacity. Default shows percentage and used/total; capacity variants show the selected GiB value, `/total GiB`, and the corresponding fraction. CPU has no extra presentation. Forms are manifest-generated, not CPU/memory-specific. Each button independently selects its presentation without changing collection frequency.

### Sources and batch samples

Optional `select_source: true` defaults to false. Bindings add nullable `source_id/source_name`; source-aware widgets require explicit selection. Source IDs have the same 1–100 ASCII character restrictions; names allow up to 160 characters without controls. Missing/removed sources retain bindings and become unavailable.

`widgets.sources.publish`, with `widgets.write`, takes `WidgetCatalog`: `sources: [{widget_id,id,name,available,error}]`, `hardware_status`, and nullable `message`. At most 512 sources per plugin; widgets must declare `select_source`, and source IDs are unique within each widget. Catalog publication fully replaces the previous catalog and removes obsolete cached samples. Hardware status is empty, `ready`, `starting`, `driver_required`, `permission_required`, `unavailable`, or `error`. Messages/source errors allow at most 512 characters without controls. Catalogs/samples never enter drafts/backups.

`WidgetData.source_id` is nullable; cache by plugin/widget/source. Published sources must exist in the catalog; ordinary widgets use null. `widgets.publish_batch` takes up to 512 `WidgetData` values under the same permission. Validate the complete batch before applying it and reject duplicate sources within a batch. The 1-MiB line limit still applies. Single `widgets.publish` remains supported. Both return `{accepted:true}`; each batch broadcasts one sample event.

Nullable `secondary_value` on `WidgetData`/`WidgetContent` allows 24 non-control characters. When present, generic rendering uses receive/send rows with direction symbols and omits detail text. Unknown, failed, or five-second-expired content uses `—`.

Frontend `discover_widget_sources(id,lease)` obtains catalogs and renews a 15-second discovery lease; `release_widget_discovery(id,lease)` releases it. There are at most 64 global sessions. Enabled plugins may run temporarily without saved bindings. Disable clears sessions; browsing never enables disabled plugins. `enable_hardware_sensors` sends `sensors.enable` only to the built-in performance plugin following explicit user intent; it is not an external elevation API. See [Performance and Sensors](PERFORMANCE-SENSORS.md).

## Installation-time and runtime icons

Optional manifest `icons: [{id,name,path}]` defaults to empty. `path` is a package-relative PNG under package path rules. IDs use 1–100 ASCII letters/digits/dots/underscores/hyphens and reject case conflicts. Names contain 1–120 characters, no controls or outer whitespace. A plugin may have 2048 effective icons, with 128 MiB total raw packaged/runtime icon content. Each PNG has the usual 20-MiB/8192-edge/32-million-pixel limits; invalid decoding rejects installation/publication.

```json
{
  "id": "example.icon-pack",
  "name": "Example icon pack",
  "version": "1.0.0",
  "api_version": 3,
  "permissions": [],
  "icons": [{ "id": "play", "name": "Play", "path": "icons/play.png" }]
}
```

An icon-only pack omits `executable`, declares at least one icon, and has no runtime permissions/widgets. The library shows its plugin name/ID and read-only source. Mount on install/update/restart; stopping/crashing/disabling does not unmount icons. Uninstall removes packaged icons and `plugin-icons/<ID>/`; transactional failure restores previous files/configuration.

Executable plugins with `icons.write` may call:

| Method          | Parameters                                       | Result                                                |
| --------------- | ------------------------------------------------ | ----------------------------------------------------- |
| `icons.publish` | `{id,name,data}` with Base64 PNG or PNG data URL | `{id,name,origin:"runtime",revision}`                 |
| `icons.list`    | `{}`                                             | Effective icons with origin and content-hash revision |
| `icons.remove`  | `{id}`                                           | `{removed:boolean}` for the runtime version           |

The host binds plugin ownership; parameters cannot choose another owner. The 1-MiB limit remains, with no chunking/external-file reads. Runtime icons override matching packaged IDs; removal reveals packaged versions or removes runtime-only entries. Publish only after atomic runtime-index persistence. Runtime icons survive stop/disable/restart/update and disappear on uninstall. Updating pixels under the same ID updates referring keys.

Optional `plugin_icon: {plugin_id,icon_id}` is mutually exclusive with personal `asset_id/default_icon_asset_id`. Missing icons/plugins remain saveable/loadable/backupable; render background/text and preserve actions until reinstall restores the same IDs. Plugin icons/composites never enter personal `assets/`; backups store references only. `assets.import` retains permanent personal-asset semantics.

Examples: [icon pack](../examples/icon-pack/README.md), [runtime publisher](../examples/icon-publisher/README.md), and `src-tauri/src/bin/icon_publisher.rs`. The publisher publishes, lists, removes, and republishes without device operations.

## Multiple-device targets

Handshake and manifests both use v3; earlier plugins are explicitly rejected. Descriptors/events identify devices. Page/display/device commands require `device_id`; obtain it from `devices.list`. `pages.list` may read offline configuration; hardware writes require an online target. UI selection never redirects plugin calls.

Samples, source catalogs, personal assets, and plugin icons are shared services; actual displays/pages/overrides belong to individual devices. The counter example counts and changes pages on the event's source device. Disable collects the entire Job tree and restores all affected devices. Package extension remains `.m18plugin`.
