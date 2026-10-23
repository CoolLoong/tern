use gpui_kit::actions;

actions!(
    tern,
    [
        Quit,
        NewWindow,
        NewTab,
        CloseTab,
        ToggleSidebar,
        ToggleHiddenFiles,
        Refresh,
        GoUp,
        GoBack,
        GoForward,
        OpenPathEntry,
        NewFolder,
        NewTextFile,
        Rename,
        OpenInTerminal,
        SelectAll,
        SelectUp,
        SelectDown,
        Properties,
        About,
        ToggleTheme,
        OpenFile,
        OpenWith,
        OpenSettings,
        Copy,
        Cut,
        Paste,
        AddToBookmark,
        MoveToTrash,
        RestoreFromTrash,
        RemoveFromRecent,
        Search,
        Cancel,
        Delete,
        Noop,
    ]
);

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::Action;

    #[test]
    fn action_names_keep_the_persisted_tern_namespace() {
        assert_eq!(NewTab.name(), "tern::NewTab");
        assert_eq!(OpenSettings.name(), "tern::OpenSettings");
        assert_eq!(MoveToTrash.name(), "tern::MoveToTrash");
        assert_eq!(Quit.name(), "tern::Quit");
    }
}
