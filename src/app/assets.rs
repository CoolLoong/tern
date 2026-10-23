use std::borrow::Cow;

use anyhow::anyhow;
use gpui_kit::assets::Assets as ComponentAssets;
use gpui_kit::{AssetSource, Result, SharedString};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "assets"]
#[include = "icons/**/*.svg"]
#[include = "icons/**/*.png"]
struct EmbeddedAssets;

pub struct TernAssets;

impl AssetSource for TernAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path.is_empty() {
            return Ok(None);
        }

        if let Some(f) = EmbeddedAssets::get(path) {
            return Ok(Some(f.data));
        }

        if let Some(f) = ComponentAssets::get(path) {
            return Ok(Some(f.data));
        }

        Err(anyhow!("could not find asset at path \"{path}\""))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut out: Vec<SharedString> = EmbeddedAssets::iter()
            .filter_map(|p| p.starts_with(path).then(|| p.into()))
            .collect();

        out.extend(ComponentAssets::iter().filter_map(|p| p.starts_with(path).then(|| p.into())));

        Ok(out)
    }
}
