use std::path::{Path, PathBuf};

use gpui_kit::component::{
    Icon,
    menu::{PopupMenu, PopupMenuItem},
    table::TableState,
};
use gpui_kit::{App, Context, Entity, Focusable, Window, prelude::FluentBuilder};

use crate::app::actions::*;
use crate::browser::table::{BrowserDelegate, BrowserMenuTarget};

pub(crate) fn build_context_menu(
    mut menu: PopupMenu,
    table: &Entity<TableState<BrowserDelegate>>,
    window: &mut Window,
    cx: &mut Context<PopupMenu>,
) -> PopupMenu {
    menu = menu.action_context(table.focus_handle(cx));

    let target = table.read(cx).delegate().menu_target();
    use gpui_common::TermuaIcon;
    let target_path = app_target(table, cx);
    let table_open_with = table.clone();
    let table_compress_a = table.clone();
    let table_compress_b = table.clone();

    // Where the view is decides the menu before how many rows are picked: a
    // trashed file offers Restore whether one row or ten are selected, and
    // "Move to Trash" there would only shuffle within the trash directory.
    let in_recent = table.read(cx).delegate().in_recent();
    let in_trash = table.read(cx).delegate().in_trash();
    let selected = table.read(cx).delegate().target_ids();
    if !matches!(target, BrowserMenuTarget::Background) && (in_recent || in_trash) {
        let paths = selected;
        let table_for_menu = table.clone();
        menu = if in_recent {
            menu.menu_with_icon(
                "Open",
                Icon::default().path(TermuaIcon::Play),
                Box::new(OpenFile),
            )
            .item(
                PopupMenuItem::new("Remove from Recent").on_click(move |_, _, cx| {
                    for path in &paths {
                        crate::files::recent::forget(std::path::Path::new(path));
                    }
                    table_for_menu
                        .update(cx, |state, cx| state.delegate_mut().refresh_at(None, cx));
                }),
            )
        } else {
            menu.menu_with_icon(
                "Restore",
                Icon::default().path(TermuaIcon::Refresh),
                Box::new(RestoreFromTrash),
            )
            .menu_with_icon(
                "Delete Permanently",
                Icon::default().path(TermuaIcon::Trash),
                Box::new(Delete),
            )
        };
        return menu;
    }

    match target {
        BrowserMenuTarget::Background => {
            menu = menu
                .menu_with_icon(
                    "Refresh",
                    Icon::default().path(TermuaIcon::Refresh),
                    Box::new(Refresh),
                )
                .menu_with_icon(
                    "Create Folder",
                    Icon::default().path(TermuaIcon::FolderPlus),
                    Box::new(NewFolder),
                )
                .menu_with_icon(
                    "Create Empty Text File",
                    Icon::default().path(TermuaIcon::FileText),
                    Box::new(NewTextFile),
                )
                .separator()
                .menu_with_icon(
                    "Paste",
                    Icon::default().path(TermuaIcon::Paste),
                    Box::new(Paste),
                )
                .separator()
                .menu_with_icon(
                    "Open In Terminal",
                    Icon::default().path(TermuaIcon::Terminal),
                    Box::new(OpenInTerminal),
                )
                .menu("Properties", Box::new(Properties));
        }
        BrowserMenuTarget::Single(kind) => {
            let is_dir = kind == crate::files::model::EntryKind::Dir;
            let open_with_submenu = build_open_with_submenu(table, cx);

            menu = menu
                .menu_with_icon(
                    "Open",
                    Icon::default().path(TermuaIcon::Play),
                    Box::new(OpenFile),
                )
                .item(
                    PopupMenuItem::new("Open in New Tab")
                        .disabled(!is_dir)
                        .on_click({
                            let path = target_path.clone();
                            move |_, window, cx| {
                                crate::workspace::commands::dispatch_new_tab_at(window, &path, cx);
                            }
                        }),
                )
                .item(
                    PopupMenuItem::new("Open in New Window")
                        .disabled(!is_dir)
                        .on_click({
                            let path = target_path;
                            move |_, _window, cx| {
                                crate::workspace::commands::dispatch_new_window(&path, cx);
                            }
                        }),
                );

            if let Some(apps) = open_with_submenu {
                let table = table_open_with;
                menu = menu.submenu_with_icon(
                    Some(Icon::default().path(TermuaIcon::Global)),
                    "Open With…",
                    window,
                    cx,
                    move |mut menu, _window, _cx| {
                        let apps = apps.clone();
                        for app in apps {
                            let app = app.clone();
                            let table = table.clone();
                            menu = menu.item(PopupMenuItem::new(app.name.clone()).on_click(
                                move |_, _, cx| {
                                    let path = app_target(&table, cx);
                                    if let Err(err) =
                                        crate::platform::open_with::launch(&app, &path)
                                    {
                                        log::warn!(
                                            "failed to open {} with {}: {err}",
                                            path,
                                            app.name
                                        );
                                    }
                                },
                            ));
                        }
                        menu
                    },
                );
            }

            menu = menu
                .separator()
                .menu_with_icon(
                    "Cut",
                    Icon::default().path(TermuaIcon::Paste),
                    Box::new(Cut),
                )
                .menu_with_icon(
                    "Copy",
                    Icon::default().path(TermuaIcon::Paste),
                    Box::new(Copy),
                )
                .menu_with_icon(
                    "Paste",
                    Icon::default().path(TermuaIcon::Paste),
                    Box::new(Paste),
                )
                .menu_with_icon(
                    "Rename…",
                    Icon::default().path(TermuaIcon::SquarePen),
                    Box::new(Rename),
                )
                .separator()
                .menu_with_icon(
                    "Open in Terminal",
                    Icon::default().path(TermuaIcon::Terminal),
                    Box::new(OpenInTerminal),
                )
                .menu_with_icon(
                    "Move to Trash",
                    Icon::default().path(TermuaIcon::Trash),
                    Box::new(MoveToTrash),
                )
                .submenu_with_icon(
                    Some(Icon::default().path(TermuaIcon::FileArchive)),
                    "Compress…",
                    window,
                    cx,
                    move |mut menu, _window, _cx| {
                        let table_zip = table_compress_a.clone();
                        let table_targz = table_compress_a.clone();
                        menu = menu
                            .item(PopupMenuItem::new("Compress to .zip").on_click(
                                move |_, _, cx| {
                                    compress_target(&table_zip, "zip", cx);
                                },
                            ))
                            .item(PopupMenuItem::new("Compress to .tar.gz").on_click(
                                move |_, _, cx| {
                                    compress_target(&table_targz, "tar.gz", cx);
                                },
                            ));
                        menu
                    },
                )
                .menu("Properties", Box::new(Properties))
                .separator()
                .item(PopupMenuItem::new("Add to bookmark").action(Box::new(AddToBookmark)));
        }
        BrowserMenuTarget::Multi { has_dir } => {
            menu = menu
                .menu_with_icon(
                    "Open",
                    Icon::default().path(TermuaIcon::Play),
                    Box::new(OpenFile),
                )
                .when(has_dir, |menu| {
                    menu.item(PopupMenuItem::new("Open in New Tab").on_click({
                        let path = target_path.clone();
                        move |_, window, cx| {
                            crate::workspace::commands::dispatch_new_tab_at(window, &path, cx);
                        }
                    }))
                    .item(PopupMenuItem::new("Open in New Window").on_click({
                        let path = target_path.clone();
                        move |_, _window, cx| {
                            crate::workspace::commands::dispatch_new_window(&path, cx)
                        }
                    }))
                })
                .separator()
                .menu_with_icon(
                    "Cut",
                    Icon::default().path(TermuaIcon::Paste),
                    Box::new(Cut),
                )
                .menu_with_icon(
                    "Copy",
                    Icon::default().path(TermuaIcon::Paste),
                    Box::new(Copy),
                )
                .menu_with_icon(
                    "Paste",
                    Icon::default().path(TermuaIcon::Paste),
                    Box::new(Paste),
                )
                .menu_with_icon(
                    "Rename…",
                    Icon::default().path(TermuaIcon::SquarePen),
                    Box::new(Rename),
                )
                .separator()
                .menu_with_icon(
                    "Open in Terminal",
                    Icon::default().path(TermuaIcon::Terminal),
                    Box::new(OpenInTerminal),
                )
                .menu_with_icon(
                    "Move to Trash",
                    Icon::default().path(TermuaIcon::Trash),
                    Box::new(MoveToTrash),
                )
                .submenu_with_icon(
                    Some(Icon::default().path(TermuaIcon::FileArchive)),
                    "Compress…",
                    window,
                    cx,
                    move |mut menu, _window, _cx| {
                        let table_zip = table_compress_b.clone();
                        let table_targz = table_compress_b.clone();
                        menu = menu
                            .item(PopupMenuItem::new("Compress to .zip").on_click(
                                move |_, _, cx| {
                                    compress_target(&table_zip, "zip", cx);
                                },
                            ))
                            .item(PopupMenuItem::new("Compress to .tar.gz").on_click(
                                move |_, _, cx| {
                                    compress_target(&table_targz, "tar.gz", cx);
                                },
                            ));
                        menu
                    },
                )
                .menu("Properties", Box::new(Properties));
        }
    }

    menu
}

