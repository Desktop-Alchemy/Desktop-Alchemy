# Desktop Alchemy

[简体中文](README.md) · English

Desktop Alchemy is a Windows x64 multi-device client built with Rust, Tauri 2, and Vue 3, with built-in support for M18 and Microsoft Audio Dock. M18 is controlled inside the client process through the official C Device SDK (`transport.dll`); no separate SDK application is required.

- M18 input from all 18 keys, static key images, key GIFs, temporary backgrounds, screen brightness, and software LED effects.
- Page editing, autosave, scene automation, 2D/3D configuration previews, and key navigation.
- An image asset and app preset library with programs, shortcuts, registered Windows apps, URLs, and custom protocols.
- Audio Dock audio settings, a five-band equalizer, and action mappings for read-only Teams-key input.
- A Windows tray, configurable close behavior, single-instance window activation, and settings backups.
- Chinese and English UI, with system-language detection and a manual preference.
- A language-independent process-plugin protocol, Rust examples, and a built-in computer performance plugin.
- An optional local MCP HTTP interface for AI Agents to manage assets, application presets, pages, scenes, device settings, plugins and backups, and launch applications.

[Documentation](docs/README.md) · [Client guide](docs/CLIENT.md) · [Development and packaging](docs/DEVELOPMENT.md) · [Plugin protocol](docs/PLUGIN-PROTOCOL.md) · [MCP](docs/MCP.md) · [Contributor instructions](AGENTS.md)

## Development startup

```powershell
pnpm install --frozen-lockfile
pnpm sdk:fetch
pnpm plugins:build
pnpm desktop
```

`pnpm desktop` starts the native client; `pnpm dev` provides a browser preview only. See [Development](docs/DEVELOPMENT.md) for the toolchain, on-demand validation, and installer build. Installed-app users do not need the development toolchain.

## Devices and extensions

Each device runs independently, offering pages, input, lighting, temporary overrides, and scenes according to its capabilities. Selecting a device changes only the editing target. Source adapters integrate through separate Rust crates and registration declarations. The development simulator provides display keys, rotary input, and analog input and is disabled in default releases. See [Device modules](docs/DEVICES.md) and [Adding a device](docs/NEW-DEVICE.md).

Audio Dock supports prompt tones, volume, push to talk, microphone noise suppression, and equalizer settings. Teams mappings default to disabled with no action; the original Windows behavior remains available. See [Audio Dock](docs/hardware/microsoft-audio-dock/README.md) and [Applications](docs/APPLICATIONS.md). The related MIT notice is indexed in [`licenses/README.md`](licenses/README.md) and distributed with the installer.

## Data and interfaces

Production data lives in the `platform-v1` subdirectory of the application data directory. Legacy data directories are not read or automatically migrated. The workspace and backup use dedicated identifiers and version 1; older formats are not imported. Each device's Core queues its operations. MCP management tools can persist application changes. The original device tools and plugins retain temporary-control semantics. MCP cannot write persistent boot images.
