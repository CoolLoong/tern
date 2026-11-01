use super::sidebar::{PlacesEvent, PlacesSidebar};
use crate::platform::volumes::{Volume, VolumeBackend};
use gpui_kit::component::{
    ActiveTheme, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    notification::Notification,
};
use gpui_kit::{
    Context, InteractiveElement, IntoElement, ParentElement, Styled, Window, div,
    prelude::FluentBuilder, px,
};
use std::{collections::HashSet, sync::Arc, time::Duration};

pub(super) struct Devices {
    pub items: Vec<Volume>,
    pub error: Option<String>,
    pub busy: HashSet<String>,
    pub backend: Arc<dyn VolumeBackend>,
    pub watch: Option<gpui_kit::Task<()>>,
    pub generation: usize,
}

impl PlacesSidebar {
    pub(super) fn watch_devices(&mut self, cx: &mut Context<Self>) {
        let backend = self.devices.backend.clone();
        let executor = cx.background_executor().clone();
        self.devices.watch = Some(cx.spawn(async move |this, cx| {
            loop {
                let Ok(generation) = this.update(cx, |this, _| this.devices.generation) else {
                    break;
                };
                let backend = backend.clone();
                let result = executor.spawn(async move { backend.list() }).await;
                if this
                    .update(cx, |this, cx| {
                        if generation != this.devices.generation || !this.devices.busy.is_empty() {
                            return;
                        }
                        let (items, error) = match result {
                            Ok(items) => (items, None),
                            Err(error) => (Vec::new(), Some(error)),
                        };
                        if this.devices.items != items || this.devices.error != error {
                            this.devices.items = items;
                            this.devices.error = error;
                            cx.notify();
                        }
                    })
                    .is_err()
                {
                    break;
                }
                executor.timer(Duration::from_secs(5)).await;
            }
        }));
    }

