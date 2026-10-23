use std::collections::HashMap;
use std::path::Path;

use gpui_kit::component::{ActiveTheme, WindowExt};
use gpui_kit::{App, Global, ParentElement, Styled, Window, div, px};

#[derive(Default)]
struct DirSizeCache(HashMap<String, Option<u64>>);

impl Global for DirSizeCache {}

pub fn show_properties(paths: Vec<String>, window: &mut Window, cx: &mut App) {
    if !cx.has_global::<DirSizeCache>() {
        cx.set_global(DirSizeCache::default());
    }

    let dir_paths: Vec<String> = paths
        .iter()
        .filter(|p| {
            std::fs::symlink_metadata(Path::new(p))
                .map(|m| m.is_dir())
                .unwrap_or(false)
        })
        .cloned()
        .collect();

    let uncached: Vec<String> = dir_paths
        .iter()
        .filter(|p| {
            !cx.global::<DirSizeCache>()
                .0
                .get(*p)
                .is_some_and(|s| s.is_some())
        })
        .cloned()
        .collect();

    if !uncached.is_empty() {
        let bg = cx.background_executor().clone();
        cx.spawn(async move |cx| {
            for p in uncached {
                let value = p.clone();
                let size = bg
                    .spawn(async move { dir_size_bytes(Path::new(&value)) })
                    .await;
                cx.update(|cx| {
                    cx.global_mut::<DirSizeCache>().0.insert(p, size);
                    cx.refresh_windows();
                });
            }
        })
        .detach();
    }

    window.open_dialog(cx, move |dialog, _window, cx| {
        let rows = build_rows(&paths, cx);
        let muted = cx.theme().muted_foreground;

        let content = div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .children(rows.into_iter().map(|(k, v)| {
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(div().w(px(90.0)).text_xs().text_color(muted).child(k))
                    .child(div().text_sm().child(v))
            }));

        let footer = gpui_kit::component::button::Button::new("properties-close")
            .label("Close")
            .on_click(move |_e, window, cx| {
                window.close_dialog(cx);
            });

        dialog.title("Properties").child(content).footer(footer)
    });
}

fn build_rows(paths: &[String], cx: &App) -> Vec<(String, String)> {
    if paths.is_empty() {
        return vec![("(nothing selected)".to_string(), String::new())];
    }

    let mut rows = Vec::new();
    for (ix, path) in paths.iter().take(5).enumerate() {
        let p = Path::new(path);
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.clone());

        let meta = std::fs::symlink_metadata(p);
        let kind = match meta.as_ref() {
            Ok(m) if m.is_dir() => "Directory",
            Ok(m) if m.file_type().is_symlink() => "Symbolic link",
            Ok(_) => "File",
            Err(_) => "Unknown",
        };

        let size = meta
            .as_ref()
            .ok()
            .and_then(|m| {
                if m.is_dir() {
                    cx.global::<DirSizeCache>()
                        .0
                        .get(path)
                        .and_then(|size| size.map(crate::browser::format::human_bytes))
                        .or_else(|| Some("…".to_string()))
                } else {
                    Some(crate::browser::format::human_bytes(m.len()))
                }
            })
            .unwrap_or_else(|| "—".to_string());

        let modified = meta
            .as_ref()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| crate::browser::format::format_properties_datetime(d.as_secs()))
            .unwrap_or_else(|| "—".to_string());

        let perms = meta
            .as_ref()
            .map(crate::files::fs::format_perms)
            .unwrap_or_default();

        let location = p
            .parent()
            .map(|d| d.to_string_lossy().to_string())
            .unwrap_or_else(|| "—".to_string());

        if ix > 0 {
            rows.push(("".to_string(), String::new()));
        }
        rows.push(("Name".to_string(), name));
        rows.push(("Type".to_string(), kind.to_string()));
        rows.push(("Size".to_string(), size));
        rows.push(("Location".to_string(), location));
        rows.push(("Modified".to_string(), modified));
        rows.push(("Permissions".to_string(), perms));
    }

    if paths.len() > 5 {
        rows.push((String::new(), format!("… and {} more", paths.len() - 5)));
    }
    rows
}

fn dir_size_bytes(dir: &Path) -> Option<u64> {
    use std::collections::VecDeque;
    use std::os::unix::fs::MetadataExt;

    let mut total = 0u64;
    let mut queue = VecDeque::new();
    queue.push_back(std::path::PathBuf::from(dir));

    while let Some(path) = queue.pop_front() {
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        total += meta.blocks() * 512;
        if meta.is_dir()
            && let Ok(entries) = std::fs::read_dir(&path)
        {
            for entry in entries.flatten() {
                queue.push_back(entry.path());
            }
        }
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[gpui_kit::test]
    fn rows_for_single_file(cx: &mut gpui_kit::TestAppContext) {
        cx.update(|cx| {
            cx.set_global(DirSizeCache::default());
            let dir = std::env::temp_dir().join(format!("tern-prop-test-{}", std::process::id()));
            fs::create_dir_all(&dir).unwrap();
            let f = dir.join("a.txt");
            fs::write(&f, "hi").unwrap();
            let rows = build_rows(&[f.to_string_lossy().to_string()], cx);
            assert!(rows.iter().any(|(k, _)| k == "Name"));
            assert!(rows.iter().any(|(k, _)| k == "Size"));
            fs::remove_dir_all(dir).ok();
        });
    }

    #[test]
    fn dir_size_reports_bytes() {
        let dir = std::env::temp_dir().join(format!("tern-prop-size-{}", std::process::id()));
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("sub/f.txt"), b"0123456789").unwrap();
        let s = dir_size_bytes(&dir).unwrap();
        assert!(s > 0);
        fs::remove_dir_all(dir).ok();
    }
}
