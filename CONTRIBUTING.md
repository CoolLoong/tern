# Contributing to Tern

Start with a focused issue or pull request describing the user-visible problem,
the expected behavior and how to reproduce it. Tern is in active development;
keep changes small enough to review and preserve existing on-disk settings and
desktop protocol identifiers.

## Development environment

Use Linux and rustup; `rust-toolchain.toml` pins the supported toolchain.
Install the system dependencies listed in the [README](README.md#getting-started).
Keep `Cargo.lock` committed and use `--locked` for builds and tests.

GPUI Kit is pinned to a Git revision because the renderer patch targets its
matching snapshot. An upgrade must review
[vendor/gpui-pre-wgpu/TERN_PATCHES.md](vendor/gpui-pre-wgpu/TERN_PATCHES.md).

## Before opening a pull request

```sh
cargo fmt --all -- --check
python3 tools/check-architecture.py
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_RELEASE_LTO=thin CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16
export RUSTFLAGS='-C link-arg=-fuse-ld=lld'
cargo clippy --release --locked --workspace --all-targets -- -D warnings
./tools/check-headless.sh
cargo build --release --locked --bin tern
git diff --check
```

GitHub Actions is limited to Linux builds, downloadable archives and manual
release drafts. The checks above remain local development commands. See
[the release guide](docs/releases.md) for versioning and release artifacts.

Use temporary directories for filesystem tests. Use GPUI TestPlatform/TestWindow
for interaction tests and an isolated session bus for desktop protocol tests.
Do not launch real applications from tests or depend on the user's installed
file opener. Add regression tests for changed behavior, especially cancellation,
data loss and file-chooser responses. Do not hide failures with skipped tests.

## Code organization

Follow [docs/architecture.md](docs/architecture.md). Keep filesystem work on
background workers and verify job identity before publishing results to GPUI.
Make cancellation and ownership visible in the code. Avoid broad refactors in
the same change as a protocol or persistence migration.

Formatting applies to application code and the shared `crates/` workspace.
Changes under `vendor/` should stay small, documented and traceable to upstream.
Use synthetic fixture paths for tests, such as `/home/u/example.txt`.

## Review information

Explain the before/after behavior, the commands you ran, and any remaining
limits. Include screenshots only when they help review a visual change, and
remove personal filenames and locations first. Performance claims must identify
the measured boundary; a headless test or a mapped-window event is not a
measurement of first paint.

Keep copyright notices and third-party license files. Contributions follow the
repository's [existing license](LICENSE).
