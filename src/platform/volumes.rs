use std::collections::HashMap;
use zbus::{
    blocking::{Connection, Proxy},
    fdo::ManagedObjects,
    zvariant::OwnedValue,
};

const SERVICE: &str = "org.freedesktop.UDisks2";
const BLOCK: &str = "org.freedesktop.UDisks2.Block";
const DRIVE: &str = "org.freedesktop.UDisks2.Drive";
const FILESYSTEM: &str = "org.freedesktop.UDisks2.Filesystem";
type Properties = HashMap<String, OwnedValue>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Volume {
    pub id: String,
    pub name: String,
    pub device: String,
    pub size: u64,
    pub mount: Option<String>,
}

pub(crate) trait VolumeBackend: Send + Sync {
    fn list(&self) -> Result<Vec<Volume>, String>;
    fn mount(&self, id: &str) -> Result<String, String>;
    fn unmount(&self, id: &str) -> Result<(), String>;
}

pub(crate) struct SystemVolumes;

fn property<T: TryFrom<OwnedValue>>(props: &Properties, key: &str) -> Option<T> {
    T::try_from(props.get(key)?.try_clone().ok()?).ok()
}

fn byte_path(bytes: Vec<u8>) -> Option<String> {
    let bytes = bytes.strip_suffix(&[0]).unwrap_or(&bytes);
    let path = std::str::from_utf8(bytes).ok()?;
    (path.starts_with('/') && !path.contains('\0')).then(|| path.to_owned())
}

fn volumes(objects: &ManagedObjects) -> Vec<Volume> {
    let mut result = Vec::new();
    for (path, interfaces) in objects {
        let Some(block) = interfaces.get(BLOCK) else {
            continue;
        };
        let Some(filesystem) = interfaces.get(FILESYSTEM) else {
            continue;
        };
        if property::<bool>(block, "HintIgnore").unwrap_or(false) {
            continue;
        }
        let Some(drive_path) = property::<zbus::zvariant::OwnedObjectPath>(block, "Drive") else {
            continue;
        };
        let Some(drive) = objects.get(&drive_path).and_then(|item| item.get(DRIVE)) else {
            continue;
        };
        let bus = property::<String>(drive, "ConnectionBus").unwrap_or_default();
        if !matches!(bus.as_str(), "usb" | "firewire" | "thunderbolt")
            && !property::<bool>(drive, "Removable").unwrap_or(false)
            && !property::<bool>(drive, "MediaRemovable").unwrap_or(false)
        {
            continue;
        }
        let Some(device) = property::<Vec<u8>>(block, "PreferredDevice").and_then(byte_path) else {
            continue;
        };
        let label = property::<String>(block, "IdLabel").unwrap_or_default();
        let name = if label.is_empty() {
            let model = property::<String>(drive, "Model").unwrap_or_default();
            if model.trim().is_empty() {
                device.clone()
            } else {
                format!("{} ({})", model.trim(), device)
            }
        } else {
            label
        };
        let mount = property::<Vec<Vec<u8>>>(filesystem, "MountPoints")
            .unwrap_or_default()
            .into_iter()
            .find_map(byte_path);
        result.push(Volume {
            id: path.to_string(),
            name,
            device,
            size: property::<u64>(block, "Size").unwrap_or(0),
            mount,
        });
    }
    result.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    result
}

impl VolumeBackend for SystemVolumes {
    fn list(&self) -> Result<Vec<Volume>, String> {
        let connection = Connection::system().map_err(|e| e.to_string())?;
        let proxy = Proxy::new(
            &connection,
            SERVICE,
            "/org/freedesktop/UDisks2",
            "org.freedesktop.DBus.ObjectManager",
        )
        .map_err(|e| e.to_string())?;
        let objects: ManagedObjects = proxy
            .call("GetManagedObjects", &())
            .map_err(|e| e.to_string())?;
        Ok(volumes(&objects))
    }

    fn mount(&self, id: &str) -> Result<String, String> {
        let volume = self
            .list()?
            .into_iter()
            .find(|v| v.id == id)
            .ok_or_else(|| "Device is no longer available".to_string())?;
        if let Some(path) = volume.mount {
            return Ok(path);
        }
        let connection = Connection::system().map_err(|e| e.to_string())?;
        let proxy = Proxy::new(&connection, SERVICE, id, FILESYSTEM).map_err(|e| e.to_string())?;
        proxy
            .call("Mount", &(HashMap::<&str, zbus::zvariant::Value>::new(),))
            .map_err(|e| e.to_string())
    }

    fn unmount(&self, id: &str) -> Result<(), String> {
        let connection = Connection::system().map_err(|e| e.to_string())?;
        let proxy = Proxy::new(&connection, SERVICE, id, FILESYSTEM).map_err(|e| e.to_string())?;
        proxy
            .call("Unmount", &(HashMap::<&str, zbus::zvariant::Value>::new(),))
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests;
