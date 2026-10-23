//! StatusNotifierItem adapter. Callbacks send commands; GPUI owns the windows.
use ksni::{Tray, TrayMethods};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Command {
    Open,
    Quit,
}

pub(crate) struct TernTray {
    id: String,
    commands: smol::channel::Sender<Command>,
}
impl TernTray {
    fn send(&self, command: Command) {
        let _ = self.commands.try_send(command);
    }
}
impl Tray for TernTray {
    fn id(&self) -> String {
        self.id.clone()
    }
    fn title(&self) -> String {
        "Tern".into()
    }
    fn status(&self) -> ksni::Status {
        ksni::Status::Active
    }
    fn icon_name(&self) -> String {
        "tern".into()
    }
    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            icon_name: "tern".into(),
            title: "Tern".into(),
            description: "文件管理器正在后台运行 · 点击打开，右键退出".into(),
            ..Default::default()
        }
    }
    fn activate(&mut self, _x: i32, _y: i32) {
        self.send(Command::Open);
    }
    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        vec![
            ksni::menu::StandardItem {
                label: "打开 Tern".into(),
                icon_name: "folder-open".into(),
                activate: Box::new(|this: &mut Self| this.send(Command::Open)),
                ..Default::default()
            }
            .into(),
            ksni::MenuItem::Separator,
            ksni::menu::StandardItem {
                label: "退出文件管理器".into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|this: &mut Self| this.send(Command::Quit)),
                ..Default::default()
            }
            .into(),
        ]
    }
    fn watcher_offline(&self, reason: ksni::OfflineReason) -> bool {
        log::debug!("tray host offline: {reason:?}; waiting for it to return");
        true
    }
}

pub(crate) struct TrayGuard(ksni::Handle<TernTray>);
impl Drop for TrayGuard {
    fn drop(&mut self) {
        drop(self.0.shutdown());
    }
}
pub(crate) async fn start(
    id: String,
    commands: smol::channel::Sender<Command>,
) -> Result<TrayGuard, ksni::Error> {
    // Keep the service alive if the shell starts later or is restarted.
    TernTray { id, commands }
        .assume_sni_available(true)
        .spawn()
        .await
        .map(TrayGuard)
}

#[cfg(test)]
mod tray_tests {
    use super::*;
    #[test]
    fn callbacks_only_enqueue_commands_and_keep_icon_active() {
        let (send, receive) = smol::channel::unbounded();
        let mut tray = TernTray {
            id: "test".into(),
            commands: send,
        };
        assert_eq!(tray.status(), ksni::Status::Active);
        assert_eq!(tray.icon_name(), "tern");
        tray.activate(0, 0);
        let menu = tray.menu();
        if let ksni::MenuItem::Standard(item) = &menu[2] {
            (item.activate)(&mut tray);
        } else {
            panic!("quit item missing")
        }
        assert_eq!(receive.try_recv().unwrap(), Command::Open);
        assert_eq!(receive.try_recv().unwrap(), Command::Quit);
        assert!(receive.try_recv().is_err());
    }

    struct Watcher(smol::channel::Sender<String>);
    #[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
    impl Watcher {
        fn register_status_notifier_item(&self, service: &str) {
            self.0.try_send(service.into()).unwrap();
        }
        #[zbus(property)]
        fn is_status_notifier_host_registered(&self) -> bool {
            true
        }
        #[zbus(property)]
        fn protocol_version(&self) -> i32 {
            0
        }
    }
    fn receive<T>(receiver: &smol::channel::Receiver<T>) -> T {
        smol::block_on(smol::future::race(
            async { receiver.recv().await.unwrap() },
            async {
                smol::Timer::after(std::time::Duration::from_secs(5)).await;
                panic!("tray event timed out");
            },
        ))
    }

    #[test]
    fn dbus_registration_activation_menu_and_host_restart_without_windows() {
        let (registered, registrations) = smol::channel::unbounded();
        let watcher = zbus::blocking::connection::Builder::session()
            .unwrap()
            .name("org.kde.StatusNotifierWatcher")
            .unwrap()
            .serve_at("/StatusNotifierWatcher", Watcher(registered))
            .unwrap()
            .build()
            .unwrap();
        let (commands, receiver) = smol::channel::unbounded();
        let tray = smol::block_on(start("tern-test".into(), commands)).unwrap();
        let service = receive(&registrations);
        let client = zbus::blocking::Connection::session().unwrap();
        let item = zbus::blocking::Proxy::new(
            &client,
            service.as_str(),
            "/StatusNotifierItem",
            "org.kde.StatusNotifierItem",
        )
        .unwrap();
        assert_eq!(item.get_property::<String>("IconName").unwrap(), "tern");
        assert_eq!(item.get_property::<String>("Status").unwrap(), "Active");
        item.call::<_, _, ()>("Activate", &(0i32, 0i32)).unwrap();
        assert_eq!(receive(&receiver), Command::Open);
        let menu = zbus::blocking::Proxy::new(
            &client,
            service.as_str(),
            "/MenuBar",
            "com.canonical.dbusmenu",
        )
        .unwrap();
        let items: Vec<(
            i32,
            std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
        )> = menu
            .call("GetGroupProperties", &(Vec::<i32>::new(), vec!["label"]))
            .unwrap();
        let quit = items
            .iter()
            .find_map(|(id, properties)| {
                properties
                    .get("label")
                    .and_then(|label| <&str>::try_from(label).ok())
                    .filter(|label| *label == "退出文件管理器")
                    .map(|_| *id)
            })
            .expect("quit menu entry exported");
        menu.call::<_, _, ()>(
            "Event",
            &(quit, "clicked", zbus::zvariant::Value::from(0i32), 0u32),
        )
        .unwrap();
        assert_eq!(receive(&receiver), Command::Quit);
        watcher
            .release_name("org.kde.StatusNotifierWatcher")
            .unwrap();
        watcher
            .request_name("org.kde.StatusNotifierWatcher")
            .unwrap();
        assert_eq!(
            receive(&registrations),
            service,
            "shell restart re-registers the same item"
        );
        smol::block_on(tray.0.shutdown());
        assert!(
            client
                .call_method(
                    Some(service.as_str()),
                    "/StatusNotifierItem",
                    Some("org.freedesktop.DBus.Properties"),
                    "Get",
                    &("org.kde.StatusNotifierItem", "Status")
                )
                .is_err()
        );
    }
}
