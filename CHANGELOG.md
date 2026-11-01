# Changelog

## 0.0.2 — 2026-11-01

### Added

- Open files with the system default application through GIO. When no handler is
  available or launching fails, choose an installed application and optionally
  remember it as the default for that file type.
- Copy selected text or the full text from previews, including mouse drag
  selection and keyboard shortcuts.
- Copy absolute file and folder paths from the context menu. Multiple selections
  are copied one path per line; the background menu copies the current directory.
- Detect external drives and removable filesystems through UDisks2. Mount and
  open partitions, unmount them, and refresh device status every five seconds.
  The collapsible Devices section appears below Bookmarks.

### Fixed

- Search only the current directory's direct children in both browser windows
  and file choosers. Deduplicate entries by path.
- Keep text-editing shortcuts, including Ctrl+A, in search and other input fields
  instead of triggering file-list actions. Preserve these bindings when settings
  are reloaded.
- Continue opening nested folders during rapid repeated double-clicks.
- Perform trash, restore, permanent deletion and preview loading on background
  workers, and refresh the UI after operations finish.
- Prefill rename dialogs with the complete original name, allow extension edits,
  and retain the original target while the dialog is open. Enter confirms and
  Escape cancels; an existing destination is never overwritten.
- Render application-selection dialogs inside the file chooser and keep Escape
  from dismissing the outer chooser while a nested dialog is open.

### Packaging

- Read the Arch package version from `Cargo.toml` and keep the product version at
  the fixed SemVer `0.0.2`.
- Include GIO and UDisks2 runtime dependencies and the GIO build dependencies in
  the Linux build workflow.
- Use a version-independent README banner with anonymous sample paths.

External-drive actions require UDisks2 and, when authentication is requested, a
running desktop polkit agent. Encrypted-volume unlocking is not included.
