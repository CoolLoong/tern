# Headless validation

The local test entry point is `./tools/check-headless.sh`. It runs every
workspace test on an isolated session bus and all font-cache integration tests
(including the host-font check), with desktop connection variables removed.
No known application failures are excluded. Run `cargo fmt --all -- --check`
for formatting. GitHub workflows cover builds and releases; tests are run locally.
Unit/integration tests and doctests are explicit targets; GUI example binaries
contain no tests and are type-checked with `cargo clippy --workspace --all-targets`.

Run `python tools/check-architecture.py` to check the scene boundaries.

The directory-loading regression tests use GPUI's `TestPlatform` and
`TestWindow`, not native windows. This command removes desktop connection
variables and runs only the listing tests (including navigation races,
selection, sorting, Recent, cancellation and shared row storage):

```sh
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_RELEASE_LTO=thin CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16
export RUSTFLAGS='-C link-arg=-fuse-ld=lld'
env -u DISPLAY -u WAYLAND_DISPLAY -u NIRI_SOCKET -u DBUS_SESSION_BUS_ADDRESS \
  cargo test --release --locked --bin tern listing -- --test-threads=1
```

Directory reads, Recent's XBEL parsing/file checks, trash origin lookups, row
indexing and sorting run on background workers. Sort/filter views share
immutable rows; only index arrays are rebuilt. Superseded workers receive a
cancellation flag, and each load/search/view stage has an identity check before
publishing. Cancellation is cooperative: it cannot interrupt a filesystem call
already in progress, but stops further work once that call returns. A native
sort already running finishes before its next cancellation check.

To run the application package by itself, including all regression tests:

```sh
dbus-run-session -- env -u DISPLAY -u WAYLAND_DISPLAY -u NIRI_SOCKET \
  cargo test --release --locked --bin tern -- --test-threads=1
```

# Desktop startup checks (opens real windows)

Tray protocol tests need an isolated session bus and never open native windows:

```sh
env -u DISPLAY -u WAYLAND_DISPLAY -u NIRI_SOCKET -u DBUS_SESSION_BUS_ADDRESS \
  dbus-run-session -- cargo test --release --locked --bin tern tray_tests -- --test-threads=1
```

Run from the repository root in a Niri session. Both tools create separate
profiles under `target/` and clean up only their own child processes. Neither
changes the installed package. When the invoking shell lacks desktop variables,
the tools use the sole Niri socket in the user's runtime directory.

```sh
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_RELEASE_LTO=thin CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16
export RUSTFLAGS='-C link-arg=-fuse-ld=lld'
cargo build --release --locked --bin tern
dbus-run-session -- cargo test --release --locked --bin tern
python tools/check-startup.py
python tools/bench-startup.py --fresh-process --samples 15 --label fresh
python tools/bench-startup.py --samples 15 --label resident
```

`check-startup.py` covers surviving-window reuse, per-window workspace saving,
close/reopen in the same process, concurrent launches and FileManager1's encoded
Chinese URI handling. It calls FileManager1 on its own profile's bus name.

The benchmark reports process launch to Niri's mapped-window event, **not first
paint, directory-load completion, or a reboot with cold disk caches**. The first
run is recorded as a warmup and excluded from the median by default. Every run
has a fresh profile/cache, so the warmup also primes font and driver caches.
JSON reports record the binary SHA-256, all samples, warmups and run directory.
Logs remain there for inspection. Use `--warmups 0 --fresh-process --samples 1`
to record an unprimed run separately; do not mix it into resident medians.

Run comparisons sequentially with the same build flags and no concurrent
build. Repeat in alternating order because desktop load and compositor frame
timing introduce noise. For stage timings, set
`RUST_LOG=warn,tern=info,gpui_wgpu=debug`.
