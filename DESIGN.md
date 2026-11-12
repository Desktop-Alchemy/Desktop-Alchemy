# Desktop UI Design Guide

This guide describes reusable design principles for device-control applications. Component names, colors, navigation labels, models, and pixel values are Desktop Alchemy examples. Other pages should derive their structure from user tasks, device capabilities, and data risks.

## Start with user tasks

Separate global tasks, object-specific tasks, and work within an object. Keep global navigation stable. Selecting a device opens its workspace, whose available sections follow its capabilities. Do not let a page appear to belong to one device while silently applying changes to another.

Prioritize frequent, reversible, understandable actions. Put infrequent or high-impact options behind progressive disclosure. Development notes, protocol explanations, and speculative features belong in documentation. Wide layouts should become readable single-column layouts on narrow windows; give each scrolling region and focus scope a clear boundary.

## Use semantic visual tokens

Define roles for canvas, text, secondary text, panels, emphasis, danger, success, and warnings. Components consume those roles rather than scattering color literals. Light and dark themes retain meaning and contrast.

Use a limited type scale, spacing scale, corner radii, and shadow system. Reuse components for the same semantics, and change appearance when meaning changes. Decorative device geometry and preview borders must not imply a hardware state that has not been read.

## Complete interaction states

Every actionable control needs normal, hover, focus, pressed, disabled, loading, and failure behavior. Unknown values differ from off, zero, or a default. Asynchronous operations identify their target and provide meaningful progress, recovery, and retry behavior.

Dialogs state confirmation and cancellation consequences. Destructive or draft-discarding dialogs initially focus the safe action. Use modal focus handling and keyboard navigation. Cancelling returns to the prior context and restores focus. Do not communicate state through color alone.

## Represent operation semantics accurately

Keep editor drafts, persisted configuration, hardware readback, and command results distinct. Queued, sent, SDK-accepted, device-completed, and visually verified are different states. Copy must not claim a stronger result than the layer can establish.

State the target, persistence, and recovery consequences of device or system changes. Label software-only previews. A last input event is not necessarily a currently held physical key. Bind asynchronous results to the originating target and version and discard stale results after switching.

## Consistency, input, and motion

Reuse controls and keyboard behavior for the same semantics. Switches represent booleans, checkboxes represent sets, and single-choice selectors represent one value. Custom visuals must retain accessible names and native input semantics.

Animate to explain a transition or state. Respect reduced-motion preferences. Stop expensive work when hidden or offscreen; avoid perpetual rotation, dynamic shadows, or decorative flashing. Provide a static alternative for 3D and complex previews, and a useful recovery path after failure.

## Preview and hardware boundaries

Preview only known data: configuration, UI selection, and received input events. Do not infer unread volume, lighting, or internal device state. Identify approximate geometry and LED positioning as illustrative. A model or browser simulation is not physical-device verification.

When 2D and 3D represent the same object, keep numbering, selection, and action semantics consistent. Rotation and key selection affect the UI only unless the user explicitly invokes a device operation.

## Desktop Alchemy implementation

- Cold blue-gray neutrals, an indigo accent, semantic colors, and light/dark themes use tokens in `src/style.css`.
- The main window has a compact sidebar, toolbar, independently scrolling workspace, and status bar. Global navigation stays separate from device sections.
- M18 has 15 display keys in a 5×3 grid and three physical keys, numbered 1–18. Its 3D preview loads on demand and stops drawing when static or hidden. The model's light and display output represents configuration, not device readback.
- `AppSwitch.vue` and `AppSelect.vue` implement project controls with accessibility, keyboard input, and theme support. Library and weekday selection retain checkbox semantics.
- Audio Dock separates audio, equalizer, and Teams-key settings. Hardware-applied state differs from a saved input mapping.
- Dialogs use modal focus management and a safe initial action. Long translated content opens at the heading. Motion honors reduced-motion settings.

See [Frontend](docs/FRONTEND.md), [M18 model assets](src/assets/devices/m18/README.md), and [Audio Dock](docs/hardware/microsoft-audio-dock/README.md) for implementation details.
