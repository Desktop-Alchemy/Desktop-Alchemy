# Documentation

Desktop Alchemy is a Windows x64 multi-device client built with Rust, Tauri 2, Vue 3, and typed device modules. Repository-wide development rules are in [AGENTS.md](../AGENTS.md).

These documents describe current contracts, reusable methods, and implementation references. Use current code and the pinned dependency manifest to resolve discrepancies. Keep run logs, captures, and intermediate research in ignored local directories rather than maintaining chronological process documents in the repository.

## Development guides

| Document                                       | Topics                                                                           |
| ---------------------------------------------- | -------------------------------------------------------------------------------- |
| [Repository guide](../AGENTS.md)               | Scope, ownership, module boundaries, resources, validation, documentation        |
| [Design](../DESIGN.md)                         | Information hierarchy, semantic tokens, accessibility, honest state presentation |
| [Development and distribution](DEVELOPMENT.md) | Toolchain, startup, builds, packaging, installed-app checks                      |
| [Testing](TESTING.md)                          | Risk-based scope, isolated state, minimal meaningful checks                      |
| [Architecture](ARCHITECTURE.md)                | Layers, adapters, queues, lifecycle, implementation map                          |
| [Device modules](DEVICES.md)                   | Identity, sessions, configuration versions, multi-device isolation               |
| [Adding a device](NEW-DEVICE.md)               | Capabilities, adapter contracts, cancellation, permissions, validation           |
| [Frontend](FRONTEND.md)                        | State ownership, async isolation, controls, previews, localization               |
| [Hardware index](hardware/README.md)           | Model-specific specifications and interface constraints                          |

## Product and interface references

| Document                                                | Topics                                                                           |
| ------------------------------------------------------- | -------------------------------------------------------------------------------- |
| [Client guide](CLIENT.md)                               | Languages, devices, pages, library, extensions, backups                          |
| [Applications and launching](APPLICATIONS.md)           | AppsFolder discovery, typed targets, shortcuts, URI execution                    |
| [Device SDK](DEVICE-SDK.md)                             | Pinned native dependencies, ABI, command mapping, receipts                       |
| [Pages, assets, and configuration](PAGES-AND-ASSETS.md) | Content modes, text visibility/position, rendering, persistence, assets, backups |
| [Plugin protocol](PLUGIN-PROTOCOL.md)                   | Packages, JSON-RPC, permissions, widgets, icons, device targets                  |
| [MCP](MCP.md)                                           | Local HTTP, authentication, application management, revisions, temporary control |
| [Performance and sensors](PERFORMANCE-SENSORS.md)       | Metrics, sources, permissions, collection lifecycle                              |
| [Hardware testing](HARDWARE-TESTING.md)                 | Physical checks, diagnostics, operation boundaries                               |
| [USB capture](hardware/m18/USB-CAPTURE.md)              | Descriptor verification, capture selection, measurement methods                  |
| [Translation files](../src/locales/README.md)           | JSON resources, translation platforms, placeholders, plural branches             |
| [Third-party licenses](../licenses/README.md)          | Canonical license texts, source versions, and distributable destinations          |

## Maintenance

- Update the owning guide when behavior changes and keep this index accurate.
- Keep stable interfaces, limitations, source attribution, and reproducible methods. Remove superseded instructions and intermediate progress narratives.
- Distinguish simulations, builds, protocol responses, SDK acceptance, bus submission, and physical observation.
- Do not publish full local serial numbers, detailed logs, captures, user data, or binary build artifacts.
- Add new device-specific behavior to its module rather than hardcoding device models in shared layers.
