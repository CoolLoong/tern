use std::sync::atomic::{AtomicBool, Ordering};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub fn copy_path(src: &Path, dst_dir: &Path, cancel: &AtomicBool) -> std::io::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "cancelled",
        ));
    }
    let name = src
        .file_name()
        .ok_or_else(|| std::io::Error::other("invalid source name"))?
        .to_string_lossy();
    let src = absolute_entry_path(src)?;
    let dst_dir = std::fs::canonicalize(dst_dir)?;
    if src == dst_dir {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "destination is source",
        ));
    }
    if std::fs::symlink_metadata(&src)?.is_dir() && dst_dir.starts_with(&src) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "destination is inside source",
        ));
    }
    copy_to(&src, &conflict_name(&dst_dir, &name), cancel)
}

fn copy_to(src: &Path, dst: &Path, cancel: &AtomicBool) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    if cancel.load(Ordering::Relaxed) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "cancelled",
        ));
    }
    let meta = std::fs::symlink_metadata(src)?;
    if meta.is_dir() {
        // Keep a partial directory private until its contents are copied.
        std::fs::DirBuilder::new().mode(0o700).create(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            copy_to(&entry.path(), &dst.join(entry.file_name()), cancel)?;
        }
        std::fs::set_permissions(
            dst,
            std::fs::Permissions::from_mode(meta.permissions().mode() & 0o777),
        )
    } else if meta.file_type().is_symlink() {
        std::os::unix::fs::symlink(std::fs::read_link(src)?, dst)
    } else {
        copy_file_buffered(src, dst, cancel)
    }
}

fn copy_file_buffered(src: &Path, dst: &Path, cancel: &AtomicBool) -> std::io::Result<()> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let mut reader = std::fs::File::open(src)?;
    let mode = reader.metadata()?.permissions().mode() & 0o777;
    let mut writer = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(dst)?;
    let mut buffer = [0; 64 * 1024];
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Interrupted,
                "cancelled",
            ));
        }
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            return Ok(());
        }
        writer.write_all(&buffer[..read])?;
    }
}

pub fn move_path(src: &Path, dst_dir: &Path, cancel: &AtomicBool) -> std::io::Result<()> {
    if cancel.load(Ordering::Relaxed) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Interrupted,
            "cancelled",
        ));
    }
    let name = src
        .file_name()
        .ok_or_else(|| std::io::Error::other("invalid source name"))?;
    let src = absolute_entry_path(src)?;
    let dst = std::fs::canonicalize(dst_dir)?.join(name);
    if src == dst {
        return std::fs::symlink_metadata(&src).map(|_| ());
    }
    if std::fs::symlink_metadata(&src)?.is_dir() && dst.starts_with(&src) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "destination is inside source",
        ));
    }
    match rename_without_replace(&src, &dst) {
        Ok(()) => Ok(()),
        Err(e) if e.raw_os_error() == Some(libc::EXDEV) => {
            copy_to(&src, &dst, cancel)?;
            remove_path(&src, true)
        }
        Err(e) => Err(e),
    }
}

/// Resolve parent aliases while preserving the selected entry itself, including
/// dangling symlinks. Canonicalizing the leaf would operate on its target.
fn absolute_entry_path(path: &Path) -> std::io::Result<PathBuf> {
    let name = path
        .file_name()
        .ok_or_else(|| std::io::Error::other("invalid file name"))?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    Ok(std::fs::canonicalize(parent)?.join(name))
}

/// Linux's atomic no-replace rename also protects dangling symlinks and avoids
/// the race between checking a destination and renaming over it.
fn rename_without_replace(src: &Path, dst: &Path) -> std::io::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let src = CString::new(src.as_os_str().as_bytes())?;
    let dst = CString::new(dst.as_os_str().as_bytes())?;
    // SAFETY: both C strings remain alive for the call, and AT_FDCWD resolves
    // relative paths against the current directory without borrowing an fd.
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            src.as_ptr(),
            libc::AT_FDCWD,
            dst.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

fn uid() -> u32 {
    unsafe { libc::getuid() }
}

