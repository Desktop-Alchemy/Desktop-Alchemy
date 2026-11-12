# Development and Distribution

## Workflow

Prepare the locked environment, develop through an explicit entry point, select risk-based checks, build delivery artifacts only when needed, and verify the installed program separately from source. Report executed checks and limits in the task or PR; keep raw output in ignored directories. A successful source run does not establish installer or hardware correctness.

## Toolchain and startup

The target is Windows x64. Development uses Visual Studio C++ Build Tools, WebView2, Node 24, pnpm 11.18.0, and Rust 1.92.0 pinned by `rust-toolchain.toml`. Publishing the built-in sensors and installer also requires the .NET 10 SDK; ordinary source tests do not.

```powershell
pnpm install --frozen-lockfile
pnpm sdk:fetch
pnpm plugins:build
pnpm desktop
```

`pnpm desktop` is `pnpm tauri dev`. `pnpm dev` is a browser preview only. Release the device from official Stream Dock and other diagnostic/control instances before starting a physical session. A single process owns the M18 SDK.

## Source map

| Location                                                 | Responsibility                                          |
| -------------------------------------------------------- | ------------------------------------------------------- |
| `src/App.vue`, `src/components/`, `src/devices/`         | UI and device views                                     |
| `src/store.ts`                                           | Drafts, state, typed Tauri calls                        |
| `src/bindings.ts`                                        | Generated public Rust types; do not edit manually       |
| `src-tauri/src/`                                         | Core, storage, rendering, plugins, MCP, device host     |
| `src-tauri/crates/device-m18/src/`                       | SDK checks, C ABI, input reader, M18 transport          |
| `src-tauri/src/bin/`                                     | Diagnostics, examples, package tool, performance plugin |
| `src-tauri/tests/plugins.rs`, `src-tauri/tests/plugins/` | Concurrent real-plugin integration harness              |
| `scripts/test.ps1`                                       | Focused source-test entry points                        |
| `scripts/fetch-sdk.ps1`, `scripts/package.ps1`           | Pinned SDK acquisition and NSIS packaging               |

Vite ignores Rust, SDK, build, and local-artifact directories to avoid binary changes triggering frontend reloads.

## Types and checks

Read [Testing](TESTING.md) and select the affected scope rather than running every command. For example:

```powershell
pnpm test -Filter core::tests
```

`pnpm test` runs core libraries. `test:mcp`, `test:plugins`, and `test:launch` cover real HTTP, plugin processes, and Windows Shell. Reserve `test:full` for necessary cross-module source regression. Source tests do not fetch the SDK or require packaged resources or .NET publication output.

After a public Rust-model change run `pnpm bindings`, then `pnpm build` to check frontend contracts. Generation is explicit and is not performed by ordinary tests. `pnpm build` runs `vue-tsc` and a Vite production build. Use formatting, relevant-target Clippy, lockfile checks, and feature checks according to the change.

DeviceManager routes commands to a device's Core. M18 uses `native_sdk` and official `transport.dll`; diagnostics must not introduce another hardware write path. See [MCP](MCP.md) and [Device modules](DEVICES.md).

Tauri's synchronous setup and Windows native callbacks are not guaranteed to enter a Tokio context. Use `tauri::async_runtime` for host tasks started there. If a Tokio handle is needed, explicitly use `tauri::async_runtime::handle().inner().spawn(...)`. Adapter tasks may use Tokio internally. For changes to startup or native input callbacks, select `pnpm test -Filter synchronous_startup_and_native_callbacks_use_the_host_runtime`; it uses simulated devices from a thread with no Tokio context.

## Browser checks

Use a fresh Vite server and independent browser sessions for the relevant UI scenarios. Mock IPC is for browser previews only, never real WebViews.

```powershell
npx --yes --package @playwright/cli playwright-cli -s=alchemymulti open http://127.0.0.1:1420/
npx --yes --package @playwright/cli playwright-cli -s=alchemymulti run-code --filename scripts/multi-device-ui-check.js
npx --yes --package @playwright/cli playwright-cli -s=alchemymulti close
```

Audio Dock uses `scripts/audio-dock-ui-check.js`; generic layouts use `scripts/platform-ui-check.js`; localization uses `scripts/i18n-ui-check.js`. Independent sessions may run concurrently. Screenshots go to ignored `output/playwright/`.

## Isolated native UI

Use debug plus `diagnostics` when validating real Tauri persistence, windows, or the tray without device ownership:

```powershell
cargo build --manifest-path src-tauri/Cargo.toml --bin desktop-alchemy --features diagnostics --locked
$env:DESKTOP_ALCHEMY_UI_TEST_ROOT = Join-Path (Get-Location) '.local/ui-acceptance'
try { Start-Process './src-tauri/target/debug/desktop-alchemy.exe' -WindowStyle Hidden }
finally { Remove-Item Env:DESKTOP_ALCHEMY_UI_TEST_ROOT }
```

