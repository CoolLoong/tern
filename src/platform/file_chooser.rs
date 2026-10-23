//! org.freedesktop.impl.portal.FileChooser backend. UI lives in browser::chooser.
use crate::files::filter::FileFilter;
use anyhow::{Context, Result, bail};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

pub(crate) const BUS_NAME: &str = "org.freedesktop.impl.portal.desktop.tern";
pub(crate) const APP_ID: &str = "io.github.coolloong.Tern.FileChooser";
pub(crate) type Dictionary = HashMap<String, OwnedValue>;
pub(crate) type Response = (u32, Dictionary);
type WireFilter = (String, Vec<(u32, String)>);
type WireChoice = (String, String, Vec<(String, String)>, String);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Open,
    Save,
    SaveMany,
}
#[derive(Clone, Debug)]
pub(crate) struct Choice {
    pub id: String,
    pub label: String,
    pub options: Vec<(String, String)>,
    pub selected: String,
}
#[derive(Clone, Debug)]
pub(crate) struct Options {
    pub mode: Mode,
    pub multiple: bool,
    pub directory: bool,
    pub folder: PathBuf,
    pub name: String,
    pub names: Vec<String>,
    pub accept_label: String,
    pub filters: Vec<FileFilter>,
    pub filter_index: usize,
    pub choices: Vec<Choice>,
}

fn get<T>(values: &Dictionary, key: &str) -> Result<Option<T>>
where
    T: TryFrom<OwnedValue>,
    T::Error: std::fmt::Display,
{
    values
        .get(key)
        .map(|value| {
            T::try_from(value.try_clone()?).map_err(|error| anyhow::anyhow!("{key}: {error}"))
        })
        .transpose()
}
fn byte_path(bytes: Vec<u8>) -> Result<PathBuf> {
    use std::os::unix::ffi::OsStringExt;
    let bytes = bytes.strip_suffix(&[0]).unwrap_or(&bytes);
    if bytes.contains(&0) {
        bail!("path contains a NUL byte");
    }
    Ok(std::ffi::OsString::from_vec(bytes.to_vec()).into())
}
fn valid_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\0'])
}

