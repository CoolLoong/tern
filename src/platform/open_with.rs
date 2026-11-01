use gio::prelude::*;
use std::{collections::HashSet, path::Path};

#[cfg(test)]
pub(crate) mod tests;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct AppEntry {
    pub id: String,
    pub name: String,
    pub recommended: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Association {
    Mime(String),
    Extension(String),
}

#[derive(Clone, Debug)]
pub(crate) struct FileInfo {
    pub content_type: String,
    pub description: String,
    pub association: Option<Association>,
    pub default_app: Option<AppEntry>,
}

pub(crate) trait AppOpener: Send + Sync {
    fn describe(&self, path: &Path) -> Result<FileInfo, String>;
    fn applications(&self, info: &FileInfo) -> Result<Vec<AppEntry>, String>;
    fn launch(&self, app: &AppEntry, path: &Path) -> Result<(), String>;
    fn set_default(&self, app: &AppEntry, association: &Association) -> Result<(), String>;
}

pub(crate) struct SystemAppOpener;

fn refresh_associations() {
    let context = gio::glib::MainContext::default();
    if let Ok(_guard) = context.acquire() {
        for _ in 0..64 {
            if !context.pending() {
                break;
            }
            context.iteration(false);
        }
    }
}

fn entry(app: &gio::AppInfo, recommended: bool) -> Option<AppEntry> {
    Some(AppEntry {
        id: app.id()?.to_string(),
        name: app.display_name().to_string(),
        recommended,
    })
}

fn installed_app(id: &str) -> Result<gio::AppInfo, String> {
    refresh_associations();
    gio::AppInfo::all()
        .into_iter()
        .find(|app| app.id().as_deref() == Some(id))
        .ok_or_else(|| format!("Application is no longer available: {id}"))
}

impl AppOpener for SystemAppOpener {
    fn describe(&self, path: &Path) -> Result<FileInfo, String> {
        refresh_associations();
        let file = gio::File::for_path(path);
        let attributes = file
            .query_info(
                "standard::content-type",
                gio::FileQueryInfoFlags::NONE,
                gio::Cancellable::NONE,
            )
            .map_err(|error| error.to_string())?;
        let content_type = attributes
            .content_type()
            .map(|value| value.to_string())
            .unwrap_or_else(|| "application/octet-stream".into());
        let unknown = gio::content_type_is_unknown(&content_type);
        let association = if unknown {
            path.extension()
                .and_then(|extension| extension.to_str())
                .filter(|extension| !extension.is_empty())
                .map(|extension| Association::Extension(extension.to_lowercase()))
        } else {
            Some(Association::Mime(content_type.clone()))
        };
        let description = match &association {
            Some(Association::Extension(extension)) => format!(".{extension} files"),
            _ => gio::content_type_get_description(&content_type).to_string(),
        };
        let default_app = match file.query_default_handler(gio::Cancellable::NONE) {
            Ok(app) => entry(&app, true)
                .filter(|app| content_type == "inode/directory" || app.id != "tern.desktop"),
            Err(error) if error.matches(gio::IOErrorEnum::NotSupported) => None,
            Err(error) => return Err(error.to_string()),
        };
        Ok(FileInfo {
            content_type,
            description,
            association,
            default_app,
        })
    }

    fn applications(&self, info: &FileInfo) -> Result<Vec<AppEntry>, String> {
        refresh_associations();
        let recommended = gio::AppInfo::all_for_type(&info.content_type);
        let recommended_ids: HashSet<_> = recommended.iter().filter_map(|app| app.id()).collect();
        let mut seen = HashSet::new();
        let mut apps: Vec<_> = recommended
            .into_iter()
            .chain(gio::AppInfo::all())
            .filter_map(|app| {
                let id = app.id()?;
                let recommended = recommended_ids.contains(&id);
                if id == "tern.desktop"
                    || (!recommended && !app.should_show())
                    || (!recommended && !app.supports_files() && !app.supports_uris())
                    || !seen.insert(id)
                {
                    return None;
                }
                entry(&app, recommended)
            })
            .collect();
        apps.sort_by(|a, b| {
            b.recommended
                .cmp(&a.recommended)
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                .then_with(|| a.id.cmp(&b.id))
        });
        Ok(apps)
    }

    fn launch(&self, app: &AppEntry, path: &Path) -> Result<(), String> {
        std::fs::metadata(path).map_err(|error| error.to_string())?;
        let native = installed_app(&app.id)?;
        let uri = gio::File::for_path(path).uri();
        gio::glib::MainContext::new()
            .block_on(native.launch_uris_future(&[uri.as_str()], gio::AppLaunchContext::NONE))
            .map_err(|error| error.to_string())?;
        let command = native.commandline().unwrap_or_else(|| native.executable());
        crate::files::recent::record_opened(path, &app.name, &command.to_string_lossy());
        Ok(())
    }

    fn set_default(&self, app: &AppEntry, association: &Association) -> Result<(), String> {
        let native = installed_app(&app.id)?;
        match association {
            Association::Mime(content_type) => native.set_as_default_for_type(content_type),
            Association::Extension(extension) => native.set_as_default_for_extension(extension),
        }
        .map_err(|error| error.to_string())
    }
}
