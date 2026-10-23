use std::io::Write;
use std::path::PathBuf;

use anyhow::Context as _;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub(crate) struct Bookmarks {
    items: Vec<String>,
}

impl gpui_kit::Global for Bookmarks {}

impl Bookmarks {
    pub(crate) fn items(&self) -> &[String] {
        &self.items
    }

    pub(crate) fn add(&mut self, path: String) {
        if !self.items.contains(&path) {
            self.items.push(path);
        }
    }
}

pub(crate) fn init(cx: &mut gpui_kit::App) {
    cx.set_global(load_from_disk());
}

fn bookmarks_path() -> PathBuf {
    crate::settings::config_dir_path().join("bookmarks.json")
}

pub(crate) fn load_from_disk() -> Bookmarks {
    let path = bookmarks_path();
    match std::fs::read_to_string(&path)
        .with_context(|| format!("read bookmarks {}", path.display()))
        .and_then(|json| serde_json::from_str(&json).context("deserialize bookmarks"))
    {
        Ok(bookmarks) => bookmarks,
        Err(err) => {
            log::debug!("ignoring unreadable bookmarks: {err:#}");
            Bookmarks::default()
        }
    }
}

pub(crate) fn save_to_disk(bookmarks: &Bookmarks) -> anyhow::Result<()> {
    let path = bookmarks_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create bookmarks dir {parent:?}"))?;
    }
    let json = serde_json::to_string_pretty(bookmarks).context("serialize bookmarks")?;
    let temporary = path.with_extension("json.tmp");
    let result = (|| -> anyhow::Result<()> {
        let mut file = std::fs::File::create(&temporary)
            .with_context(|| format!("create bookmarks temp {temporary:?}"))?;
        file.write_all(json.as_bytes())
            .with_context(|| format!("write bookmarks temp {temporary:?}"))?;
        file.sync_all()
            .with_context(|| format!("sync bookmarks temp {temporary:?}"))?;
        std::fs::rename(&temporary, &path)
            .with_context(|| format!("replace bookmarks {path:?}"))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}
