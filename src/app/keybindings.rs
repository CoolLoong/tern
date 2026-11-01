use super::actions::*;
use gpui_kit::{App, KeyBinding, KeyBindingContextPredicate};

pub const KEYBINDINGS: &[(&str, &str, &str, &str)] = &[
    ("new-tab", "New Tab", "ctrl-t", "NewTab"),
    ("close-tab", "Close Tab", "ctrl-w", "CloseTab"),
    ("new-window", "New Window", "ctrl-n", "NewWindow"),
    ("refresh", "Refresh", "F5", "Refresh"),
    ("back", "Back", "alt-left", "GoBack"),
    ("forward", "Forward", "alt-right", "GoForward"),
    ("up", "Up", "alt-up", "GoUp"),
    ("path", "Go to Path", "ctrl-l", "OpenPathEntry"),
    ("sidebar", "Toggle Places", "ctrl-b", "ToggleSidebar"),
    ("hidden", "Show Hidden Files", "ctrl-h", "ToggleHiddenFiles"),
    ("search", "Search", "ctrl-f", "Search"),
    ("copy", "Copy", "ctrl-c", "Copy"),
    ("cut", "Cut", "ctrl-x", "Cut"),
    ("paste", "Paste", "ctrl-v", "Paste"),
    ("rename", "Rename", "F2", "Rename"),
    ("delete", "Move to Trash", "delete", "MoveToTrash"),
    ("new-folder", "New Folder", "ctrl-shift-n", "NewFolder"),
    ("select-all", "Select All", "ctrl-a", "SelectAll"),
    ("select-up", "Select Previous", "up", "SelectUp"),
    ("select-down", "Select Next", "down", "SelectDown"),
];

fn action_for(name: &str) -> Box<dyn gpui_kit::Action> {
    match name {
        "NewTab" => Box::new(NewTab),
        "CloseTab" => Box::new(CloseTab),
        "NewWindow" => Box::new(NewWindow),
        "Refresh" => Box::new(Refresh),
        "GoBack" => Box::new(GoBack),
        "GoForward" => Box::new(GoForward),
        "GoUp" => Box::new(GoUp),
        "OpenPathEntry" => Box::new(OpenPathEntry),
        "ToggleSidebar" => Box::new(ToggleSidebar),
        "ToggleHiddenFiles" => Box::new(ToggleHiddenFiles),
        "Copy" => Box::new(Copy),
        "Cut" => Box::new(Cut),
        "Paste" => Box::new(Paste),
        "Rename" => Box::new(Rename),
        "MoveToTrash" => Box::new(MoveToTrash),
        "Search" => Box::new(Search),
        "Cancel" => Box::new(Cancel),
        "RestoreFromTrash" => Box::new(RestoreFromTrash),
        "RemoveFromRecent" => Box::new(RemoveFromRecent),
        "Delete" => Box::new(Delete),
        "NewFolder" => Box::new(NewFolder),
        "SelectAll" => Box::new(SelectAll),
        "SelectUp" => Box::new(SelectUp),
        "SelectDown" => Box::new(SelectDown),
        other => {
            // A keybinding that names an action this table does not know is a
            // configuration mistake, not a crash: skip it and keep the rest of
            // the keymap usable.
            log::error!("ignoring keybinding for unknown action {other}");
            Box::new(Noop)
        }
    }
}

pub fn configured_keybindings(settings: &crate::settings::SettingsFile) -> Vec<KeyBinding> {
    KEYBINDINGS
        .iter()
        .map(|(id, _, default, action)| {
            let context = matches!(
                *action,
                "Copy"
                    | "Cut"
                    | "Paste"
                    | "MoveToTrash"
                    | "Delete"
                    | "SelectAll"
                    | "SelectUp"
                    | "SelectDown"
            )
            .then(|| {
                KeyBindingContextPredicate::parse("!Input && !TextView && !TernPreview")
                    .expect("valid file action context")
                    .into()
            });
            KeyBinding::load(
                &settings
                    .keybindings
                    .get(*id)
                    .cloned()
                    .unwrap_or_else(|| (*default).into()),
                action_for(action),
                context,
                false,
                None,
                &gpui_kit::DummyKeyboardMapper,
            )
            .expect("valid Tern keybinding")
        })
        .collect()
}

pub fn register(settings: &crate::settings::SettingsFile, cx: &mut App) {
    let retained: Vec<_> = cx
        .key_bindings()
        .borrow()
        .bindings()
        .filter(|binding| !binding.action().name().starts_with("tern::"))
        .cloned()
        .collect();
    cx.clear_key_bindings();
    cx.bind_keys(retained.into_iter().chain(configured_keybindings(settings)));
}
