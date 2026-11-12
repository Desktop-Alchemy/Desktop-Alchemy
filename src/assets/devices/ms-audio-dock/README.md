# Audio Dock Client Visual Assets

`audio-dock.glb` is the Revision 2 model generated and synchronized by `node scripts/build-audio-dock-model.mjs`. `model-info.json` records dimensions, approximation limits, source SVGs, pinned versions, and hashes. See [Microsoft Audio Dock](../../../../docs/hardware/microsoft-audio-dock/README.md#appearance-and-3d-model).

`audio-dock-poster.png` is a 480×360 transparent top-down rendering from the same client scene. Run `scripts/render-audio-dock-poster.js` in an independent Vite browser to regenerate it. Rendering at the target size reduces fabric-texture moire. The device list loads only the poster; settings load the engine/GLB on entering 3D, initially top-down.

Source SVGs and licenses are under `scripts/assets/audio-dock-icons/`; the frontend distributes their license text. Units are meters with +Y up and +Z forward. Some structures are photo estimates, not assembly dimensions. The client hides the standalone model's cable without deleting it from the GLB.
