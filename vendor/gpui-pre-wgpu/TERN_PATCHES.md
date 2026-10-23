# Tern's gpui-pre-wgpu patch

Base: crates.io `gpui-pre-wgpu 0.3.5`, upstream Zed revision
`d89e9c2124b2786a390c7a451c7488601b4da2e1`.
Registry checksum: `640b666a16ddf9504e2eb3e7a997acb2b2adfaa7859925bdd36fc3acfd8f30a7`.
The Apache-2.0 license is preserved in this directory.

Local changes:

- `font_cache.rs`, registered in `gpui_wgpu.rs` and called by
  `cosmic_text_system.rs`: Linux font metadata persistence. v3 invalidates v2.
  Includes all font faces, family languages and generic mappings; fonts and
  fallback behavior are still provided by cosmic-text/fontdb. Missing, corrupt,
  unsupported or changed inputs fall back to the full scan.
- Cache dependency discovery precedes the font scan and is revalidated before
  publishing. It tracks custom/empty/missing directories, symlink targets,
  included configs, and font files. Size, inode, mtime and ctime participate.
  Configuration path resolution uses fontconfig-parser 0.5.8, the version used
  by fontdb 0.23.0. Recheck that pairing when upgrading. Matching/alias rules are
  left to fontdb; the watcher deliberately tracks a superset of its sources.
- `wgpu_context.rs` / `wgpu_renderer.rs`: keep pipelines **and their layouts**
  together in a device-local cache. `PipelineKey` enumerates all inputs to
  pipeline creation; changes to those inputs require revisiting the key.
  New device/context creation (including device-loss recovery) gets a new cache.
- Debug logs time font scanning, GPU initialization and pipeline compilation.
  These logs are disabled by Tern's normal log filter.

Validation:

```sh
export CARGO_BUILD_JOBS=2 CARGO_PROFILE_RELEASE_LTO=thin CARGO_PROFILE_RELEASE_CODEGEN_UNITS=16
export RUSTFLAGS='-C link-arg=-fuse-ld=lld'
cargo test --release --locked -p gpui-pre-wgpu --test font_cache
cargo test --release --locked -p gpui-pre-wgpu --test font_cache -- --include-ignored
```

The second command also checks the host's installed fonts. This integration
test target is separate because the upstream snapshot's text-system unit tests
reference Zed font fixtures outside the published crate. Do not represent the
cache tests as a complete upstream renderer test run.

On dependency upgrades, diff this crate against the exact registry release,
review the three patches above, run the cache tests and Tern desktop checks,
and measure cold-process and resident reopen latency separately. GPU-driver
and device-loss behavior still needs testing on the target hardware.
