# Desktop Alchemy Development Guide

These instructions apply to the entire repository. Read this file and the [documentation index](docs/README.md), then select the relevant module guides. Use current code, the pinned SDK manifest, and evidence appropriate to the change as the source of truth. The user's current explicit instructions take precedence.

## General principles

- Establish the task, affected boundaries, and current state before choosing changes. Preserve existing worktree modifications.
- Maintain clear module contracts: callers express intent; the module that owns a resource validates, queues, and executes operations. Upper layers must not bypass that owner.
- Model persistent configuration, editor drafts, runtime state, and external operation results separately. Report a save as successful only after persistence succeeds. Associate asynchronous results with the target and revision that initiated them.
- Give devices, processes, files, and services explicit owners, deadlines, cancellation, and shutdown order. Do not report a resource as released until release is confirmed.
- Select the smallest meaningful validation for the risk. Distinguish static checks, simulations, integration tests, and physical hardware verification. Report actual evidence and remaining limits.
- Write guides around concepts, decisions, and reusable workflows. Put repository paths, commands, device models, and numeric limits in implementation examples; do not turn project-specific examples into universal rules.

## Project and entry points

Desktop Alchemy is a Windows x64 multi-device desktop client built with Rust, Tauri 2, Vue 3, and Pinia. The built-in physical adapters are M18 and Microsoft Audio Dock. M18 uses the official precompiled C SDK, `transport.dll`. Installers include its required runtime libraries; users do not need Node, Python, or Rust.

```text
UI / local plugins / MCP
          ↓
Core: validation, temporary overrides, serial queues, receipts
          ↓
native_sdk: image encoding, frame scheduling, C ABI, input reads
          ↓
official transport.dll → USB HID → M18
```

| Change                                    | Read first                                                                                    | Implementation entry points                                                               |
| ----------------------------------------- | --------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| Startup, builds, checks, distribution     | [Development](docs/DEVELOPMENT.md)                                                            | `package.json`, `scripts/`, `.github/workflows/`                                          |
| Core queues, lifecycle, recovery          | [Architecture](docs/ARCHITECTURE.md), [Pages and assets](docs/PAGES-AND-ASSETS.md)            | `src-tauri/src/core.rs`, `app.rs`                                                         |
| Device commands, connections, SDK updates | [Device SDK](docs/DEVICE-SDK.md), [M18](docs/hardware/m18/README.md)                          | `devices/mod.rs`, `crates/device-m18/`, `led.rs`, `vendor/device-sdk/native/SOURCES.json` |
| UI and editors                            | [Frontend](docs/FRONTEND.md), [Client guide](docs/CLIENT.md), [Design](DESIGN.md)             | `src/App.vue`, `src/store.ts`, `src/components/`                                          |
| Configuration, imports, rendering         | [Pages and assets](docs/PAGES-AND-ASSETS.md)                                                  | `model.rs`, `storage.rs`, `render.rs`                                                     |
| Agent tools and local HTTP                | [MCP](docs/MCP.md)                                                                            | `src-tauri/src/mcp.rs`, `mcp/tests.rs`                                                    |
| Process plugins                           | [Plugin protocol](docs/PLUGIN-PROTOCOL.md)                                                    | `plugins.rs`, `process_lock.rs`, `examples/counter/`                                      |
| Hardware diagnosis and performance        | [Hardware testing](docs/HARDWARE-TESTING.md), [USB capture](docs/hardware/m18/USB-CAPTURE.md) | `src-tauri/src/bin/diagnose.rs`, `.local/`                                                |

## Device control constraints

These M18, Audio Dock, SDK, and Windows requirements are specific to this repository. Keep generic interfaces for new devices and put device-specific behavior in their adapters.

- Route UI, MCP, plugin, and source-diagnostic operations through DeviceManager to the correct device Core. M18 must reuse its module's `native_sdk`. Do not restore the WebSocket SDK executable, SDK ports, legacy JSON device protocol, or another USB write path. MCP HTTP and plugin stdio are upper-layer interfaces and must use the device queue.
- Exactly one process owns the SDK at a time; that owner may manage several device handles. Preserve the shared client/diagnostic process lock. Exit official Stream Dock and other control instances before hardware tests; never terminate unknown processes to acquire the device.
- Prepare and encode every image before uploading a page. Submit images consecutively and call `transport_refresh` once at the end. Do not refresh per key, wait, read files, decode images, or interleave another source during the upload. A standalone key-image command still requires a refresh.
- Recheck page generation, source generation, and device path immediately before submission. MCP/plugin cancellation also covers dequeued work that has not been submitted. Work already in the official SDK queue cannot be guaranteed retractable.
- Public display-key numbers run from top-left to bottom-right, 1–15; the bottom physical keys are 16–18. Image output uses `sdk::m18_key_index`. Do not flip native input again after normalization in `native_sdk`.
- Dim LEDs by scaling RGB while retaining original colors. Do not enable the native LED-brightness interface that produces incorrect behavior on the supported hardware. Software animation uses a coalesced single slot rather than filling the normal command queue.
- Match C signatures, structure layouts, and DLL lifetimes to the pinned headers. Wait for the read thread to finish before destroying handles or unloading libraries. An idle read timeout is not a disconnect. A native library crash has no separate-process isolation.
- Fetch SDK files only from `vendor/device-sdk/native/SOURCES.json`; `pnpm sdk:fetch` reads this local manifest. DLL updates must update versions, hashes, sizes, runtime validation, and source documentation together, then verify ABI and hardware behavior. Never load unknown same-name DLLs from PATH or let installed builds fall back to a development directory.

