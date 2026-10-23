use super::*;

#[gpui_kit::test]
fn named_operations_refresh_after_completion_and_report_errors(cx: &mut gpui_kit::TestAppContext) {
    let root = std::env::temp_dir().join(format!(
        "tern-named-operation-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&root).unwrap();
    cx.update(|cx| crate::app::runtime::init_app(cx, &crate::settings::SettingsFile::default()));
    let mut browser = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view =
            cx.new(|cx| BrowserView::new_at(Some(root.to_string_lossy().into_owned()), window, cx));
        browser = Some(view.clone());
        gpui_kit::component::Root::new(view, window, cx)
    });
    let browser = browser.unwrap();
    cx.run_until_parked();
    cx.update(|window, cx| {
        browser.update(cx, |view, cx| {
            view.on_new_text_file(&NewTextFile, window, cx);
            view.op
                .as_ref()
                .unwrap()
                .input
                .update(cx, |input, cx| input.set_value("created.txt", window, cx));
            view.confirm_op(window, cx);
        })
    });
    cx.run_until_parked();
    assert!(root.join("created.txt").is_file());
    cx.update(|window, cx| {
        assert!(
            browser
                .read(cx)
                .table
                .read(cx)
                .delegate()
                .visible
                .iter()
                .any(|row| row.name == "created.txt")
        );
        assert!(window.notifications(cx).is_empty());
    });
    std::fs::write(root.join("created.txt"), "keep original").unwrap();
    cx.update(|window, cx| {
        browser.update(cx, |view, cx| {
            view.on_new_text_file(&NewTextFile, window, cx);
            view.op
                .as_ref()
                .unwrap()
                .input
                .update(cx, |input, cx| input.set_value("created.txt", window, cx));
            view.confirm_op(window, cx);
        })
    });
    cx.run_until_parked();
    assert_eq!(
        std::fs::read(root.join("created.txt")).unwrap(),
        b"keep original"
    );
    cx.update(|window, cx| assert_eq!(window.notifications(cx).len(), 1));
    std::fs::remove_dir_all(root).unwrap();
}

#[gpui_kit::test]
fn double_click_path_enters_editor_and_escape_leaves_it(cx: &mut gpui_kit::TestAppContext) {
    cx.update(|cx| crate::app::runtime::init_app(cx, &crate::settings::SettingsFile::default()));
    let dir = std::env::temp_dir().to_string_lossy().into_owned();
    let (view, cx) =
        cx.add_window_view(|window, cx| BrowserView::new_at(Some(dir.clone()), window, cx));
    cx.run_until_parked();
    let bounds = cx
        .debug_bounds("browser-path-area")
        .expect("path area is laid out");
    let position = gpui_kit::point(bounds.right() - px(8.0), bounds.center().y);
    cx.simulate_click(position, gpui_kit::Modifiers::default());
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
    cx.update(|_, cx| {
        assert!(view.read(cx).path_editing);
        assert_eq!(view.read(cx).path_input.read(cx).value().as_ref(), dir);
    });
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    cx.update(|_, cx| assert!(!view.read(cx).path_editing));
}
