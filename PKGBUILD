# Maintainer: CoolLoong <wingdon@foxmail.com>
pkgname=tern
pkgver=$(awk -F '"' '
    /^[[:space:]]*\[workspace\.package\]/ { in_package = 1; next }
    /^[[:space:]]*\[/ { in_package = 0 }
    in_package && /^[[:space:]]*version[[:space:]]*=/ { print $2; exit }
' "$startdir/Cargo.toml")
# Required by makepkg; keep fixed. Product releases use only the SemVer pkgver.
pkgrel=1
pkgdesc="A fast native file manager built with Rust and GPUI"
arch=('x86_64')
url="https://github.com/CoolLoong/tern"
license=('AGPL-3.0-only')
depends=('fontconfig' 'freetype2' 'glib2' 'glibc' 'hicolor-icon-theme' 'libgcc' 'libx11' 'libxcb'
         'libxkbcommon' 'libxkbcommon-x11' 'openssl' 'shared-mime-info' 'vulkan-icd-loader'
         'udisks2' 'wayland' 'xdg-utils' 'zlib')
makedepends=('cargo' 'clang' 'cmake' 'lld' 'pkgconf')
optdepends=('zip: create ZIP archives')
optdepends+=('xdg-desktop-portal: browser file chooser portal integration')
options=('!debug' '!lto')
source=()
sha256sums=()

build() {
    cd "$startdir"
    export CARGO_TARGET_DIR="$startdir/target"
    export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
    export CARGO_PROFILE_RELEASE_LTO=thin
    export CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16
    export RUSTFLAGS="${TERN_RUSTFLAGS:--C link-arg=-fuse-ld=lld}"
    cargo build --release --locked --bin tern
}

package() {
    cd "$startdir"
    install -Dm755 target/release/tern "$pkgdir/usr/bin/tern"
    install -Dm644 packaging/linux/tern.desktop "$pkgdir/usr/share/applications/tern.desktop"
    install -Dm644 assets/icons/tern.svg "$pkgdir/usr/share/icons/hicolor/scalable/apps/tern.svg"
    install -Dm644 packaging/linux/tern.FileManager1.service "$pkgdir/usr/share/tern/dbus/org.freedesktop.FileManager1.service"
    install -Dm644 packaging/linux/tern.portal "$pkgdir/usr/share/xdg-desktop-portal/portals/tern.portal"
    install -Dm644 packaging/linux/tern.portal.service "$pkgdir/usr/share/dbus-1/services/org.freedesktop.impl.portal.desktop.tern.service"
    install -Dm644 packaging/linux/tern-file-chooser.kdl "$pkgdir/usr/share/tern/tern-file-chooser.kdl"
    install -Dm644 LICENSE "$pkgdir/usr/share/licenses/tern/LICENSE"
    install -Dm644 THIRD_PARTY_NOTICES.md "$pkgdir/usr/share/licenses/tern/THIRD_PARTY_NOTICES.md"
}

PKGDEST="$startdir/target"
BUILDDIR="$startdir/target/makepkg"
PKGEXT='.pkg.tar.zst'
COMPRESSZST=(zstd -c -T2 -9 -)
