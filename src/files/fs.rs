use std::path::Path;

use crate::files::model::{Entry, EntryKind};

#[cfg(test)]
pub(crate) fn read_dir(dir: &str) -> std::io::Result<Vec<Entry>> {
    read_dir_cancellable(dir, &crate::files::jobs::Cancellation::default())
}

pub(crate) fn read_dir_cancellable(
    dir: &str,
    cancel: &crate::files::jobs::Cancellation,
) -> std::io::Result<Vec<Entry>> {
    cancel.check()?;
    collect_entries(std::fs::read_dir(dir)?, cancel, |entry| {
        entry_with_metadata(entry, cancel)
    })
}

fn collect_entries(
    mut entries: impl Iterator<Item = std::io::Result<std::fs::DirEntry>>,
    cancel: &crate::files::jobs::Cancellation,
    mut read: impl FnMut(std::fs::DirEntry) -> std::io::Result<Entry>,
) -> std::io::Result<Vec<Entry>> {
    let mut out = Vec::new();
    loop {
        // An in-flight OS call cannot be interrupted, but no further entries
        // or metadata are requested once that call returns after cancellation.
        cancel.check()?;
        let Some(entry) = entries.next() else { break };
        let entry = entry?;
        cancel.check()?;
        let entry = read(entry)?;
        cancel.check()?;
        out.push(entry);
    }
    Ok(out)
}

fn entry_with_metadata(
    entry: std::fs::DirEntry,
    cancel: &crate::files::jobs::Cancellation,
) -> std::io::Result<Entry> {
    let name = entry.file_name().to_string_lossy().to_string();
    let path = entry.path();
    cancel.check()?;
    let ft = entry.file_type()?;
    let kind = if ft.is_dir() {
        EntryKind::Dir
    } else if ft.is_symlink() {
        EntryKind::Symlink
    } else if ft.is_file() {
        EntryKind::File
    } else {
        EntryKind::Other
    };

    cancel.check()?;
    let meta = entry.metadata().ok();
    let size = if kind == EntryKind::Dir {
        None
    } else {
        meta.as_ref().map(|m| m.len())
    };
    let modified = meta
        .as_ref()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    let perms = meta.as_ref().map(format_perms);

    Ok(Entry::new_with_meta(
        path.to_string_lossy().to_string(),
        name,
        kind,
        size,
        modified,
        perms,
    ))
}

pub(crate) fn parent_dir(path: &str) -> Option<String> {
    if path.is_empty() || path == "/" {
        return None;
    }
    // Synthetic locations have no parent on disk.
    if crate::files::recent::is_recent(path) {
        return None;
    }
    Path::new(path)
        .parent()
        .map(|p| p.to_string_lossy().to_string())
}

pub(crate) fn is_hidden_name(name: &str) -> bool {
    name.starts_with('.') && name != "." && name != ".."
}

pub(crate) fn format_perms(meta: &std::fs::Metadata) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = meta.permissions().mode();
        let mut out = String::with_capacity(11);
        for (bit, ch) in [
            (0o400, 'r'),
            (0o200, 'w'),
            (0o100, 'x'),
            (0o040, 'r'),
            (0o020, 'w'),
            (0o010, 'x'),
            (0o004, 'r'),
            (0o002, 'w'),
            (0o001, 'x'),
        ] {
            out.push(if mode & bit != 0 { ch } else { '-' });
        }
        out
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
        String::new()
    }
}

pub(crate) fn read_bytes_limit(path: &str, max_bytes: usize) -> std::io::Result<(Vec<u8>, bool)> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut out = Vec::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            return Ok((out, false));
        }
        out.extend_from_slice(&buf[..n]);
        if out.len() > max_bytes {
            return Ok((out, true));
        }
    }
}

pub(crate) fn join(parent: &str, name: &str) -> String {
    if parent.is_empty() {
        return name.to_string();
    }
    if parent.ends_with('/') {
        format!("{parent}{name}")
    } else {
        format!("{parent}/{name}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn read_dir_maps_entries() {
        let dir = std::env::temp_dir().join(format!("tern-fs-test-{}", std::process::id()));
        fs::create_dir_all(dir.join("sub")).unwrap();
        fs::write(dir.join("file.txt"), b"hello").unwrap();
        fs::write(dir.join(".hidden"), b"x").unwrap();

        let entries = read_dir(dir.to_string_lossy().as_ref()).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"sub"));
        assert!(names.contains(&"file.txt"));
        assert!(names.contains(&".hidden"));

        let sub = entries.iter().find(|e| e.name == "sub").unwrap();
        assert_eq!(sub.kind, EntryKind::Dir);
        assert_eq!(sub.size, None);

        let file = entries.iter().find(|e| e.name == "file.txt").unwrap();
        assert_eq!(file.kind, EntryKind::File);
        assert_eq!(file.size, Some(5));
        assert!(file.perms.is_some());

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn parent_and_hidden_helpers() {
        assert_eq!(parent_dir("/a/b"), Some("/a".to_string()));
        assert_eq!(parent_dir("/"), None);
        assert!(is_hidden_name(".git"));
        assert!(!is_hidden_name("git"));
        assert_eq!(join("/a", "b"), "/a/b");
        assert_eq!(join("/a/", "b"), "/a/b");
    }

    #[test]
    fn listing_cancellation_stops_before_the_next_entry_or_metadata_request() {
        let dir = std::env::temp_dir().join(format!("tern-cancel-read-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["one", "two", "three"] {
            std::fs::write(dir.join(name), b"data").unwrap();
        }
        let token = crate::files::jobs::Cancellation::default();
        let worker_token = token.clone();
        let (started_tx, started_rx) = std::sync::mpsc::channel();
        let (resume_tx, resume_rx) = std::sync::mpsc::channel();
        let worker_dir = dir.clone();
        let worker = std::thread::spawn(move || {
            let mut read_count = 0;
            let mut next_count = 0;
            let entries = std::fs::read_dir(&worker_dir)
                .unwrap()
                .inspect(|_| next_count += 1);
            let result = collect_entries(entries, &worker_token, |entry| {
                read_count += 1;
                let value = entry_with_metadata(entry, &worker_token)?;
                started_tx.send(()).unwrap();
                resume_rx
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap();
                Ok(value)
            });
            (result, read_count, next_count)
        });
        started_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        token.cancel();
        resume_tx.send(()).unwrap();
        let (result, reads, nexts) = worker.join().unwrap();
        assert_eq!(result.unwrap_err().kind(), std::io::ErrorKind::Interrupted);
        assert_eq!((reads, nexts), (1, 1));
        assert_eq!(
            read_dir_cancellable("/not-opened", &token)
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::Interrupted
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
