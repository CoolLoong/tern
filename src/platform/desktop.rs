use std::ffi::OsString;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct OpenTarget {
    pub(crate) directory: String,
    pub(crate) selected: Vec<String>,
}

fn local_path(value: OsString) -> Result<PathBuf> {
    let path = if let Some(value) = value.to_str().filter(|value| value.contains("://")) {
        url::Url::parse(value)?
            .to_file_path()
            .map_err(|_| anyhow::anyhow!("Only local file:// URLs are supported"))?
    } else {
        PathBuf::from(value)
    };
    let path = std::path::absolute(path)?;
    std::fs::symlink_metadata(&path).with_context(|| format!("Cannot open {}", path.display()))?;
    Ok(path)
}

pub(crate) fn parse_arguments(
    arguments: impl IntoIterator<Item = OsString>,
) -> Result<Vec<OpenTarget>> {
    let mut select = false;
    let mut options = true;
    let mut paths = Vec::new();
    for argument in arguments {
        if options && argument == "--" {
            options = false;
        } else if options && argument == "--select" {
            select = true;
        } else if options && argument.as_encoded_bytes().starts_with(b"-") {
            bail!("Unknown option: {}", argument.to_string_lossy());
        } else {
            paths.push(local_path(argument)?);
        }
    }
    let mut targets: Vec<OpenTarget> = Vec::new();
    for path in paths {
        let selecting = select || !path.is_dir();
        let directory = if selecting {
            path.parent().unwrap_or(Path::new("/"))
        } else {
            &path
        }
        .canonicalize()?
        .to_string_lossy()
        .into_owned();
        let selected = selecting.then(|| {
            Path::new(&directory)
                .join(path.file_name().unwrap_or_default())
                .to_string_lossy()
                .into_owned()
        });
        let index = targets
            .iter()
            .position(|target| target.directory == directory);
        let target = if let Some(index) = index {
            &mut targets[index]
        } else {
            targets.push(OpenTarget {
                directory,
                selected: Vec::new(),
            });
            targets.last_mut().expect("new target")
        };
        if let Some(selected) = selected
            && !target.selected.contains(&selected)
        {
            target.selected.push(selected);
        }
    }
    Ok(targets)
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct OpenRequest {
    pub(crate) targets: Vec<OpenTarget>,
}

pub(crate) struct Instance {
    pub(crate) requests: smol::channel::Receiver<OpenRequest>,
    pub(crate) connection: zbus::blocking::Connection,
}

struct FileManager {
    requests: smol::channel::Sender<OpenRequest>,
}

impl FileManager {
    fn launch(&self, uris: Vec<String>, select: bool) -> zbus::fdo::Result<()> {
        if uris.is_empty() {
            return Ok(());
        }
        let mut arguments = Vec::new();
        if select {
            arguments.push(OsString::from("--select"));
        }
        arguments.push(OsString::from("--"));
        arguments.extend(uris.into_iter().map(OsString::from));
        let targets = parse_arguments(arguments)
            .map_err(|error| zbus::fdo::Error::InvalidArgs(error.to_string()))?;
        self.requests
            .try_send(OpenRequest { targets })
            .map_err(|error| zbus::fdo::Error::LimitsExceeded(error.to_string()))
    }
}

#[zbus::interface(name = "org.freedesktop.FileManager1")]
impl FileManager {
    fn show_folders(&self, uris: Vec<String>, _startup_id: &str) -> zbus::fdo::Result<()> {
        self.launch(uris, false)
    }

    fn show_items(&self, uris: Vec<String>, _startup_id: &str) -> zbus::fdo::Result<()> {
        self.launch(uris, true)
    }

    fn show_item_properties(&self, _uris: Vec<String>, _startup_id: &str) -> zbus::fdo::Result<()> {
        Err(zbus::fdo::Error::NotSupported(
            "Open Properties from the Tern context menu".into(),
        ))
    }
}

struct Application {
    requests: smol::channel::Sender<OpenRequest>,
}

#[zbus::interface(name = "io.github.coolloong.Tern")]
impl Application {
    fn open(&self, payload: &str) -> zbus::fdo::Result<u32> {
        let request = serde_json::from_str(payload)
            .map_err(|error| zbus::fdo::Error::InvalidArgs(error.to_string()))?;
        self.requests
            .try_send(request)
            .map_err(|error| zbus::fdo::Error::LimitsExceeded(error.to_string()))?;
        Ok(std::process::id())
    }
}

fn niri_request(request: serde_json::Value) -> Option<serde_json::Value> {
    use std::io::{BufRead, Write};
    let path = std::env::var_os("NIRI_SOCKET")?;
    let mut socket = std::os::unix::net::UnixStream::connect(path).ok()?;
    socket
        .set_read_timeout(Some(std::time::Duration::from_millis(100)))
        .ok()?;
    socket
        .set_write_timeout(Some(std::time::Duration::from_millis(100)))
        .ok()?;
    writeln!(socket, "{request}").ok()?;
    let mut reply = String::new();
    std::io::BufReader::new(socket).read_line(&mut reply).ok()?;
    serde_json::from_str(&reply).ok()
}

pub(crate) fn activate_existing(pid: u32) {
    let Some(reply) = niri_request(serde_json::json!("Windows")) else {
        return;
    };
    let Some(windows) = reply["Ok"]["Windows"].as_array() else {
        return;
    };
    let window = windows
        .iter()
        .filter(|window| {
            window["pid"].as_u64() == Some(u64::from(pid))
                && window["app_id"].as_str() == Some("tern")
        })
        .max_by_key(|window| {
            (
                window["focus_timestamp"]["secs"].as_u64().unwrap_or(0),
                window["focus_timestamp"]["nanos"].as_u64().unwrap_or(0),
            )
        });
    if let Some(id) = window.and_then(|window| window["id"].as_u64()) {
        let _ = niri_request(serde_json::json!({"Action": {"FocusWindow": {"id": id}}}));
    }
}

pub(crate) fn instance_name(config: &Path, default_config: &Path) -> String {
    let name = "io.github.coolloong.Tern";
    let config = normalized_config_path(config);
    let default_config = normalized_config_path(default_config);
    if config == default_config {
        return name.into();
    }
    let hash = config
        .as_os_str()
        .as_encoded_bytes()
        .iter()
        .fold(0xcbf29ce484222325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });
    format!("{name}.Profile{hash:016x}")
}

