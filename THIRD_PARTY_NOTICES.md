# Third-party notices

Tern incorporates and depends on the following projects. This document records
provenance; it does not replace the licenses supplied by those projects.

| Component | Origin | License / notes |
| --- | --- | --- |
| `crates/gpui_common`, `crates/gpui_dock`, `crates/gpui_transfer`, `crates/menubar` | [termua](https://github.com/iamazy/termua) | AGPL-3.0; existing attribution retained |
| Dock component ancestry | [gpui-component](https://github.com/longbridge/gpui-component), through termua | Original attribution retained in `crates/gpui_dock/README.md` |
| `assets/icons` except `tern.svg` | Assets carried over from termua | Includes third-party application marks; ownership and any upstream asset-specific terms remain with their authors |
| `vendor/gpui-pre-wgpu` | gpui-pre-wgpu 0.3.5 / Zed snapshot | [Apache-2.0](vendor/gpui-pre-wgpu/LICENSE-APACHE); changes and exact base recorded in [TERN_PATCHES.md](vendor/gpui-pre-wgpu/TERN_PATCHES.md) |
| GPUI Kit and its components | [longbridge/gpui-kit](https://github.com/longbridge/gpui-kit) | Dependency pinned in `Cargo.toml` and `Cargo.lock`; consult upstream licenses |

The root [LICENSE](LICENSE) contains the AGPL version 3 text matching Tern's
existing license declaration. Other Cargo dependencies retain their own licenses
and notices. Application names and logos identify compatible external tools;
their inclusion does not imply endorsement.

`assets/icons/tern.svg` is original project artwork covered by the root license.
The README banner is original presentation artwork based on the Tern interface
and project mark. It uses generic example files and anonymous paths.
