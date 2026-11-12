# Client Guide

Desktop Alchemy is a local Windows x64 client for M18 and Microsoft Audio Dock. See [Development](DEVELOPMENT.md) for source setup and the [documentation index](README.md) for technical references.

## Interface language

By default, follow the Windows user's first preferred display language. Simplified/Traditional Chinese use the Simplified Chinese UI; all other languages and detection failures use English. Later Chinese entries do not override a different first preference.

Open App settings at the bottom of the sidebar and choose Follow system, Chinese, or English. Successful saving immediately updates the main window, tray panel, and native fallback menu and survives restart. Failure retains the previous language. Switching preserves drafts and running devices/plugins. System language is detected at startup and when Follow system is selected again.

Saved names, key text, device images, external plugin content, and technical log bodies retain their values. Application-provided native-dialog titles follow the UI; Windows buttons follow Windows. Localized error summaries retain original information in Diagnostic details.

## Devices and M18

Connect M18, close other software owning it, and wait for the connected state. Click the toolbar connection button for model, firmware, serial, connection errors, and reconnect. The workbench previews saved/draft configuration and controls screen/LED brightness and software effects: solid, breathing, color cycle, rainbow, wave, and comet. Animated effects offer speed; breathing/wave/comet offer a primary color. Per-LED static colors are in Hardware tests. The 3D view illustrates color/brightness/speed, without hardware readback, calibrated positions, or synchronized phase. 2D strips are decorative.

Installation includes the official native libraries; UI/MCP/plugins share one device connection. No separate SDK service is required. Reinstall if resources are missing or fail verification.

The M18 workbench defaults to 3D: drag to rotate, wheel to zoom, click keys to select, or switch to 2D. The page editor defaults to 2D. Model label replaces the top M18 marking with up to 32 text characters or a static library image. After saving it updates both previews, is stored per profile/backed up, and can return to default. It changes only the software model.

Device default effect selects firmware lighting, observed as color cycling on the supported M18. Brightness is unavailable in that mode; Use selected static color restores controlled lighting. Save/restore effect, speed, brightness, and color on reconnect/restart. Software animation continues in the tray; full exit returns to firmware defaults. Firmware effects use a static illustration. Hide/leave the view or set zero brightness to stop preview animation; reduced-motion mode uses a static frame.

## Audio Dock

Select Microsoft Audio Dock in Devices. It has Audio settings, Equalizer, and Teams key sections. Audio controls include prompt sounds/volume, push-to-talk, and microphone noise reduction. EQ provides presets and five draft bands with Save and apply; custom presets support creation/deletion. Section changes retain unapplied bands; changing presets asks before discarding them. Factory presets cannot be overwritten/deleted; renaming is unavailable.

Teams mapping is independent and autosaved. New profiles have mapping off with no action. Choose a program/shortcut, registered application, or URL/custom protocol. The configured view shows the icon/name/URL summary; Change target opens editing, with optional arguments/directory in Advanced options. The preset picker lists existing presets only. Test run and Finish editing share the action row. Mapping does not suppress Windows' original Teams behavior. Offline profiles remain editable/testable, and switching devices does not stop listening.

Connection details provide model, serial, hardware/color/firmware information and copying. Another owner produces a busy state; close that program and retry/reconnect. Desktop Alchemy does not terminate it, and M18 continues independently.

Audio settings use current device readback and are separate from local Teams autosave. Settings cannot be edited offline. Startup/reconnect reads without restoring local audio values. Successful changes write and read back. After uncertain readback, use Read again instead of repeating writes. Avoid simultaneous adjustments in different applications.

## Pages

The editor combines a page list, key layout, and properties. Display keys are 1–15, five columns by three rows from the top left. Bottom physical keys 16–18 run previous/home/next page by default.

Create, duplicate, reorder, rename, or delete pages, keeping at least one. Edit key text, colors, image/GIF, and press action. Changes autosave after a short pause. The save status offers retry and preserves failed edits. Selecting a page changes the editing target only; Activate this page saves first and explicitly changes the active device page. Editing another page does not interrupt the displayed one. Imported assets are copied locally, independent of original files.

Choose None, Text, or Pattern as the base display content. None shows only the background; Text shows the retained text without the image; Pattern shows the selected image or icon, with optional text. Show text hides the label without deleting it; the text field stays editable, and text position/color controls are disabled while hidden. Position the whole text block at the top, center, or bottom. Switching to Text enables the label; switching to Pattern disables it by default. Reselecting the same content preserves its settings. Existing pages keep their previous appearance until edited.

