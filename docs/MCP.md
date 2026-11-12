# Local MCP Interface

Desktop Alchemy provides optional Streamable HTTP through the existing core queues. The call path is Agent → MCP → DeviceManager → explicit device Core → adapter. M18 then uses `native_sdk` and official `transport.dll`. MCP is not an SDK server and opens no additional USB connection. Installed Windows x64 clients do not require Node, Python, or Rust.

## Connection

Enable MCP in Extensions → MCP · AI app management. The default endpoint is `http://127.0.0.1:18718/mcp`. Apply a changed port explicitly; a busy port reports failure instead of silently changing. The service defaults to off and persists its enabled state.

Copy the address and token into an Agent client supporting Streamable HTTP and custom headers:

```text
URL: http://127.0.0.1:18718/mcp
Authorization: Bearer <token copied from Desktop Alchemy>
```

The token is available through dedicated desktop operations, not HTTP tools. Resetting it stops the old service, restores temporary display control, and restarts; update clients afterward. The listener is IPv4 loopback only, without remote access, stdio, OAuth, or legacy HTTP+SSE. Stateless requests need no `Mcp-Session-Id`; the official Rust MCP SDK handles protocol framing.

Authenticate every request. Host must be the endpoint's `127.0.0.1:port`. Origin may be absent; if present, it must be that endpoint's `http://127.0.0.1:port` origin.

## Temporary device tools

| Tool                          | Arguments                                                                | Result                                                                                                          |
| ----------------------------- | ------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------- |
| `alchemy_get_status`          | `{}`                                                                     | `{api_version: 1, devices: DeviceDto[]}` with identity, connection, generic definition, capabilities, and input |
| `alchemy_import_asset`        | `{"path":"C:/images/key.png"}` or `{"data":"data:image/png;base64,..."}` | `asset_id`                                                                                                      |
| `alchemy_device_command`      | `{"device_id":"...","command":{"type":"..."}}` with command fields       | `receipt_id`, `status: queued`, `verification: not_observed`                                                    |
| `alchemy_get_command_receipt` | `{"device_id":"...","receipt_id":"..."}`                                 | Core receipt, or an error when absent/expired                                                                   |

Results include structured JSON and text. This status projection excludes internal DeviceState, saved configuration, paths, and locks. Dedicated management tools below provide saved configuration and module settings. Stable errors contain code/message: `invalid_argument`, `not_found`, `offline`, `unsupported`, `config_unavailable`, `resource_exhausted`, `stale_operation`, `version_unsupported`, `internal`. Within a major API version, add optional fields/capabilities only; callers should ignore unknown response fields. Requests are strict. Logical failures use `isError: true`; malformed arguments may receive protocol parameter errors. Offline devices, unavailable capabilities, full queues, invalid images, and out-of-range values never become hardware success.

`alchemy_device_command` requires `device_id`; obtain it from status. There is no default target. M18 commands are validated against the target's layout/capabilities:

| Type                                              | Other fields                                                       |
| ------------------------------------------------- | ------------------------------------------------------------------ |
| `screen_brightness`                               | `value`: 0–100                                                     |
| `led_brightness`                                  | `value`: 0–255, RGB scaling; unavailable in firmware-default mode  |
| `led_color`                                       | `color`: original `#RRGGBB`                                        |
| `led_colors`                                      | `colors`: 24 `[R,G,B]` arrays, components 0–255                    |
| `reset_led`                                       | None; restore firmware default                                     |
| `key_image`                                       | `key`: 1–15, `asset_id`                                            |
| `key_data`                                        | `key`: 1–15, `data`: Base64 or image data URL                      |
| `bulk_image`                                      | Nonempty `keys` within 1–15, `asset_id`; one image for all targets |
| `background`                                      | `asset_id`; always temporary                                       |
| `background_data`                                 | `data`; always temporary                                           |
| `clear_key`                                       | `key`: 1–15                                                        |
| `key_gif`                                         | `key`: 1–15, imported GIF `asset_id`                               |
| `clear_gif`                                       | `key`: 1–15                                                        |
| `clear_all`, `refresh`                            | None                                                               |
| `start_gif`, `stop_gif`, `gif_status`, `firmware` | None                                                               |