impl Options {
    pub(crate) fn parse(mode: Mode, values: &Dictionary) -> Result<Self> {
        let current_file = get::<Vec<u8>>(values, "current_file")?
            .map(byte_path)
            .transpose()?;
        let folder = get::<Vec<u8>>(values, "current_folder")?
            .map(byte_path)
            .transpose()?
            .or_else(|| {
                current_file
                    .as_ref()
                    .and_then(|p| p.parent().map(Path::to_owned))
            })
            .unwrap_or_else(|| home::home_dir().unwrap_or_else(|| PathBuf::from("/")));
        if !folder.is_absolute() || folder.to_str().is_none() {
            bail!("current_folder must be an absolute UTF-8 path");
        }
        let name = get::<String>(values, "current_name")?
            .or_else(|| {
                current_file.and_then(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
            })
            .unwrap_or_default();
        let names = get::<Vec<Vec<u8>>>(values, "files")?
            .unwrap_or_default()
            .into_iter()
            .map(|bytes| {
                byte_path(bytes)?
                    .into_os_string()
                    .into_string()
                    .map_err(|_| anyhow::anyhow!("non-UTF-8 filename"))
            })
            .collect::<Result<Vec<_>>>()?;
        if names.iter().any(|name| !valid_name(name))
            || (mode == Mode::SaveMany && names.is_empty())
        {
            bail!("invalid save filenames");
        }
        let mut filters = get::<Vec<WireFilter>>(values, "filters")?
            .unwrap_or_default()
            .into_iter()
            .map(|(name, rules)| FileFilter { name, rules })
            .collect::<Vec<_>>();
        let current = get::<WireFilter>(values, "current_filter")?
            .map(|(name, rules)| FileFilter { name, rules });
        let filter_index = if let Some(current) = current {
            if let Some(index) = filters.iter().position(|filter| *filter == current) {
                index
            } else {
                filters.push(current);
                filters.len() - 1
            }
        } else {
            0
        };
        for filter in &filters {
            for (kind, pattern) in &filter.rules {
                match kind {
                    0 => {
                        glob::Pattern::new(pattern)?;
                    }
                    1 => {}
                    _ => bail!("unknown filter kind"),
                }
            }
        }
        let choices = get::<Vec<WireChoice>>(values, "choices")?
            .unwrap_or_default()
            .into_iter()
            .map(|(id, label, options, selected)| {
                let options = if options.is_empty() {
                    vec![("false".into(), "No".into()), ("true".into(), "Yes".into())]
                } else {
                    options
                };
                let selected = if options.iter().any(|(id, _)| *id == selected) {
                    selected
                } else {
                    options[0].0.clone()
                };
                Choice {
                    id,
                    label,
                    options,
                    selected,
                }
            })
            .collect();
        Ok(Self {
            mode,
            multiple: mode == Mode::Open && get::<bool>(values, "multiple")?.unwrap_or(false),
            directory: mode == Mode::SaveMany || get::<bool>(values, "directory")?.unwrap_or(false),
            folder,
            name,
            names,
            accept_label: get::<String>(values, "accept_label")?
                .unwrap_or_else(|| if mode == Mode::Open { "Open" } else { "Save" }.into())
                .replace('_', ""),
            filters,
            filter_index,
            choices,
        })
    }
    pub(crate) fn filter(&self) -> Option<FileFilter> {
        self.filters.get(self.filter_index).cloned()
    }

    /// Runs on a worker. Returns an overwrite confirmation rather than silently replacing files.
    pub(crate) fn resolve(
        &self,
        cwd: &str,
        selected: &[PathBuf],
        name: &str,
        overwrite: bool,
    ) -> Result<Resolution> {
        let cwd = Path::new(cwd);
        let paths = match self.mode {
            Mode::Open => {
                let paths = if selected.is_empty() && self.directory {
                    vec![cwd.to_owned()]
                } else {
                    selected.to_vec()
                };
                if paths.is_empty() {
                    bail!("请选择文件");
                }
                if !self.multiple && paths.len() != 1 {
                    bail!("请选择一个项目");
                }
                for path in &paths {
                    let metadata = path
                        .metadata()
                        .with_context(|| format!("无法访问 {}", path.display()))?;
                    if self.directory != metadata.is_dir() {
                        bail!(if self.directory {
                            "请选择文件夹"
                        } else {
                            "请选择文件，不要选择文件夹"
                        });
                    }
                    if !self.directory
                        && self.filter().is_some_and(|filter| {
                            !filter.matches(&path.file_name().unwrap_or_default().to_string_lossy())
                        })
                    {
                        bail!("所选文件不符合当前文件类型");
                    }
                }
                paths
            }
            Mode::Save => {
                if !valid_name(name) {
                    bail!("请输入不含路径分隔符的文件名");
                }
                if !cwd.is_dir() {
                    bail!("目标文件夹不存在");
                }
                let path = cwd.join(name);
                if path.is_dir() {
                    bail!("同名文件夹已存在");
                }
                if path.exists() && !overwrite {
                    return Ok(Resolution::Overwrite(path));
                }
                vec![path]
            }
            Mode::SaveMany => {
                let folder = if selected.is_empty() {
                    cwd.to_owned()
                } else if selected.len() == 1 {
                    selected[0].clone()
                } else {
                    bail!("请选择一个文件夹")
                };
                if !folder.is_dir() {
                    bail!("请选择文件夹");
                }
                let mut paths = Vec::new();
                for name in &self.names {
                    let candidate = crate::files::operations::conflict_name(&folder, name);
                    if paths.contains(&candidate) {
                        bail!("请求包含重复文件名");
                    }
                    paths.push(candidate);
                }
                paths
            }
        };
        let uris = paths
            .into_iter()
            .map(|path| {
                let path = std::path::absolute(path)?;
                url::Url::from_file_path(path)
                    .map(|url| url.to_string())
                    .map_err(|_| anyhow::anyhow!("无法转换文件路径"))
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Resolution::Selected(uris))
    }

    pub(crate) fn response(&self, uris: Vec<String>) -> Response {
        let mut results = Dictionary::new();
        results.insert("uris".into(), owned(uris));
        results.insert(
            "choices".into(),
            owned(
                self.choices
                    .iter()
                    .map(|c| (c.id.clone(), c.selected.clone()))
                    .collect::<Vec<_>>(),
            ),
        );
        if let Some(filter) = self.filter() {
            results.insert("current_filter".into(), owned((filter.name, filter.rules)));
        }
        (0, results)
    }
}

pub(crate) enum Resolution {
    Selected(Vec<String>),
    Overwrite(PathBuf),
}
pub(crate) fn owned<'a, T: Into<Value<'a>>>(value: T) -> OwnedValue {
    value.into().try_to_owned().expect("owned portal value")
}

pub(crate) struct Pending {
    completed: AtomicBool,
    reply: smol::channel::Sender<Response>,
    cancel: smol::channel::Sender<()>,
}
impl Pending {
    pub(crate) fn finish(&self, reply: Response) -> bool {
        if self.completed.swap(true, Ordering::AcqRel) {
            return false;
        }
        let _ = self.reply.try_send(reply);
        true
    }
    pub(crate) fn cancel(&self) {
        if self.finish((1, Dictionary::new())) {
            let _ = self.cancel.try_send(());
        }
    }
    pub(crate) fn completed(&self) -> bool {
        self.completed.load(Ordering::Acquire)
    }
}
pub(crate) struct ChooserRequest {
    pub title: String,
    pub options: Options,
    pub pending: Arc<Pending>,
    pub cancelled: smol::channel::Receiver<()>,
}
struct Request {
    pending: Arc<Pending>,
    owner: String,
}
#[zbus::interface(name = "org.freedesktop.impl.portal.Request")]
impl Request {
    fn close(&self, #[zbus(header)] header: zbus::message::Header<'_>) -> zbus::fdo::Result<()> {
        if header.sender().map(|name| name.as_str()) != Some(self.owner.as_str()) {
            return Err(zbus::fdo::Error::AccessDenied(
                "request belongs to another caller".into(),
            ));
        }
        self.pending.cancel();
        Ok(())
    }
}
struct FileChooser {
    requests: smol::channel::Sender<ChooserRequest>,
}
impl FileChooser {
    async fn choose(
        &self,
        connection: &zbus::Connection,
        owner: String,
        handle: OwnedObjectPath,
        title: String,
        options: Dictionary,
        mode: Mode,
    ) -> zbus::fdo::Result<Response> {
        let options = Options::parse(mode, &options)
            .map_err(|error| zbus::fdo::Error::InvalidArgs(error.to_string()))?;
        let (reply_tx, reply_rx) = smol::channel::bounded(1);
        let (cancel_tx, cancel_rx) = smol::channel::bounded(1);
        let pending = Arc::new(Pending {
            completed: AtomicBool::new(false),
            reply: reply_tx,
            cancel: cancel_tx,
        });
        let inserted = connection
            .object_server()
            .at(
                handle.clone(),
                Request {
                    pending: pending.clone(),
                    owner,
                },
            )
            .await?;
        if !inserted {
            return Err(zbus::fdo::Error::InvalidArgs(
                "duplicate request handle".into(),
            ));
        }
        if self
            .requests
            .try_send(ChooserRequest {
                title,
                options,
                pending: pending.clone(),
                cancelled: cancel_rx,
            })
            .is_err()
        {
            pending.finish((2, Dictionary::new()));
        }
        let response = reply_rx
            .recv()
            .await
            .unwrap_or_else(|_| (2, Dictionary::new()));
        connection
            .object_server()
            .remove::<Request, _>(handle)
            .await?;
        Ok(response)
    }
}
#[zbus::interface(name = "org.freedesktop.impl.portal.FileChooser")]
impl FileChooser {
    #[zbus(property, name = "version")]
    fn version(&self) -> u32 {
        4
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "XDG FileChooser wire signature plus zbus context"
    )]
    async fn open_file(
        &self,
        handle: OwnedObjectPath,
        _app_id: String,
        _parent_window: String,
        title: String,
        options: Dictionary,
        #[zbus(connection)] connection: &zbus::Connection,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> zbus::fdo::Result<Response> {
        self.choose(
            connection,
            header.sender().unwrap().to_string(),
            handle,
            title,
            options,
            Mode::Open,
        )
        .await
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "XDG FileChooser wire signature plus zbus context"
    )]
    async fn save_file(
        &self,
        handle: OwnedObjectPath,
        _app_id: String,
        _parent_window: String,
        title: String,
        options: Dictionary,
        #[zbus(connection)] connection: &zbus::Connection,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> zbus::fdo::Result<Response> {
        self.choose(
            connection,
            header.sender().unwrap().to_string(),
            handle,
            title,
            options,
            Mode::Save,
        )
        .await
    }
    #[allow(
        clippy::too_many_arguments,
        reason = "XDG FileChooser wire signature plus zbus context"
    )]
    async fn save_files(
        &self,
        handle: OwnedObjectPath,
        _app_id: String,
        _parent_window: String,
        title: String,
        options: Dictionary,
        #[zbus(connection)] connection: &zbus::Connection,
        #[zbus(header)] header: zbus::message::Header<'_>,
    ) -> zbus::fdo::Result<Response> {
        self.choose(
            connection,
            header.sender().unwrap().to_string(),
            handle,
            title,
            options,
            Mode::SaveMany,
        )
        .await
    }
}
pub(crate) fn connect() -> Result<(
    zbus::blocking::Connection,
    smol::channel::Receiver<ChooserRequest>,
)> {
    connect_named(BUS_NAME)
}
fn connect_named(
    name: &str,
) -> Result<(
    zbus::blocking::Connection,
    smol::channel::Receiver<ChooserRequest>,
)> {
    let (requests, receiver) = smol::channel::bounded(16);
    let connection = zbus::blocking::connection::Builder::session()?
        .serve_at("/org/freedesktop/portal/desktop", FileChooser { requests })?
        .name(name)?
        .build()?;
    Ok((connection, receiver))
}

