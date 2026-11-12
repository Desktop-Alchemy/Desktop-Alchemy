# Adding a Device

This guide covers buttons, encoders, sensors, displays, and composite devices. OS APIs and SDKs vary by adapter; configuration, identity, lifecycle, authorization, and validation principles remain shared.

First decide whether a new adapter is needed. Extend an existing adapter's capability description for firmware variations. A separate discovery mechanism, connection lifecycle, or transport may justify a new module. Do not make shared host code branch on a new model.

## 1. Define capabilities

Before coding, specify each app-visible capability's range, units, optionality, and failure semantics.

| Capability         | Questions                                                                                  |
| ------------------ | ------------------------------------------------------------------------------------------ |
| Identity/discovery | How are physical devices, multiple interfaces, and duplicate discoveries distinguished?    |
| Input              | Are reports edges, increments, or samples? How are release, units, and ranges represented? |
| Output             | Which reversible operations exist, with what targets, dimensions, formats, and budgets?    |
| Configuration      | Which values persist, which are readback, and which are temporary overrides?               |
| Lifecycle          | How do disconnect, cancellation, timeout, and exit stop work and release resources?        |

Do not collapse interface presence, successful calls, and verified physical effects into one boolean. Pageless devices must not fabricate pages; unknown readback stays unknown rather than using guessed defaults.

## 2. Define the adapter boundary

The adapter owns device discovery, protocol parsing, connection, encoding, and submission. The host owns stable public identity, persistence, scheduling, permissions, events, and UI lifecycle. Pass typed commands/results, never arbitrary reports, native handles, or internal locks.

Assign one explicit owner for a process-exclusive SDK and all its compatible device handles. Do not create another write path. Put blocking calls on workers so UI, HTTP, and plugin communication do not execute device I/O directly.

## 3. Stabilize identity and sessions

Do not derive identity solely from enumeration order, current selection, or a changing path. The adapter provides a reproducible local ID; the host combines it with the adapter ID into an opaque public ID. Reconnection always creates a new generation, even for an unchanged path/serial.

Capture device ID and Connection when work begins. Recheck the session before submission, state publication, or settings persistence. A delayed response belongs only to its originating device, never the device selected later.

## 4. Design configuration and migration

Keep persistent configuration separate from runtime state. Use explicit versions, defaults, strict validation, and an independent schema for private module data. The host stores its payload opaquely. Migrate one version at a time, validate the completed candidate, and atomically commit; failure retains the original file and memory.

```text
read current → build candidate → public validation → module validation
             → atomic save → publish state
```

A successful device write does not establish successful local persistence. Do not silently copy readback into saved configuration. Reference assets using host-managed/content-addressed IDs rather than arbitrary file paths.

## 5. Normalize input and declare output

Normalize once in the adapter. Buttons use stable control IDs and press/release edges, rotary input retains signed increments, and analog input declares finite ranges, units, thresholds, and hysteresis. Specify deduplication, reconnect baselines, and timestamps. UI and adapters must not independently reinterpret key numbering.

Display/batch capabilities declare stable region IDs, slots, dimensions, layout, and per-item limits. Prepare the complete user operation and validate the total budget before queuing an indivisible batch. Never interleave other sources inside it. Do not claim atomic hardware presentation unless the device contract guarantees it.

## 6. Handle concurrency and shutdown

Serialize dependent work per device; provide bounded, fair scheduling between devices. Limit operation count, memory, and wait times, and reject oversized requests before queuing.

Cancellation reaches preparation, queued work, and waits. Unsubmitted work can be withdrawn; native SDK/driver work may not be. Shutdown stops new work, cancels and awaits workers, then destroys handles, unloads libraries, and frees shared buffers. Destructors must not be relied on to kill blocked threads.

## 7. Expose minimal permissions

Use strictly parsed types and explicit target IDs; reject unknown fields, operations, and unauthorized targets. Do not expose arbitrary USB/SDK messages, Shell execution, or persistent settings to external extensions without a deliberate permission contract and authorization. Revalidate identity, generation, and permissions on each request.

## 8. Validate by risk

Use simulation and temporary roots for identity conflicts, duplicates, disconnects, stale sessions, delayed responses, cancellation, migration failure, budget limits, and shutdown. Separately check physical input, output, readback, reconnect, sustained operation, and exit when needed. Compilation, protocol success, SDK receipts, USB submission, and visual observation are distinct evidence.

## Repository implementation

1. Implement DeviceAdapter in an independent crate using AdapterHost for discovery, Connections, and batches. Do not depend on Tauri/Core.
2. `DeviceDescriptor.id` is module-local; host-generated global IDs are opaque to callers.
3. Provide `config_version`, defaults, validation, incremental migration, and `asset_references` for persistent assets.
4. Retrieve work with `next_batch`, check `Host::current` for connection/page/source/resource generations, and report with `complete`.
5. Register UI under `src/devices/<module>/` and send requests through a fixed device context.
6. M18 normalizes public keys 1–18 in its adapter. Audio Dock has a Teams input and separate settings channel; no page editor is shown for a pageless device.

See [Device modules](DEVICES.md), [Testing](TESTING.md), and [Hardware testing](HARDWARE-TESTING.md).

## Development simulator

Run `pnpm tauri dev --features device-simulator`. It is excluded from default releases and provides 32 displays of 80×80 pixels, a rotary encoder, and a 0–100% slider. It can simulate GIF batches and interface receipts, not physical drawing.

The desktop module service can inject input into an explicitly selected simulator:

```json
{ "type": "rotary", "control_id": "knob", "delta": -7 }
```

```json
{ "type": "analog", "control_id": "slider", "value": 75 }
```

These are module-request examples, not an arbitrary external device-input API.
