# Hardware Testing and Troubleshooting

## Method and boundaries

Define the capability, expected change, duration, and recovery method. Distinguish read-only diagnosis, temporary output, and persistent writes; confirm destructive/persistent changes separately. Identify the target through descriptors/device identity and release other owners normally. Never kill unknown processes to acquire hardware.

Record a baseline in independent data or recoverable configuration, change one main variable, and define measurement endpoints. Distinguish command construction, interface acceptance, bus submission, readback, and physical observation. Validate cancellation, timeout, reconnect, and shutdown recovery. Compare the final state to the baseline; preserve an uncertain/failed result rather than hiding it with further writes.

Select physical tests only for changed hardware behavior or an explicit need, using [Testing](TESTING.md). Keep serials, raw logs, before/after values, and detailed run artifacts locally. Report outcomes/limits in the task or PR rather than creating process-log documents.

## M18 UI checks

1. Connect the device after releasing official/diagnostic owners and wait for readiness.
2. Press/release all 18 keys in Hardware tests. Compare highlights, bottom keys, and physical positions; validate display and input numbering independently.
3. Save drafts, open the brightness pattern, and compare white/grayscale/RGB/gradient/checkerboard at low, medium, and high brightness. Return to the original page afterward.
4. Check solid, breathing, cycle, rainbow, wave, and comet; speed, primary color, and brightness 0/128/255; firmware-default switching; reconnect/restart recovery. Check all 24 static LED positions separately.
5. Test file/Base64 single-key images, same-image batches, clears, temporary background, and refresh. Restore the active saved page after temporary visual tests.
6. Choose a GIF independently and check binding, play, stop, clear, and loop status. SDK loop status does not prove actual rendering.
7. Record explicit observations as pass/fail/unsupported, leaving unknown capabilities unverified. The app can export a local report.

Writing a boot background replaces persistent device data and requires a separate confirmation. Automated diagnostics do not perform this write.

## Disconnect, stability, and cleanup

- Unplugging clears paths, key state, and pending commands. Reconnection obtains a new path/session and restores the saved page/settings.
- Surface SDK worker/call errors and follow connection supervision/retry limits. There is no SDK child process to kill. A native-library crash may terminate the client because the SDK is in-process.
- Rapid page switching discards unsent old work; brightness drags retain the latest complete output.
- For sustained-operation changes, observe at least 30 minutes of health/queue/error behavior with explicit restart/retry notes.
- Exit waits for readers, handles, and plugin processes. Restart preserves pages/configuration. Hidden windows are not exited processes.

## Command-line diagnosis

Exit the desktop owner first:

```powershell
cargo run --manifest-path src-tauri/Cargo.toml --bin m18-diagnose --features diagnostics -- --seconds 60 --data-dir .local/diagnosis
```

`--exercise` changes images, temporary backgrounds, LEDs, and GIFs and restores a numbered page, without writing a boot background. `--seconds 1830` permits a run exceeding 30 minutes. Never share device ownership with the client.

`--page-tour` cycles saved pages from the diagnostic root twice at approximately three-second intervals and returns to the original page. Copy configuration/assets to an independent `.local/` root if needed; do not write diagnostics into daily user data. USB capture requires [current descriptor identification](hardware/m18/USB-CAPTURE.md).

`--led-effects` uses temporary settings for solid, five animations, slow rainbow, and breathing brightness, then restores firmware default. It runs about 40–60 seconds and requires actual observation:

```powershell
cargo run --manifest-path src-tauri/Cargo.toml --bin m18-diagnose --features diagnostics -- --led-effects
```

## Troubleshooting

Production data is in `%APPDATA%/dev.masterain.desktopalchemy/platform-v1/`; see [Pages and assets](PAGES-AND-ASSETS.md) for files. Keep full local paths/serials out of shared reports.

| Symptom                         | Inspect                                                                                       |
| ------------------------------- | --------------------------------------------------------------------------------------------- |
| No device/connection            | Other owners, re-enumeration, connection error, owning SDK PID                                |
| Missing/mismatched SDK          | Fetch pinned DLLs for source builds or reinstall the complete package; no SDK server fallback |
| Asset/decode failure            | Managed copy, normalized local path, actual decode result                                     |
| Flipped rows                    | Output slot mapping versus already-normalized input; never flip input twice                   |
| LED color changes while dimming | RGB scaling and accidental use of native brightness commands                                  |
| Firmware default cycles colors  | Select static color for a fixed result; default mode is not reliably software-dimmable        |
| Good preview, wrong hardware    | Submission, SDK receipt, and physical effect separately                                       |
| Saved edits absent on screen    | Persistence, activation, asset preparation, and hardware-task errors separately               |

