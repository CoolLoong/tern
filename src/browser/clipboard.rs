use std::path::PathBuf;

#[derive(Clone, Debug, Default)]
pub struct ClipboardState {
    items: Vec<PathBuf>,
    cut: bool,
}

impl gpui_kit::Global for ClipboardState {}

impl ClipboardState {
    pub(crate) fn set(&mut self, items: Vec<PathBuf>, cut: bool) {
        self.items = items;
        self.cut = cut;
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub(crate) fn take(&mut self) -> (Vec<PathBuf>, bool) {
        let items = std::mem::take(&mut self.items);
        let cut = self.cut;
        self.cut = false;
        (items, cut)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_state_defaults_empty() {
        let state = ClipboardState::default();
        assert!(state.is_empty());
    }

    #[test]
    fn set_and_take_round_trips() {
        let mut state = ClipboardState::default();
        state.set(vec![PathBuf::from("/a")], true);
        let (items, cut) = state.take();
        assert_eq!(items, vec![PathBuf::from("/a")]);
        assert!(cut);
        assert!(state.is_empty());
    }
}
