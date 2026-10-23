#!/usr/bin/env bash
# Run the same tests locally and in CI without connecting to the user's desktop.
set -euo pipefail
cd "$(dirname "$0")/.."

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export CARGO_PROFILE_RELEASE_LTO="${CARGO_PROFILE_RELEASE_LTO:-thin}"
export CARGO_PROFILE_RELEASE_CODEGEN_UNITS="${CARGO_PROFILE_RELEASE_CODEGEN_UNITS:-16}"
export RUSTFLAGS="${RUSTFLAGS:--C link-arg=-fuse-ld=lld}"

env -u DISPLAY -u WAYLAND_DISPLAY -u NIRI_SOCKET -u DBUS_SESSION_BUS_ADDRESS \
  dbus-run-session -- cargo test --release --locked --workspace --lib --bins --tests -- --test-threads=1
env -u DISPLAY -u WAYLAND_DISPLAY -u NIRI_SOCKET -u DBUS_SESSION_BUS_ADDRESS \
  dbus-run-session -- cargo test --release --locked --workspace --doc -- --test-threads=1
env -u DISPLAY -u WAYLAND_DISPLAY -u NIRI_SOCKET -u DBUS_SESSION_BUS_ADDRESS \
  cargo test --release --locked -p gpui-pre-wgpu --test font_cache -- --include-ignored --test-threads=1
