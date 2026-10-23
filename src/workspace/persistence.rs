use std::{
    io::Write,
    path::{Path, PathBuf},
};

use anyhow::Context as _;
use gpui_dock::DockAreaState;

pub(crate) const STATE_VERSION: usize = 2;

pub(crate) fn state_path() -> PathBuf {
    crate::settings::config_dir_path().join("workspace.json")
}

pub(crate) fn save_to_path(path: &Path, mut state: DockAreaState) -> anyhow::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("create workspace state dir {parent:?}"))?;
    }
    state.left_dock = None;
    let json = serde_json::to_string_pretty(&state).context("serialize dock workspace state")?;
    let temporary = path.with_extension("json.tmp");
    let result = (|| -> anyhow::Result<()> {
        let mut file = std::fs::File::create(&temporary)
            .with_context(|| format!("create workspace temp state {temporary:?}"))?;
        file.write_all(json.as_bytes())
            .with_context(|| format!("write workspace temp state {temporary:?}"))?;
        file.sync_all()
            .with_context(|| format!("sync workspace temp state {temporary:?}"))?;
        std::fs::rename(&temporary, path)
            .with_context(|| format!("replace workspace state {path:?}"))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}

pub(crate) fn load_from_path(path: &Path) -> anyhow::Result<DockAreaState> {
    let json = std::fs::read_to_string(path)
        .with_context(|| format!("read dock workspace state {path:?}"))?;
    serde_json::from_str(&json).context("deserialize dock workspace state")
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_dock::PanelState;

    fn unique_path(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("tern-workspace-{label}-{nanos}.json"))
    }

    #[test]
    fn dock_state_round_trips_through_disk() {
        let path = unique_path("round-trip");
        let state = DockAreaState {
            version: Some(STATE_VERSION),
            center: PanelState::default(),
            left_dock: None,
            right_dock: None,
            bottom_dock: None,
        };

        super::save_to_path(&path, state.clone()).expect("save dock state");
        let loaded = super::load_from_path(&path).expect("load dock state");
        assert_eq!(loaded, state);
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn corrupt_dock_state_is_reported() {
        let path = unique_path("corrupt");
        std::fs::write(&path, "not json").expect("write corrupt state");
        assert!(super::load_from_path(&path).is_err());
        std::fs::remove_file(path).ok();
    }
}
