# Privacy and local data

Tern has no application analytics, telemetry upload or cloud synchronization.
Normal file browsing is local. External programs launched through Open With or
the terminal have their own behavior; builds fetch public dependencies.

## Data written by Tern

| Data | Default location | Contents |
| --- | --- | --- |
| Preferences | `~/.config/tern/settings.json` | Theme, hidden-file setting and keybindings |
| Workspace | `~/.config/tern/workspace.json` | Window layout and open locations |
| Bookmarks | `~/.config/tern/bookmarks.json` | Saved filesystem locations |
| Sidebar state | `~/.config/tern/state.json` | Persisted sidebar UI state |
| Font cache | `~/.cache/tern/fontdb-v3-*.json` | Font file locations, metadata and family mappings |
| Recent files | `~/.local/share/recently-used.xbel` | Shared desktop recent-file records |
| Trash | Home or per-volume XDG trash | Deleted payloads and their original locations |

Configuration follows `XDG_CONFIG_HOME`. The font cache and Recent file use their
respective XDG locations. File paths may reveal names, project names or other
personal details.

Tern uses the session D-Bus for single-instance requests, file manager requests,
the system tray and file chooser. The chooser returns selected file URIs to the
desktop portal. Startup and diagnostic logs can include home, configuration and file paths.

## Sharing diagnostics

Use a small temporary directory with invented filenames. Redact personal paths,
document contents and identifiers from logs and screenshots. Do not attach your
entire configuration, Recent database, environment variables or home directory.
