use super::*;
use zbus::zvariant::{OwnedObjectPath, Str, Value};

fn fixture(bus: &str, removable: bool, label: &str, mounts: Vec<Vec<u8>>) -> ManagedObjects {
    let drive_path = OwnedObjectPath::try_from("/org/freedesktop/UDisks2/drives/test").unwrap();
    let block_path =
        OwnedObjectPath::try_from("/org/freedesktop/UDisks2/block_devices/sdb1").unwrap();
    let drive = HashMap::from([
        ("ConnectionBus".into(), OwnedValue::from(Str::from(bus))),
        ("Removable".into(), OwnedValue::from(removable)),
        ("Model".into(), OwnedValue::from(Str::from("External SSD"))),
    ]);
    let block = HashMap::from([
        (
            "Drive".into(),
            OwnedValue::try_from(Value::from(drive_path.clone())).unwrap(),
        ),
        ("IdLabel".into(), OwnedValue::from(Str::from(label))),
        ("Size".into(), OwnedValue::from(1024_u64)),
        (
            "PreferredDevice".into(),
            OwnedValue::try_from(Value::from(b"/dev/sdb1\0".to_vec())).unwrap(),
        ),
    ]);
    let fs = HashMap::from([(
        "MountPoints".into(),
        OwnedValue::try_from(Value::from(mounts)).unwrap(),
    )]);
    HashMap::from([
        (
            drive_path,
            HashMap::from([(DRIVE.try_into().unwrap(), drive)]),
        ),
        (
            block_path,
            HashMap::from([
                (BLOCK.try_into().unwrap(), block),
                (FILESYSTEM.try_into().unwrap(), fs),
            ]),
        ),
    ])
}

#[test]
fn usb_fixed_disks_and_removable_media_are_detected() {
    for (bus, removable, expected) in [
        ("usb", false, 1),
        ("thunderbolt", false, 1),
        ("sdio", true, 1),
        ("nvme", false, 0),
    ] {
        assert_eq!(
            volumes(&fixture(bus, removable, "资料", vec![])).len(),
            expected
        );
    }
}

#[test]
fn mounted_partitions_keep_labels_paths_and_stable_ids() {
    let objects = fixture(
        "usb",
        false,
        "备份",
        vec!["/run/media/user/备份 disk\0".as_bytes().to_vec()],
    );
    let devices = volumes(&objects);
    assert_eq!(devices.len(), 1);
    assert_eq!(devices[0].name, "备份");
    assert_eq!(
        devices[0].mount.as_deref(),
        Some("/run/media/user/备份 disk")
    );
    assert_eq!(devices[0].device, "/dev/sdb1");
    assert_eq!(devices[0].size, 1024);
    assert!(volumes(&ManagedObjects::new()).is_empty());
}

#[test]
fn ignored_and_non_filesystem_devices_are_excluded() {
    let mut objects = fixture("usb", false, "", vec![]);
    assert_eq!(volumes(&objects)[0].name, "External SSD (/dev/sdb1)");
    let block = objects
        .values_mut()
        .find(|v| v.contains_key(BLOCK))
        .unwrap();
    block
        .get_mut(BLOCK)
        .unwrap()
        .insert("HintIgnore".into(), true.into());
    assert!(volumes(&objects).is_empty());
    let block = objects
        .values_mut()
        .find(|v| v.contains_key(BLOCK))
        .unwrap();
    block.get_mut(BLOCK).unwrap().remove("HintIgnore");
    block.remove(FILESYSTEM);
    assert!(volumes(&objects).is_empty());
}

#[test]
fn invalid_mount_paths_are_not_opened() {
    for bytes in [
        vec![255, 0],
        b"relative\0".to_vec(),
        b"/path\0extra\0".to_vec(),
    ] {
        assert_eq!(
            volumes(&fixture("usb", false, "disk", vec![bytes]))[0].mount,
            None
        );
    }
}