/// The user's home trash. The XDG spec derives the default from `$HOME`, but
/// `$HOME` can be unset entirely (root, systemd units, bare containers).
/// Falling back to a per-uid directory under the temp dir keeps trashing
/// working there instead of failing outright.
pub fn trash_root() -> PathBuf {
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(data_home).join("Trash");
    }
    if let Some(home) = std::env::var_os("HOME").filter(|v| !v.is_empty()) {
        return PathBuf::from(home).join(".local/share/Trash");
    }
    std::env::temp_dir().join(format!("trash-{}", uid()))
}

/// Mount points, longest path first, so the first match is the most specific
/// mount containing a path.
fn mount_points() -> Vec<PathBuf> {
    let mut points: Vec<PathBuf> = std::fs::read_to_string("/proc/mounts")
        .map(|contents| {
            contents
                .lines()
                .filter_map(|line| line.split_whitespace().nth(1))
                .map(unescape_mount_path)
                .collect()
        })
        .unwrap_or_default();
    if !points.iter().any(|p| p == Path::new("/")) {
        points.push(PathBuf::from("/"));
    }
    points.sort_by_key(|path| std::cmp::Reverse(path.as_os_str().len()));
    points
}

/// `/proc/mounts` escapes space, tab, newline and backslash as octal.
fn unescape_mount_path(raw: &str) -> PathBuf {
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut ix = 0;
    while ix < bytes.len() {
        if bytes[ix] == b'\\'
            && let (Some(a), Some(b), Some(c)) =
                (bytes.get(ix + 1), bytes.get(ix + 2), bytes.get(ix + 3))
            && let (Some(a), Some(b), Some(c)) = (octal(*a), octal(*b), octal(*c))
        {
            out.push(a * 64 + b * 8 + c);
            ix += 4;
            continue;
        }
        out.push(bytes[ix]);
        ix += 1;
    }
    PathBuf::from(String::from_utf8_lossy(&out).into_owned())
}

fn octal(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'7' => Some(byte - b'0'),
        _ => None,
    }
}

fn mount_of(path: &Path, mount_points: &[PathBuf]) -> PathBuf {
    mount_points
        .iter()
        .find(|mount| path.starts_with(mount))
        .cloned()
        .unwrap_or_else(|| PathBuf::from("/"))
}

/// Trash location for a file, following the spec's ordering: the file's own
/// volume when it has one, otherwise the home trash.
///
/// Trashing on the same volume is a plain `rename`; routing everything to the
/// home trash would force a full copy for files on other volumes.
fn trash_root_for(path: &Path, mount_points: &[PathBuf]) -> PathBuf {
    let home_trash = trash_root();
    let mount = mount_of(path, mount_points);
    // Same volume as the home trash: the spec says use the home trash.
    if mount_of(&home_trash, mount_points) == mount {
        return home_trash;
    }

    let uid = uid();
    // Shared `/Trash` on the volume root is preferred, but it must already
    // exist and be owned by us — creating it would need root. Fall back to a
    // per-user `.Trash-<uid>`.
    let shared = mount.join(".Trash").join(uid.to_string());
    if shared.is_dir() {
        return shared;
    }
    mount.join(format!(".Trash-{uid}"))
}

/// Every trash location to consult: the home trash plus all per-volume ones.
pub fn trash_roots() -> Vec<PathBuf> {
    let mut roots = vec![trash_root()];
    let home = trash_root();
    for mount in mount_points() {
        let uid = uid();
        for candidate in [
            mount.join(".Trash").join(uid.to_string()),
            mount.join(format!(".Trash-{uid}")),
        ] {
            if candidate.is_dir() && candidate != home && !roots.contains(&candidate) {
                roots.push(candidate);
            }
        }
    }
    roots
}