Images appear within their key screens; GIFs play only in Pattern mode and do not overlay text. GIF patterns disable the text-overlay controls. Switching modes retains text, images, and press actions. The temporary whole-screen background is covered by normal opaque key images, so ordinary page editing does not offer it. Hardware tests retain temporary backgrounds and separately confirmed persistent boot-image writes. Existing background data survives saves/backups.

All 18 keys support launch actions for `.exe` and executable-target `.lnk` files. Supply arguments/working directory and use Test run. Quote arguments containing spaces. Shortcut arguments precede additional arguments; an empty working directory uses the shortcut's value, then the target directory. Update moved targets. Launched programs run independently and are not closed with Desktop Alchemy.

## Library

Library contains Assets and Applications tabs, selectable with mouse or arrow keys. Assets supports batch PNG/JPEG/GIF import, nested folders, rename/move/delete, and reusable image selection. Identical content within a folder reuses entries; different images with the same name gain a suffix. Initial indexing collects saved-page assets, excluding brightness tests. Choosing an image enters page autosave; direct import also adds it to the root. Deleting library entries/folders retains underlying images used by pages.

Applications stores reusable launch presets. Add a program/shortcut with a readable name, arguments, and directory. A shortcut can be retained or resolved to its original executable/arguments/directory; Cancel changes nothing. Cache icons automatically and use Refresh icon to retry. Search by name/path, edit, test, or remove. Different arguments may create separate presets. Only paths are indexed; moving the source requires updating them.

Add Windows application opens the installed desktop/Store application catalog with names/icons. Save the selected name and optional arguments without exposing raw Shell launch identities in normal forms. Registered applications use their system working directory and may ignore arguments. Uninstalled applications retain their presets/actions and fail explicitly when invoked. Install them on a destination computer after restoring a backup.

Choose from applications copies a preset to a key/input action. Empty display keys receive the cached icon and a default/blank title may receive the application name; custom text/images remain. Replacing a preset changes only previously automatic icons. Bottom keys receive actions only. Subsequent preset edits, icon refresh, or removal do not rewrite existing keys or delete original programs/cached assets. Initial preset indexing deduplicates existing saved launch targets; it does not continually rescan.

## Tray and app settings

Close/Alt+F4 hides to the Windows tray by default, preserving drafts and keeping actions, plugins, MCP, and lighting active. Left-click restores the window. Right-click opens a themed panel with connection status, Devices/Pages/Extensions, and Show window/App settings/Exit. Right-click again, click outside, or press Escape to dismiss; Tab/arrows/Enter are supported. Starting another instance restores the existing window.

Tray navigation preserves edits and existing modals; Show window keeps the current location. A failed panel uses the native Show window/Exit menu. App settings can persist direct exit instead of hiding. True exit first saves all drafts and asks about discarding only if saving fails. Cancel returns to editing. Tray-creation failure is logged and window close exits normally.

## Settings backup and restore

Export settings creates `.m18backup` with imported images/application icons, personal library, launch targets/arguments, saved profiles/pages/actions, themes, device settings/capability observations, plugin IDs/bindings/disabled state, model labels, close behavior, and language preference. Export waits for saving and stops on failure.

Import settings validates and summarizes the time/version/counts. Import and restart replaces configuration and loses unsaved drafts. Before replacement an automatic backup is saved under local `backups/`; startup notices/logs show the result/path. Failure retains or rolls back original configuration.

