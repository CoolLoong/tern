use super::interaction_tests::{Fixture, browser};
use super::*;
use crate::platform::open_with::tests::{Call, MockOpener};
use gpui_kit::{Modifiers, TestAppContext, VisualTestContext, point};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

fn install_backend(
    view: &Entity<BrowserView>,
    default: bool,
    cx: &mut VisualTestContext,
) -> Arc<MockOpener> {
    let executor = cx.update(|_, cx| cx.background_executor().clone());
    let backend = Arc::new(MockOpener::new(default, move || {
        assert!(!executor.is_main_thread())
    }));
    view.update(cx, |view, _| view.app_opener = backend.clone());
    backend
}

fn double_click(view: &Entity<BrowserView>, path: &Path, cx: &mut VisualTestContext) {
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
}

fn click(selector: &'static str, cx: &mut VisualTestContext) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("missing {selector}"));
    cx.simulate_click(bounds.center(), Modifiers::default());
    cx.run_until_parked();
}

fn mime() -> crate::platform::open_with::Association {
    crate::platform::open_with::Association::Mime(
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document".into(),
    )
}

#[gpui_kit::test]
fn double_click_document_uses_default_application_once(cx: &mut TestAppContext) {
    let fixture = Fixture::new("default-app");
    let path = fixture.write("文档 with spaces;$(literal).docx");
    let (view, cx) = browser(&fixture, false, cx);
    let backend = install_backend(&view, true, cx);
    double_click(&view, &path, cx);
    assert_eq!(
        backend.state.lock().unwrap().calls,
        [
            Call::Describe(path.clone()),
            Call::Launch("writer.desktop".into(), path)
        ]
    );
    assert!(cx.debug_bounds("app-chooser").is_none());
}

#[gpui_kit::test]
fn missing_default_requires_selection_and_remembers_only_after_open(cx: &mut TestAppContext) {
    let fixture = Fixture::new("choose-default");
    let path = fixture.write("document.docx");
    let (view, cx) = browser(&fixture, false, cx);
    let backend = install_backend(&view, false, cx);
    double_click(&view, &path, cx);
    click("app-chooser-open", cx);
    assert_eq!(
        backend.state.lock().unwrap().calls,
        [Call::Describe(path.clone()), Call::Applications]
    );
    click("app-choice-1", cx);
    assert!(
        !backend
            .state
            .lock()
            .unwrap()
            .calls
            .iter()
            .any(|call| matches!(call, Call::SetDefault(..)))
    );
    click("app-chooser-open", cx);
    assert!(cx.debug_bounds("app-chooser").is_none());
    assert_eq!(
        backend.state.lock().unwrap().calls,
        [
            Call::Describe(path.clone()),
            Call::Applications,
            Call::Launch("editor.desktop".into(), path.clone()),
            Call::SetDefault("editor.desktop".into(), mime())
        ]
    );
    double_click(&view, &path, cx);
    assert!(cx.debug_bounds("app-chooser").is_none());
    assert_eq!(
        backend.state.lock().unwrap().calls.last(),
        Some(&Call::Launch("editor.desktop".into(), path))
    );
}

#[gpui_kit::test]
fn opening_once_and_cancelling_leave_default_unchanged(cx: &mut TestAppContext) {
    let fixture = Fixture::new("open-once");
    let path = fixture.write("document.docx");
    let (view, cx) = browser(&fixture, false, cx);
    let backend = install_backend(&view, false, cx);
    double_click(&view, &path, cx);
    click("app-chooser-cancel", cx);
    assert_eq!(
        backend.state.lock().unwrap().calls,
        [Call::Describe(path.clone()), Call::Applications]
    );
    double_click(&view, &path, cx);
    click("app-choice-0", cx);
    click("app-chooser-default", cx);
    click("app-chooser-open", cx);
    assert!(backend.state.lock().unwrap().default.is_none());
    assert!(
        !backend
            .state
            .lock()
            .unwrap()
            .calls
            .iter()
            .any(|call| matches!(call, Call::SetDefault(..)))
    );
}

#[gpui_kit::test]
fn failed_default_offers_another_app_and_does_not_save_failed_choice(cx: &mut TestAppContext) {
    let fixture = Fixture::new("failed-default");
    let path = fixture.write("document.docx");
    let (view, cx) = browser(&fixture, false, cx);
    let backend = install_backend(&view, true, cx);
    backend.state.lock().unwrap().fail_launch = Some("writer.desktop".into());
    double_click(&view, &path, cx);
    assert!(cx.debug_bounds("app-chooser").is_some());
    click("app-choice-0", cx);
    click("app-chooser-open", cx);
    assert!(cx.debug_bounds("app-chooser").is_some());
    assert!(
        !backend
            .state
            .lock()
            .unwrap()
            .calls
            .iter()
            .any(|call| matches!(call, Call::SetDefault(..)))
    );
    click("app-choice-1", cx);
    click("app-chooser-open", cx);
    assert!(cx.debug_bounds("app-chooser").is_none());
    assert_eq!(
        backend.state.lock().unwrap().default.as_ref().unwrap().id,
        "editor.desktop"
    );
}

