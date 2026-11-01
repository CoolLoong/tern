use super::*;
use gpui_kit::{TestAppContext, VisualTestContext};
use std::path::PathBuf;

pub(super) struct Fixture(PathBuf);

struct BrowserTestRoot(Entity<BrowserView>);

impl Render for BrowserTestRoot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .child(self.0.clone())
            .children(gpui_kit::component::Root::render_dialog_layer(window, cx))
            .children(gpui_kit::component::Root::render_notification_layer(
                window, cx,
            ))
    }
}

impl Fixture {
    pub(super) fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "tern-interaction-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }

    pub(super) fn write(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"fixture").unwrap();
        path
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub(super) fn browser<'a>(
    fixture: &Fixture,
    chooser: bool,
    cx: &'a mut TestAppContext,
) -> (Entity<BrowserView>, &'a mut VisualTestContext) {
    cx.update(|cx| crate::app::runtime::init_app(cx, &crate::settings::SettingsFile::default()));
    let mut browser = None;
    let (_, cx) = cx.add_window_view(|window, cx| {
        let dir = fixture.0.to_string_lossy().into_owned();
        let view = cx.new(|cx| {
            if chooser {
                BrowserView::new_chooser(dir, window, cx)
            } else {
                BrowserView::new_at(Some(dir), window, cx)
            }
        });
        view.update(cx, |view, _| {
            view.app_opener =
                std::sync::Arc::new(crate::platform::open_with::tests::NoExternalApps);
        });
        browser = Some(view.clone());
        let content = cx.new(|_| BrowserTestRoot(view));
        gpui_kit::component::Root::new(content, window, cx)
    });
    (browser.unwrap(), cx)
}

fn visible_paths(view: &Entity<BrowserView>, cx: &mut VisualTestContext) -> Vec<PathBuf> {
    cx.update(|_, cx| {
        view.read(cx)
            .table
            .read(cx)
            .delegate()
            .visible
            .iter()
            .map(|row| PathBuf::from(&row.id))
            .collect()
    })
}

fn query(view: &Entity<BrowserView>, value: &str, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.on_search(&Search, window, cx);
        });
    });
    cx.run_until_parked();
    cx.simulate_keystrokes("ctrl-a backspace");
    if !value.is_empty() {
        cx.simulate_input(value);
    }
    cx.run_until_parked();
    cx.update(|_, cx| assert_eq!(view.read(cx).table.read(cx).delegate().query(), value));
}

fn check_search_text_shortcuts(chooser: bool, cx: &mut TestAppContext) {
    let fixture = Fixture::new("text-shortcuts");
    let alpha = fixture.write("alpha.txt");
    let beta = fixture.write("beta.txt");
    let (view, cx) = browser(&fixture, chooser, cx);
    query(&view, "alpha", cx);
    cx.simulate_keystrokes("ctrl-a");
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx)
                .search_input
                .read(cx)
                .selected_value()
                .as_ref(),
            "alpha"
        );
        assert_eq!(view.read(cx).status(cx).selected, 0);
    });
    cx.simulate_input("beta");
    assert_eq!(visible_paths(&view, cx), vec![beta.clone()]);
    cx.simulate_keystrokes("ctrl-a ctrl-c");
    cx.update(|_, cx| {
        assert_eq!(
            cx.read_from_clipboard().unwrap().text().as_deref(),
            Some("beta")
        )
    });
    cx.simulate_keystrokes("ctrl-x");
    cx.update(|_, cx| assert!(view.read(cx).search_input.read(cx).value().is_empty()));
    cx.simulate_keystrokes("ctrl-v");
    cx.update(|_, cx| assert_eq!(view.read(cx).search_input.read(cx).value().as_ref(), "beta"));
    assert_eq!(visible_paths(&view, cx), vec![beta.clone()]);
    cx.simulate_keystrokes("ctrl-a delete");
    cx.update(|_, cx| {
        assert!(view.read(cx).search_input.read(cx).value().is_empty());
        assert_eq!(view.read(cx).status(cx).selected, 0);
    });
    assert_eq!(visible_paths(&view, cx), vec![alpha.clone(), beta.clone()]);
    assert!(alpha.is_file() && beta.is_file());
    cx.update(|window, cx| window.focus(&view.read(cx).focus_handle(cx), cx));
    cx.run_until_parked();
    cx.simulate_keystrokes("ctrl-a");
    cx.update(|_, cx| assert_eq!(view.read(cx).status(cx).selected, 2));
}

#[gpui_kit::test]
fn main_search_text_shortcuts_do_not_select_or_operate_on_files(cx: &mut TestAppContext) {
    check_search_text_shortcuts(false, cx);
}

#[gpui_kit::test]
fn chooser_search_text_shortcuts_do_not_select_or_operate_on_files(cx: &mut TestAppContext) {
    check_search_text_shortcuts(true, cx);
}