fn percent_encode(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut out = String::new();
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

fn move_into(src: &Path, dst: &Path) -> std::io::Result<()> {
    match rename_without_replace(src, dst) {
        Ok(()) => return Ok(()),
        Err(error) if error.raw_os_error() == Some(libc::EXDEV) => {}
        Err(error) => return Err(error),
    }
    let cancel = AtomicBool::new(false);
    copy_to(src, dst, &cancel)?;
    remove_path(src, true)
}

fn trash_path(root: &Path, path: &Path) -> std::io::Result<()> {
    let files = root.join("files");
    let info = root.join("info");
    std::fs::create_dir_all(&files)?;
    std::fs::create_dir_all(&info)?;

    let name = path
        .file_name()
        .ok_or_else(|| std::io::Error::other("invalid file name"))?;
    let source = absolute_entry_path(path)?;

    let mut attempt = 0usize;
    loop {
        attempt += 1;
        let in_trash = if attempt == 1 {
            name.to_owned()
        } else {
            let mut n = name.to_owned();
            n.push(format!(".{attempt}"));
            n
        };
        let mut info_name = in_trash.clone();
        info_name.push(".trashinfo");
        let info_file = info.join(&info_name);

        match std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&info_file)
        {
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(err) => return Err(err),
            Ok(mut file) => {
                // Written before the move: a leftover info file with no payload
                // is harmless, the reverse is not.
                let date = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S");
                writeln!(file, "[Trash Info]")?;
                writeln!(file, "Path={}", percent_encode(&source))?;
                writeln!(file, "DeletionDate={date}")?;
                drop(file);
            }
        }

        match move_into(&source, &files.join(&in_trash)) {
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                let _ = std::fs::remove_file(&info_file);
                continue;
            }
            Err(err) => {
                let _ = std::fs::remove_file(&info_file);
                return Err(err);
            }
            Ok(()) => return Ok(()),
        }
    }
}

/// The trash location a payload lives in, derived from the path itself so
/// entries work regardless of which volume they were trashed on.
fn trash_root_of(payload: &Path) -> Option<PathBuf> {
    let dir = payload.parent()?;
    if dir.file_name()? != "files" {
        return None;
    }
    Some(dir.parent()?.to_path_buf())
}

/// Directory holding the trashed payloads. Browsable like any other directory.
pub fn trash_files_dir() -> PathBuf {
    trash_root().join("files")
}

/// The path a trashed entry was deleted from, read from its `.trashinfo`
/// sibling. `path` must point inside [`trash_files_dir`].
pub fn trashed_original(path: &Path) -> Option<String> {
    let root = trash_root_of(path)?;
    let file_name = path.file_name()?;
    let mut info_name = file_name.to_owned();
    info_name.push(".trashinfo");
    let info = root.join("info").join(info_name);
    let contents = std::fs::read_to_string(info).ok()?;
    let raw = contents
        .lines()
        .find_map(|line| line.strip_prefix("Path="))?;
    Some(percent_decode(raw.trim()))
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

/// Puts a trashed entry back where it came from. Refuses when something already
/// occupies that path rather than overwriting it.
pub fn restore_from_trash(path: &Path) -> std::io::Result<()> {
    let original = trashed_original(path)
        .ok_or_else(|| std::io::Error::other("no trash info for this entry"))?;
    let destination = PathBuf::from(&original);
    if let Some(parent) = destination.parent() {
        std::fs::create_dir_all(parent)?;
    }
    move_into(path, &destination)?;

    if let (Some(root), Some(name)) = (trash_root_of(path), path.file_name()) {
        let mut info_name = name.to_owned();
        info_name.push(".trashinfo");
        let _ = std::fs::remove_file(root.join("info").join(info_name));
    }
    Ok(())
}

/// Removes a trashed entry for good, info file included.
pub fn delete_permanently(path: &Path) -> std::io::Result<()> {
    let root = trash_root_of(path);
    let info_name = path.file_name().map(|name| {
        let mut info_name = name.to_owned();
        info_name.push(".trashinfo");
        info_name
    });
    remove_path(path, true)?;
    if let (Some(root), Some(info_name)) = (root, info_name) {
        let _ = std::fs::remove_file(root.join("info").join(info_name));
    }
    Ok(())
}

/// Empties the trash: every payload plus its `.trashinfo`. Returns the entries
/// that could not be removed.
pub fn empty_trash() -> Vec<PathBuf> {
    let mut failed = Vec::new();
    for root in trash_roots() {
        for sub in ["files", "info"] {
            let Ok(entries) = std::fs::read_dir(root.join(sub)) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if remove_path(&path, true).is_err() {
                    failed.push(path);
                }
            }
        }
    }
    failed
}

