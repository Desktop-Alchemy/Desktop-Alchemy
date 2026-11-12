# Device SDK Integration

## Integration principles

1. Establish the contract and license: distinguish public headers, wrappers, and precompiled implementation. Headers alone do not mean the transport can be rebuilt from open source.
2. Pin reproducible sources, lengths, and hashes. Load only from expected directories and reject unknown same-name libraries. Keep one dependency manifest as the download source of truth.
3. Maintain a thin owner for ABI layouts, errors, handles, threads, and cleanup. Upper layers use typed commands/results rather than loading another SDK or bypassing its queue.
4. Specify ordering, indivisible batches, cancellation cutoffs, and timeout semantics. SDK success is neither completed presentation nor hardware readback.
5. Validate resource integrity, ABI, parsing, mock transport, and failures before checking actual target hardware and installed resources. Compare identical encoded inputs with explicit timing endpoints.

## M18 source and validation

The Windows client directly loads the official Device SDK C transport library. It does not require the official desktop application and does not use the Plugin SDK as a hardware layer.

- Pinned upstream commit: `324d498de6981ee5d9d437bf05818599b3c20781`.
- `vendor/device-sdk/native/SOURCES.json` records the source, download location, sizes, and SHA-256 values of `transport.dll` and three Microsoft C++ runtime DLLs.
- `src-tauri/crates/device-m18/src/native_sdk.rs` and `sdk.rs` define this project's C ABI/layout adaptation.
- Retain the SDK license notices indexed in `licenses/README.md` when distributing.

`pnpm sdk:fetch` reads the local manifest, verifies the pinned four DLLs, and refreshes the canonical SDK license texts in `licenses/`. It does not download an SDK executable or an old archive. DLLs are not committed to Git. Runtime checks verify all four hashes before copying them to `native-sdk-runtime/` and loading with restricted search paths.

Installed builds and installation self-checks use only installed `sdk/native/` resources. Development and source diagnostics explicitly use `vendor/device-sdk/native/`, independent of the working directory. Missing/mismatched installed resources are errors, never a reason to fall back to a source checkout or PATH.

The pinned SDK exposes C interfaces, C++ wrappers, and upper device declarations, but supplies the low-level transport library as a precompiled binary. The app maintains lifecycle, image preparation, batches, refresh, input parsing, and GIF scheduling in its adapter; it does not rebuild or replace the entire transport implementation. Binary fixes and ABI evolution still require upstream releases. No legacy WebSocket backend is provided.

## Updating the SDK

Updates are deliberate, pinned changes. Repeating `pnpm sdk:fetch` retrieves the same configured version; updating the vendor's desktop application does not update this dependency.

1. Compare headers, C ABI, errors, HID structures, supported devices, and licenses for the proposed release.
2. Update the commit, four sizes/hashes in SOURCES.json, and the matching `NATIVE_FILES`/pinned-commit checks in `sdk.rs`. Never replace DLLs alone.
3. Fetch files, reconcile function signatures/layouts in `native_sdk.rs`, update source documentation, and select [development checks](DEVELOPMENT.md#types-and-checks). Matching hashes alone do not establish ABI or hardware compatibility.
4. Use [USB capture](hardware/m18/USB-CAPTURE.md) with identical encoded images to inspect BAT/STP ordering and submission spans. Check input, lighting, images/GIF, MCP-stop recovery, reconnect, and installed self-check behavior for the affected device/firmware. Keep rollback dependencies and detailed run artifacts locally.

## Ownership and connection lifecycle

A dedicated worker serializes SDK submission. A separate reader handles blocking reads and idle timeouts. Enumeration is restricted to supported M18 VID/PID and vendor HID usage. Client and diagnostics share an ownership lock and do not take over official Stream Dock. Release other external control programs normally before a hardware session. Plugin subprocesses retain Job Object ownership independently of the SDK.

There is no SDK server/port. `sdk_pid` identifies the client/diagnostic process holding the DLL. Typed in-process events carry connect/disconnect, key edges, and diagnostic headers; UI, MCP, and plugins never receive DLL handles. Optional MCP HTTP has a separate lifecycle.

On exit, stop animation, finish the read thread, destroy device handles, and unload libraries. Disconnect invalidates paths and pending work. Re-enumeration restores the active page and saved settings. Read identities, serials, and firmware from devices rather than inventing defaults. Recheck capture descriptors after reconnecting.

## Command mapping

| Public operation           | C SDK / adapter behavior                                                                    |
| -------------------------- | ------------------------------------------------------------------------------------------- |
| Screen brightness          | `transport_set_key_brightness`, 0–100                                                       |
| LED brightness             | RGB scaling into whole/per-LED color output; native `setLEDBrightness` is disabled          |
| Whole/per-LED colors       | `transport_set_led_color` / `transport_set_single_led_color`                                |
| Software LED animation     | At most one complete 24-color frame per 50 ms; a coalesced slot, no per-frame receipt flood |
| Firmware default effect    | `transport_reset_led_color`                                                                 |
| File/Base64 key image      | Encode 64×64 JPEG and call `transport_set_key_image_stream`                                 |
| Same image on several keys | Consecutive uploads followed by one refresh                                                 |
| Clear one/all display keys | `transport_clear_key` / `transport_clear_all_keys`                                          |
| Temporary background       | `transport_set_background_frame_stream`, 480×272 RAM frame                                  |
| Persistent boot background | `transport_set_background_image_stream`, separate confirmation                              |
| Key GIF                    | Host-decoded frames/delays scheduled by the same worker, without per-key page refreshes     |
| Refresh/firmware query     | `transport_refresh` / `transport_get_firmware_version`                                      |

All image/GIF/clear paths use `sdk::m18_key_index`. Native input is normalized once to public keys 1–18 in `native_sdk`. Neither frontend nor plugins translate device slots. See [M18](hardware/m18/README.md) for geometry and LED constraints.

The public SDK has no image-readback call. Input `read` is not a command for downloading the device's existing boot image. A retained local source image is distinct from hardware contents; see [Boot-image readback](hardware/m18/README.md#boot-image-readback).

## Page batches and receipts

Render and encode the entire page before any submission. Submit background, 15 key images, and final refresh consecutively; never insert STP between page keys or interleave another source. GIF frames use the same submission worker. Standalone MCP/plugin key images refresh once.

After preparation, revalidate page/source generations and device path. Stopping MCP/plugins cancels prepared or dequeued work that has not been submitted. Reject an oversized page or preparation failure before any partial upload. Work accepted into the SDK cannot be guaranteed retractable.

The SDK return belongs to the current work item. Nonzero error codes are not success; idle read timeouts only mean no input is available.

| State               | Meaning                                               |
| ------------------- | ----------------------------------------------------- |
| Saved               | Local configuration was atomically persisted          |
| `queued`            | Core accepted work, not yet submitted                 |
| `sent`              | Submitted to the official C SDK queue                 |
| `acknowledged`      | SDK call returned successfully, not hardware readback |
| Physically verified | Explicit observation of the device effect             |

Continuous submission and one refresh constrain host-side ordering. They do not prove firmware swaps a complete page atomically. Capture timing ends at USB submission, not completed device drawing.

## Multiple M18 devices

One SDK owner manages several handles/readers. DeviceManager binds them to independent Core instances, and the worker rotates between complete batches. Input is independent of the selected UI device. One handle's open/command/heartbeat failure must not reset other handles; retry targets a specific device. Preserve whole-page refresh, resource validation, and reader-before-handle cleanup. See [Device modules](DEVICES.md).