#[gpui_kit::test]
fn reloading_shortcuts_preserves_text_editing_and_replaces_old_file_binding(
    cx: &mut TestAppContext,
) {
    let fixture = Fixture::new("shortcut-reload");
    fixture.write("alpha.txt");
    fixture.write("beta.txt");
    let (view, cx) = browser(&fixture, false, cx);
    let mut settings = crate::settings::SettingsFile::default();
    settings
        .keybindings
        .insert("select-all".into(), "ctrl-alt-a".into());
    cx.update(|_, cx| {
        crate::app::keybindings::register(&settings, cx);
        crate::app::keybindings::register(&settings, cx);
    });
    query(&view, "alpha", cx);
    cx.simulate_keystrokes("ctrl-a");
    cx.update(|_, cx| {
        assert_eq!(
            view.read(cx)
                .search_input
                .read(cx)
                .selected_value()
                .as_ref(),
            "alpha"
        )
    });
    cx.simulate_keystrokes("backspace");
    assert_eq!(visible_paths(&view, cx).len(), 2);
    cx.update(|window, cx| window.focus(&view.read(cx).focus_handle(cx), cx));
    cx.run_until_parked();
    cx.simulate_keystrokes("ctrl-a");
    cx.update(|_, cx| assert_eq!(view.read(cx).status(cx).selected, 0));
    cx.simulate_keystrokes("ctrl-alt-a");
    cx.update(|_, cx| assert_eq!(view.read(cx).status(cx).selected, 2));
}

#[gpui_kit::test]
fn rapid_double_clicks_keep_opening_nested_folders(cx: &mut TestAppContext) {
    let fixture = Fixture::new("rapid-open");
    std::fs::create_dir_all(fixture.0.join("folder/folder/folder/folder")).unwrap();
    let (view, cx) = browser(&fixture, false, cx);
    let mut directory = fixture.0.clone();
    for click_count in 1..=8 {
        let position = cx.update(|_, cx| {
            let bounds = view
                .read(cx)
                .table
                .read(cx)
                .vertical_scroll_handle
                .0
                .borrow()
                .base_handle
                .bounds();
            assert!(bounds.size.height > px(0.0));
            gpui_kit::point(bounds.left() + px(80.0), bounds.top() + px(16.0))
        });
        cx.simulate_event(gpui_kit::MouseDownEvent {
            position,
            button: MouseButton::Left,
            modifiers: Default::default(),
            click_count,
            first_mouse: false,
        });
        cx.simulate_event(gpui_kit::MouseUpEvent {
            position,
            button: MouseButton::Left,
            modifiers: Default::default(),
            click_count,
        });
        cx.run_until_parked();
        if click_count.is_multiple_of(2) {
            directory.push("folder");
        }
        cx.update(|_, cx| {
            assert_eq!(
                view.read(cx).current_dir(cx).as_deref(),
                directory.to_str(),
                "navigation after click {click_count}"
            );
        });
    }
}

#[gpui_kit::test]
fn chooser_search_only_returns_current_children_once(cx: &mut TestAppContext) {
    let fixture = Fixture::new("chooser-search");
    let report = fixture.write("report.txt");
    fixture.write("archive/report.txt");
    fixture.write("folder-hit/folder-hit/report.txt");
    fixture.write(".report-hidden.txt");
    let (view, cx) = browser(&fixture, true, cx);
    query(&view, "report", cx);
    assert_eq!(visible_paths(&view, cx), vec![report.clone()]);
    query(&view, "folder", cx);
    assert_eq!(visible_paths(&view, cx), vec![fixture.0.join("folder-hit")]);
    query(&view, "REPORT", cx);
    let new_report = fixture.write("report-new.txt");
    cx.update(|window, cx| view.update(cx, |view, cx| view.refresh(window, cx)));
    cx.run_until_parked();
    assert_eq!(visible_paths(&view, cx), vec![new_report, report]);
    query(&view, "", cx);
    assert_eq!(visible_paths(&view, cx).len(), 4);
}

#[gpui_kit::test]
fn batch_delete_returns_before_disk_work_and_refreshes_once(cx: &mut TestAppContext) {
    let fixture = Fixture::new("delete");
    let first = fixture.write("first.txt");
    let second = fixture.write("folder/second.txt");
    let (view, cx) = browser(&fixture, false, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.on_select_all(&SelectAll, window, cx);
            view.on_delete_permanently(&Delete, window, cx);
            assert!(first.is_file(), "delete must return before filesystem work");
            assert!(second.is_file());
            assert_eq!(cx.global::<TransferCenterState>().completed_epoch(), 0);
        });
    });
    cx.run_until_parked();
    assert!(!first.exists());
    assert!(!second.exists());
    assert!(visible_paths(&view, cx).is_empty());
    cx.update(|window, cx| {
        assert_eq!(cx.global::<TransferCenterState>().completed_epoch(), 1);
        assert!(window.notifications(cx).is_empty());
    });
}

