# Computer Performance and Hardware Sensors

The built-in `dev.masterain.performance` plugin publishes runtime widget data through ordinary JSON-RPC stdio. Core and the adapter own connections, encoding, page generations, overrides, and frame coalescing. Collectors never load the device SDK or own M18.

## Metrics and sources

| Widget                        | Collection                                                                 | Display formats                              |
| ----------------------------- | -------------------------------------------------------------------------- | -------------------------------------------- |
| CPU                           | PDH `Processor Information(_Total)\% Processor Time`                       | Total utilization                            |
| Memory                        | `GlobalMemoryStatusEx`                                                     | Percentage, available, used                  |
| Disk capacity                 | `FindFirstVolumeW` / `FindNextVolumeW`, mount paths, `GetDiskFreeSpaceExW` | Available, used, used percentage             |
| Network speed                 | `GetIfTable2` InOctets/OutOctets deltas and monotonic elapsed time         | Download/upload, download only, upload only  |
| System battery                | `GetSystemPowerStatus`                                                     | Charge/power state, estimated remaining time |
| Temperature/fan/power/voltage | LibreHardwareMonitorLib 0.9.6, including nested hardware channels          | °C, RPM, W, V                                |

Enumerate local fixed/removable volumes with mount paths, excluding network, unmounted, and optical volumes. Hash volume GUIDs into source IDs. Network IDs derive from interface GUIDs; exclude loopback and NDIS/WFP filter layers while retaining physical, VPN, virtual, and disconnected interfaces. Limits are 64 volumes and 128 interfaces per collection. Available disk capacity follows current-user Windows quota semantics.

Hash native sensor identities into source IDs. Retain device hierarchy/full channel names and at most 256 supported channels per round. Cache each source separately. Remove missing sources' old samples while retaining saved bindings; wait for fresh samples when the source returns. Changed driver/library IDs require reselection, never automatic name-based rebinding.

CPU/memory, network, disk, and battery use independent collectors; hardware sensors use a separate process. Slow collection must not block other metrics or plugin heartbeats. Disk/battery sampling is about two seconds; other metrics about one second. Network collection resets its baseline on first sample, disconnect, counter rollback, or intervals exceeding five seconds. Invalid data remains distinct from a valid zero.

## Permissions and lifecycle

The Rust plugin launches self-contained `m18-sensors.exe` from fixed `plugins/performance/sensors/publish/` resources and first enumerates with ordinary permissions. It checks PawnIO registration/device access. Some GPU/disk metrics need no elevation; CPU/motherboard availability depends on hardware/drivers.

Only the explicit Enable hardware sensors UI action sends `sensors.enable`. If a driver is required, verify the bundled PawnIO 2.2.0 installer hash before opening it. If permission is required, launch the same collector's broker mode with `runas`. There is no permanent Windows collector service, automatic UAC, scheduled task, or elevated main client. Cancelled installation/authorization waits for explicit retry while basic metrics continue.

Ordinary collectors belong to the plugin Job. The elevated broker owns its own Job, retains the original parent handle, and uses a randomly named local duplex pipe. Its ACL is current-user-only; peers verify actual pipe PIDs, executable paths, and Windows session. Commands are limited to enumeration, subscriptions, reads, heartbeat, and stop, producing metric JSON. No arbitrary executable path, command, code, or hardware-write request is accepted. Input lines are bounded. The coordinator heartbeats every 500 ms; a three-second absence cancels work and an independent watchdog can terminate a stuck collector. Blocked hardware calls cannot extend life indefinitely after parent/connection loss.

Stopping the plugin asks collectors to exit within five seconds. The host grants the built-in plugin seven seconds for graceful cleanup, then reclaims the ordinary Job. Abnormal stop/timeout is an error. Driver installation is an independent user flow; disabling collection does not uninstall a shared driver. Failed collection is not restarted in a loop; retry is explicit.

## Display and persistence

Bindings may include optional `source_id/source_name`. Source catalogs and samples are volatile. Backups retain the selected source, not programs, drivers, authorization, or history. Moving to another computer usually requires source reselection.

Source discovery uses a short lease. An enabled plugin can discover while editing a widget without a saved binding; closing the properties releases it. Discovery does not enable plugins or elevate collectors. See [Sources and batch samples](PLUGIN-PROTOCOL.md#sources-and-batch-samples).

Widgets use a large value, short title, secondary text, and optional ratio bar. Network's two-line format shows download above upload with vector direction icons. Temperature, fan, power, and voltage have no invented default range/progress. Invalid or older-than-five-second samples display `—`; details remain in the client.

## Build and validation methods

- Development publication uses .NET 10 and NuGet `packages.lock.json`. Delivered Windows x64 components are self-contained.
- `scripts/build-sensors.ps1` builds, fetches/verifies signed driver resources, copies canonical license texts from `licenses/` into the self-contained component payload, performs hardware-free self-checks, and generates per-file sizes/hashes in FILES.json. Third-party provenance is in the component's SOURCES.json. Outputs/downloads/local artifacts are ignored.
- `pnpm plugins:build` and `pnpm package` include this workflow. Installation self-check verifies resources/runtime dependencies before SDK/firmware checks, without installing drivers, opening GUI, or proving sensor/screen effects.
- Select library tests or `pnpm test:plugins -Filter performance::` through [Testing](TESTING.md). Cases cover formatting, source isolation/removal, atomic batch validation, persistence, catalogs, blocking/crashed collectors, leases, cleanup, and backward-compatible optional fields, without device SDK access.
- Component `--self-test` validates formats, input limits, same-program/session identity, and actual pipe stop/disconnect/heartbeat deadlines with simulated processes. It does not install drivers, access physical sensors, or trigger UAC.

See [Extended sensors](HARDWARE-TESTING.md#extended-sensors) for physical comparison methods. Probe coverage, battery behavior, driver/elevation, screen readability, and Windows scaling need their own relevant checks; builds and SDK acceptance do not establish them.