    pub(super) fn activate_volume(
        &mut self,
        volume: Volume,
        unmount: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.devices.busy.insert(volume.id.clone()) {
            return;
        }
        self.devices.generation += 1;
        let backend = self.devices.backend.clone();
        let id = volume.id.clone();
        let worker = cx.background_executor().spawn(async move {
            let result = if unmount {
                backend.unmount(&id).map(|_| None)
            } else {
                backend.mount(&id).map(Some)
            };
            (result, backend.list())
        });
        cx.spawn_in(window, async move |this, cx| {
            let (result, listing) = worker.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.devices.busy.remove(&volume.id);
                this.devices.generation += 1;
                if let Ok(items) = listing {
                    this.devices.items = items;
                    this.devices.error = None;
                }
                match result {
                    Ok(Some(path)) => cx.emit(PlacesEvent::Navigate(path)),
                    Ok(None) => {}
                    Err(error) => window.push_notification(
                        Notification::error(format!("{}: {error}", volume.name)),
                        cx,
                    ),
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn render_devices(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let muted = cx.theme().muted_foreground;
        div()
            .flex()
            .flex_col()
            .child(Self::section_header(
                "places-devices",
                "Devices",
                self.devices_open,
                |this, cx| {
                    this.devices_open = !this.devices_open;
                    cx.notify();
                },
                cx,
            ))
            .when(self.devices_open, |this| {
                this.when(self.devices.error.is_some(), |this| {
                    this.child(
                        div()
                            .pl(px(22.0))
                            .pr(px(4.0))
                            .py(px(3.0))
                            .text_sm()
                            .text_color(muted)
                            .child("Disk service unavailable. Install/start udisks2."),
                    )
                })
                .when(
                    self.devices.items.is_empty() && self.devices.error.is_none(),
                    |this| {
                        this.child(
                            div()
                                .pl(px(22.0))
                                .pr(px(4.0))
                                .py(px(3.0))
                                .text_sm()
                                .text_color(muted)
                                .child("No external drives"),
                        )
                    },
                )
                .children(self.devices.items.iter().map(|volume| {
                    let busy = self.devices.busy.contains(&volume.id);
                    let open = volume.clone();
                    let unmount = volume.clone();
                    div()
                        .id(gpui_kit::SharedString::from(volume.id.clone()))
                        .flex()
                        .items_center()
                        .gap(px(6.0))
                        .pl(px(22.0))
                        .pr(px(4.0))
                        .py(px(3.0))
                        .rounded_sm()
                        .hover(|this| this.bg(cx.theme().list_hover.opacity(0.4)))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .cursor_pointer()
                                .on_mouse_down(
                                    gpui_kit::MouseButton::Left,
                                    cx.listener(move |this, _, window, cx| {
                                        this.activate_volume(open.clone(), false, window, cx)
                                    }),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(6.0))
                                        .child(
                                            gpui_kit::component::Icon::new(
                                                gpui_kit::component::IconName::Folder,
                                            )
                                            .small()
                                            .text_color(muted),
                                        )
                                        .child(div().text_sm().child(volume.name.clone())),
                                )
                                .child(div().pl(px(22.0)).text_xs().text_color(muted).child(
                                    format!(
                                        "{} · {} · {}",
                                        volume.device,
                                        crate::browser::format::human_bytes(volume.size),
                                        if busy {
                                            "Working…"
                                        } else if volume.mount.is_some() {
                                            "Mounted"
                                        } else {
                                            "Click to mount"
                                        }
                                    ),
                                )),
                        )
                        .when(volume.mount.is_some() && !busy, |this| {
                            this.child(Button::new("unmount").label("Unmount").ghost().on_click(
                                cx.listener(move |this, _, window, cx| {
                                    this.activate_volume(unmount.clone(), true, window, cx)
                                }),
                            ))
                        })
                }))
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{AppContext, TestAppContext};
    use std::sync::Mutex;

    struct Mock {
        calls: Mutex<Vec<&'static str>>,
        fail: bool,
        items: Mutex<Vec<Volume>>,
        executor: gpui_kit::BackgroundExecutor,
    }
    impl VolumeBackend for Mock {
        fn list(&self) -> Result<Vec<Volume>, String> {
            assert!(!self.executor.is_main_thread());
            Ok(self.items.lock().unwrap().clone())
        }
        fn mount(&self, _: &str) -> Result<String, String> {
            assert!(!self.executor.is_main_thread());
            self.calls.lock().unwrap().push("mount");
            if self.fail {
                Err("Permission denied".into())
            } else {
                Ok("/tmp/external disk".into())
            }
        }
        fn unmount(&self, _: &str) -> Result<(), String> {
            assert!(!self.executor.is_main_thread());
            self.calls.lock().unwrap().push("unmount");
            if self.fail {
                Err("Device busy".into())
            } else {
                Ok(())
            }
        }
    }
    fn volume() -> Volume {
        Volume {
            id: "disk".into(),
            name: "External".into(),
            device: "/dev/test".into(),
            size: 1024,
            mount: None,
        }
    }
    fn check(fail: bool, cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::app::runtime::init_app(cx, &crate::settings::SettingsFile::default())
        });
        let backend = Arc::new(Mock {
            calls: Mutex::new(vec![]),
            fail,
            items: Mutex::new(vec![]),
            executor: cx.background_executor.clone(),
        });
        let mut sidebar = None;
        let (_, cx) = cx.add_window_view(|window, cx| {
            let view = cx.new(|cx| PlacesSidebar::new(window, cx));
            view.update(cx, |view, _| view.devices.backend = backend.clone());
            sidebar = Some(view.clone());
            gpui_kit::component::Root::new(view, window, cx)
        });
        let sidebar = sidebar.unwrap();
        let opened = Arc::new(Mutex::new(Vec::new()));
        let captured = opened.clone();
        let _sub = cx.update(|_, cx| {
            cx.subscribe(&sidebar, move |_, event: &PlacesEvent, _| {
                let PlacesEvent::Navigate(path) = event;
                captured.lock().unwrap().push(path.clone());
            })
        });
        cx.update(|window, cx| {
            sidebar.update(cx, |view, cx| {
                view.activate_volume(volume(), false, window, cx);
                view.activate_volume(volume(), false, window, cx);
            })
        });
        cx.run_until_parked();
        assert_eq!(*backend.calls.lock().unwrap(), ["mount"]);
        assert_eq!(opened.lock().unwrap().len(), usize::from(!fail));
        cx.update(|window, cx| {
            sidebar.update(cx, |view, cx| {
                view.activate_volume(volume(), true, window, cx)
            })
        });
        cx.run_until_parked();
        assert_eq!(*backend.calls.lock().unwrap(), ["mount", "unmount"]);
        cx.update(|window, cx| {
            assert!(sidebar.read(cx).devices.busy.is_empty());
            assert_eq!(window.notifications(cx).is_empty(), !fail);
        });
    }
    #[gpui_kit::test]
    fn external_volume_operations_run_on_workers_and_open_only_after_mount(
        cx: &mut TestAppContext,
    ) {
        check(false, cx);
    }
    #[gpui_kit::test]
    fn external_volume_failures_do_not_navigate_and_allow_retry(cx: &mut TestAppContext) {
        check(true, cx);
    }
    #[gpui_kit::test]
    fn device_watch_updates_after_insertion_and_removal(cx: &mut TestAppContext) {
        cx.update(|cx| {
            crate::app::runtime::init_app(cx, &crate::settings::SettingsFile::default())
        });
        let executor = cx.executor();
        let backend = Arc::new(Mock {
            calls: Mutex::new(vec![]),
            fail: false,
            items: Mutex::new(vec![volume()]),
            executor: executor.clone(),
        });
        let (sidebar, cx) = cx.add_window_view(|window, cx| {
            let mut sidebar = PlacesSidebar::new(window, cx);
            sidebar.devices.backend = backend.clone();
            sidebar.watch_devices(cx);
            sidebar
        });
        cx.run_until_parked();
        cx.update(|_, cx| assert_eq!(sidebar.read(cx).devices.items, vec![volume()]));
        backend.items.lock().unwrap().clear();
        executor.advance_clock(Duration::from_secs(5));
        cx.run_until_parked();
        cx.update(|_, cx| assert!(sidebar.read(cx).devices.items.is_empty()));
        assert!(backend.calls.lock().unwrap().is_empty());
    }
}