Programs, shortcuts, and plugin binaries are not copied. Another computer may need path updates/plugin reinstallation. Ordinary plugins need manual startup; bound enabled widgets start as usual. Local MCP settings/tokens remain. Device boot images, logs, and rebuildable caches are excluded. Import trusted backups because restored actions retain their launch behavior. See [Settings export and import](PAGES-AND-ASSETS.md#settings-export-and-import).

## Extensions

MCP provides optional local AI application management through Streamable HTTP. Copy its address/token into the Agent configuration. AI can manage assets, launch presets, pages/actions/scenes, device settings, plugins and backups, including launching applications and restoring inspected backups without per-call desktop confirmation. Saved management changes survive disabling; the original images/GIF/background/brightness/LED tools remain temporary and are restored when disabled. See [MCP](MCP.md).

Select a `.m18plugin` or the built-in example, inspect it, and confirm installation. Managed files survive deletion of the source archive. New executable plugins are disabled until enabled. Ordinary plugins require manual restart after client restart; enabled widget plugins run with saved bindings. Replacement confirms the same ID and preserves bindings/state. Uninstall stops/removes files but retains bindings. Native plugins run with your user privileges: enable trusted sources only. Disable ends the plugin and child processes, stops performance sampling, and persists across restarts.

Counter example: key 1 increments; key 2 selects the next page. Development reference: [Plugin Protocol](PLUGIN-PROTOCOL.md).

## Hardware tests and logs

Hardware tests provides key location/input, brightness patterns, individual/batch images, Base64, backgrounds, GIF, per-LED colors, capability observations, and report export. These operations affect hardware. See [Hardware Testing](HARDWARE-TESTING.md). Return to original page leaves brightness tests; Restore current page withdraws temporary visuals. Boot-image writes survive power loss and require independent confirmation.

Open runtime logs from navigation/footer. Queued/sent/accepted/failed operation states and application/key/plugin events are shown. Accepted means the interface accepted work; observe physical screens/lighting separately. Bottom-right notifications expire without using workspace space. Themes support system/light/dark.

Local data defaults to `%APPDATA%/dev.masterain.desktopalchemy/platform-v1/`. See [Pages, Assets, and Configuration](PAGES-AND-ASSETS.md) for file ownership.

## Computer performance buttons

Select a display key, open Display content, browse/search plugins/content/presentations, and confirm Use this content. Cancel leaves the key unchanged. CPU shows total utilization; memory defaults to percentage and used/total GiB. Configure text/background/accent/progress independently. Saved bindings start sampling and resume on restart; multiple keys/pages may share metrics. Actions remain independent. Ordinary content restores original text/images; physical keys have no display content.

Computer performance is bundled. Disable persists and restores original visuals; Enable runs it when bindings exist; errors offer Retry. Missing/initial/stale values show `—`, not fabricated zero. Sampling continues hidden to the tray and stops on full exit. Memory presentations include percentage, available capacity, and used capacity per button.

### Additional metrics and sources

Disk capacity, network rate, system battery, temperature, fan speed, power, and voltage may require an explicit source in plugin settings. Each button retains its source when renamed/disconnected instead of switching automatically. Full device/channel names appear in the source picker; keys use short titles.

Disk defaults to available/total, with used capacity/percentage alternatives in GiB/TiB. Network uses actual receive/transmit bytes for the selected interface, together or separately, in B/s/KiB/s/MiB/s/GiB/s. It includes LAN/Internet traffic and does not sum VPN/physical interfaces automatically. First samples, resets, and reconnects show `—`; valid idle rates show `0 B/s`.

System battery uses Windows charge/discharge/remaining-time reports, excluding Bluetooth peripherals. Missing battery/estimates show `—` with a reason. Disk/battery update about every two seconds; CPU/memory/network/hardware sensors about every second.

Try hardware collection without elevation first. Enable hardware sensors explicitly requests a bundled signed-driver installation/Windows authorization where needed. Cancellation leaves basic metrics active. Browsing/restarting never automatically prompts UAC or installs a persistent collector service. The driver may be shared by other monitoring software and is not automatically removed on uninstall.

Availability depends on hardware, drivers, and permissions. Chassis temperature requires an actual sensor, never an estimate from unrelated readings. Fan/power/voltage are read-only and do not control hardware. See [Performance and Sensors](PERFORMANCE-SENSORS.md).

## Transparent backgrounds and plugin icons

Background color accepts a picker, HEX, and common colors. Transparent PNG regions reveal that color; opaque pixels cover it. The transparent common-keys folder offers light/dark symbols.

Installed plugin icons appear in read-only plugin folders under Library → Assets and in image pickers. Stopping/disabling does not remove them. Uninstall removes the folder while retaining key references/background/text/actions and a missing-source indication. Reinstalling matching IDs restores images. Icon-only packs need no startup.

## Multiple devices and saved profiles

The toolbar selects the editing target while other devices continue running. Disconnecting retains its offline profile and never redirects control. Device identity markers distinguish matching models. Library/application presets/extensions are shared; pages/scenes/settings belong to each profile. Failed drafts survive switching; exit tries saving all drafts.

The platform uses its own workspace/backup formats and leaves older directories untouched without automatic import. Saved profiles opens offline/read-only configurations; unavailable modules, unsupported versions, and invalid data explain their restrictions. Hardware-test manual disabling is separate from observed results; a recorded failure does not rewrite base capability declarations.
