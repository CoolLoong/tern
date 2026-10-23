use std::{io::Write, path::PathBuf};

use anyhow::Context as _;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct SidebarState {
    pub width: f32,
    pub visible: bool,
}

impl Default for SidebarState {
    fn default() -> Self {
        Self {
            width: 240.0,
            visible: true,
        }
    }
}

pub(crate) fn state_path() -> PathBuf {
    crate::settings::config_dir_path().join("state.json")
}

pub(crate) fn load_from_disk() -> SidebarState {
    let path = state_path();
    match std::fs::read_to_string(&path)
        .with_context(|| format!("read state file {}", path.display()))
        .and_then(|json| serde_json::from_str(&json).context("deserialize state file"))
    {
        Ok(state) => state,
        Err(err) => {
            log::debug!("ignoring unreadable state file: {err:#}");
            SidebarState::default()
        }
    }
}

pub(crate) fn save_to_disk(state: &SidebarState) -> anyhow::Result<()> {
    let path = state_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("create state dir {parent:?}"))?;
    }
    let json = serde_json::to_string_pretty(state).context("serialize state")?;
    let temporary = path.with_extension("json.tmp");
    let result = (|| -> anyhow::Result<()> {
        let mut file = std::fs::File::create(&temporary)
            .with_context(|| format!("create state temp {temporary:?}"))?;
        file.write_all(json.as_bytes())
            .with_context(|| format!("write state temp {temporary:?}"))?;
        file.sync_all()
            .with_context(|| format!("sync state temp {temporary:?}"))?;
        std::fs::rename(&temporary, &path).with_context(|| format!("replace state {path:?}"))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}
