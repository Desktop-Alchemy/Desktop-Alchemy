# Microsoft Audio Dock

## Scope and defaults

Built-in module ID: `microsoft.audio-dock`. Default Cargo feature `device-ms-audio-dock` is independent of `device-m18`. Rust code lives in `src-tauri/crates/device-audio-dock/src/`; views in `src/devices/ms-audio-dock/`. Preserve the bundled [copyright/license notice](../../../licenses/audio-dock-remapper-MIT.txt).

The device has one logical input, `1` (Teams key), without pages, displays, or lighting. New profiles have mapping disabled and no action, with no import from other tools. Mapping supports offline editing, autosave, and test runs. Listening preserves Windows' behavior. Audio settings use a separate configuration HID collection and do not belong to mapping/page configuration.

## Appearance and 3D model

Cards use transparent top-view posters derived from the same model. Audio settings, Equalizer, and Teams key share a lazy-loaded top-view 3D preview with image switching, drag rotation, wheel zoom, and front/top/back views. Narrow windows stack vertically. Section changes retain viewpoint/EQ drafts; device changes/home release the scene. The client omits the cable for compact views; standalone GLB retains it.

The Teams view subtly highlights the key. When the same ready device's snapshot `pressed` contains key 1, its cap uses the accent color and moves down 0.12 mm; release/offline clears it. The model does not invoke actions/write settings/infer mute or volume. Image/3D share status text.

Static scenes do not continuously redraw; hidden/offscreen scenes pause. Pixel ratio is capped at 1.5, without autorotation, inertia, dynamic shadows, or postprocessing. WebGL load/initialization/context failure falls back to the poster with retry while controls remain usable. Unmount releases observers/events/geometries/materials/embedded textures/environment maps/WebGL.

Generate from Microsoft specifications/product imagery:

```powershell
node scripts/build-audio-dock-model.mjs
node scripts/export-audio-dock-preview.mjs
```

Ignored `output/audio-dock-model/` contains `microsoft-audio-dock.glb`, self-contained `audio-dock-3d-preview.html`, `model-info.json`, instructions, and licenses. Generation synchronizes GLB/metadata to `src/assets/devices/ms-audio-dock/` and copies icon license texts from the central `licenses/` directory into generated deliverables. glTF 2.0 uses meters, +Y up, +Z front, origin at bottom center, embedded fabric color/normal PNGs, and named keys/rear ports/cable. The standalone preview supports rotation/zoom/four views/cable visibility/download without external rendering dependencies.

Posters use client `scene3d.ts`. In an isolated Vite browser, run `scripts/render-audio-dock-poster.js` through browser automation to generate transparent `audio-dock-poster.png`. Render directly at 480×360 to reduce fabric moiré when scaled. Synchronize after model/scene changes. The script rejects a real Tauri WebView.

Body dimensions are 167.6×80.2×81 mm. Button order/rear connectors follow product imagery and local specifications. Shoulder curvature, local dimensions, port depth, microphone holes, and bottom are estimates. The shortened presentation cable expands the overall bounds beyond body dimensions. There is no adapter/internal-speaker/manufacturing-tolerance model; do not use it for manufacturing/assembly.

Top marks use cached SVGs: outlined Teams and consistent thin-line media/volume/microphone symbols. `scripts/assets/audio-dock-icons/SOURCES.json` records versions/hashes/licenses. Validate sources, then convert curves/holes to 0.04-mm geometry; adjust display scale/silver-gray color without redrawing/compositing marks. Offline regeneration needs no runtime textures/online service. Delivery `icons/` retains originals, manifest, and licenses.

## Input and identity

Shared `RawInputService` owns a message-only window thread. Deduplicate usages and register `RIDEV_INPUTSINK | RIDEV_DEVNOTIFY` at startup even without devices, allowing later arrivals. The blocking `GetMessageW` loop does no idle polling. Hotplug/retry updates the device set without re-registering. Registration definitions are fixed for a process; restart after changes.

Match only VID `045E`, PID `084D`, Usage Page `FF99`, Usage `0001`. Report prefix `9B 01` is press, `9B 00` release; ignore other reports. Enforce DWORD alignment, a 64-KiB read limit, declared Raw Input header length, report sizes/counts, arithmetic overflow checks, and actual buffer bounds.