This mode uses independent data and does not open device DLLs. Debug still requires `pnpm dev`; release builds ignore this variable. `scripts/platform-native-ui-check.js` expects two offline configurations, `a` and `b`, in that directory. Select the `main` WebView through a localhost-only WebView2 CDP endpoint. The script checks isolation, absent SDK connections, and real IPC before operating. Never run it against the everyday client.

An isolated diagnostics instance uses a directory-specific application lock and skips normal single-instance activation; production and SDK locks remain unchanged. For parallel sessions, give the test process its own `WEBVIEW2_USER_DATA_FOLDER`. If CDP is needed, set `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS='--remote-debugging-port=9227 --remote-debugging-address=127.0.0.1'` for that process only and restore the caller's environment afterward. Diagnostics-only tray toggle and fallback-menu events are not production APIs.

Check native windows, physical tray interaction, mixed DPI, multiple monitors, and hardware output only where relevant. Browser success does not establish those behaviors. Backup restoration and plugin replacement require isolated data.

## Built-in plugins and sensors

`pnpm plugins:build` builds the counter and icon-publisher examples, performance plugin, package tool, and sensor components, and produces example `.m18plugin` packages. For release output:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-plugins.ps1 -Release
```

The script temporarily disables Tauri resource collection to bootstrap executables that will later become bundle resources. It restores the environment afterward. Stop a known development client using those executables before replacing them; never terminate unknown owners.

`scripts/build-sensors.ps1` uses NuGet lockfiles to publish a self-contained Windows x64 .NET component. It verifies the PawnIO 2.2.0 installer hash and signature, performs hardware-free self-checks, and generates a resource manifest. The ignored `plugins/performance/sensors/publish/` directory is bundled in full. Do not publish into it concurrently with a Rust/Tauri bundle build.

Ordinary library, HTTP, plugin-protocol, and Shell tests do not require publication. Users do not need .NET preinstalled. The app does not automatically install drivers or create a background service. See [Performance and sensors](PERFORMANCE-SENSORS.md).

`pnpm plugin:pack -- <plugin-directory> [output.m18plugin]` uses the host's validator. The installer includes the counter package and performance plugin, but does not automatically install icon examples. The package tool itself is not distributed. See [Plugin protocol](PLUGIN-PROTOCOL.md).

## Packaging and installation self-check

Package when bundle resources or installation behavior changes, or when an installer is actually required:

```powershell
pnpm package
```

The script builds release plugins and sensors, then the Tauri NSIS installer under `src-tauri/target/release/bundle/nsis/`. It includes the frontend, Rust program, official `transport.dll`, three C++ runtime DLLs, source manifest, licenses, counter package, and performance resources. No separate WebSocket SDK process is used. End users do not need Rust, Node, Python, or .NET; WebView2 provides the UI runtime.

Release device ownership before running the installed executable's headless check from its installation directory:

```powershell
Start-Process './desktop-alchemy.exe' -ArgumentList '--check-installation' -WindowStyle Hidden -Wait -RedirectStandardOutput check.json -RedirectStandardError check-error.log
```

The check verifies installed resources and sensor hashes, loads the C SDK, reads device/firmware receipts, then stops the read thread and releases handles. It does not verify physical screen/LED output or driver/UAC behavior. Missing installed resources must not fall back to a development directory. Resource-skipping test settings are not valid for installation checks.

Separately verify process/handle cleanup, upgrade preservation, restart recovery, and absence of development-runtime requirements where the release needs them. Source tests do not update installed binaries. Preserve configuration, MCP settings, assets, and local evidence when cleaning known obsolete caches.

## Branding and platform workspace

The UI, windows, tray, MCP, executable, and installer use Desktop Alchemy / `desktop-alchemy.exe`. The Tauri identifier is `dev.masterain.desktopalchemy`; backups use `desktop-alchemy.platform.backup` v1. Production data is `%APPDATA%/dev.masterain.desktopalchemy/platform-v1/`. Legacy directories are not read or migrated; isolated tests use their explicitly assigned root.

After a logo change, run `pnpm tauri icon src/assets/brand/logo.png` to regenerate `src-tauri/icons/`. Fontsource fonts and license texts are indexed in `licenses/README.md`; preserve the applicable notices in bundles. Diagnostic/test programs are not installed.

The Rust workspace is `src-tauri/Cargo.toml`, with shared lockfile/target and interface/device crates under `crates/`. The development simulator is enabled with `pnpm tauri dev --features device-simulator` and is not a default release feature. `scripts/check-device-features.ps1` checks feature boundaries sequentially; `scripts/check-bindings.ps1` checks generated types. See [Adding a device](NEW-DEVICE.md), [Testing](TESTING.md), and [Hardware testing](HARDWARE-TESTING.md).