#[gpui_kit::test]
fn delete_reports_partial_failure_without_changing_new_location(cx: &mut TestAppContext) {
    let fixture = Fixture::new("delete-failure");
    let first = fixture.write("first.txt");
    let missing = fixture.write("missing.txt");
    let destination = Fixture::new("delete-navigation");
    let keep = destination.write("keep.txt");
    let (view, cx) = browser(&fixture, false, cx);
    std::fs::remove_file(&missing).unwrap();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.on_select_all(&SelectAll, window, cx);
            view.on_delete_permanently(&Delete, window, cx);
            view.change_dir(destination.0.to_string_lossy().into_owned(), cx);
        });
    });
    cx.run_until_parked();
    assert!(!first.exists());
    assert_eq!(visible_paths(&view, cx), vec![keep]);
    cx.update(|window, cx| {
        assert_eq!(window.notifications(cx).len(), 1);
        assert_eq!(cx.global::<TransferCenterState>().completed_epoch(), 1);
        assert!(cx.global::<TransferCenterState>().is_empty());
    });
}

#[gpui_kit::test]
fn main_browser_projects_search_only_returns_direct_children(cx: &mut TestAppContext) {
    let fixture = Fixture::new("projects-search");
    for name in [
        "tern/target/package/tern",
        "tern/vendor/tern",
        "other-project/deps/tern",
        "other-project/deps/internal_redirect",
        "other-project/deps/internal_testing",
        "other-project/deps/internals",
        "other-project/deps/patterns",
    ] {
        std::fs::create_dir_all(fixture.0.join(name)).unwrap();
    }
    let (view, cx) = browser(&fixture, false, cx);
    query(&view, "tern", cx);
    assert_eq!(visible_paths(&view, cx), vec![fixture.0.join("tern")]);
    query(&view, "TERN", cx);
    assert_eq!(visible_paths(&view, cx), vec![fixture.0.join("tern")]);
    query(&view, "", cx);
    assert_eq!(visible_paths(&view, cx).len(), 2);
}

#[gpui_kit::test]
fn chooser_search_combines_name_hidden_and_file_filters(cx: &mut TestAppContext) {
    let fixture = Fixture::new("chooser-filters");
    let report = fixture.write("report.txt");
    fixture.write("report.png");
    let hidden = fixture.write(".report.txt");
    let nested = fixture.write("report-folder/report.txt");
    let (view, cx) = browser(&fixture, true, cx);
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.set_file_filter(
                Some(crate::files::filter::FileFilter {
                    name: "Text".into(),
                    rules: vec![(0, "*.txt".into())],
                }),
                cx,
            );
        });
    });
    query(&view, "*report*", cx);
    assert_eq!(
        visible_paths(&view, cx),
        vec![fixture.0.join("report-folder"), report.clone()]
    );
    cx.update(|window, cx| view.update(cx, |view, cx| view.toggle_hidden(window, cx)));
    cx.run_until_parked();
    assert_eq!(
        visible_paths(&view, cx),
        vec![fixture.0.join("report-folder"), hidden, report]
    );
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.change_dir(
                fixture
                    .0
                    .join("report-folder")
                    .to_string_lossy()
                    .into_owned(),
                cx,
            );
        });
    });
    cx.run_until_parked();
    assert_eq!(visible_paths(&view, cx), vec![nested]);
    cx.update(|_, cx| assert!(view.read(cx).table.read(cx).delegate().query().is_empty()));
}

#[gpui_kit::test]
fn opening_a_search_result_resets_query_and_restores_list_focus(cx: &mut TestAppContext) {
    let fixture = Fixture::new("search-navigation");
    let nested = fixture.write("report-folder/child.txt");
    let (view, cx) = browser(&fixture, true, cx);
    query(&view, "report", cx);
    let position = cx.update(|_, cx| {
        let bounds = view
            .read(cx)
            .table
            .read(cx)
            .vertical_scroll_handle
            .0
            .borrow()
            .base_handle
            .bounds();
        gpui_kit::point(bounds.left() + px(80.0), bounds.top() + px(16.0))
    });
    for click_count in 1..=2 {
        cx.simulate_event(gpui_kit::MouseDownEvent {
            position,
            button: MouseButton::Left,
            modifiers: Default::default(),
            click_count,
            first_mouse: false,
        });
        cx.simulate_event(gpui_kit::MouseUpEvent {
            position,
            button: MouseButton::Left,
            modifiers: Default::default(),
            click_count,
        });
        cx.run_until_parked();
    }
    assert_eq!(visible_paths(&view, cx), vec![nested]);
    cx.update(|window, cx| {
        let view = view.read(cx);
        assert!(!view.search_open);
        assert!(view.table.read(cx).delegate().query().is_empty());
        assert!(view.focus_handle(cx).is_focused(window));
    });
}