/// Resolve existing ancestors too: a new profile must keep the same D-Bus
/// identity after GPUI creates its directory, including through symlink aliases.
fn normalized_config_path(path: &Path) -> PathBuf {
    if let Ok(canonical) = path.canonicalize() {
        return canonical;
    }
    if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
        return normalized_config_path(parent).join(name);
    }
    std::path::absolute(path).unwrap_or_else(|_| path.to_owned())
}

pub(crate) fn connect_instance(
    name: &str,
    initial: Option<&OpenRequest>,
    file_manager: bool,
) -> Result<Option<Instance>> {
    let (sender, requests) = smol::channel::bounded(64);
    let connection = zbus::blocking::connection::Builder::session()?
        .serve_at(
            "/io/github/coolloong/Tern",
            Application {
                requests: sender.clone(),
            },
        )?
        .serve_at(
            "/org/freedesktop/FileManager1",
            FileManager { requests: sender },
        )?
        .build()?;
    for _ in 0..3 {
        match connection
            .request_name_with_flags(name, zbus::fdo::RequestNameFlags::DoNotQueue.into())
        {
            Ok(
                zbus::fdo::RequestNameReply::PrimaryOwner
                | zbus::fdo::RequestNameReply::AlreadyOwner,
            ) => {
                if file_manager
                    && let Err(error) = connection.request_name_with_flags(
                        "org.freedesktop.FileManager1",
                        zbus::fdo::RequestNameFlags::DoNotQueue.into(),
                    )
                {
                    eprintln!("tern: cannot register FileManager1: {error}");
                }
                return Ok(Some(Instance {
                    requests,
                    connection,
                }));
            }
            Ok(_) | Err(zbus::Error::NameTaken) => {
                let Some(initial) = initial else {
                    return Ok(None);
                };
                let payload = serde_json::to_string(initial)?;
                match connection.call_method(
                    Some(name),
                    "/io/github/coolloong/Tern",
                    Some("io.github.coolloong.Tern"),
                    "Open",
                    &(payload,),
                ) {
                    Ok(reply) => {
                        if initial.targets.is_empty()
                            && let Ok(pid) = reply.body().deserialize::<u32>()
                        {
                            activate_existing(pid);
                        }
                        return Ok(None);
                    }
                    Err(zbus::Error::MethodError(error, _, _))
                        if matches!(
                            error.as_str(),
                            "org.freedesktop.DBus.Error.NameHasNoOwner"
                                | "org.freedesktop.DBus.Error.ServiceUnknown"
                        ) =>
                    {
                        continue;
                    }
                    Err(error) => return Err(error.into()),
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
    bail!("Tern instance disappeared while opening a window")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_names_are_stable_and_isolated() {
        let default = Path::new("/home/test/.config/tern");
        let other = Path::new("/tmp/tern-profile");
        assert_eq!(instance_name(default, default), "io.github.coolloong.Tern");
        assert_eq!(instance_name(other, default), instance_name(other, default));
        assert_ne!(
            instance_name(other, default),
            instance_name(default, default)
        );
        assert!(zbus::names::WellKnownName::try_from(instance_name(other, default)).is_ok());
    }

    #[test]
    fn profile_alias_keeps_identity_before_and_after_directory_creation() {
        let root = std::env::temp_dir().join(format!("tern-profile-alias-{}", std::process::id()));
        std::fs::create_dir_all(root.join("real")).unwrap();
        std::os::unix::fs::symlink(root.join("real"), root.join("alias")).unwrap();
        let real = root.join("real/tern");
        let alias = root.join("alias/tern");
        let default = Path::new("/home/test/.config/tern");
        let before = instance_name(&real, default);
        assert_eq!(before, instance_name(&alias, default));
        std::fs::create_dir(&real).unwrap();
        assert_eq!(before, instance_name(&alias, default));
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn queue_limit_rejects_overflow_without_losing_accepted_requests() {
        let name = format!("io.github.coolloong.Tern.TestLimit{}", std::process::id());
        let primary = connect_instance(&name, None, false).unwrap().unwrap();
        let request = OpenRequest {
            targets: Vec::new(),
        };
        for _ in 0..64 {
            assert!(
                connect_instance(&name, Some(&request), false)
                    .unwrap()
                    .is_none()
            );
        }
        assert!(connect_instance(&name, Some(&request), false).is_err());
        assert_eq!(primary.requests.len(), 64);
    }

    #[test]
    fn secondary_launch_forwards_to_primary_without_another_instance() {
        let name = format!("io.github.coolloong.Tern.TestForward{}", std::process::id());
        let request = OpenRequest {
            targets: vec![OpenTarget {
                directory: "/tmp/中文 目录".into(),
                selected: vec!["/tmp/中文 目录/file.txt".into()],
            }],
        };
        let primary = connect_instance(&name, Some(&request), false)
            .unwrap()
            .unwrap();
        assert!(
            connect_instance(&name, Some(&request), false)
                .unwrap()
                .is_none()
        );
        let forwarded = primary.requests.try_recv().unwrap();
        assert_eq!(forwarded.targets, request.targets);
        assert!(primary.requests.try_recv().is_err());
        assert!(connect_instance(&name, None, false).unwrap().is_none());
    }

    #[test]
    fn simultaneous_launches_elect_one_primary_without_lost_requests() {
        let name = format!("io.github.coolloong.Tern.TestRace{}", std::process::id());
        let barrier = std::sync::Barrier::new(4);
        let results = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..4)
                .map(|_| {
                    let name = &name;
                    let barrier = &barrier;
                    scope.spawn(move || {
                        barrier.wait();
                        connect_instance(
                            name,
                            Some(&OpenRequest {
                                targets: Vec::new(),
                            }),
                            false,
                        )
                        .unwrap()
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .collect::<Vec<_>>()
        });
        assert_eq!(results.iter().filter(|result| result.is_some()).count(), 1);
        let primary = results.into_iter().flatten().next().unwrap();
        assert_eq!(primary.requests.len(), 3);
    }

    #[test]
    fn folders_and_encoded_uris_open_the_same_location() {
        let directory =
            std::env::temp_dir().join(format!("tern desktop 空格 {}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let uri = url::Url::from_directory_path(&directory).unwrap();
        let targets =
            parse_arguments([directory.clone().into_os_string(), uri.as_str().into()]).unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(
            targets[0].directory,
            directory.canonicalize().unwrap().to_string_lossy()
        );
        assert!(targets[0].selected.is_empty());
        std::fs::remove_dir(directory).unwrap();
    }

    #[test]
    fn selecting_groups_items_by_parent_and_preserves_symlink_names() {
        let directory = std::env::temp_dir().join(format!("tern select {}", std::process::id()));
        std::fs::create_dir_all(directory.join("subfolder")).unwrap();
        std::fs::write(directory.join("文件.txt"), "fixture").unwrap();
        std::os::unix::fs::symlink("文件.txt", directory.join("link")).unwrap();
        let targets = parse_arguments([
            "--select".into(),
            "--".into(),
            directory.join("文件.txt").into_os_string(),
            directory.join("subfolder").into_os_string(),
            directory.join("link").into_os_string(),
        ])
        .unwrap();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].selected.len(), 3);
        assert!(targets[0].selected[2].ends_with("/link"));
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn nonlocal_uris_unknown_options_and_missing_paths_are_rejected() {
        for argument in [
            "https://example.com/",
            "file://remote-host/tmp",
            "--unknown",
            "/tern-missing-path-for-test",
        ] {
            assert!(parse_arguments([argument.into()]).is_err(), "{argument}");
        }
        assert!(parse_arguments(Vec::new()).unwrap().is_empty());
    }
}
