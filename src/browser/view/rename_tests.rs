use super::interaction_tests::{Fixture, browser};
use super::*;
use gpui_kit::{Modifiers, TestAppContext, VisualTestContext};

fn select(view: &Entity<BrowserView>, path: &Path, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        view.read(cx).table.clone().update(cx, |table, cx| {
            let index = table
                .delegate()
                .visible
                .position(path.to_str().unwrap())
                .unwrap();
            table.delegate_mut().select_single(index);
            cx.notify();
        });
        window.focus(&view.read(cx).focus_handle(cx), cx);
    });
    cx.run_until_parked();
}

fn active_input(view: &Entity<BrowserView>, cx: &mut VisualTestContext) -> Entity<InputState> {
    cx.update(|window, cx| {
        let input = view.read(cx).op.as_ref().unwrap().input.clone();
        assert!(input.read(cx).focus_handle(cx).is_focused(window));
        input
    })
}

fn click(selector: &'static str, cx: &mut VisualTestContext) {
    let bounds = cx.debug_bounds(selector).unwrap();
    cx.simulate_click(bounds.center(), Modifiers::default());
    cx.run_until_parked();
}

fn check_rename(chooser: bool, cx: &mut TestAppContext) {
    let fixture = Fixture::new("rename-extension");
    let original = fixture.write("计划 🐦.txt");
    let (view, cx) = browser(&fixture, chooser, cx);
    select(&view, &original, cx);
    cx.simulate_keystrokes("f2");
    let input = active_input(&view, cx);
    cx.update(|_, cx| {
        assert_eq!(input.read(cx).value().as_ref(), "计划 🐦.txt");
        assert_eq!(input.read(cx).selected_value().as_ref(), "计划 🐦");
    });
    cx.simulate_input("总结");
    cx.simulate_keystrokes("f2");
    cx.update(|_, cx| assert_eq!(input.read(cx).value().as_ref(), "总结.txt"));
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    let renamed = original.with_file_name("总结.txt");
    assert!(!original.exists());
    assert_eq!(std::fs::read(&renamed).unwrap(), b"fixture");
    assert!(cx.debug_bounds("browser-op").is_none());
    select(&view, &renamed, cx);
    cx.simulate_keystrokes("f2 end shift-left shift-left shift-left");
    let input = active_input(&view, cx);
    cx.update(|_, cx| {
        assert_eq!(input.read(cx).value().as_ref(), "总结.txt");
        assert_eq!(input.read(cx).selected_value().as_ref(), "txt");
    });
    cx.simulate_input("md");
    click("browser-op-confirm", cx);
    let changed_extension = original.with_file_name("总结.md");
    assert!(!renamed.exists());
    assert_eq!(std::fs::read(&changed_extension).unwrap(), b"fixture");
    cx.update(|window, cx| {
        assert!(window.notifications(cx).is_empty());
        assert!(
            view.read(cx)
                .table
                .read(cx)
                .delegate()
                .visible
                .contains(changed_extension.to_str().unwrap())
        );
    });
}

#[gpui_kit::test]
fn main_rename_prefills_name_and_edits_extension(cx: &mut TestAppContext) {
    check_rename(false, cx);
}

#[gpui_kit::test]
fn chooser_rename_prefills_name_and_edits_extension(cx: &mut TestAppContext) {
    check_rename(true, cx);
}

#[gpui_kit::test]
fn rename_preserves_hidden_names_spaces_and_directory_dots(cx: &mut TestAppContext) {
    let fixture = Fixture::new("rename-names");
    let cases = [
        (".env", ".env"),
        (".配置.json", ".配置"),
        ("archive.tar.gz", "archive.tar"),
        ("README", "README"),
        ("  notes.txt  ", "  notes"),
    ];
    let files: Vec<_> = cases.iter().map(|(name, _)| fixture.write(name)).collect();
    let child = fixture.write("folder.with.dots/child.txt");
    let (view, cx) = browser(&fixture, false, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.toggle_hidden(window, cx);
        });
    });
    cx.run_until_parked();
    for ((name, stem), path) in cases.iter().zip(&files) {
        select(&view, path, cx);
        cx.simulate_keystrokes("f2");
        let input = active_input(&view, cx);
        cx.update(|_, cx| {
            assert_eq!(input.read(cx).value().as_ref(), *name);
            assert_eq!(input.read(cx).selected_value().as_ref(), *stem);
        });
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(
            std::fs::read(path).unwrap_or_else(|error| panic!("{name:?}: {error}")),
            b"fixture"
        );
    }
    let directory = child.parent().unwrap();
    select(&view, directory, cx);
    cx.simulate_keystrokes("f2");
    let input = active_input(&view, cx);
    cx.update(|_, cx| assert_eq!(input.read(cx).selected_value().as_ref(), "folder.with.dots"));
    cx.simulate_input("renamed.folder");
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(!directory.exists());
    assert_eq!(
        std::fs::read(directory.with_file_name("renamed.folder").join("child.txt")).unwrap(),
        b"fixture"
    );
    cx.update(|window, cx| assert!(window.notifications(cx).is_empty()));
}