Prefer a hashed physical USB-parent instance matching VID/PID, avoiding vendor-reused ContainerIds that merge separate units. Fall back to ContainerId, then normalized path hash. Serial-less units may gain a new identity when moved between ports. Group one physical device's HID collections; Raw Input HANDLE is a connection-local lookup key only. Collection changes/disconnect/reconnect invalidate connection generations. Press state and unstarted actions never cross generations. Bind configuration to stable `device_id`, not enumeration order.

The shared dispatcher checks device/path/connection generation/logical key, updates state, filters duplicate edges, and publishes source events. Enabled actions execute in the background with bounded work per device, so blocking Shell calls do not occupy input threads. M18 SDK input uses the same dispatcher after M18 numbering normalization.

Valid edges also publish Core snapshots for immediate pressed visuals. The last press time remains on Teams after release, displayed locally, until disconnect/path change clears it. This lets users confirm delivery after returning from a launched application.

## Ownership and coexistence

Mapping coordination uses `Global\MsAudioDockRemapper.SingleInstance`. Another owner's lock marks Audio Dock busy and blocks mapping while M18 remains independent. Never terminate that owner. After it closes, retry in connection details or reconnect. Offline/busy profiles remain editable with manual test runs.

## Device settings

Secondary navigation has Audio settings, Equalizer, and Teams key through shared navigation/breadcrumbs. Audio settings uses `AppSwitch` for prompt/call options; EQ has factory/custom presets, five vertical bands, and explicit saving. Teams has mapping, target summary, and expandable editing/advanced settings. Device metadata stays in connection details.

Section changes retain unapplied EQ drafts. Confirm before discarding them for another preset; Cancel continues editing. Band edits do not write/preview hardware. Only successful save updates the current preset. Audio settings shows synchronizing/applied/error/read-again separately from Teams autosave. Target summary shows target/Change target; edit mode offers registered-app and preset pickers. Preset selection lists existing presets rather than duplicating registration. Retain target icons and place Test run beside Finish editing.

Connected indicates input availability, not successful settings readback. Auto-read only when the device target/availability changes, never because loading/readback/unrelated workspace snapshots change. Retain read errors and retry controls.

Support prompt enable/0–100 volume, push-to-talk, standard/low-volume microphone noise reduction, EQ selection/five bands. Connection details reads/copies model/serial/hardware/color/firmware. Unknown values remain unread; do not invent defaults. Serial numbers do not enter ordinary logs/configuration/backups. Completed information reads update workspace snapshots for the same connection generation, including cards, without first opening controls; reconnect invalidates caches.

Readback is authoritative. Startup/reconnect/control entry/Read again only query; there is no startup restoration/offline audio save. Toggles/modes submit directly. Volume submits on pointer release or after keyboard adjustment settles. Show Applied only after readback. Successful prompt enable/volume changes request one built-in audition, including disabling prompts; dragging does not continuously audition. Audition failure clearly reports that settings applied but playback failed, requiring reread without automatically resending settings.

EQ drafts require Save and apply to create/overwrite a custom preset. New custom preset names/edits five bands from the current values; Cancel writes nothing. Delete this preset confirms with Cancel focused. Revalidate slot/name; if active, switch to and verify Flat before deletion. Read back to confirm slot release, removal, and unchanged remaining presets. Factory slots 0–4 are immutable; custom slots 5–15 allow −12 to +12 dB. Names allow 19 UTF-8 bytes. Renaming is unavailable. Full slots disable creation but allow overwrite/deletion.

Strictly match the configuration collection to VID `045E`, PID `084D`, Usage `FF07:0222`. Associate by the same USB-parent identity as Teams; refuse unreliable association instead of selecting by order. Use the Windows HID stack without a replacement driver or Accessory Center DLL. Requests are 64-byte Feature reports `24 command length payload`; responses are Input `27 command status length payload`. Empty-payload `GET_INPUT_REPORT` write acknowledgments may have only three bytes, while interrupt captures pad to 64. Wire bands run high-to-low; public types run low-to-high.

Path: `DeviceManager → Audio Dock module service → module settings queue → Windows HID`. Serialize per device, with prerequisite reads, bounded writes, and readback in a transaction. Check connection generations and cancel obsolete tasks on disconnect. Overlapped I/O has a two-second deadline; wait for cancellation completion before releasing buffers. Never automatically retry timed-out writes; reopen/read on the next attempt. Configuration-channel errors do not change Raw Input connection state.

Windows HID uses shared handles; this application's transactions also hold a per-device process lock. Accessory Center background services may retain shared handles, so process existence alone does not prove exclusive ownership. Avoid adjusting settings in two tools: private reports lack transaction IDs and cannot arbitrate third-party writes. Report sharing/access errors as busy; command/readback mismatch invalidates the session and requires reread. Never terminate another process automatically.