#[cfg(test)]
pub(crate) fn mock_request(
    options: Options,
) -> (ChooserRequest, smol::channel::Receiver<Response>) {
    let (reply, receiver) = smol::channel::bounded(1);
    let (cancel, cancelled) = smol::channel::bounded(1);
    let pending = Arc::new(Pending {
        completed: AtomicBool::new(false),
        reply,
        cancel,
    });
    (
        ChooserRequest {
            title: "Pick".into(),
            options,
            pending,
            cancelled,
        },
        receiver,
    )
}

#[cfg(test)]
mod chooser_tests {
    use super::*;

    #[test]
    fn options_filters_choices_and_encoded_uris_round_trip() {
        let root = std::env::temp_dir().join(format!("tern-portal-options-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let file = root.join("中文 图片.png");
        std::fs::write(&file, b"fixture").unwrap();
        let filter = ("Images".to_string(), vec![(1u32, "image/*".to_string())]);
        let mut dict = Dictionary::new();
        dict.insert(
            "current_folder".into(),
            owned(
                root.to_string_lossy()
                    .as_bytes()
                    .iter()
                    .copied()
                    .chain([0])
                    .collect::<Vec<_>>(),
            ),
        );
        dict.insert("multiple".into(), owned(true));
        dict.insert("filters".into(), owned(vec![filter.clone()]));
        dict.insert(
            "choices".into(),
            owned(vec![(
                "mode".to_string(),
                "Mode".to_string(),
                vec![
                    ("a".to_string(), "A".to_string()),
                    ("b".to_string(), "B".to_string()),
                ],
                "b".to_string(),
            )]),
        );
        let options = Options::parse(Mode::Open, &dict).unwrap();
        assert!(options.multiple);
        assert_eq!(options.folder, root);
        let Resolution::Selected(uris) = options
            .resolve(
                root.to_str().unwrap(),
                std::slice::from_ref(&file),
                "",
                false,
            )
            .unwrap()
        else {
            panic!()
        };
        assert_eq!(
            url::Url::parse(&uris[0]).unwrap().to_file_path().unwrap(),
            file
        );
        let (code, result) = options.response(uris);
        assert_eq!(code, 0);
        assert_eq!(
            get::<WireFilter>(&result, "current_filter").unwrap(),
            Some(filter)
        );
        assert_eq!(
            get::<Vec<(String, String)>>(&result, "choices")
                .unwrap()
                .unwrap(),
            vec![("mode".into(), "b".into())]
        );
        let text = root.join("bad.txt");
        std::fs::write(&text, b"x").unwrap();
        assert!(
            options
                .resolve(root.to_str().unwrap(), &[text], "", false)
                .is_err()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn save_requires_overwrite_confirmation_and_directory_selection_is_checked() {
        let root = std::env::temp_dir().join(format!("tern-portal-save-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("old.txt"), b"keep").unwrap();
        let options = Options::parse(Mode::Save, &Dictionary::new()).unwrap();
        assert!(matches!(
            options
                .resolve(root.to_str().unwrap(), &[], "old.txt", false)
                .unwrap(),
            Resolution::Overwrite(_)
        ));
        assert!(matches!(
            options
                .resolve(root.to_str().unwrap(), &[], "old.txt", true)
                .unwrap(),
            Resolution::Selected(_)
        ));
        assert!(
            options
                .resolve(root.to_str().unwrap(), &[], "../escape", true)
                .is_err()
        );
        assert_eq!(std::fs::read(root.join("old.txt")).unwrap(), b"keep");
        let mut dict = Dictionary::new();
        dict.insert("directory".into(), owned(true));
        let options = Options::parse(Mode::Open, &dict).unwrap();
        assert!(matches!(
            options
                .resolve(root.to_str().unwrap(), &[], "", false)
                .unwrap(),
            Resolution::Selected(_)
        ));
        assert!(
            options
                .resolve(root.to_str().unwrap(), &[root.join("old.txt")], "", false)
                .is_err()
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn dbus_response_and_close_lifecycle_without_ui() {
        let name = format!(
            "org.freedesktop.impl.portal.desktop.tern.Test{}",
            std::process::id()
        );
        let (_server, requests) = connect_named(&name).unwrap();
        let client = zbus::blocking::Connection::session().unwrap();
        let path =
            OwnedObjectPath::try_from("/org/freedesktop/portal/desktop/request/test/open").unwrap();
        let call = |client: zbus::blocking::Connection, path: OwnedObjectPath| {
            let name = name.clone();
            std::thread::spawn(move || {
                client
                    .call_method(
                        Some(name.as_str()),
                        "/org/freedesktop/portal/desktop",
                        Some("org.freedesktop.impl.portal.FileChooser"),
                        "OpenFile",
                        &(path, "test.app", "", "Pick a file", Dictionary::new()),
                    )
                    .unwrap()
                    .body()
                    .deserialize::<Response>()
                    .unwrap()
            })
        };
        let worker = call(client.clone(), path.clone());
        let request = smol::block_on(smol::future::race(
            async { requests.recv().await.unwrap() },
            async {
                smol::Timer::after(std::time::Duration::from_secs(5)).await;
                panic!("portal did not dispatch request")
            },
        ));
        assert_eq!(request.title, "Pick a file");
        request.pending.finish(
            request
                .options
                .response(vec!["file:///tmp/selected".into()]),
        );
        let result = worker.join().unwrap();
        assert_eq!(result.0, 0);
        assert_eq!(
            get::<Vec<String>>(&result.1, "uris").unwrap().unwrap(),
            ["file:///tmp/selected"]
        );
        assert!(
            client
                .call_method(
                    Some(name.as_str()),
                    path.as_str(),
                    Some("org.freedesktop.impl.portal.Request"),
                    "Close",
                    &()
                )
                .is_err()
        );
        let worker = call(client.clone(), path.clone());
        let request = smol::block_on(smol::future::race(
            async { requests.recv().await.unwrap() },
            async {
                smol::Timer::after(std::time::Duration::from_secs(5)).await;
                panic!("portal did not dispatch request")
            },
        ));
        let other = zbus::blocking::Connection::session().unwrap();
        assert!(
            other
                .call_method(
                    Some(name.as_str()),
                    path.as_str(),
                    Some("org.freedesktop.impl.portal.Request"),
                    "Close",
                    &()
                )
                .is_err()
        );
        client
            .call_method(
                Some(name.as_str()),
                path.as_str(),
                Some("org.freedesktop.impl.portal.Request"),
                "Close",
                &(),
            )
            .unwrap();
        assert_eq!(worker.join().unwrap().0, 1);
        assert!(request.cancelled.try_recv().is_ok());
        assert!(
            !request.pending.finish((0, Dictionary::new())),
            "a cancelled request cannot later succeed"
        );
    }
}
