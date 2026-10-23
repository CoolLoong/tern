#!/usr/bin/env bash
# Package an already-built Linux binary. This script does not publish anything.
set -euo pipefail
cd "$(dirname "$0")/.."

version=$(python3 - <<'PY'
import re
import tomllib
from pathlib import Path
version = tomllib.loads(Path('Cargo.toml').read_text())['workspace']['package']['version']
if not re.fullmatch(r'[0-9]+\.[0-9]+\.[0-9]+(?:-[0-9A-Za-z.-]+)?', version):
    raise SystemExit('Unsupported release version in Cargo.toml')
print(version)
PY
)
if [[ $(uname -s) != Linux || $(uname -m) != x86_64 ]]; then
  echo 'This package target requires Linux x86_64.' >&2
  exit 1
fi
binary=target/release/tern
actual_version=$(env -u DISPLAY -u WAYLAND_DISPLAY -u NIRI_SOCKET -u DBUS_SESSION_BUS_ADDRESS "$binary" --version)
if [[ "$actual_version" != "tern $version" ]]; then
  echo 'Build target/release/tern from the current version before packaging.' >&2
  exit 1
fi

name="tern-$version-linux-x86_64"
staging=$(mktemp -d "$PWD/target/release-package.XXXXXX")
trap 'rm -rf -- "$staging"' EXIT
package="$staging/$name"
install -Dm755 "$binary" "$package/bin/tern"
install -Dm644 packaging/linux/tern.desktop "$package/share/applications/tern.desktop"
install -Dm644 assets/icons/tern.svg "$package/share/icons/hicolor/scalable/apps/tern.svg"
install -Dm644 packaging/linux/tern.FileManager1.service "$package/share/tern/dbus/org.freedesktop.FileManager1.service"
install -Dm644 packaging/linux/tern.portal "$package/share/xdg-desktop-portal/portals/tern.portal"
install -Dm644 packaging/linux/tern.portal.service "$package/share/dbus-1/services/org.freedesktop.impl.portal.desktop.tern.service"
install -Dm644 packaging/linux/tern-file-chooser.kdl "$package/share/tern/tern-file-chooser.kdl"
install -m644 LICENSE THIRD_PARTY_NOTICES.md "$package/"
printf '%s\n' "$version" > "$package/VERSION"
cat > "$package/README.txt" <<'EOF'
Tern for Linux x86_64

Run ./bin/tern inside a desktop session. Native runtime libraries are required;
this archive is not statically linked. Official workflow builds use Ubuntu 24.04.

Desktop and portal integration resources are under share/. Portal service files
expect /usr/bin/tern after installation. For managed installation on Arch Linux,
use the source repository's PKGBUILD instead of copying files over an existing package.

Build, runtime dependencies and desktop setup:
https://github.com/CoolLoong/tern
EOF

mkdir -p target/dist
archive="target/dist/$name.tar.gz"
checksum="$archive.sha256"
tar -C "$staging" -czf "$archive" "$name"
(cd target/dist && sha256sum "$name.tar.gz" > "$name.tar.gz.sha256")
if [[ -n ${GITHUB_OUTPUT:-} ]]; then
  printf 'version=%s\narchive=%s\nchecksum=%s\n' "$version" "$archive" "$checksum" >> "$GITHUB_OUTPUT"
fi
printf '%s\n%s\n' "$archive" "$checksum"
