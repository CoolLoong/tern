use super::*;
use gpui_kit::{Entity, TestAppContext};
use std::path::PathBuf;

struct Fixture(PathBuf);
impl Fixture {
    fn new(label: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("tern-browser-{label}-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn path(&self) -> String {
        self.0.to_string_lossy().into_owned()
    }
    fn write(&self, name: &str, bytes: &[u8]) -> String {
        let path = self.0.join(name);
        std::fs::write(&path, bytes).unwrap();
        path.to_string_lossy().into_owned()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// GPUI's TestAppContext uses TestPlatform/TestWindow: no native window,
// compositor, session bus or renderer is opened by these tests.
fn table(cx: &mut TestAppContext) -> Entity<TableState<BrowserDelegate>> {
    cx.update(gpui_kit::component::init);
    cx.add_window(|window, cx| TableState::new(BrowserDelegate::new(), window, cx))
        .root(cx)
        .unwrap()
}

#[gpui_kit::test]
fn rapid_navigation_and_recent_load_publish_only_the_current_directory(cx: &mut TestAppContext) {
    let a = Fixture::new("nav-a");
    let b = Fixture::new("nav-b");
    a.write("a", b"a");
    b.write("b", b"b");
    let table = table(cx);
    table.update(cx, |table, cx| {
        let delegate = table.delegate_mut();
        delegate.cd(crate::files::recent::RECENT_URI.into(), cx);
        assert!(
            delegate.loading.contains(crate::files::recent::RECENT_URI),
            "Recent must start a background load too"
        );
        assert!(delegate.visible.is_empty());
        delegate.cd(a.path(), cx);
        delegate.cd(b.path(), cx);
        delegate.cd(a.path(), cx); // same path as a cancelled earlier request
        assert!(
            delegate.visible.is_empty(),
            "IO and preparation have not run in the foreground"
        );
    });
    cx.run_until_parked();
    cx.update(|cx| {
        let delegate = table.read(cx).delegate();
        assert_eq!(delegate.directory.as_deref(), Some(a.path().as_str()));
        assert_eq!(
            delegate
                .visible
                .iter()
                .map(|r| r.name.as_str())
                .collect::<Vec<_>>(),
            ["a"]
        );
        assert!(delegate.loading.is_empty());
    });
}

#[gpui_kit::test]
fn in_flight_sort_hidden_and_selection_changes_survive_completion(cx: &mut TestAppContext) {
    let fixture = Fixture::new("options");
    fixture.write("small", b"1");
    fixture.write("big", b"123456789");
    let hidden = fixture.write(".hidden", b"12");
    let table = table(cx);
    table.update(cx, |table, cx| {
        let delegate = table.delegate_mut();
        delegate.cd(fixture.path(), cx);
        delegate.set_sort(SortColumn::Size, cx);
        delegate.set_sort(SortColumn::Size, cx);
        delegate.select_after_load(vec![hidden.clone()]);
    });
    cx.run_until_parked();
    cx.update(|cx| {
        let delegate = table.read(cx).delegate();
        assert_eq!(
            delegate
                .visible
                .iter()
                .map(|r| r.name.as_str())
                .collect::<Vec<_>>(),
            ["big", ".hidden", "small"]
        );
        assert!(delegate.interaction.selected_ids.contains(&hidden));
    });
    table.update(cx, |table, cx| {
        table.delegate_mut().set_show_hidden(false, cx)
    });
    cx.run_until_parked();
    cx.update(|cx| {
        let delegate = table.read(cx).delegate();
        assert!(!delegate.visible.contains(&hidden));
        assert!(delegate.interaction.selected_ids.is_empty());
    });
}

#[gpui_kit::test]
fn search_refresh_and_clear_use_the_right_snapshot(cx: &mut TestAppContext) {
    let fixture = Fixture::new("search");
    let old = fixture.write("hit-old", b"1");
    fixture.write("other", b"2");
    let table = table(cx);
    table.update(cx, |table, cx| table.delegate_mut().cd(fixture.path(), cx));
    cx.run_until_parked();
    table.update(cx, |table, cx| {
        table.delegate_mut().set_query("hit".into(), cx)
    });
    cx.run_until_parked();
    cx.update(|cx| assert_eq!(table.read(cx).delegate().visible.len(), 1));
    std::fs::remove_file(&old).unwrap();
    let new = fixture.write("hit-new", b"3");
    table.update(cx, |table, cx| {
        table.delegate_mut().refresh_dir(fixture.path(), cx);
        table.delegate_mut().refresh_at(None, cx);
    });
    cx.run_until_parked();
    cx.update(|cx| {
        let delegate = table.read(cx).delegate();
        assert!(!delegate.visible.contains(&old));
        assert!(delegate.visible.contains(&new));
        assert_eq!(delegate.visible.len(), 1);
        assert!(!delegate.searching);
    });
    table.update(cx, |table, cx| {
        table.delegate_mut().set_query(String::new(), cx)
    });
    cx.run_until_parked();
    cx.update(|cx| {
        let delegate = table.read(cx).delegate();
        assert_eq!(delegate.visible.len(), 2);
        assert!(delegate.visible.contains(&new));
        assert!(!delegate.search_truncated);
    });
}
