# Minimal Testing Guide

For contributors, agents, and CI. Choose the smallest credible checks for the affected behavior and its failure consequences. Do not turn a list of available commands into a mandatory checklist.

## Choose evidence by risk

1. Identify what the change might break. Types and formats need static checks; state transitions need focused unit tests; messages, processes, and networks need integration checks; native windows, hardware, and installers require their actual environments.
2. Reuse or extend an existing meaningful case. New regression tests should cover a concrete failure. Do not add tests to inflate counts, mirror implementation, or repeatedly assert constants and resource counts.
3. Use temporary data, explicit targets, independent ports and processes, and cleanup. Do not overwrite user data or compete for hardware. Serialize only demonstrated shared-resource conflicts.
4. Expand scope only for a related failure, unresolved concern, or changes that cross meaningful boundaries. Do not repeat successful checks just to finish a task.
5. State which checks actually ran, their environment, outcome, and limits in the task or PR. Keep detailed logs in ignored local directories. Do not maintain chronological test reports in source documentation.

Documentation, comments, and copy-only changes normally need review and internal-link checks only. Simple styling needs a focused visual check. Neither requires application tests, a build, or packaging by default.

## Repository constraints

- Focus on persistence and recovery, queue atomicity and cancellation, device isolation, input boundaries, permissions, and process lifecycle.
- Use Rust's default concurrency. Never set global `--test-threads=1`. Frontend checks, Rust checks, and independent browser sessions may run concurrently; Cargo compilations sharing a target directory must run sequentially.
- Isolate only tests sharing a proven conflicting resource. Real SDK ownership, one native test client, and one sensor publication directory still require exclusive ownership.
- Automated tests do not connect to physical devices or overwrite user data. Run hardware verification only when hardware behavior changes or a physical check is explicitly needed.
- Simulations, compilation, protocol success, and SDK receipts do not establish physical behavior.

## Select checks

Run commands from the repository root. `-Filter` matches a substring of Rust test names.

| Change                                     | Smallest relevant check                                                                                                                                   |
| ------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Documentation, comments, copy              | Review the diff and references; no tests, build, or packaging                                                                                             |
| Simple styling                             | Inspect the affected UI; use `pnpm build` only if Vue/TS structure also changed                                                                           |
| Vue, store, frontend types                 | `pnpm build`; select one UI scenario if save, switching, or input flows changed                                                                           |
| One Rust module                            | `pnpm test -Filter storage::tests`, replacing the filter with the affected module/case                                                                    |
| Interacting Core/storage modules           | `pnpm test`                                                                                                                                               |
| MCP HTTP, auth, protocol, lifecycle        | `pnpm test:mcp`, plus related Core cases if shared behavior changes                                                                                       |
| Plugin packages, lifecycle, collection     | Focused library cases such as `pnpm test -Filter plugin_package::tests`; use `pnpm test:plugins` for process behavior, optionally with `-Filter faults::` |
| Windows launch, shortcuts, AppsFolder, URI | Library tests for validation; `pnpm test:launch` for Shell behavior, optionally with `-Filter shortcut`                                                   |
| Public Rust models                         | `pnpm bindings`, then `pnpm build` and affected module tests                                                                                              |
| Device registration or features            | Relevant combinations only, e.g. `cargo check --manifest-path src-tauri/Cargo.toml --no-default-features --features device-ms-audio-dock --locked`        |
| SDK files, bundle resources, installer     | `pnpm package`, inspect relevant resources, and release ownership before installed-device self-checks                                                     |

`pnpm test` runs core workspace libraries with default device features. It excludes HTTP, plugin subprocesses, Windows Shell integration, binding generation, UI, and hardware. `scripts/test.ps1` temporarily disables Tauri bundle-resource collection and restores the caller's environment. Source tests need neither downloaded SDK DLLs nor packaged plugins or published .NET output. Process suites build their own test executables.

Plugin package, icon, fault, and collection integration cases share the `plugins` test target and run concurrently under one harness with their own directories/processes. HTTP cases use independent ports and temporary data. Do not start another Cargo compiler against the same target directory.

## Full suite

Use the full suite only when a cross-subsystem change cannot be bounded or a release requires complete source regression:

```powershell
pnpm test:full
```

It covers core, HTTP, real plugin processes, Windows Shell, and feature binaries, but not hardware, UI, installers, or long performance runs. Reproduce a failing case before rerunning the entire suite.

Select formatting, Clippy, builds, and packaging in the same way. Rust changes may require `cargo fmt --manifest-path src-tauri/Cargo.toml --check` and Clippy for the affected targets; not every change requires all targets and features.

## UI and CI

Start a fresh Vite server for mock scripts that dynamically import source modules. HMR-versioned URLs and unversioned imports can otherwise create separate module instances. Chinese-selector fixtures explicitly use the store's `setLanguage('zh-CN')` rather than depending on the test machine's language.

| Script                                | Behavior                                                                                                        |
| ------------------------------------- | --------------------------------------------------------------------------------------------------------------- |
| `scripts/platform-ui-check.js`        | Generic 32-key layout, rotary/analog bindings, page factory, unavailable configurations                         |
| `scripts/content-ui-check.js`         | Base content modes, retained text, positions, GIF controls, failed drafts, isolated 2D/3D preview requests       |
| `scripts/mcp-management-ui-check.js`  | Desktop/MCP edit conflicts, retained drafts, safe reload confirmation, subsequent saves                         |
| `scripts/multi-device-ui-check.js`    | Device switching, failed-save drafts, target isolation, module-owned test pages                                 |
| `scripts/audio-dock-ui-check.js`      | Settings synchronization, equalizer drafts, app selection, input bindings                                       |
| `scripts/platform-native-ui-check.js` | Isolated real Tauri persistence, offline configurations, page templates, invalid targets                        |
| `scripts/i18n-ui-check.js`            | Message compilation, plural branches, draft retention, failed language saves, narrow English layout             |
| `scripts/i18n-native-ui-check.js`     | Real language persistence, system detection, tray synchronization, permission boundaries, native fallback menus |

Use `pnpm check:i18n` for resource structure, keys, placeholders, references, and untranslated Chinese UI literals. Run browser scripts through Playwright CLI's `run-code --filename`, using separate sessions for independent scenarios. Never inject mock IPC into a native WebView. See [Development](DEVELOPMENT.md#isolated-native-ui) for the hardware-free diagnostics client.

CI uses path filters to select independent frontend and Rust core jobs. Documentation-only changes do not trigger them. Ordinary CI does not fetch the SDK, publish sensor components, collect installer resources, or run full/slow process suites. Relevant contributors run HTTP, plugin, and Shell suites on demand; the Windows workflow also allows explicit suite selection.