fn app_target(table: &Entity<TableState<BrowserDelegate>>, cx: &App) -> String {
    table
        .read(cx)
        .delegate()
        .target_ids()
        .first()
        .cloned()
        .unwrap_or_default()
}

fn build_open_with_submenu(
    table: &Entity<TableState<BrowserDelegate>>,
    cx: &App,
) -> Option<Vec<crate::platform::open_with::AppEntry>> {
    let path = app_target(table, cx);
    if path.is_empty() {
        return None;
    }
    let mime = crate::platform::open_with::mime_for_path(&path);
    let apps = crate::platform::open_with::apps_for_mime(
        &mime,
        &crate::platform::open_with::scan_desktop_apps(),
    );
    if apps.is_empty() { None } else { Some(apps) }
}

fn compress_target(table: &Entity<TableState<BrowserDelegate>>, format: &str, cx: &App) {
    let ids = table.read(cx).delegate().target_ids();
    let paths: Vec<PathBuf> = ids.iter().map(PathBuf::from).collect();
    if paths.is_empty() {
        return;
    }
    crate::files::operations::compress_paths(&paths, format);
}

pub(crate) fn open_terminal_in(dir: &Path) {
    let launch = |cmd: &str, args: &[&str]| {
        std::process::Command::new(cmd)
            .args(args)
            .arg(dir)
            .spawn()
            .is_ok()
    };
    if launch("xdg-terminal-exec", &[])
        || launch("x-terminal-emulator", &[])
        || launch("konsole", &["--workdir"])
        || launch("gnome-terminal", &["--working-directory"])
        || launch("xfce4-terminal", &["--working-directory"])
        || launch("kitty", &["--directory"])
        || launch("alacritty", &["--working-directory"])
        || launch("foot", &["--working-directory"])
        || launch("wezterm", &["start", "--cwd"])
    {
        return;
    }
    log::warn!("no terminal emulator found for {}", dir.display());
}
