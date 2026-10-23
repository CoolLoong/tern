#[test]
fn mount_paths_are_unescaped_and_sorted_longest_first() {
    assert_eq!(
        unescape_mount_path("/media/my\\040disk"),
        PathBuf::from("/media/my disk")
    );
    assert_eq!(
        unescape_mount_path("/mnt/plain"),
        PathBuf::from("/mnt/plain")
    );

    let mounts = vec![
        PathBuf::from("/media/usb"),
        PathBuf::from("/media"),
        PathBuf::from("/"),
    ];
    // Longest match wins, so a file on the usb stick resolves to its mount.
    assert_eq!(
        mount_of(Path::new("/media/usb/photo.jpg"), &mounts),
        PathBuf::from("/media/usb")
    );
    assert_eq!(
        mount_of(Path::new("/etc/hosts"), &mounts),
        PathBuf::from("/")
    );
}

#[test]
fn payload_root_is_derived_from_the_path() {
    assert_eq!(
        trash_root_of(Path::new("/home/u/.local/share/Trash/files/a.txt")),
        Some(PathBuf::from("/home/u/.local/share/Trash"))
    );
    assert_eq!(
        trash_root_of(Path::new("/media/usb/.Trash-1000/files/a.txt")),
        Some(PathBuf::from("/media/usb/.Trash-1000"))
    );
    // Not inside a trash payload directory.
    assert_eq!(trash_root_of(Path::new("/home/u/a.txt")), None);
}

use super::*;
use std::fs;

fn tempdir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "tern-ops-{label}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn copy_file_works() {
    let src = tempdir("copyfile");
    let a = src.join("a.txt");
    fs::write(&a, "hello").unwrap();
    let dst = tempdir("copydst");
    copy_path(&a, &dst, &AtomicBool::new(false)).unwrap();
    assert_eq!(fs::read(dst.join("a.txt")).unwrap(), b"hello");
    fs::remove_dir_all(src).ok();
    fs::remove_dir_all(dst).ok();
}