`persistent` is not accepted and raw SDK messages are not forwarded. Keys 16–18 have no display. LED dimming uses RGB scaling rather than the incompatible native brightness operation. These temporary tools do not edit scenes or saved software-animation settings. Use the management tools for those operations. Current input state is queryable, but short key events may be missed; input events are not pushed.

## Application management

Enabling MCP and giving a client the Bearer token authorizes application management, including persistent edits, program launches, plugin lifecycle and backup restoration. There is no additional management toggle or per-call desktop confirmation. Keep the token private. Plugin protocol permissions do not change when MCP permissions expand.

All requests reject unknown nested fields. Saved-resource edits require the revision returned by the corresponding read. Revisions describe resource content, not transient input or queue state. The owner checks the revision and publishes the new state only after atomic persistence. A stale request returns `stale_operation`: read again and reapply the intended change. Do not retry an old whole-resource write with a newly copied revision without reconciling its content.

| Read/discovery tool                                             | Result and use                                                                                                                                 |
| --------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| `alchemy_get_workspace`                                         | Saved profile IDs, linked device IDs, management scope and unsupported operations                                                              |
| `alchemy_get_profile`                                           | Explicit `profile_id`; saved configuration, per-resource `revisions`, definition, separate runtime and scene status; supports offline profiles |
| `alchemy_get_library`                                           | Optional `query`; `{revision, data}` with personal assets and read-only plugin icons; revision covers the complete personal index              |
| `alchemy_preview_image`                                         | Typed `image` reference; 160-pixel PNG data URL and MCP image content, without hardware output                                                 |
| `alchemy_preview_key`                                           | `profile_id` and proposed `visual`; current compositor output as a data URL and MCP image content, without saving/uploading                    |
| `alchemy_get_applications`                                      | Optional `query`; presets with full-library revision, names, typed launch targets and cached icons                                             |
| `alchemy_get_registered_applications`                           | Windows AppsFolder entries with exact identities, including duplicate names                                                                    |
| `alchemy_resolve_shortcut`                                      | Local `.lnk` `path`; original target/arguments/directory without launch                                                                        |
| `alchemy_get_running_programs`, `alchemy_check_scene_condition` | Explicit profile; running programs or tri-state condition result                                                                               |
| `alchemy_get_device_settings`                                   | Explicit device; Audio Dock fresh hardware readback, or M18 saved settings/revision with readback limitation and cached status                 |
| `alchemy_get_plugins`                                           | Plugin definitions, permissions, execution state and current widget samples                                                                    |
| `alchemy_get_app_settings`                                      | `{revision,data}` preferences plus `theme` and `theme_revision`; no credentials                                                                |
| `alchemy_get_diagnostics`                                       | Explicit device; connection error and retained command receipts, excluding SDK paths and serials                                               |
| `alchemy_get_restore_result`                                    | Last startup restore notice; distinct from successful staging                                                                                  |

| Management tool                                       | Operation                                                                                                                                |
| ----------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `alchemy_edit_library`                                | `expected_revision` and typed `edit`: create/rename folders, rename items, move, delete metadata                                         |
| `alchemy_import_library`                              | Revision, optional folder, 1–1000 items with name and exactly one path/data; optional `replace_item_id` replaces only that library entry |
| `alchemy_edit_applications`                           | Revision and typed save/delete/refresh-icon operation; typed Program, RegisteredApplication or Uri targets                               |
| `alchemy_edit_pages`                                  | Profile and pages revision; create/duplicate/rename/reorder/delete/update/replace pages, set key visuals/bindings or apply a preset      |
| `alchemy_edit_bindings`                               | Profile and bindings revision; replace pageless input bindings or apply an application preset; new mapping starts disabled               |
| `alchemy_activate_page`                               | Device and saved page ID; explicit activation, separate from editing                                                                     |
| `alchemy_edit_scenes`, `alchemy_retry_scenes`         | Save complete scene settings (including pause state) with scenes revision, or retry evaluation                                           |
| `alchemy_update_device_settings`                      | Typed M18 saved settings with revision, or typed Audio Dock audio/EQ update through the module settings queue                            |
| `alchemy_manage_device`                               | Typed reconnect retry, explicit profile binding, or declared capability enable/disable                                                   |
| `alchemy_discover_widgets`                            | Plugin ID and discovery lease; discover presentations/sources, or release the lease; may start an enabled plugin                         |
| `alchemy_update_app_settings`, `alchemy_update_theme` | Revision-checked preferences/model appearance or global theme; update the running desktop                                                |
| `alchemy_start_operation`, `alchemy_get_operation`    | Start/poll explicit launch, plugin or backup jobs described below                                                                        |
| `alchemy_forget_operations`                           | Explicitly remove selected terminal operation IDs from history; never reuse their request IDs afterward                                  |

