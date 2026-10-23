#[derive(Debug, Clone)]
pub(crate) struct MenuBarState {
    pub expanded: bool,
    pub selected_ix: Option<usize>,
}

impl Default for MenuBarState {
    fn default() -> Self {
        Self {
            expanded: true,
            selected_ix: None,
        }
    }
}

impl MenuBarState {
    pub fn on_fold_click(&mut self) {
        self.expanded = true;
        self.selected_ix = Some(0);
    }

    pub fn on_menu_click(&mut self, ix: usize) {
        if !self.expanded {
            // Only fold menu should be visible while collapsed.
            if ix == 0 {
                self.expanded = true;
                self.selected_ix = Some(0);
            }
            return;
        }

        if self.selected_ix == Some(ix) {
            self.selected_ix = None;
        } else {
            self.selected_ix = Some(ix);
        }
    }

    /// Hovering another top-level menu switches to it, but only once a menu is
    /// already open. Hover on a closed bar must not open anything — that is what
    /// made the bar fire as the pointer travelled past it.
    ///
    /// Index 0 is the fold/app menu: it is not a drop-down of its own, so
    /// hovering it switches back to it rather than opening a popup.
    pub fn on_menu_hover(&mut self, ix: usize) {
        if self.selected_ix.is_none() {
            return;
        }
        self.selected_ix = Some(ix);
    }

    pub fn on_cancel(&mut self) {
        self.selected_ix = None;
    }

    pub fn on_dismiss(&mut self) {
        self.on_cancel();
    }
}

#[cfg(test)]
mod tests {
    use super::MenuBarState;

    #[test]
    fn fold_click_expands_and_selects_fold_menu() {
        let mut s = MenuBarState::default();
        s.on_fold_click();
        assert!(s.expanded);
        assert_eq!(s.selected_ix, Some(0));
    }

    #[test]
    fn repeated_app_menu_click_keeps_the_bar_visible() {
        let mut s = MenuBarState::default();
        s.on_fold_click();
        s.on_fold_click();
        assert!(s.expanded);
        assert_eq!(s.selected_ix, Some(0));
    }

    #[test]
    fn switching_menus_requires_a_click() {
        let mut s = MenuBarState::default();
        s.on_fold_click(); // expand, selected = 0
        s.on_menu_click(2);
        assert_eq!(s.selected_ix, Some(2));

        // Clicking the open menu closes it, rather than reopening.
        s.on_menu_click(2);
        assert_eq!(s.selected_ix, None);
    }

    #[test]
    fn hover_switches_only_while_a_menu_is_open() {
        let mut s = MenuBarState::default();
        // Nothing open: hovering must not open anything.
        s.on_menu_hover(2);
        assert_eq!(s.selected_ix, None);

        s.on_menu_click(2);
        assert_eq!(s.selected_ix, Some(2));

        // Something open: hovering another menu switches to it.
        s.on_menu_hover(3);
        assert_eq!(s.selected_ix, Some(3));
    }

    #[test]
    fn dismiss_closes_the_popup_and_keeps_the_bar_visible() {
        let mut s = MenuBarState::default();
        s.on_fold_click();
        s.on_dismiss();
        assert!(s.expanded);
        assert_eq!(s.selected_ix, None);
    }
}
