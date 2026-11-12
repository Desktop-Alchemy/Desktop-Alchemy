# Device Modules and Multi-Device Operation

The principles below apply to multi-device hosts. Paths, versions, budgets, and limits describe this repository and are not defaults for unrelated devices.

## Ownership and contracts

Adapters own discovery, connections, and protocol conversion; the host owns configuration, authorization, UI state, and shared scheduling. Typed capabilities and operations connect them. Shared interfaces expose neither raw handles nor internal locks, and adapters cannot alter another device's state.

Separate stable logical identity from connection sessions. Reconnection invalidates the previous token even when paths or serial numbers are unchanged. Async work captures the originating target/session and revalidates before submitting or publishing results.

Keep persisted configuration, hardware readback, and temporary overrides separate. Version and strictly validate configuration, migrate incrementally, and publish only after atomic persistence. Preserve unknown data and expose unsupported configuration as read-only rather than discarding it.

Capabilities determine which UI sections exist. Device-specific workflows are registered in their module; shared code must not fabricate unavailable features based on model names. Dependent work is serial per device and bounded/fair across devices. Cancellation distinguishes unsubmitted work from operations already handed to a native SDK. Shutdown stops new work, waits for workers, then releases resources.

Public APIs expose needed operations through explicit target IDs and DTOs. Recheck authorization, limits, and identity at each entry point; one interface's permissions do not grant another interface's operations.

## Source boundaries

`src-tauri/crates/device-api` depends on neither Tauri/Core nor specific hardware. `device-m18`, `device-audio-dock`, and `device-simulator` depend on that interface. Do not provide adapters with Core, DeviceManager, internal locks, or native handles. See [Adding a device](NEW-DEVICE.md).

DeviceManager registers adapters and owns discovery, profile binding, supervision, and fair scheduling. WorkspaceServices shares assets, icons, and applications. Each Core independently owns its connection, queue, overrides, page/source/session generations, input, and receipts. Frontend selection affects editing only.

An adapter-scoped `AdapterHost` publishes discovery, connections, input, and module state and provides batches belonging to that adapter. `Connection` is a host-issued token used for disconnect, input, state updates, and settings commits. Adapters provide local identities; the host derives opaque global IDs from `(adapter_id, local_id)`. Callers must return IDs unchanged, never parse them. A mismatched owner or duplicate identity rejects the complete discovery update and retains the previous discovery state.

The host waits for adapter tasks on cancellation; a failing adapter marks only its module failed. Adapters must wait for read threads and blocking setting requests before destroying handles/unloading libraries. A timeout preserves ownership and reports failure rather than claiming release.

M18 shares one official SDK owner across compatible device handles. Page preparation/encoding precedes uninterrupted image submission and one final refresh. Audio Dock has its own typed serial configuration-HID queue and read-only Teams input. Module services are desktop-only unless explicitly exposed. Typed MCP management may access Audio Dock settings through its owning module service; plugins and temporary device commands do not gain settings access.

## Configuration and persistence

Production data lives under `platform-v1/` in the Tauri app-data directory. Legacy directories remain untouched: no reading, migration, or deletion. Diagnostic/test roots remain the explicit independent paths supplied by the caller.

- `config.json`: `format: desktop-alchemy.workspace`, `version: 1`, with `global` and `profiles`.
- Each profile has public `config_version: 1`, module `schema_version`, and opaque `module_data`.
- Backups: `desktop-alchemy.platform.backup`, `format_version: 1`. Older formats are rejected.

Parse the envelope first and profiles separately. Unknown modules, newer profile versions, or invalid profile data produce `config_error` and read-only access for that profile. Preserve raw JSON, including unknown nested fields, when changing other profiles or exporting/restoring. A malformed whole workspace or unsupported workspace version preserves the source file and reports an error.

Modules provide defaults, validation, current configuration version, and `migrate_module(from, data)`, advancing by exactly one version. Validate every step and atomically save the completed result. Failure retains the original disk and memory state.

Every profile change follows: candidate → public validation → module validation → asset-reference validation → atomic save → state publication. Modules declare persistent references through `asset_references`; backups retain the content-addressed asset set, including references hidden in unknown module payloads. Hardware readback and local configuration commits are separate outcomes.

Capabilities come from the adapter/current session. Hardware-verification notes do not replace support declarations. `disabled_capabilities` is a separate manual override.

## Input, displays, and queues

`DeviceDefinition` declares stable string control IDs, display-region dimensions/positions, layout, and page support. The adapter/backend creates default pages. Display content and actions are independent: paged devices use `Page.bindings`; pageless devices use `AppConfig.bindings`.

- Buttons are deduplicated and actions execute on press edges.
- Rotary input preserves signed increments; each nonzero report triggers the matching direction once.
- Analog input must be finite and within the declared range. Normalized threshold crossing supports both directions and hysteresis. Defaults are threshold 0.5 and hysteresis 0.02. The first sample establishes a baseline; reconnect resets it.

Plugins may subscribe to complete `input` events. Actions execute in the background with at most eight concurrent actions per device and a session recheck before execution.

A complete page is an indivisible batch. Public limits are 1024 operations and 64 MiB of prepared data; modules may be stricter. Reject the complete oversized batch before queuing and check encoded size again in the adapter. Before submission, call `Host::current` to validate connection, page, source, and resource generations. Report completion to the host. Source cancellation includes dequeued but unsubmitted work; SDK-queued work may not be retractable.

## UI and public interfaces

Each frontend module declares preview, controls, tests, and optional sections in `views.ts` and registers in `src/devices/index.ts`. `DeviceViews.tests` must be a module-owned component or `false`: M18 registers its test component; Audio Dock disables tests. Modules without a declaration have no test page and use generic preview/bindings. The host supplies optional navigation/mounting only, not shared assumptions about hardware test contents or controls.

DeviceScope gives components a fixed target context. Module-specific composables remain in their module. Requests use the centralized client and declared backend module services; unknown operations are rejected. Late responses cannot write through a newly selected global target.

Plugin v3 and MCP connection status use independent DeviceDto values rather than internal runtime snapshots. Dedicated MCP management tools expose projected saved configuration with resource revisions, never transport handles or credentials. Handshakes describe methods, events, and capabilities. Within a major version, add optional fields only; semantic changes require a major version. Requests are strict. See [Plugin protocol](PLUGIN-PROTOCOL.md) and [MCP](MCP.md).

## Focused checks

`pnpm test -Filter devices::platform_tests` covers identity, sessions, configuration, and generic input. `scripts/check-device-features.ps1` sequentially checks no devices, M18 only, Audio Dock only, and default devices plus simulator. `scripts/check-bindings.ps1` validates Rust/TypeScript consistency. `scripts/platform-ui-check.js` is a hardware-free browser scenario.
