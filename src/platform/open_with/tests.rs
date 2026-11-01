use super::*;
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

pub(crate) struct NoExternalApps;
impl AppOpener for NoExternalApps {
    fn describe(&self, _: &Path) -> Result<FileInfo, String> {
        panic!("unexpected application opening in headless fixture")
    }
    fn applications(&self, _: &FileInfo) -> Result<Vec<AppEntry>, String> {
        panic!("unexpected application discovery")
    }
    fn launch(&self, _: &AppEntry, _: &Path) -> Result<(), String> {
        panic!("headless tests must not launch desktop applications")
    }
    fn set_default(&self, _: &AppEntry, _: &Association) -> Result<(), String> {
        panic!("headless tests must not change real associations")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Call {
    Describe(PathBuf),
    Applications,
    Launch(String, PathBuf),
    SetDefault(String, Association),
}

pub(crate) struct MockState {
    pub default: Option<AppEntry>,
    pub apps: Vec<AppEntry>,
    pub calls: Vec<Call>,
    pub fail_launch: Option<String>,
    pub fail_save: bool,
    pub fail_describe: bool,
}

pub(crate) struct MockOpener {
    pub state: Mutex<MockState>,
    check_worker: Arc<dyn Fn() + Send + Sync>,
}

impl MockOpener {
    pub fn new(default: bool, check_worker: impl Fn() + Send + Sync + 'static) -> Self {
        let writer = AppEntry {
            id: "writer.desktop".into(),
            name: "Writer".into(),
            recommended: true,
        };
        let editor = AppEntry {
            id: "editor.desktop".into(),
            name: "Editor".into(),
            recommended: true,
        };
        Self {
            state: Mutex::new(MockState {
                default: default.then_some(writer.clone()),
                apps: vec![writer, editor],
                calls: Vec::new(),
                fail_launch: None,
                fail_save: false,
                fail_describe: false,
            }),
            check_worker: Arc::new(check_worker),
        }
    }
}

impl AppOpener for MockOpener {
    fn describe(&self, path: &Path) -> Result<FileInfo, String> {
        (self.check_worker)();
        let mut state = self.state.lock().unwrap();
        state.calls.push(Call::Describe(path.into()));
        if state.fail_describe {
            return Err("File is unavailable".into());
        }
        let mime = "application/vnd.openxmlformats-officedocument.wordprocessingml.document";
        Ok(FileInfo {
            content_type: mime.into(),
            description: "Word documents".into(),
            association: Some(Association::Mime(mime.into())),
            default_app: state.default.clone(),
        })
    }
    fn applications(&self, _: &FileInfo) -> Result<Vec<AppEntry>, String> {
        (self.check_worker)();
        let mut state = self.state.lock().unwrap();
        state.calls.push(Call::Applications);
        Ok(state.apps.clone())
    }
    fn launch(&self, app: &AppEntry, path: &Path) -> Result<(), String> {
        (self.check_worker)();
        let mut state = self.state.lock().unwrap();
        state.calls.push(Call::Launch(app.id.clone(), path.into()));
        if state.fail_launch.as_deref() == Some(&app.id) {
            return Err("Application could not start".into());
        }
        Ok(())
    }
    fn set_default(&self, app: &AppEntry, association: &Association) -> Result<(), String> {
        (self.check_worker)();
        let mut state = self.state.lock().unwrap();
        state
            .calls
            .push(Call::SetDefault(app.id.clone(), association.clone()));
        if state.fail_save {
            return Err("Default settings are read-only".into());
        }
        state.default = Some(app.clone());
        Ok(())
    }
}

#[test]
fn system_associations_and_launch_arguments_round_trip() {
    use std::{
        os::unix::fs::{PermissionsExt, symlink},
        process::Command,
        time::{Duration, Instant},
    };
    const CHILD: &str = "TERN_NATIVE_APP_TEST_ROOT";
    if let Some(root) = std::env::var_os(CHILD) {
        let root = PathBuf::from(root);
        let backend = SystemAppOpener;
        let text_file = root.join("文档 with spaces;$(touch SHOULD_NOT_EXIST).txt");
        std::fs::write(&text_file, "hello world\n").unwrap();
        let info = backend.describe(&text_file).unwrap();
        assert_eq!(info.content_type, "text/plain");
        let apps = backend.applications(&info).unwrap();
        let editor = apps.iter().find(|app| app.id == "editor.desktop").unwrap();
        assert_eq!(editor.name, "User Editor");
        assert_eq!(
            apps.iter().filter(|app| app.id == "editor.desktop").count(),
            1
        );
        assert!(!apps.iter().any(|app| app.id == "hidden.desktop"));
        backend
            .set_default(editor, &Association::Mime("text/plain".into()))
            .unwrap();
        let configured = backend.describe(&text_file).unwrap();
        assert_eq!(
            configured.default_app.as_ref().unwrap().id,
            "editor.desktop"
        );
        backend
            .launch(configured.default_app.as_ref().unwrap(), &text_file)
            .unwrap();
        let marker = root.join("launched-args");
        let deadline = Instant::now() + Duration::from_secs(5);
        while !marker.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let args = std::fs::read(&marker).unwrap();
        assert_eq!(
            args,
            [text_file.as_os_str().as_encoded_bytes(), b"\0"].concat()
        );
        assert!(!root.join("SHOULD_NOT_EXIST").exists());
        std::fs::remove_file(&marker).unwrap();
        let viewer = apps
            .iter()
            .find(|app| app.id == "uri-viewer.desktop")
            .unwrap();
        backend.launch(viewer, &text_file).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while !marker.exists() && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let args = std::fs::read(&marker).unwrap();
        let args: Vec<_> = args
            .split(|byte| *byte == 0)
            .filter(|arg| !arg.is_empty())
            .map(|arg| String::from_utf8(arg.to_vec()).unwrap())
            .collect();
        assert_eq!(args.len(), 5);
        assert_eq!(args[0], "Viewer with spaces");
        assert_eq!(
            args[1],
            root.join("data/applications/uri-viewer.desktop")
                .to_string_lossy()
        );
        assert!(
            args[2] == gio::File::for_path(&text_file).uri()
                || args[2] == text_file.to_string_lossy()
        );
        assert_eq!(args[3..], ["--icon", "document-open"]);
        struct LaunchReceiver(PathBuf);
        #[zbus::interface(name = "org.freedesktop.Application")]
        impl LaunchReceiver {
            fn open(
                &self,
                uris: Vec<String>,
                _platform_data: std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
            ) {
                std::fs::write(&self.0, uris.join("\n")).unwrap();
            }
        }
        let dbus_marker = root.join("dbus-opened");
        let _service = zbus::blocking::connection::Builder::session()
            .unwrap()
            .name("org.example.TernFixture")
            .unwrap()
            .serve_at(
                "/org/example/TernFixture",
                LaunchReceiver(dbus_marker.clone()),
            )
            .unwrap()
            .build()
            .unwrap();
        let dbus_app = apps
            .iter()
            .find(|app| app.id == "org.example.TernFixture.desktop")
            .unwrap();
        backend.launch(dbus_app, &text_file).unwrap();
        assert_eq!(
            std::fs::read_to_string(dbus_marker).unwrap(),
            gio::File::for_path(&text_file).uri()
        );
        let unknown = root.join("document.tern-docs-fixture");
        std::fs::write(&unknown, [0, 1, 2, 3, 4]).unwrap();
        let unknown_info = backend.describe(&unknown).unwrap();
        assert!(unknown_info.default_app.is_none());
        assert_eq!(
            unknown_info.association,
            Some(Association::Extension("tern-docs-fixture".into()))
        );
        backend
            .set_default(editor, unknown_info.association.as_ref().unwrap())
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if backend
                .describe(&unknown)
                .unwrap()
                .default_app
                .as_ref()
                .is_some_and(|app| app.id == "editor.desktop")
            {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "extension association was not applied"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(root.join("config/mimeapps.list").is_file());
        assert!(backend.describe(&root.join("missing.docx")).is_err());
        return;
    }
    struct Fixture(PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let root = std::env::temp_dir().join(format!(
        "tern-native-apps-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _fixture = Fixture(root.clone());
    for dir in [
        "data/applications",
        "system/applications",
        "config",
        "system-config",
        "cache",
        "state",
    ] {
        std::fs::create_dir_all(root.join(dir)).unwrap();
    }
    symlink("/usr/share/mime", root.join("system/mime")).unwrap();
    let launcher = root.join("test-launcher");
    std::fs::write(
        &launcher,
        "#!/bin/sh\nprintf '%s\\0' \"$@\" > \"$TERN_NATIVE_APP_TEST_ROOT/launched-args\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755)).unwrap();
    let executable = launcher
        .to_string_lossy()
        .replace('\\', "\\\\")
        .replace('"', "\\\"");
    let entry = |name: &str, fields: &str, extra: &str| {
        format!(
            "[Desktop Entry]\nType=Application\nName={name}\nExec=\"{executable}\" {fields}\nTerminal=false\n{extra}\n"
        )
    };
    std::fs::write(
        root.join("system/applications/editor.desktop"),
        entry("System Editor", "%f", ""),
    )
    .unwrap();
    std::fs::write(
        root.join("data/applications/editor.desktop"),
        entry("User Editor", "%f", "MimeType=text/plain;"),
    )
    .unwrap();
    std::fs::write(
        root.join("system/applications/hidden.desktop"),
        entry("Hidden Override", "%f", ""),
    )
    .unwrap();
    std::fs::write(
        root.join("data/applications/hidden.desktop"),
        "[Desktop Entry]\nType=Application\nHidden=true\n",
    )
    .unwrap();
    std::fs::write(
        root.join("data/applications/uri-viewer.desktop"),
        entry("Viewer with spaces", "%c %k %u %i", "Icon=document-open"),
    )
    .unwrap();
    std::fs::write(root.join("data/applications/org.example.TernFixture.desktop"), "[Desktop Entry]\nType=Application\nName=DBus test application\nExec=/usr/bin/false %U\nDBusActivatable=true\n").unwrap();
    let result = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "platform::open_with::tests::system_associations_and_launch_arguments_round_trip",
            "--nocapture",
        ])
        .env(CHILD, &root)
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_DATA_DIRS", root.join("system"))
        .env("XDG_CONFIG_HOME", root.join("config"))
        .env("XDG_CONFIG_DIRS", root.join("system-config"))
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("XDG_CURRENT_DESKTOP", "TernTest")
        .env("LC_ALL", "C")
        .env("GIO_USE_PORTALS", "0")
        .env("GIO_USE_VFS", "local")
        .env_remove("DISPLAY")
        .env_remove("WAYLAND_DISPLAY")
        .current_dir(&root)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}
