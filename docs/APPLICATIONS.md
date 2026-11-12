# Application Catalog and Shared Launching

## Catalog and icons

`application_catalog::ApplicationCatalog` enumerates the current user's complete AppsFolder entries, including desktop programs, Store apps, and other Shell registrations. RegisteredApplication.id comes from `SIGDN_DESKTOPABSOLUTEPARSING`; display names are never identity or replacement keys. The typed Tauri entry point is `registered_applications`. Sorting, searching, and duplicate names must not change target identity.

Enumeration and icon extraction run on blocking workers. The picker requests only visible icons, with at most three concurrent frontend requests. The backend caches identity-to-content-hash PNG mappings. Extract icons from the exact Shell item through IShellItemImageFactory/WIC, preserving transparency; failure shows a placeholder without replacing the target. Program targets retain exe/lnk extraction and custom-image protection.

## Targets and execution

`KeyAction::Launch { target }` uses typed targets:

| Target                | Saved fields                             | Execution                                                                                   |
| --------------------- | ---------------------------------------- | ------------------------------------------------------------------------------------------- |
| Program               | `path`, `arguments`, `working_directory` | exe/lnk with the original argument string and shortcut/working-directory rules              |
| RegisteredApplication | `identity`, `arguments`                  | Resolve the original AppsFolder parsing name; legacy AUMID identities use system activation |
| Uri                   | `uri`                                    | Send the complete URI to Shell separately, with no extra argument string                    |

`launch_service` centralizes validation, execution, and errors. Adapters do not call Shell. Physical actions and explicit test-launch requests share this service; offline test launching does not require a device handle. Explicit MCP management jobs may launch these typed targets. Plugins have no additional program-launch permission.

Program validation allows temporarily missing files; execution then reports a clear error. Keep missing registrations in configuration and fail explicitly rather than substituting a name match or first result. AppsFolder arguments are passed to Shell, but their treatment depends on the provider/app, which may retain original arguments and ignore additions. Use Program when deterministic shortcut argument/working-directory handling is required. AUMID targets use IApplicationActivationManager.

Preserve complete URI text. Validate scheme, structure, host, control/whitespace characters, special characters, and percent encoding. Protocols such as `msteams:` need no `//`; HTTP(S) requires a host. Check system associations, including ordinary commands, DelegateExecute, and DropTarget, then send the whole URI as `lpFile`. Never compose a command interpreter or decode and split a URI into arguments.

An `.lnk` can remain a shortcut or be read-only resolved to its exe target. Delayed picker, shortcut, icon, and catalog responses must be discarded after cancellation, target replacement, unmount, or an edit-session change.

## Persistence and backups

Production data uses independent `platform-v1` storage; legacy directories are not read or migrated. Pages and pageless devices share ControlBinding and typed Launch targets. Legacy `launch_program` normalization and old-index import are not supported.

`applications.json` has independent version 2. If missing, initialize deduplicated presets from valid workspace bindings. Preserve a damaged index. Workspace/backup use their dedicated version-1 identifiers and reject old formats. Typed targets and cached icons participate in backup transactions; plugin binaries, MCP credentials, logs, and hardware boot images are excluded.

## Validation methods

`program_launch` tests use their own temporary executable, shortcuts, unique current-user protocol, and Start-menu registration, cleaning registrations afterward. Relevant cases cover Unicode, spaces, arguments, directories, complete URIs, missing handlers, full enumeration, and exact Shell-item execution. Browser checks cover duplicate names, sorting/filtering, icon failure, cancelled late results, and device switching. Select checks through [Testing](TESTING.md).