Personal uploads retain immutable, content-addressed originals. Batch imports return per-item results; partial success is explicit. Moves/renames change metadata, and deletion retains bytes needed by saved pages and drafts. Replacement creates a new asset ID and changes only the selected library reference: existing page images remain unchanged. Plugin icon folders remain read-only. Existing image format, path, size and dimension checks also apply to management uploads.

A page edit refreshes only an affected active page; editing another page does not activate it. Applying a preset copies its target and follows the UI rules for default titles/icons while preserving custom visuals. Later preset edits do not rewrite existing bindings. Saved pages, bindings, scenes and device settings have separate revisions. The desktop keeps draft base revisions even when background snapshots arrive, rejects stale autosaves, and retains drafts on failure. Reload saved configuration is an explicit discard action with confirmation.

Page and preview visuals accept optional `content_mode` (`none`, `text`, `pattern` or null for legacy inference), `show_text` (defaults to true), and `text_position` (`top`, `center`, `bottom`, default center). Existing visuals retain their appearance. Hidden text and images stay saved; GIFs play only in Pattern mode and never overlay text. See [Pages and assets](PAGES-AND-ASSETS.md) for rendering semantics.

M18 settings are saved configuration, not physical readback. Persisted changes and queue errors are reported separately using `saved`, `receipt_id`, `display_error`, and `verification`. Audio Dock settings use the existing typed module queue and readback transactions, including factory/custom EQ restrictions. A failed or uncertain readback is an error, never `readback_confirmed`; reread before deciding what to do next. Disabling MCP does not roll back these persistent management changes.

### Launch, plugin and backup operations

`alchemy_start_operation` accepts `{request_id, operation}`. The typed operation is one of `launch`, `plugin_inspect`, `plugin_install`, `plugin_enable`, `plugin_start`, `plugin_stop`, `plugin_remove`, `enable_hardware_sensors`, `backup_export`, `backup_inspect`, or `backup_restore`. Inspect/install/backup operations use explicit local paths; install and restore additionally require the fingerprint returned by inspection.

The reply contains `operation_id` and status. Poll `alchemy_get_operation` with that ID until terminal. Repeating a request ID with identical content returns the same operation; different content with that ID is rejected. This prevents duplicate launches, installations and restores after HTTP retries. Up to eight jobs run at once. A local, atomic `mcp-operations.json` ledger retains up to 1024 records across restarts and is excluded from backups. Interrupted jobs remain interrupted and are never automatically resubmitted. Corrupt history is preserved and disables new jobs rather than risking duplicate execution. Explicitly forget terminal records to free space; deduplication ends for those IDs.

`completed` means the owning operation completed: a launch was accepted by the OS, a plugin transaction finished, or a backup was exported/inspected/staged. It does not mean a launched application finished. Installation retains the package owner's disabled/replacement rules; enabling and starting are explicit operations. Stop waits for the existing process-tree cleanup.

Backup export contains saved state, excludes editor drafts and credentials, and does not flush or discard UI drafts. Restore validates the inspection fingerprint, stages the existing transaction, and requests normal shutdown/restart after a successful HTTP response has been sent. Poll the job to allow a pending restart response to complete; reconnect afterward and read `alchemy_get_restore_result`. Startup restoration creates its normal automatic backup and supports rollback. A completed staging job is not proof that startup restoration succeeded.