#[gpui_kit::test]
fn default_save_failure_is_reported_after_file_opens(cx: &mut TestAppContext) {
    let fixture = Fixture::new("default-save-error");
    let path = fixture.write("document.docx");
    let (view, cx) = browser(&fixture, false, cx);
    let backend = install_backend(&view, false, cx);
    backend.state.lock().unwrap().fail_save = true;
    double_click(&view, &path, cx);
    click("app-choice-0", cx);
    click("app-chooser-open", cx);
    assert!(cx.debug_bounds("app-chooser").is_none());
    cx.update(|window, cx| assert_eq!(window.notifications(cx).len(), 1));
    assert!(backend.state.lock().unwrap().default.is_none());
}

#[gpui_kit::test]
fn upload_chooser_double_click_accepts_the_file_without_launching_an_app(cx: &mut TestAppContext) {
    let fixture = Fixture::new("portal-accept");
    let path = fixture.write("document.docx");
    let (view, cx) = browser(&fixture, true, cx);
    let backend = install_backend(&view, true, cx);
    let accepted = Arc::new(Mutex::new(Vec::<PathBuf>::new()));
    let captured = accepted.clone();
    let _subscription = cx.update(|_, cx| {
        cx.subscribe(&view, move |_, event, _| {
            let BrowserEvent::AcceptSelection(paths) = event;
            captured.lock().unwrap().extend(paths.clone());
        })
    });
    double_click(&view, &path, cx);
    assert_eq!(*accepted.lock().unwrap(), [path]);
    assert!(backend.state.lock().unwrap().calls.is_empty());
}

#[gpui_kit::test]
fn open_with_uses_the_chooser_and_keeps_the_existing_default_by_default(cx: &mut TestAppContext) {
    let fixture = Fixture::new("explicit-open-with");
    let path = fixture.write("document.docx");
    let (view, cx) = browser(&fixture, false, cx);
    let backend = install_backend(&view, true, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.table.update(cx, |table, _| {
                table.delegate_mut().select_single(0);
            });
            view.on_open_with(&OpenWith, window, cx);
        })
    });
    cx.run_until_parked();
    click("app-choice-1", cx);
    click("app-chooser-open", cx);
    assert_eq!(
        backend.state.lock().unwrap().default.as_ref().unwrap().id,
        "writer.desktop"
    );
    assert_eq!(
        backend.state.lock().unwrap().calls.last(),
        Some(&Call::Launch("editor.desktop".into(), path))
    );
}

#[gpui_kit::test]
fn cancelling_a_chooser_stops_queued_requests_and_allows_reopening(cx: &mut TestAppContext) {
    let fixture = Fixture::new("open-queue");
    let first = fixture.write("first.docx");
    let second = fixture.write("second.docx");
    let (view, cx) = browser(&fixture, false, cx);
    let backend = install_backend(&view, false, cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_paths(
                vec![first.clone(), first.clone(), second],
                false,
                window,
                cx,
            );
        })
    });
    cx.run_until_parked();
    assert_eq!(
        backend.state.lock().unwrap().calls,
        [Call::Describe(first.clone()), Call::Applications]
    );
    cx.simulate_keystrokes("escape");
    cx.run_until_parked();
    assert!(cx.debug_bounds("app-chooser").is_none());
    cx.update(|_, cx| {
        assert!(view.read(cx).open_requests.is_empty());
        assert!(view.read(cx).opening_path.is_none());
    });
    double_click(&view, &first, cx);
    assert!(cx.debug_bounds("app-chooser").is_some());
}

#[gpui_kit::test]
fn no_applications_and_unavailable_files_report_without_launching(cx: &mut TestAppContext) {
    let fixture = Fixture::new("missing-apps");
    let path = fixture.write("document.docx");
    let (view, cx) = browser(&fixture, false, cx);
    let backend = install_backend(&view, false, cx);
    backend.state.lock().unwrap().apps.clear();
    double_click(&view, &path, cx);
    assert!(cx.debug_bounds("app-chooser").is_some());
    click("app-chooser-open", cx);
    assert!(
        !backend
            .state
            .lock()
            .unwrap()
            .calls
            .iter()
            .any(|call| matches!(call, Call::Launch(..)))
    );
    click("app-chooser-cancel", cx);
    backend.state.lock().unwrap().fail_describe = true;
    double_click(&view, &path, cx);
    assert!(cx.debug_bounds("app-chooser").is_none());
    cx.update(|window, cx| {
        assert_eq!(window.notifications(cx).len(), 1);
        assert!(view.read(cx).opening_path.is_none());
    });
}