After SDK/protocol changes, select key physical cases again. For UI/copy-only changes, inspect controls, disabled states, confirmations, and arguments rather than overwriting device images to demonstrate layout correctness.

## Tray, library, and launch actions

Start with isolated browser/native UI checks from [Development](DEVELOPMENT.md#isolated-native-ui). In actual native windows check close/Alt+F4 hiding, tray restore/menu/quit, second-launch activation, persisted close settings, and unsaved-draft cancellation/discard. Observe process/plugin/MCP cleanup; hiding is not shutdown.

For the library, check nested folders, partially failed imports, rename/move, recursive-delete confirmation, and picker filtering. Selecting an image enters page autosave; cancelling retains the draft. Background/branding pickers exclude GIF where unsupported. Deleting a library entry must not break a saved page's retained asset bytes.

Use a temporary owned test program for path/arguments/working-directory checks. With exclusive hardware ownership, bind display and bottom keys and test in foreground and tray-hidden states. Holding does not repeatedly launch; release and press triggers again. Missing executables produce a recoverable launch error rather than preventing configuration load. Do not use destructive real-app arguments in automated tests.

Record browser, native-window, simulated-input, physical-input, and relevant 100%/150% Windows-scaling checks separately. Installer resource/self-check validation is not replaced by debug isolation.

## Performance widgets

Release all control owners and use a fresh independent root. A copied diagnostic/resources directory plus `--plugin-resources <directory>` avoids executable locks during independent builds.

```powershell
cargo build --manifest-path src-tauri/Cargo.toml --bin m18-diagnose --features diagnostics --locked
./src-tauri/target/debug/m18-diagnose.exe --performance-widgets --seconds 1800 --data-dir .local/performance-acceptance-new
```

The scenario creates CPU/memory and non-widget pages, switches, disables/re-enables collection, and observes the selected duration. Existing configurations are not overwritten. Ordinary plugin stdio feeds the same Core/adapter. `performance-health.jsonl` samples valid metrics, queues, pending frame slots, SDK accept/failure counts, and asset/cache file counts every ten seconds. Counts are not physical drawing completion. Inspect preparation and SDK errors together. Stop disables the plugin and exits the SDK without boot-image writes.

Visually inspect readability, updates, switching isolation, and disable recovery. Compare CPU to the same PerfMon total-processor counter and memory to the same physical-memory definition; differing sample windows need not match frame-for-frame. Watch host/plugin working sets, file counts, and bounded queues over 30 minutes. Test reconnect and restart separately.

## Plugin packages and complete stop

For relevant lifecycle changes use `pnpm test:plugins -Filter process::`. Native package import/replace/uninstall checks use isolated data, not user plugins/pages. Check native dialogs/composition separately. Imported ordinary executables start disabled; enabled widgets run only with saved bindings, not merely because a picker/preview opened.

When disabling, verify UI state, parent/Job-child exit, and stopped sample timestamps. Cover stuck handshake, ignored shutdown, surviving child, rapid toggles, failed persistence, and retry. Timeouts must not display success. Restart preserves disabled state; updates preserve it and button bindings. Installed-resource success does not verify screen presentation, scaling, or the setup wizard.

## Extended sensors

Use `m18-diagnose.exe --performance-sensors --seconds 1800 --data-dir .local/sensors-acceptance-new` with a new root. It reuses performance switching, stop/restart, health, and cleanup behavior, choosing a local volume, connected network interface, system battery, and ordinarily readable sensors. `sensor-catalog.json` and `config.json` retain the selected IDs; health output contains sample changes/failures. It neither installs drivers nor elevates automatically nor writes boot backgrounds.

Compare the same Windows volume capacity and interface byte counters over actual monotonic intervals. Check network-baseline rebuilding after reconnect. Preserve full hardware/channel names and compare the same collector library; do not rename an unknown probe to a guessed location. Independently check UAC cancellation, driver cancellation/success, elevated broker exit, and parent-process loss. A batteryless computer validates only the no-battery state; charge/discharge/time estimates require a battery. Screen readability requires observation, not SDK counters.