Desktop `module_request(device_id, operation, payload)` and typed MCP management tools access the same owning module service for read/update operations, bounded by Rust `AudioDockSettingsUpdate`. Plugins and M18 `HardwareCommand` gain no settings-write/raw-HID access. Settings write/read hardware; runtime module JSON does not add device-specific public Core fields. Persistent module configuration uses separately versioned `module_data`.

## Protocol reference

| Feature                    | Write/query commands                                                                              |
| -------------------------- | ------------------------------------------------------------------------------------------------- |
| Prompt sound               | `34 / 35`; enable = eight `FF`, disable = eight `00`; read nonzero as enabled                     |
| Prompt volume              | `36 / 37`; one byte, 0–100                                                                        |
| Prompt audition            | `4E`; fixed `10 00 00 00 00 00 00 00`, without changing the prompt mask                           |
| Push-to-talk               | `3A / 3B`; one byte, 0/1                                                                          |
| Microphone noise reduction | `60 / 61`; standard 0, low-volume 1                                                               |
| Equalizer                  | `40 / 41` presets; `42` deletion with one slot byte; `43` used-slot mask; `44 / 45` active preset |
| Device information         | `18` serial, `1C` color, `02` hardware, `2F` firmware; verified product identity for model        |

Hardware version combines the high/low nibbles of `02` payload byte 6 and byte 5. Firmware combines `2F` payload byte 3, bytes 1–2 as little-endian integer, and byte 0. Validate complete EQ name/band lengths, exactly five bands, and signed ranges. Never format arbitrary bytes as a known setting. Audition/deletion follow the Dock implementation in official ContainerApp.Main 5.193.137.0; other Microsoft audio devices use different `42` formats.

Raw captures/analysis copies/detailed diagnostics may remain in ignored local storage, not in source documentation. Keep this reference focused on reusable contracts and procedures rather than chronological experiment results.

## Diagnostics and validation

```powershell
cargo build --manifest-path src-tauri/Cargo.toml --bin alchemy-input-diagnose --bin m18-test-launcher --features diagnostics --locked
./src-tauri/target/debug/alchemy-input-diagnose.exe
./src-tauri/target/debug/alchemy-input-diagnose.exe --raw-input
```

Both runs use independent temporary configuration. The first does not start Raw Input; the second listens only to Audio Dock, without opening M18 SDK. Warm up two seconds, then measure 60 seconds of process CPU and output registration/scan/report counts, cleaning temporary data afterward. Compare in the same idle environment; a zero increment below clock resolution is not proof of zero cost.

During the listener run, press/release Teams and reconnect to check full edges/recovery. For actual action validation, explicitly add `--launch-test`: with exactly one Dock it temporarily binds the owned test launcher, waits up to 180 seconds, reports `physical_launch`/arguments, and cleans up without modifying user configuration. Read-only input success does not establish successful launching.

Unit tests use simulated input/settings transport, never real hardware. Cover strict matching, malformed reports, no-device startup, stable registration, isolated bindings, old generations, duplicates, no-page capabilities, and atomic failures. HTTP tests reject Dock display/write/launch without expanding MCP permissions. Distinguish mock, SDK, native-window, and physical evidence in task/PR results.

### Settings diagnostics

```powershell
cargo build --manifest-path src-tauri/Cargo.toml --bin audio-dock-diagnose --no-default-features --features diagnostics,device-ms-audio-dock --locked
./src-tauri/target/debug/audio-dock-diagnose.exe
```

Exit the client normally to release ownership. This diagnostic uses independent temporary configuration and the same DeviceManager/Core, without M18 SDK; default mode queries only. `--output=<local-file>` may cache full serial-bearing results for local research. Use `--exercise` only for explicitly authorized physical testing: capture initial values, modify/read/restore each, then compare full settings. `--exercise-custom` changes/restores only the currently selected custom preset without creating a permanent test preset. On failure inspect actual readback; ACK alone does not prove restoration.

`pnpm test -Filter protocol::tests` runs focused protocol tests. `scripts/audio-dock-ui-check.js` uses memory IPC in an isolated Vite browser to check preset creation, late responses, reread failures, offline disabling, and Teams isolation. Browser evidence is separate from physical evidence.

`--exercise-presets` requires explicit physical-test authorization. Create a uniquely named preset in a free slot, verify bands/activation, delete only that new preset, restore the original, and compare the complete snapshot. Never remove preexisting presets.
