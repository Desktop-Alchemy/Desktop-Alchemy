# Desktop Alchemy Brand Asset

`logo.png` is the transparent application artwork on a 1254×1254 canvas. Preserve its bottom-left peeking composition, clipped silhouette, scale, and tilt. Keep dark interior details opaque. Do not center the character, complete the cropped illustration, or alter the canvas to fill unused space.

Navigation, app settings, and favicon reuse this file. Regenerate Windows window, taskbar, tray, and NSIS icons from the same source:

```powershell
pnpm tauri icon src/assets/brand/logo.png
```

Platform-specific size/format conversion belongs to Tauri CLI. The application display name is Desktop Alchemy and its executable is `desktop-alchemy.exe`. The application identifier is `dev.masterain.desktopalchemy`; the backup-format identifier is documented in [Pages and assets](../../../docs/PAGES-AND-ASSETS.md).