#[test]
fn copying_private_files_and_directories_does_not_widen_access() {
    use std::os::unix::fs::PermissionsExt;
    let root = tempdir("private-copy");
    let source = root.join("private");
    fs::create_dir(&source).unwrap();
    fs::set_permissions(&source, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(source.join("secret.txt"), "private fixture").unwrap();
    fs::set_permissions(source.join("secret.txt"), fs::Permissions::from_mode(0o600)).unwrap();
    let destination = root.join("destination");
    fs::create_dir(&destination).unwrap();
    copy_path(&source, &destination, &AtomicBool::new(false)).unwrap();
    assert_eq!(
        fs::metadata(destination.join("private"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(
        fs::metadata(destination.join("private/secret.txt"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn copy_dir_tree_works() {
    let src = tempdir("copydir");
    fs::create_dir_all(src.join("sub")).unwrap();
    fs::write(src.join("sub/x.txt"), "x").unwrap();
    let dst = tempdir("copydirtree");
    copy_path(&src, &dst, &AtomicBool::new(false)).unwrap();
    let name = src.file_name().unwrap();
    assert_eq!(fs::read(dst.join(name).join("sub/x.txt")).unwrap(), b"x");
    fs::remove_dir_all(src).ok();
    fs::remove_dir_all(dst).ok();
}

#[test]
fn move_rename_works() {
    let src = tempdir("move");
    let a = src.join("m.txt");
    fs::write(&a, "m").unwrap();
    let dst = tempdir("movedst");
    move_path(&a, &dst, &AtomicBool::new(false)).unwrap();
    assert!(!a.exists());
    assert_eq!(fs::read(dst.join("m.txt")).unwrap(), b"m");
    fs::remove_dir_all(src).ok();
    fs::remove_dir_all(dst).ok();
}

#[test]
fn move_and_rename_refuse_existing_files_and_dangling_links() {
    let root = tempdir("no-overwrite");
    let dst = root.join("destination");
    fs::create_dir(&dst).unwrap();
    let source = root.join("keep.txt");
    let occupied = dst.join("keep.txt");
    fs::write(&source, "source").unwrap();
    fs::write(&occupied, "destination").unwrap();
    assert_eq!(
        move_path(&source, &dst, &AtomicBool::new(false))
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::AlreadyExists
    );
    assert_eq!(
        rename_path(source.to_str().unwrap(), occupied.to_str().unwrap())
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::AlreadyExists
    );
    assert_eq!(fs::read(&source).unwrap(), b"source");
    assert_eq!(fs::read(&occupied).unwrap(), b"destination");
    fs::remove_file(&occupied).unwrap();
    std::os::unix::fs::symlink("missing", &occupied).unwrap();
    assert_eq!(
        move_path(&source, &dst, &AtomicBool::new(false))
            .unwrap_err()
            .kind(),
        std::io::ErrorKind::AlreadyExists
    );
    assert_eq!(fs::read_link(&occupied).unwrap(), Path::new("missing"));
    assert_eq!(fs::read(&source).unwrap(), b"source");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn copying_symlinks_preserves_the_link_even_when_broken() {
    let root = tempdir("copy-symlinks");
    let dst = root.join("destination");
    fs::create_dir(&dst).unwrap();
    fs::write(root.join("target"), "keep").unwrap();
    for (name, target) in [("link", "target"), ("broken", "missing")] {
        let source = root.join(name);
        std::os::unix::fs::symlink(target, &source).unwrap();
        copy_path(&source, &dst, &AtomicBool::new(false)).unwrap();
        assert_eq!(fs::read_link(dst.join(name)).unwrap(), Path::new(target));
        // A dangling destination is occupied, so copying again gets a new name.
        copy_path(&source, &dst, &AtomicBool::new(false)).unwrap();
        assert_eq!(
            fs::read_link(dst.join(format!("{name} (2)"))).unwrap(),
            Path::new(target)
        );
    }
    assert_eq!(fs::read(root.join("target")).unwrap(), b"keep");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn trash_and_restore_operate_on_the_link_without_overwriting() {
    let root = tempdir("trash-symlink");
    let trash = root.join("trash");
    fs::create_dir_all(trash.join("files")).unwrap();
    fs::create_dir_all(trash.join("info")).unwrap();
    // Preserve a previous payload and its metadata when names collide.
    fs::write(trash.join("files/link"), "older payload").unwrap();
    fs::write(trash.join("info/link.trashinfo"), "older metadata").unwrap();
    fs::write(root.join("target"), "keep target").unwrap();
    let link = root.join("link");
    std::os::unix::fs::symlink("target", &link).unwrap();
    trash_path(&trash, &link).unwrap();
    let payload = trash.join("files/link.2");
    assert_eq!(fs::read_link(&payload).unwrap(), Path::new("target"));
    assert!(fs::symlink_metadata(&link).is_err());
    assert_eq!(fs::read(root.join("target")).unwrap(), b"keep target");
    assert_eq!(
        fs::read(trash.join("info/link.trashinfo")).unwrap(),
        b"older metadata"
    );
    std::os::unix::fs::symlink("missing", &link).unwrap();
    assert_eq!(
        restore_from_trash(&payload).unwrap_err().kind(),
        std::io::ErrorKind::AlreadyExists
    );
    assert_eq!(fs::read_link(&link).unwrap(), Path::new("missing"));
    fs::remove_file(&link).unwrap();
    restore_from_trash(&payload).unwrap();
    assert_eq!(fs::read_link(&link).unwrap(), Path::new("target"));
    assert_eq!(
        fs::read(trash.join("files/link")).unwrap(),
        b"older payload"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn cancel_aborts_copy() {
    let src = tempdir("cancel");
    let a = src.join("big.bin");
    fs::write(&a, vec![0u8; 1024]).unwrap();
    let dst = tempdir("canceldst");
    let cancel = AtomicBool::new(true);
    assert!(copy_path(&a, &dst, &cancel).is_err());
    fs::remove_dir_all(src).ok();
    fs::remove_dir_all(dst).ok();
}

#[test]
fn conflict_generates_alt_name() {
    let base = tempdir("conflict");
    fs::write(base.join("f.txt"), "1").unwrap();
    fs::write(base.join("f (2).txt"), "2").unwrap();
    let name = conflict_name(&base, "f.txt");
    assert_eq!(name.file_name().unwrap().to_string_lossy(), "f (3).txt");
    fs::remove_dir_all(base).ok();
}

#[test]
fn create_dir_and_rename() {
    let base = tempdir("crud");
    let created = create_dir(base.to_string_lossy().as_ref(), "newdir").unwrap();
    assert!(created.is_dir());
    rename_path(
        created.to_string_lossy().as_ref(),
        base.join("renamed").to_string_lossy().as_ref(),
    )
    .unwrap();
    assert!(base.join("renamed").is_dir());
    fs::remove_dir_all(base).ok();
}

#[test]
fn new_file_refuses_collisions_and_names_outside_its_directory() {
    let root = tempdir("new-file");
    let parent = root.to_str().unwrap();
    let path = create_file(parent, "new.txt").unwrap();
    fs::write(&path, "keep").unwrap();
    assert_eq!(
        create_file(parent, "new.txt").unwrap_err().kind(),
        std::io::ErrorKind::AlreadyExists
    );
    for name in [
        "../escape.txt",
        "/escape.txt",
        "sub/file.txt",
        "folder/",
        "..",
        "bad\0name",
    ] {
        assert_eq!(
            create_file(parent, name).unwrap_err().kind(),
            std::io::ErrorKind::InvalidInput
        );
        assert_eq!(
            create_dir(parent, name).unwrap_err().kind(),
            std::io::ErrorKind::InvalidInput
        );
    }
    assert_eq!(fs::read(path).unwrap(), b"keep");
    fs::remove_dir_all(root).unwrap();
}