pub fn trash_paths(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mounts = mount_points();
    let home = trash_root();
    let mut failed = Vec::new();
    for path in paths {
        // The file's own volume first — a rename beats copying it across
        // filesystems. If that location cannot be used, fall back to the home
        // trash, which the spec allows when per-volume trash is unavailable.
        let preferred = trash_root_for(path, &mounts);
        let err = match trash_path(preferred.as_path(), path) {
            Ok(()) => continue,
            Err(err) if preferred != home => {
                log::debug!(
                    "per-volume trash at {preferred:?} failed for {}: {err:#}; using the home trash",
                    path.display()
                );
                trash_path(home.as_path(), path).err()
            }
            Err(err) => Some(err),
        };
        if let Some(err) = err {
            log::warn!("failed to trash {}: {err:#}", path.display());
            failed.push(path.clone());
        }
    }
    failed
}

fn remove_path(path: &Path, recursive: bool) -> std::io::Result<()> {
    let meta = std::fs::symlink_metadata(path)?;
    if meta.is_dir() && recursive {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

pub fn create_dir(parent: &str, name: &str) -> std::io::Result<PathBuf> {
    let name = valid_name(name)?;
    let path = Path::new(parent).join(name);
    std::fs::create_dir(&path)?;
    Ok(path)
}

pub fn create_file(parent: &str, name: &str) -> std::io::Result<PathBuf> {
    let path = Path::new(parent).join(valid_name(name)?);
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    Ok(path)
}

pub fn rename_path(src: &str, dst: &str) -> std::io::Result<()> {
    let destination = Path::new(dst);
    valid_name(
        destination
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or_default(),
    )?;
    let source = absolute_entry_path(Path::new(src))?;
    let destination = absolute_entry_path(destination)?;
    if source == destination {
        return std::fs::symlink_metadata(&source).map(|_| ());
    }
    rename_without_replace(&source, &destination)
}

pub(crate) fn valid_name(name: &str) -> std::io::Result<&str> {
    if name.is_empty()
        || matches!(name, "." | "..")
        || name.contains(['/', '\0'])
        || Path::new(name).components().count() != 1
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "invalid file name",
        ));
    }
    Ok(name)
}

pub fn conflict_name(base: &Path, name: &str) -> PathBuf {
    let mut candidate = base.join(name);
    let mut ix = 2;
    while std::fs::symlink_metadata(&candidate).is_ok() {
        let stem = Path::new(name)
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| name.to_string());
        let ext = Path::new(name)
            .extension()
            .map(|s| format!(".{}", s.to_string_lossy()))
            .unwrap_or_default();
        candidate = base.join(format!("{stem} ({ix}){ext}"));
        ix += 1;
    }
    candidate
}

pub fn compress_paths(paths: &[PathBuf], format: &str) {
    if paths.is_empty() {
        return;
    }
    let Some(first) = paths.first() else { return };
    let parent = first.parent().unwrap_or(Path::new("."));
    let base = first
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "archive".to_string());
    let target = parent.join(format!("{base}.{format}"));
    let target = conflict_name(
        parent,
        target
            .file_name()
            .unwrap_or_default()
            .to_str()
            .unwrap_or("archive"),
    );

    if let Err(err) = smol::block_on(compress_async(paths, &target, format)) {
        log::warn!("compress failed: {err:#}");
    }
}

async fn compress_async(paths: &[PathBuf], target: &Path, format: &str) -> anyhow::Result<()> {
    use smol::process::Command;
    let mut cmd = match format {
        "zip" => {
            let mut cmd = Command::new("zip");
            cmd.arg("-r").arg(target).args(paths);
            cmd
        }
        _ => {
            let mut cmd = Command::new("tar");
            cmd.arg("-czf").arg(target).args(paths);
            cmd
        }
    };
    let status = cmd.status().await?;
    if status.success() {
        Ok(())
    } else {
        Err(anyhow::anyhow!("compress failed with {status}"))
    }
}

#[cfg(test)]
mod tests;
