use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::files::model::{Entry, EntryKind};

/// Marks the synthetic "Recent" location. It is not a real directory, so it can
/// never collide with a filesystem path.
pub(crate) const RECENT_URI: &str = "recent://";

pub(crate) fn is_recent(uri: &str) -> bool {
    uri == RECENT_URI
}

fn xbel_path() -> Option<PathBuf> {
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(data_home).join("recently-used.xbel"));
    }
    let home = std::env::var_os("HOME").filter(|v| !v.is_empty())?;
    Some(PathBuf::from(home).join(".local/share/recently-used.xbel"))
}

/// One `<bookmark>` entry, in the order the file lists them.
struct Bookmark {
    uri: String,
    modified: Option<String>,
}

/// Minimal `recently-used.xbel` reader.
///
/// The document is a flat list of `<bookmark href="file://…" modified="…"/>`
/// elements. Rather than pull in an XML parser for three attributes, scan the
/// text — the format is machine-generated and stable.
#[cfg(test)]
fn parse_bookmarks(xml: &str) -> Vec<Bookmark> {
    parse_bookmarks_cancellable(xml, &crate::files::jobs::Cancellation::default())
        .unwrap_or_default()
}

fn parse_bookmarks_cancellable(
    xml: &str,
    cancel: &crate::files::jobs::Cancellation,
) -> std::io::Result<Vec<Bookmark>> {
    let mut out = Vec::new();
    for chunk in xml.split("<bookmark ").skip(1) {
        cancel.check()?;
        let tag = match chunk.split_once('>') {
            Some((tag, _)) => tag,
            None => continue,
        };
        let Some(uri) = attribute(tag, "href") else {
            continue;
        };
        out.push(Bookmark {
            uri: decode_entities(&uri),
            modified: attribute(tag, "modified").map(|v| decode_entities(&v)),
        });
    }
    cancel.check()?;
    Ok(out)
}

