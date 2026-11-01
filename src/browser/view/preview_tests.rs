use super::interaction_tests::{Fixture, browser};
use super::*;
use gpui_kit::{Modifiers, TestAppContext, VisualTestContext, point};

fn select_file(view: &Entity<BrowserView>, path: &Path, cx: &mut VisualTestContext) {
    let position = cx.update(|_, cx| {
        let table = view.read(cx).table.read(cx);
        let index = table
            .delegate()
            .visible
            .position(path.to_str().unwrap())
            .unwrap();
        let bounds = table.vertical_scroll_handle.0.borrow().base_handle.bounds();
        point(
            bounds.left() + px(80.0),
            bounds.top() + px(index as f32 * 32.0 + 16.0),
        )
    });
    cx.simulate_click(position, Modifiers::default());
    cx.run_until_parked();
    cx.update(|_, cx| {
        assert!(matches!(
            view.read(cx).preview.content,
            PreviewContent::Text { .. }
        ));
        assert_eq!(
            view.read(cx).preview.target.as_ref().unwrap().id,
            path.to_str().unwrap()
        );
    });
}

#[gpui_kit::test]
fn preview_selection_copies_text_without_copying_the_file(cx: &mut TestAppContext) {
    let fixture = Fixture::new("preview-selection");
    let path = fixture.write("sample.txt");
    std::fs::write(&path, "  alpha beta\nnext line\n").unwrap();
    let (view, cx) = browser(&fixture, false, cx);
    select_file(&view, &path, cx);
    let bounds = cx.debug_bounds("browser-preview-body").unwrap();
    let position = bounds.origin + point(px(44.0), px(28.0));
    cx.simulate_event(gpui_kit::MouseDownEvent {
        position,
        button: MouseButton::Left,
        modifiers: Default::default(),
        click_count: 2,
        first_mouse: false,
    });
    cx.simulate_event(gpui_kit::MouseUpEvent {
        position,
        button: MouseButton::Left,
        modifiers: Default::default(),
        click_count: 2,
    });
    cx.run_until_parked();
    let selected = cx.update(gpui_kit::base::TextSelection::selected_text);
    assert_eq!(selected, "alpha");
    cx.update(|_, cx| {
        cx.write_to_clipboard(gpui_kit::ClipboardItem::new_string("unchanged".into()))
    });
    cx.simulate_keystrokes("ctrl-c");
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some(selected.as_str())
        );
        assert!(
            cx.global::<crate::browser::clipboard::ClipboardState>()
                .is_empty()
        );
    });
    cx.simulate_keystrokes("ctrl-a ctrl-c");
    cx.update(|_, cx| {
        let text = cx.read_from_clipboard().unwrap().text().unwrap();
        assert!(
            text.starts_with("  alpha beta\nnext line"),
            "copied: {text:?}"
        );
        assert!(!text.contains("```"));
        assert!(
            cx.global::<crate::browser::clipboard::ClipboardState>()
                .is_empty()
        );
    });
    assert_eq!(
        std::fs::read_to_string(path).unwrap(),
        "  alpha beta\nnext line\n"
    );
}

fn drag_fragment(
    cx: &mut VisualTestContext,
    from: gpui_kit::Point<Pixels>,
    to: gpui_kit::Point<Pixels>,
) -> String {
    cx.simulate_mouse_down(from, MouseButton::Left, Modifiers::default());
    cx.update(|window, cx| {
        let _ = window.draw(cx);
    });
    for fraction in [0.25, 0.5, 0.75, 1.0] {
        let position = point(
            from.x + (to.x - from.x) * fraction,
            from.y + (to.y - from.y) * fraction,
        );
        cx.simulate_mouse_move(position, Some(MouseButton::Left), Modifiers::default());
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
    }
    let held = cx.update(gpui_kit::base::TextSelection::selected_text);
    assert!(!held.is_empty(), "dragging must start a text selection");
    cx.simulate_mouse_up(to, MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    for _ in 0..3 {
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
    }
    let released = cx.update(gpui_kit::base::TextSelection::selected_text);
    assert_eq!(
        released, held,
        "selection must survive releasing the mouse and repainting"
    );
    released
}

fn check_preview_drag(chooser: bool, cx: &mut TestAppContext) {
    let fixture = Fixture::new("preview-drag");
    let path = fixture.write("fragment.txt");
    let original = "abcdefghijklmnopqrstuvwxyz\n第二行包含中文和空格\nthird line content\nfourth line content\nfifth line content\n";
    std::fs::write(&path, original).unwrap();
    let (view, cx) = browser(&fixture, chooser, cx);
    select_file(&view, &path, cx);
    let bounds = cx.debug_bounds("browser-preview-body").unwrap();
    let start = bounds.origin + point(px(44.0), px(28.0));
    let end = bounds.origin + point(px(96.0), px(28.0));
    let selected = drag_fragment(cx, start, end);
    let fragment = selected.trim_end_matches('\n');
    assert!(
        fragment.chars().count() >= 2 && fragment.len() < 26,
        "selected: {selected:?}"
    );
    assert!("abcdefghijklmnopqrstuvwxyz".contains(fragment));
    cx.simulate_keystrokes("ctrl-c");
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some(selected.as_str())
        );
        assert!(view.read(cx).marquee.is_none());
        assert_eq!(view.read(cx).status(cx).selected, 1);
        assert!(
            cx.global::<crate::browser::clipboard::ClipboardState>()
                .is_empty()
        );
    });
    assert_eq!(drag_fragment(cx, end, start), selected);
    let cross_line = drag_fragment(cx, start, bounds.origin + point(px(96.0), px(68.0)));
    assert!(
        cross_line.trim_end_matches('\n').contains('\n'),
        "selected: {cross_line:?}"
    );
    assert!(original.contains(cross_line.trim_end_matches('\n')));
    assert!(cross_line.len() < original.len());
    view.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
    cx.simulate_keystrokes("ctrl-c");
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some(cross_line.as_str())
        );
        assert!(view.read(cx).marquee.is_none());
        assert_eq!(view.read(cx).status(cx).selected, 1);
    });
    assert_eq!(std::fs::read_to_string(path).unwrap(), original);
}