Read [Device modules](docs/DEVICES.md) for multi-device isolation and configuration versions, and [Adding a device](docs/NEW-DEVICE.md) for new adapters. Production data uses the `platform-v1` subdirectory; do not read or migrate legacy data directories.

## Configuration and public contracts

- The frontend calls typed Tauri APIs only through `src/store.ts`. Generate `src/bindings.ts` from Rust using `pnpm bindings`; do not edit it by hand.
- Keep unsaved editor drafts separate from background snapshots. A save requires successful atomic persistence; a successful device call does not mean configuration was saved.
- Plugin writes and the four original MCP device tools are temporary control; restore their overrides when they stop. Explicit MCP management tools may persist application configuration, manage plugins, launch typed targets and restore inspected backups. Use the owning services and revision checks; do not expand plugin permissions. Persistent boot backgrounds require separate explicit confirmation; MCP cannot write them, and automated tests must not overwrite the user's boot image.
- Import and validate assets through the core. Do not expose device handles or arbitrary SDK messages to upper layers. Module guides and code define supported formats, size, dimensions, and path constraints.
- MCP listens only on `127.0.0.1`. Preserve Bearer, Host, and Origin checks. Tokens are available only through dedicated operations and must not enter snapshots, logs, screenshots, reports, or the repository. Store MCP settings independently of page drafts.
- Plugin permissions constrain host APIs; they are not an OS sandbox. Preserve message-size limits, request timeouts, and Windows Job Object child-process cleanup.

## Workflow and validation

Inspect `git status`, relevant implementation, and guides first. Without a request, do not commit, push, reset the worktree, or rewrite unrelated modules.

```powershell
pnpm install --frozen-lockfile
pnpm sdk:fetch
pnpm desktop
```

Read [Testing](docs/TESTING.md) before running checks. Test core behavior, remove obsolete or duplicate cases, and do not add tests merely for coverage counts or implementation details. Documentation, comments, and copy-only changes do not require tests. For simple styling, inspect the affected UI.

- For one Rust module use `pnpm test -Filter module::tests`; use `pnpm test` for interacting core modules. Run `pnpm test:mcp`, `pnpm test:plugins`, or `pnpm test:launch` only for the relevant HTTP, process-plugin, or Windows Shell behavior.
- Run independent checks concurrently. Use Rust's default test concurrency; never set global `--test-threads=1`. Isolate only demonstrated conflicts. Do not run concurrent Cargo compilations against the same target directory. Frontend and backend checks may run in parallel.
- Do not default to full tests, every feature combination, builds, Clippy, UI scripts, or packaging. Reserve `pnpm test:full` for changes whose effects cannot be bounded locally or a necessary release regression. Repeat passing checks only after related changes, a failure, or an unresolved concern.
- Generate bindings only when Rust public models change, then check the frontend contract. Update lockfiles with dependency changes; do not hide known failures by narrowing checks.

`pnpm dev` is a browser preview, not evidence of working hardware or an installed application. Automated tests use isolated temporary data without loading the real SDK and do not require packaged resources. Real MCP HTTP and plugin-process tests are on-demand suites.

- For packaging or SDK-resource changes, run `pnpm package`, inspect installed resources, and run `--check-installation` from the installation directory after releasing device ownership. Source checks do not update the current installed or running client.
- Inspect actual rendering and interaction where needed. Browser mocks do not replace native-window, Windows scaling, or physical-device checks. Keep everyday UI concise; developer explanations belong in docs and test controls in Hardware tests.
- Complete necessary operations within the user's authorization. Explain actual needs involving draft loss, unplugging hardware, or visual observation, while respecting existing test constraints and avoiding repeated permission requests.
- Enumerate current USB topology and verify the target's descriptor VID/PID before capture. Recalibrate after reconnecting. PnP port numbers are not USBPcap addresses; the presence of packets alone does not prove M18 traffic was captured.
- Distinguish builds, protocol tests, SDK acceptance, USB submission, and visual verification. `acknowledged` does not mean drawing has completed. Compare performance using identical encoded images and explicit measurement endpoints.

## Documentation and local artifacts

Maintain current behavior in the owning module guide and keep `docs/README.md` up to date. Documentation is written in English; the default Chinese README links to its English counterpart. Keep reusable test methods in the testing and capture guides. Do not create acceptance diaries, chronological progress reports, intermediate experiment documents, or instructions to append to them. Report a task's executed checks and limits in the task or PR, and keep detailed run artifacts in ignored local directories.

Do not commit `.local/`, `output/`, user data, or binary build artifacts. Captures, IDA databases, detailed logs, and serial numbers may be cached locally within the user's authorization; preserve original failure labels. Existing local artifacts do not authorize new device operations. All current M18 operations still use the C SDK backend.

Shared input, AppsFolder discovery, and launch targets are documented in [Microsoft Audio Dock](docs/hardware/microsoft-audio-dock/README.md) and [Applications](docs/APPLICATIONS.md). Audio Dock does not use the M18 SDK. Its settings use the owning Core's typed configuration-HID queue; Teams input stays read-only. Typed MCP management may read/write settings through that module service; plugins and the temporary device-command API receive no settings-write permission. Shared actions execute in the background public input dispatcher.