fn attribute(tag: &str, name: &str) -> Option<String> {
    let needle = format!("{name}=\"");
    let start = tag.find(&needle)? + needle.len();
    let rest = &tag[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

fn decode_entities(value: &str) -> String {
    value
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// `file:///home/x/a%20b` -> `/home/x/a b`. Returns `None` for other schemes
/// (http, trash, recent …), which this list cannot open as files.
fn file_uri_to_path(uri: &str) -> Option<String> {
    let rest = uri.strip_prefix("file://")?;
    // `file://host/path` is legal but a non-empty host means a remote file.
    let path = if let Some(path) = rest.strip_prefix('/') {
        format!("/{path}")
    } else if let Some((host, path)) = rest.split_once('/') {
        if host.is_empty() || host == "localhost" {
            format!("/{path}")
        } else {
            return None;
        }
    } else {
        return None;
    };
    Some(percent_decode(&path))
}

fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut ix = 0;
    while ix < bytes.len() {
        if bytes[ix] == b'%'
            && let (Some(hi), Some(lo)) = (bytes.get(ix + 1), bytes.get(ix + 2))
            && let (Some(hi), Some(lo)) = (hex_value(*hi), hex_value(*lo))
        {
            out.push(hi * 16 + lo);
            ix += 3;
            continue;
        }
        out.push(bytes[ix]);
        ix += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// Reads the recent-files list, most recently used first, de-duplicated by path
/// and filtered to entries that still exist.
pub(crate) fn read_entries(
    cancel: &crate::files::jobs::Cancellation,
) -> std::io::Result<Vec<Entry>> {
    cancel.check()?;
    let Some(path) = xbel_path() else {
        return Ok(Vec::new());
    };
    read_entries_from(&path, cancel)
}

pub(crate) fn read_entries_from(
    path: &Path,
    cancel: &crate::files::jobs::Cancellation,
) -> std::io::Result<Vec<Entry>> {
    cancel.check()?;
    let Ok(xml) = std::fs::read_to_string(path) else {
        return Ok(Vec::new());
    };
    entries_from_xml(&xml, cancel, |path| std::fs::symlink_metadata(path))
}

fn entries_from_xml(
    xml: &str,
    cancel: &crate::files::jobs::Cancellation,
    mut metadata: impl FnMut(&str) -> std::io::Result<std::fs::Metadata>,
) -> std::io::Result<Vec<Entry>> {
    let mut rows: Vec<(String, Option<u64>)> = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    for bookmark in parse_bookmarks_cancellable(xml, cancel)? {
        cancel.check()?;
        let Some(path) = file_uri_to_path(&bookmark.uri) else {
            continue;
        };
        let modified = bookmark.modified.as_deref().and_then(parse_timestamp);
        if let Some(&ix) = seen.get(&path) {
            // Same file listed twice: the newest timestamp wins.
            if modified > rows[ix].1 {
                rows[ix].1 = modified;
            }
            continue;
        }
        seen.insert(path.clone(), rows.len());
        rows.push((path, modified));
    }

    rows.sort_by_key(|row| std::cmp::Reverse(row.1));
    let mut entries = Vec::with_capacity(rows.len());
    for (path, modified) in rows {
        cancel.check()?;
        // Deduplicate before probing files. Ordinary files need one stat;
        // only symlinks also need a target check to exclude broken links.
        let meta = metadata(&path);
        cancel.check()?;
        let Ok(meta) = meta else { continue };
        if meta.file_type().is_symlink() && !Path::new(&path).exists() {
            continue;
        }
        cancel.check()?;
        let name = std::path::Path::new(&path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.clone());
        let kind = if meta.is_dir() {
            EntryKind::Dir
        } else if meta.file_type().is_symlink() {
            EntryKind::Symlink
        } else {
            EntryKind::File
        };
        let size = (!meta.is_dir()).then_some(meta.len());
        let perms = Some(crate::files::fs::format_perms(&meta));
        entries.push(Entry::new_with_meta(
            path, name, kind, size, modified, perms,
        ));
    }
    cancel.check()?;
    Ok(entries)
}

/// `2026-10-12T15:03:34Z` -> unix seconds. Anything unexpected yields `None` so
/// the entry still lists, just without a usable timestamp.
fn parse_timestamp(value: &str) -> Option<u64> {
    use chrono::{DateTime, NaiveDateTime, Utc};
    if let Ok(dt) = DateTime::parse_from_rfc3339(value) {
        return u64::try_from(dt.with_timezone(&Utc).timestamp()).ok();
    }
    for format in ["%Y-%m-%dT%H:%M:%S%.fZ", "%Y-%m-%dT%H:%M:%SZ"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(value, format) {
            return u64::try_from(naive.and_utc().timestamp()).ok();
        }
    }
    None
}

/// Records that `path` was just opened, the same way nemo does through
/// `gtk_recent_manager_add_full`.
///
/// Existing bookmarks for the file are replaced rather than appended to, so the
/// list does not grow by one entry per open. If the XBEL cannot be parsed it is
/// left untouched: clobbering a file other applications also write would lose
/// their history too.
pub(crate) fn record_opened(path: &Path, app_name: &str, app_exec: &str) {
    let Some(xbel) = xbel_path() else {
        return;
    };
    let uri = path_to_file_uri(path);
    let now = chrono::Local::now()
        .format("%Y-%m-%dT%H:%M:%S%.3fZ")
        .to_string();

    let existing = std::fs::read_to_string(&xbel).unwrap_or_else(|_| empty_document());
    let Some(updated) = upsert_bookmark(&existing, &uri, app_name, app_exec, &now) else {
        log::warn!("not recording recent file: {xbel:?} is not a writable XBEL document");
        return;
    };

    if let Some(parent) = xbel.parent()
        && let Err(err) = std::fs::create_dir_all(parent)
    {
        log::warn!("failed to create {parent:?}: {err:#}");
        return;
    }
    if let Err(err) = write_atomically(&xbel, &updated) {
        log::warn!("failed to record recent file in {xbel:?}: {err:#}");
    }
}

fn empty_document() -> String {
    "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<xbel version=\"1.0\" \
     xmlns:bookmark=\"http://www.freedesktop.org/standards/desktop-bookmarks\" \
     xmlns:mime=\"http://www.freedesktop.org/standards/shared-mime-info\">\n</xbel>\n"
        .to_string()
}

fn write_atomically(xbel: &Path, contents: &str) -> std::io::Result<()> {
    let tmp = xbel.with_extension("xbel.tern-tmp");
    std::fs::write(&tmp, contents)?;
    std::fs::rename(&tmp, xbel)
}

/// Replaces the `<bookmark>` for `uri` (dropping any previous copy) and inserts
/// the refreshed one first, which is the order the list is read back in.
/// Splits a document into (head, blocks, tail) where `blocks` are the
/// `<bookmark .../>` and `<bookmark>…</bookmark>` elements between the
/// opening tag and `</xbel>`.
fn document_parts(xml: &str) -> Option<(&str, Vec<&str>, &str)> {
    let open = xml.find("<xbel")?;
    let close = xml.rfind("</xbel>")?;
    let head_end = xml[open..].find('>')? + open + 1;

    let mut blocks = Vec::new();
    let mut rest = &xml[head_end..close];
    while let Some(start) = find_bookmark_start(rest) {
        let tag_end = rest[start..].find('>')? + start;
        let self_closing = rest[start..tag_end].trim_end().ends_with('/');
        let end = if self_closing {
            tag_end + 1
        } else {
            rest[tag_end + 1..].find("</bookmark>")? + tag_end + 1 + "</bookmark>".len()
        };
        blocks.push(&rest[start..end]);
        rest = &rest[end..];
    }
    Some((&xml[..head_end], blocks, &xml[close..]))
}

fn find_bookmark_start(xml: &str) -> Option<usize> {
    let mut offset = 0;
    while let Some(relative) = xml[offset..].find("<bookmark") {
        let start = offset + relative;
        let next = xml.as_bytes().get(start + "<bookmark".len()).copied()?;
        if next == b'>' || next.is_ascii_whitespace() {
            return Some(start);
        }
        offset = start + "<bookmark".len();
    }
    None
}

fn block_has_href(block: &str, uri: &str) -> bool {
    block.contains(&format!("href=\"{uri}\""))
}

/// Drops the bookmark for `path`. Rewrites only when something was actually
/// removed, so a miss never touches a file other applications also write.
pub(crate) fn forget(path: &Path) {
    let Some(xbel) = xbel_path() else {
        return;
    };
    let Ok(xml) = std::fs::read_to_string(&xbel) else {
        return;
    };
    let Some(updated) = remove_bookmark(&xml, &path_to_file_uri(path)) else {
        return;
    };
    if let Err(err) = write_atomically(&xbel, &updated) {
        log::warn!("failed to update {xbel:?}: {err:#}");
    }
}

fn remove_bookmark(xml: &str, uri: &str) -> Option<String> {
    let (head, blocks, tail) = document_parts(xml)?;
    if !blocks.iter().any(|block| block_has_href(block, uri)) {
        return None;
    }
    let mut out = String::with_capacity(xml.len());
    out.push_str(head);
    out.push('\n');
    for block in blocks {
        if !block_has_href(block, uri) {
            out.push_str(block);
            out.push('\n');
        }
    }
    out.push_str(tail);
    Some(out)
}

fn upsert_bookmark(
    xml: &str,
    uri: &str,
    app_name: &str,
    app_exec: &str,
    now: &str,
) -> Option<String> {
    let (head, blocks, tail) = document_parts(xml)?;

    let entry = format!(
        "  <bookmark href=\"{uri}\" added=\"{now}\" modified=\"{now}\" visited=\"{now}\">\n    \
         <info><metadata owner=\"http://freedesktop.org\">\n      \
         <bookmark:applications><bookmark:application name=\"{}\" exec=\"{}\" modified=\"{now}\" count=\"1\"/></bookmark:applications>\n    \
         </metadata></info>\n  </bookmark>\n",
        escape(app_name),
        escape(app_exec),
    );

    let mut out = String::with_capacity(xml.len() + entry.len());
    out.push_str(head);
    out.push('\n');
    out.push_str(&entry);
    for block in blocks {
        if !block_has_href(block, uri) {
            out.push_str(block);
            out.push('\n');
        }
    }
    out.push_str(tail);
    Some(out)
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// `/home/u/a b.txt` -> `file:///home/u/a%20b.txt`
fn path_to_file_uri(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut out = String::from("file://");
    for byte in path.as_os_str().as_bytes() {
        let safe = matches!(
            byte,
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'.' | b'-' | b'_' | b'~'
        );
        if safe {
            out.push(*byte as char);
        } else {
            out.push_str(&format!("%{byte:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_deduplicates_before_metadata_and_keeps_recent_timestamps() {
        let root = std::env::temp_dir().join(format!("tern-recent-listing-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("中文 文件.txt");
        std::fs::write(&file, b"test").unwrap();
        let uri = path_to_file_uri(&file);
        let xml = format!(
            "<xbel><bookmark href=\"{uri}\" modified=\"2020-01-01T00:00:00Z\"/><bookmark href=\"{uri}\" modified=\"2026-01-01T00:00:00Z\"/></xbel>"
        );
        let cancel = crate::files::jobs::Cancellation::default();
        let mut calls = 0;
        let rows = entries_from_xml(&xml, &cancel, |path| {
            calls += 1;
            std::fs::symlink_metadata(path)
        })
        .unwrap();
        assert_eq!(calls, 1);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "中文 文件.txt");
        assert_eq!(rows[0].size, Some(4));
        assert_eq!(rows[0].modified, parse_timestamp("2026-01-01T00:00:00Z"));
        let xbel = root.join("recent.xbel");
        std::fs::write(&xbel, &xml).unwrap();
        assert_eq!(read_entries_from(&xbel, &cancel).unwrap(), rows);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn listing_cancellation_stops_remaining_file_checks() {
        let cancel = crate::files::jobs::Cancellation::default();
        let xml = "<xbel><bookmark href=\"file:///a\"/><bookmark href=\"file:///b\"/></xbel>";
        let mut calls = 0;
        let result = entries_from_xml(xml, &cancel, |_| {
            calls += 1;
            cancel.cancel();
            Err(std::io::Error::from(std::io::ErrorKind::NotFound))
        });
        assert_eq!(calls, 1);
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::Interrupted);
        assert_eq!(
            read_entries_from(Path::new("/not-opened"), &cancel)
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::Interrupted
        );
    }

    #[test]
    fn parses_bookmarks_and_decodes_paths() {
        let xml = r#"<?xml version="1.0"?>
<xbel><bookmark href="file:///home/u/a%20b.txt" added="2026-01-01T00:00:00Z" modified="2026-01-02T03:04:05Z" visited="2026-01-02T03:04:05Z"/><bookmark href="file:///tmp/x" modified="2026-01-03T00:00:00Z"/></xbel>"#;
        let bookmarks = parse_bookmarks(xml);
        assert_eq!(bookmarks.len(), 2);
        assert_eq!(
            file_uri_to_path(&bookmarks[0].uri).as_deref(),
            Some("/home/u/a b.txt")
        );
        assert_eq!(
            parse_timestamp(bookmarks[1].modified.as_deref().unwrap()),
            Some(1767398400)
        );
    }

    #[test]
    fn upsert_replaces_existing_and_puts_it_first() {
        let xml = "<?xml version=\"1.0\"?>\n<xbel version=\"1.0\">\n  <bookmark href=\"file:///a\" modified=\"2020-01-01T00:00:00Z\"/>\n  <bookmark href=\"file:///b\" modified=\"2020-01-01T00:00:00Z\"><info/></bookmark>\n</xbel>\n";
        let out = upsert_bookmark(xml, "file:///b", "tern", "xdg-open", "2026-01-01T00:00:00Z")
            .expect("valid xbel");
        assert_eq!(
            out.matches("href=\"file:///b\"").count(),
            1,
            "b must appear once"
        );
        assert!(
            out.contains("href=\"file:///a\""),
            "unrelated bookmark survives"
        );
        let b = out.find("file:///b").unwrap();
        let a = out.find("file:///a").unwrap();
        assert!(b < a, "the re-opened file moves to the front");
        assert_eq!(parse_bookmarks(&out).len(), 2);
    }

    #[test]
    fn remove_drops_only_the_named_bookmark() {
        let xml = "<?xml version=\"1.0\"?>\n<xbel version=\"1.0\">\n  <bookmark href=\"file:///a\" modified=\"2020-01-01T00:00:00Z\"/>\n  <bookmark href=\"file:///b\" modified=\"2020-01-01T00:00:00Z\"><info/></bookmark>\n</xbel>\n";
        let out = remove_bookmark(xml, "file:///a").expect("a is present");
        assert!(!out.contains("file:///a"));
        assert!(out.contains("file:///b"));
        assert_eq!(parse_bookmarks(&out).len(), 1);

        assert!(
            remove_bookmark(xml, "file:///missing").is_none(),
            "no-op stays untouched"
        );
    }

    #[test]
    fn path_uri_round_trip() {
        let uri = path_to_file_uri(Path::new("/home/u/a b.txt"));
        assert_eq!(uri, "file:///home/u/a%20b.txt");
        assert_eq!(file_uri_to_path(&uri).as_deref(), Some("/home/u/a b.txt"));
    }

    #[test]
    fn ignores_non_file_schemes() {
        assert_eq!(file_uri_to_path("http://example.com/x"), None);
        assert_eq!(file_uri_to_path("trash:///x"), None);
        assert_eq!(
            file_uri_to_path("file:///etc/hosts").as_deref(),
            Some("/etc/hosts")
        );
    }
}