#[gpui_kit::test]
fn preview_mouse_drag_selects_fragments_in_main_window(cx: &mut TestAppContext) {
    check_preview_drag(false, cx);
}

#[gpui_kit::test]
fn preview_mouse_drag_selects_fragments_in_chooser(cx: &mut TestAppContext) {
    check_preview_drag(true, cx);
}

#[gpui_kit::test]
fn preview_copy_button_preserves_text_and_tracks_the_displayed_file(cx: &mut TestAppContext) {
    let fixture = Fixture::new("preview-copy-all");
    let first = fixture.write("first.md");
    let second = fixture.write("second.txt");
    let original = "  中文文本\r\n```rust\r\nlet value = 1;\r\n```\r\n最后一行  ";
    std::fs::write(&first, original).unwrap();
    std::fs::write(&second, "second file\n").unwrap();
    let (view, cx) = browser(&fixture, false, cx);
    select_file(&view, &first, cx);
    std::fs::write(&first, "changed on disk").unwrap();
    let bounds = cx.debug_bounds("browser-preview-copy").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::default());
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some(original)
        );
        assert!(
            cx.global::<crate::browser::clipboard::ClipboardState>()
                .is_empty()
        );
    });
    select_file(&view, &second, cx);
    let bounds = cx.debug_bounds("browser-preview-copy").unwrap();
    cx.simulate_click(bounds.center(), Modifiers::default());
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("second file\n")
        )
    });
    for content in [
        PreviewContent::Loading,
        PreviewContent::Error {
            message: "unavailable".into(),
        },
    ] {
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.preview.content = content;
                view.copy_preview_text(cx);
                cx.notify();
            });
            assert_eq!(
                cx.read_from_clipboard().unwrap().text().as_deref(),
                Some("second file\n")
            );
        });
        cx.run_until_parked();
        assert!(cx.debug_bounds("browser-preview-copy").is_none());
    }
    assert_eq!(std::fs::read_to_string(first).unwrap(), "changed on disk");
    assert_eq!(std::fs::read_to_string(second).unwrap(), "second file\n");
}

#[gpui_kit::test]
fn preview_select_all_copies_literal_markdown_and_stays_read_only(cx: &mut TestAppContext) {
    let fixture = Fixture::new("preview-markdown");
    let path = fixture.write("notes.md");
    let original = "  alpha 中文\n```rust\nlet value = 1;\n```\n# **literal heading**\n````\nend\n";
    std::fs::write(&path, original).unwrap();
    let (view, cx) = browser(&fixture, false, cx);
    select_file(&view, &path, cx);
    let bounds = cx.debug_bounds("browser-preview-body").unwrap();
    cx.simulate_click(
        bounds.origin + point(px(44.0), px(28.0)),
        Modifiers::default(),
    );
    cx.simulate_keystrokes("ctrl-a ctrl-c");
    cx.update(|_, cx| {
        let copied = cx.read_from_clipboard().unwrap().text().unwrap();
        assert_eq!(
            copied.trim_end_matches('\n'),
            original.trim_end_matches('\n')
        );
    });
    cx.simulate_keystrokes("ctrl-x ctrl-v delete");
    cx.run_until_parked();
    assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
    cx.update(|_, cx| {
        assert!(
            cx.global::<crate::browser::clipboard::ClipboardState>()
                .is_empty()
        );
        assert!(cx.global::<TransferCenterState>().is_empty());
        assert!(matches!(
            view.read(cx).preview.content,
            PreviewContent::Text { .. }
        ));
    });
    cx.update(|window, cx| window.focus(&view.read(cx).focus_handle(cx), cx));
    cx.run_until_parked();
    cx.simulate_keystrokes("ctrl-c");
    cx.update(|_, cx| {
        let (paths, cut) = cx
            .global::<crate::browser::clipboard::ClipboardState>()
            .clone()
            .take();
        assert_eq!(paths, [path]);
        assert!(!cut);
    });
}
