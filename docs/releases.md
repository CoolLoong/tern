# Builds and releases

The project has one version: **0.0.1**. `Cargo.toml` is the source of truth
for the executable version, archive name and GitHub release tag (`v0.0.1`).
Rebuilding does not change this version or add a build/revision suffix. A new
product release explicitly changes the SemVer version.

`PKGBUILD` uses the same `pkgver`. Its mandatory Arch-only `pkgrel` is fixed at
`1` and is never incremented for local rebuilds. This distribution metadata is
not part of Tern's product version; public release archives use the version from
`Cargo.toml` directly.

## Build artifacts

[Build and release](../.github/workflows/build.yml) builds Linux x86_64 on pushes
to `main`/`master`, pull requests and manual runs. It uploads a versioned archive
containing the binary, desktop resources and license notices, plus a SHA-256 file.

To produce the same archive locally:

```sh
cargo build --release --locked --bin tern
./tools/package-release.sh
cd target/dist
sha256sum --check tern-0.0.1-linux-x86_64.tar.gz.sha256
```

The binary is dynamically linked. Workflow builds use Ubuntu 24.04; local builds
use the libraries/toolchain of the machine that built them.

## Release

Run the workflow manually with `create_release` enabled to create a **draft**
release with the current version and built artifacts. The option defaults to
false. Review the draft and explicitly publish it in GitHub when ready.

Pushes and pull requests build downloadable artifacts. Manual release runs can
create drafts. Development checks are documented in [CONTRIBUTING.md](../CONTRIBUTING.md)
and the local test entry point is `tools/check-headless.sh`.
