use crate::workspace::windows::{open_tern_window, open_window};
use gpui_kit::App;
use gpui_transfer::TransferCenterState;

pub(crate) fn run(
    settings: crate::settings::SettingsFile,
    initial: Option<crate::platform::desktop::OpenRequest>,
    instance: crate::platform::desktop::Instance,
) {
    gpui_kit::application()
        .with_quit_mode(gpui_kit::QuitMode::Explicit)
        .with_assets(crate::app::assets::TernAssets)
        .run(move |cx: &mut App| {
            init_app(cx, &settings);
            init_tray(cx);
            if let Some(request) = initial {
                open_request(request, cx);
            }
            cx.spawn(async move |cx| {
                let _connection = instance.connection;
                while let Ok(request) = instance.requests.recv().await {
                    cx.update(|cx| open_request(request, cx));
                }
            })
            .detach();
        });
}

// Only the elected file-manager process gets an icon. The portal helper and
// init_app's headless test callers must not register duplicate tray entries.
fn init_tray(cx: &mut App) {
    use crate::platform::tray::{self, Command};
    let config = crate::settings::config_dir_path();
    let default = home::home_dir().unwrap_or_default().join(".config/tern");
    let id = crate::platform::desktop::instance_name(&config, &default);
    let (sender, receiver) = smol::channel::unbounded();
    let task = cx
        .background_executor()
        .spawn(async move { tray::start(id, sender).await });
    cx.spawn(async move |cx| {
        let _tray = match task.await {
            Ok(tray) => tray,
            Err(error) => {
                log::warn!("cannot start system tray: {error}");
                return;
            }
        };
        while let Ok(command) = receiver.recv().await {
            cx.update(|cx| match command {
                Command::Open => {
                    open_request(
                        crate::platform::desktop::OpenRequest {
                            targets: Vec::new(),
                        },
                        cx,
                    );
                    // Niri may ignore ordinary Wayland activation without a token.
                    cx.background_executor()
                        .spawn(async {
                            crate::platform::desktop::activate_existing(std::process::id());
                        })
                        .detach();
                }
                Command::Quit => {
                    let windows = cx
                        .global::<crate::workspace::state::WindowState>()
                        .browser_windows
                        .clone();
                    for handle in windows.into_iter().rev() {
                        if handle
                            .update(cx, |root, _, cx| {
                                if let Ok(view) =
                                    root.view()
                                        .clone()
                                        .downcast::<crate::workspace::window::TernWindow>()
                                {
                                    view.read(cx).save_workspace(cx);
                                }
                            })
                            .is_ok()
                        {
                            break;
                        }
                    }
                    cx.quit();
                }
            });
        }
    })
    .detach();
}

pub(crate) fn init_app(cx: &mut App, settings: &crate::settings::SettingsFile) {
    cx.set_app_identity("tern", "Tern");
    menubar::init(cx);
    gpui_dock::init(cx);
    gpui_kit::component::init(cx);

    crate::workspace::state::init(cx);
    cx.set_global(crate::workspace::status::StatusState::default());
    cx.set_global(crate::browser::clipboard::ClipboardState::default());
    crate::navigation::bookmarks::init(cx);
    cx.set_global(TransferCenterState::default());

    settings.apply_to_app(None, cx);
    crate::app::keybindings::register(settings, cx);
    crate::app::menus::set_app_menus(cx);
    crate::workspace::window::register_panel_factories(cx);
}

fn open_request(request: crate::platform::desktop::OpenRequest, cx: &mut App) {
    if request.targets.is_empty() {
        // The most recently created window may already be closed. Keep the
        // focus order of all browser windows so reopening never duplicates one.
        let windows = cx
            .global::<crate::workspace::state::WindowState>()
            .browser_windows
            .clone();
        for handle in windows.into_iter().rev() {
            if handle
                .update(cx, |_, window, _| window.activate_window())
                .is_ok()
            {
                cx.activate(true);
                return;
            }
        }
        let settings = crate::settings::load_settings_from_disk().unwrap_or_default();
        open_tern_window(&settings, cx);
    } else {
        let settings = crate::settings::load_settings_from_disk().unwrap_or_default();
        for target in request.targets {
            open_window(
                &settings,
                Some(target.directory),
                false,
                target.selected,
                cx,
            );
        }
    }
    cx.activate(true);
}