Management never exposes arbitrary Shell text, raw SDK/HID messages, MCP credentials, persistent boot-image writes or hardware image downloads. The public M18 SDK has no image-readback operation. Commands remain explicitly addressed and module-owned; no second device connection is opened.

## Image example

These are `tools/call` params; the client SDK performs initialization and HTTP headers:

```json
{ "name": "alchemy_import_asset", "arguments": { "path": "C:/images/key.png" } }
```

Use the returned ID on key 1:

```json
{
  "name": "alchemy_device_command",
  "arguments": {
    "device_id": "<device_id>",
    "command": { "type": "key_image", "key": 1, "asset_id": "<asset_id>" }
  }
}
```

A standalone image command already refreshes. An explicit refresh is also available:

```json
{
  "name": "alchemy_device_command",
  "arguments": { "device_id": "<device_id>", "command": { "type": "refresh" } }
}
```

Provide exactly one of path or Base64. Paths must be local absolute regular files, not relative paths, URLs, UNC/device paths, alternate streams, or network drives. Imports copy PNG/JPEG/GIF into managed storage, with a 20 MiB limit, maximum edge 8192, and maximum 32 million pixels. HTTP bodies are limited to 32 MiB; chunked asset upload is not supported.

`gif_status` and `firmware` are asynchronous queries. Inspect their receipt and status afterward; cached values may predate the request. Only the latest 60 receipts are retained, and page refreshes may generate several internal receipts. Expired receipts are unavailable through this tool.

## Temporary control and recovery

All Agents share the MCP source; targets remain explicit per command. HTTP disconnect does not revoke visuals. The four temporary device tools do not persist pages, brightness, or lighting. Management writes explicitly persist and survive service shutdown. Subsequent UI/plugin writes follow core coalescing/override rules; MCP supplies no per-client exclusive lock.

Disabling blocks further tools and cancels pending MCP-owned work. If MCP left temporary overrides, redraw the current page, restore saved settings, and replay other still-valid plugin overrides. Queries/imports alone do not trigger a redraw. Submitted work cannot be retracted. Page changes/reconnect use normal core restoration. GIF start/stop counts as temporary control requiring restoration.

`queued` is core acceptance, `sent` is SDK-queue submission, and `acknowledged` is SDK-call success. None establishes a physically correct display or lighting effect.

## Configuration and implementation

`mcp.json` is independently and atomically stored in the production `platform-v1` data root. It contains enabled state, port, and credentials. Do not upload/share it or put tokens in snapshots, logs, screenshots, or reports. A corrupt MCP config does not prevent client startup; preserve the original file until explicitly reconfigured.

`mcp` owns HTTP, typed tools, access checks, and lifecycle. Core source/generation markers provide precise cancellation. Rendering and SDK execution reuse existing adapters. Dedicated Tauri operations provide status, configuration, and token management; page drafts cannot overwrite these settings.

HTTP/1 uses Axum plus the official MCP transport and Hyper shutdown handling. Complete the response and close the write side; on Windows allow 50 ms for pending sends, then wait at most two seconds for peer closure to avoid resetting a short connection before delivery. Service shutdown requests graceful connection closure and reclaims tasks after three seconds. Request and header-read deadlines are 30 seconds.

Run `pnpm test:mcp` only for relevant changes. It uses real local HTTP and the official Rust client with simulated devices, covering discovery, temporary and management tools, protocol versions, auth/Host/Origin, body limits, validation, stop/cancellation, GIF recovery, port conflicts, token reset, config reload, and repeated short connections. It does not load the SDK. See [Testing](TESTING.md). Management cases cover library transactions, saved profiles, application presets, revisions, preferences, typed settings dispatch, operation deduplication, plugin packages and backup staging. Legacy `m18_*` tool names are unsupported; callers must use current names and explicit device IDs. Changing the UI editing selection does not change a command or receipt's target.