#[gpui_kit::test]
fn rename_can_add_remove_and_replace_extensions_with_select_all(cx: &mut TestAppContext) {
    let fixture = Fixture::new("rename-full-name");
    let mut path = fixture.write("notes");
    let (view, cx) = browser(&fixture, false, cx);
    for name in ["notes.txt", "notes.docx", "notes", "  notes  "] {
        select(&view, &path, cx);
        cx.simulate_keystrokes("f2 ctrl-a");
        let input = active_input(&view, cx);
        cx.update(|_, cx| {
            assert_eq!(input.read(cx).selected_value(), input.read(cx).value());
        });
        cx.simulate_input(name);
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert!(!path.exists());
        path.set_file_name(name);
        assert_eq!(std::fs::read(&path).unwrap(), b"fixture");
    }
    cx.update(|window, cx| assert!(window.notifications(cx).is_empty()));
}

#[gpui_kit::test]
fn rename_keeps_the_original_context_target(cx: &mut TestAppContext) {
    let fixture = Fixture::new("rename-target");
    let original = fixture.write("first.txt");
    let other = fixture.write("second.txt");
    let (view, cx) = browser(&fixture, false, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.table.update(cx, |table, _| {
                table.delegate_mut().secondary_press(Some(0));
            });
            view.on_rename(&Rename, window, cx);
        });
    });
    cx.run_until_parked();
    let input = active_input(&view, cx);
    cx.update(|_, cx| assert_eq!(input.read(cx).value().as_ref(), "first.txt"));
    cx.simulate_keystrokes("ctrl-a");
    cx.simulate_input("renamed.md");
    cx.update(|_, cx| {
        view.read(cx).table.clone().update(cx, |table, _| {
            table.delegate_mut().secondary_press(Some(1));
        });
    });
    cx.simulate_keystrokes("enter");
    cx.run_until_parked();
    assert!(!original.exists());
    assert_eq!(std::fs::read(other).unwrap(), b"fixture");
    assert_eq!(
        std::fs::read(original.with_file_name("renamed.md")).unwrap(),
        b"fixture"
    );
}

#[gpui_kit::test]
fn rename_cancel_invalid_names_and_conflicts_preserve_files(cx: &mut TestAppContext) {
    let fixture = Fixture::new("rename-validation");
    let original = fixture.write("source.txt");
    let occupied = fixture.write("occupied.md");
    std::fs::write(&occupied, b"keep destination").unwrap();
    let (view, cx) = browser(&fixture, true, cx);
    for cancel_with_button in [false, true] {
        select(&view, &original, cx);
        cx.simulate_keystrokes("f2 ctrl-a");
        cx.simulate_input("cancelled.md");
        if cancel_with_button {
            click("browser-op-cancel", cx);
        } else {
            cx.simulate_keystrokes("escape");
        }
        cx.update(|window, cx| {
            assert!(view.read(cx).op.is_none());
            assert!(view.read(cx).focus_handle(cx).is_focused(window));
        });
        assert!(original.is_file());
        assert!(!original.with_file_name("cancelled.md").exists());
    }
    for name in ["", "..", "../escape.txt", "occupied.md"] {
        select(&view, &original, cx);
        cx.simulate_keystrokes("f2 ctrl-a backspace");
        if !name.is_empty() {
            cx.simulate_input(name);
        }
        cx.simulate_keystrokes("enter");
        cx.run_until_parked();
        assert_eq!(std::fs::read(&original).unwrap(), b"fixture");
        assert_eq!(std::fs::read(&occupied).unwrap(), b"keep destination");
        cx.update(|window, cx| assert!(!window.notifications(cx).is_empty()));
    }
}

#[gpui_kit::test]
fn rename_requires_a_single_file_or_folder(cx: &mut TestAppContext) {
    let fixture = Fixture::new("rename-selection");
    fixture.write("first.txt");
    fixture.write("second.txt");
    let (view, cx) = browser(&fixture, false, cx);
    cx.update(|window, cx| window.focus(&view.read(cx).focus_handle(cx), cx));
    cx.simulate_keystrokes("f2");
    cx.update(|_, cx| assert!(view.read(cx).op.is_none()));
    cx.simulate_keystrokes("ctrl-a f2");
    cx.update(|_, cx| assert!(view.read(cx).op.is_none()));
    cx.update(|_, cx| {
        view.read(cx).table.clone().update(cx, |table, _| {
            table.delegate_mut().secondary_press(None);
        });
    });
    cx.simulate_keystrokes("f2");
    cx.update(|_, cx| assert!(view.read(cx).op.is_none()));
}
