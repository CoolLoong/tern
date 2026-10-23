# Desktop integration

The Arch package installs the desktop entry and icon. It also installs the
FileManager1 activation file under `/usr/share/tern/dbus/` so it does not
automatically replace another desktop file manager.

To select Tern as the directory opener for your account:

```sh
xdg-mime default tern.desktop inode/directory
```

To opt into D-Bus file-manager activation after installing the package:

```sh
install -Dm644 /usr/share/tern/dbus/org.freedesktop.FileManager1.service \
  "${XDG_DATA_HOME:-$HOME/.local/share}/dbus-1/services/org.freedesktop.FileManager1.service"
```

An already running file manager may still own `org.freedesktop.FileManager1`
until it exits or the session restarts. To revert activation, remove the
per-user service file you installed and select your preferred directory opener.

The system tray requires a StatusNotifierItem-compatible host. Closing Tern's
windows leaves the process resident; the tray/application Quit command saves
the workspace and exits. The file chooser uses a separate helper without a
second tray icon.

Browser upload/save dialogs use the XDG FileChooser portal rather than the
directory-opening MIME association. See [file-chooser.md](file-chooser.md) for
portal selection and the Niri floating-window rule.
