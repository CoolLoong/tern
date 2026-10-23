use cosmic_text::{FontSystem, fontdb};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

#[derive(Serialize, Deserialize)]
struct CachedFace {
    path: PathBuf,
    index: u32,
    families: Vec<(String, u16)>,
    post_script_name: String,
    style: u8,
    weight: u16,
    stretch: u16,
    monospaced: bool,
}

#[derive(Serialize, Deserialize)]
struct Cache {
    version: u32,
    stamps: BTreeMap<PathBuf, Option<Stamp>>,
    families: [String; 5],
    faces: Vec<CachedFace>,
}

// Include inode/ctime so atomic replacement and preserved-mtime edits invalidate.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Stamp {
    device: u64,
    inode: u64,
    len: u64,
    modified: (i64, i64),
    changed: (i64, i64),
    mode: u32,
}

fn stamp(path: &Path) -> std::io::Result<Option<Stamp>> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    Ok(Some(Stamp {
        device: metadata.dev(),
        inode: metadata.ino(),
        len: metadata.len(),
        modified: (metadata.mtime(), metadata.mtime_nsec()),
        changed: (metadata.ctime(), metadata.ctime_nsec()),
        mode: metadata.mode(),
    }))
}

/// Capture inputs BEFORE scanning fonts, never when the background writer runs.
/// A change during discovery/scan/write makes the cache unusable, not stale.
#[derive(Default)]
struct Snapshot {
    stamps: BTreeMap<PathBuf, Option<Stamp>>,
    visited: BTreeSet<PathBuf>,
    font_dirs: BTreeSet<PathBuf>,
}

impl Snapshot {
    fn record(&mut self, path: &Path) -> Option<()> {
        let path = std::path::absolute(path).ok()?;
        let value = stamp(&path).ok()?;
        if let Some(old) = self.stamps.get(&path) {
            if old != &value {
                return None;
            }
        } else {
            self.stamps.insert(path, value);
        }
        Some(())
    }

    fn config(&mut self, path: &Path) -> Option<()> {
        self.record(path)?; // includes missing optional files/directories
        if !path.exists() {
            return Some(());
        }
        let canonical = path.canonicalize().ok()?;
        self.record(&canonical)?; // also track symlink targets
        if !self.visited.insert(canonical) {
            return Some(());
        }
        if path.is_dir() {
            for entry in fs::read_dir(path).ok()? {
                let entry = entry.ok()?;
                if !entry.file_type().ok()?.is_dir() {
                    self.config(&entry.path())?;
                }
            }
        } else {
            // Track a superset of fontdb's sources. Ignore matching/alias rules
            // here: fontdb may reject one after already applying earlier dirs.
            // Its path resolver handles relative, XDG and tilde paths for us.
            let xml = fs::read_to_string(path).ok()?;
            let document = match roxmltree::Document::parse_with_options(
                &xml,
                roxmltree::ParsingOptions {
                    allow_dtd: true,
                    ..Default::default()
                },
            ) {
                Ok(document) => document,
                Err(_) => return Some(()), // fontdb ignores malformed XML/README
            };
            if document.root_element().tag_name().name() != "fontconfig" {
                return Some(());
            }
            for node in document.root_element().children() {
                match node.tag_name().name() {
                    "dir" => {
                        let dir = fontconfig_parser::Dir {
                            prefix: node.attribute("prefix").unwrap_or("default").parse().ok()?,
                            path: node.text()?.into(),
                            ..Default::default()
                        };
                        self.font_dirs.insert(dir.calculate_path(path));
                    }
                    "include" => {
                        let include = fontconfig_parser::Include {
                            prefix: node.attribute("prefix").unwrap_or("default").parse().ok()?,
                            path: node.text()?.into(),
                            ..Default::default()
                        };
                        self.config(&include.calculate_path(path))?;
                    }
                    _ => {}
                }
            }
        }
        Some(())
    }

    fn fonts(&mut self, path: &Path) -> Option<()> {
        self.record(path)?;
        if !path.exists() {
            return Some(());
        }
        let canonical = path.canonicalize().ok()?;
        self.record(&canonical)?;
        if !self.visited.insert(canonical) {
            return Some(());
        }
        if path.is_dir() {
            for entry in fs::read_dir(path).ok()? {
                let path = entry.ok()?.path();
                // Follow directory symlinks just as fontdb does; visited breaks cycles.
                if path.is_dir()
                    || matches!(
                        path.extension().and_then(|s| s.to_str()),
                        Some("ttf" | "ttc" | "otf" | "otc" | "TTF" | "TTC" | "OTF" | "OTC")
                    )
                {
                    self.fonts(&path)?;
                }
            }
        }
        Some(())
    }

    fn system() -> Option<Self> {
        let mut snapshot = Self::default();
        let home = PathBuf::from(std::env::var_os("HOME")?);
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        if let Some(file) = std::env::var_os("FONTCONFIG_FILE") {
            snapshot.config(Path::new(&file))?;
        } else {
            // Watch both potential user/global branches. Extra invalidations are
            // harmless; missing optional includes must be detected when created.
            for path in [
                config.join("fontconfig/fonts.conf"),
                PathBuf::from("/etc/fonts/local.conf"),
                PathBuf::from("/etc/fonts/fonts.conf"),
            ] {
                snapshot.config(&path)?;
            }
        }
        // fontdb's fallback when fontconfig does not supply directories.
        snapshot.font_dirs.extend([
            PathBuf::from("/usr/share/fonts"),
            PathBuf::from("/usr/local/share/fonts"),
            home.join(".fonts"),
            home.join(".local/share/fonts"),
        ]);
        snapshot.visited.clear();
        for root in std::mem::take(&mut snapshot.font_dirs) {
            snapshot.fonts(&root)?;
        }
        Some(snapshot)
    }

    fn unchanged(stamps: &BTreeMap<PathBuf, Option<Stamp>>) -> bool {
        stamps
            .iter()
            .all(|(path, expected)| stamp(path).is_ok_and(|actual| actual == *expected))
    }
}

fn language(id: u16) -> fontdb::Language {
    ttf_parser::name::Name {
        platform_id: ttf_parser::PlatformId::Windows,
        encoding_id: 1,
        language_id: id,
        name_id: 1,
        name: &[],
    }
    .language()
}

fn cache_path() -> Option<PathBuf> {
    use std::hash::{Hash, Hasher};
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    std::env::current_dir().ok()?.hash(&mut hash);
    for name in [
        "HOME",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "FONTCONFIG_FILE",
        "FONTCONFIG_PATH",
    ] {
        std::env::var_os(name).hash(&mut hash);
    }
    let root = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))?;
    Some(
        root.join("tern")
            .join(format!("fontdb-v3-{:016x}.json", hash.finish())),
    )
}

fn save(path: &Path, db: &fontdb::Database, snapshot: Snapshot) -> Option<()> {
    if !Snapshot::unchanged(&snapshot.stamps) {
        return None;
    }
    let mut language_ids = HashMap::new();
    for id in 0..=u16::MAX {
        language_ids.entry(language(id) as u16).or_insert(id);
    }
    let mut faces = Vec::new();
    for face in db.faces() {
        let source = match &face.source {
            fontdb::Source::File(path) | fontdb::Source::SharedFile(path, _) => path,
            _ => return None,
        };
        // Do not publish if our dependency discovery missed a fontdb source.
        if !snapshot.stamps.get(source).is_some_and(Option::is_some) {
            log::debug!("Font cache did not track source {}", source.display());
            return None;
        }
        faces.push(CachedFace {
            path: source.clone(),
            index: face.index,
            families: face
                .families
                .iter()
                .map(|(name, lang)| Some((name.clone(), *language_ids.get(&(*lang as u16))?)))
                .collect::<Option<_>>()?,
            post_script_name: face.post_script_name.clone(),
            style: match face.style {
                fontdb::Style::Normal => 0,
                fontdb::Style::Italic => 1,
                fontdb::Style::Oblique => 2,
            },
            weight: face.weight.0,
            stretch: face.stretch.to_number(),
            monospaced: face.monospaced,
        });
    }
    let families = [
        fontdb::Family::Serif,
        fontdb::Family::SansSerif,
        fontdb::Family::Cursive,
        fontdb::Family::Fantasy,
        fontdb::Family::Monospace,
    ]
    .map(|family| db.family_name(&family).to_owned());
    let cache = Cache {
        version: 3,
        stamps: snapshot.stamps,
        families,
        faces,
    };
    let bytes = serde_json::to_vec(&cache).ok()?;
    fs::create_dir_all(path.parent()?).ok()?;
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static SERIAL: AtomicU64 = AtomicU64::new(0);
    let temporary = path.with_extension(format!(
        "{}-{}.tmp",
        std::process::id(),
        SERIAL.fetch_add(1, Ordering::Relaxed)
    ));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .ok()?;
        file.write_all(&bytes).ok()?;
        if !Snapshot::unchanged(&cache.stamps) {
            return None;
        }
        fs::rename(&temporary, path).ok()
    })();
    let _ = fs::remove_file(temporary);
    result
}

fn load(path: &Path) -> Option<fontdb::Database> {
    if fs::metadata(path).ok()?.len() > 16 * 1024 * 1024 {
        return None;
    }
    let cache: Cache = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    if cache.version != 3
        || cache.faces.is_empty()
        || cache.stamps.is_empty()
        || !Snapshot::unchanged(&cache.stamps)
    {
        return None;
    }
    let mut db = fontdb::Database::new();
    let widths = [
        fontdb::Stretch::UltraCondensed,
        fontdb::Stretch::ExtraCondensed,
        fontdb::Stretch::Condensed,
        fontdb::Stretch::SemiCondensed,
        fontdb::Stretch::Normal,
        fontdb::Stretch::SemiExpanded,
        fontdb::Stretch::Expanded,
        fontdb::Stretch::ExtraExpanded,
        fontdb::Stretch::UltraExpanded,
    ];
    for face in cache.faces {
        if face.families.is_empty() || !cache.stamps.get(&face.path).is_some_and(Option::is_some) {
            return None;
        }
        let style = match face.style {
            0 => fontdb::Style::Normal,
            1 => fontdb::Style::Italic,
            2 => fontdb::Style::Oblique,
            _ => return None,
        };
        db.push_face_info(fontdb::FaceInfo {
            id: fontdb::ID::dummy(),
            source: fontdb::Source::File(face.path),
            index: face.index,
            families: face
                .families
                .into_iter()
                .map(|(name, id)| (name, language(id)))
                .collect(),
            post_script_name: face.post_script_name,
            style,
            weight: fontdb::Weight(face.weight),
            stretch: *widths.get(usize::from(face.stretch.checked_sub(1)?))?,
            monospaced: face.monospaced,
        });
    }
    let [serif, sans, cursive, fantasy, mono] = cache.families;
    db.set_serif_family(serif);
    db.set_sans_serif_family(sans);
    db.set_cursive_family(cursive);
    db.set_fantasy_family(fantasy);
    db.set_monospace_family(mono);
    Some(db)
}

pub(crate) fn font_system() -> FontSystem {
    let started = std::time::Instant::now();
    let path = cache_path();
    if let Some(db) = path.as_deref().and_then(load) {
        log::info!(
            "Font metadata cache hit: {} faces in {:?}",
            db.len(),
            started.elapsed()
        );
        return FontSystem::new_with_locale_and_db(
            sys_locale::get_locale().unwrap_or_else(|| "en-US".into()),
            db,
        );
    }
    let snapshot = path.as_ref().and_then(|_| Snapshot::system());
    let system = FontSystem::new();
    log::debug!("Font metadata full scan: {:?}", started.elapsed());
    if let Some((path, snapshot)) = path.zip(snapshot) {
        let db = system.db().clone();
        let _ = std::thread::Builder::new()
            .name("font-cache-writer".into())
            .spawn(move || {
                let _ = save(&path, &db, snapshot);
            });
    }
    system
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_preserves_faces_and_invalidates_font_and_config_changes() {
        let root = std::env::temp_dir().join(format!("tern-font-cache-{}", std::process::id()));
        fs::create_dir_all(root.join("fonts")).unwrap();
        let source = root.join("fonts/font.ttf");
        fs::write(&source, b"fixture").unwrap();
        let config = root.join("fonts.conf");
        fs::write(&config, b"<fontconfig/>").unwrap();
        let mut db = fontdb::Database::new();
        db.push_face_info(fontdb::FaceInfo {
            id: fontdb::ID::dummy(),
            source: fontdb::Source::File(source.clone()),
            index: 2,
            families: vec![
                (
                    "中文".into(),
                    fontdb::Language::Chinese_PeoplesRepublicOfChina,
                ),
                ("Test".into(), fontdb::Language::English_UnitedStates),
            ],
            post_script_name: "Test-Bold".into(),
            style: fontdb::Style::Italic,
            weight: fontdb::Weight(700),
            stretch: fontdb::Stretch::Condensed,
            monospaced: true,
        });
        let path = root.join("db.json");
        let snapshot = || {
            let mut snapshot = Snapshot::default();
            snapshot.fonts(&root.join("fonts")).unwrap();
            snapshot.config(&config).unwrap();
            snapshot
        };
        save(&path, &db, snapshot()).unwrap();
        let cached = load(&path).unwrap();
        let original = db.faces().next().unwrap();
        let restored = cached.faces().next().unwrap();
        assert_eq!(original.families, restored.families);
        assert_eq!(original.index, restored.index);
        assert_eq!(original.style, restored.style);
        assert_eq!(original.weight, restored.weight);
        assert_eq!(original.stretch, restored.stretch);
        assert_eq!(original.monospaced, restored.monospaced);
        fs::write(root.join("fonts/new.ttf"), b"new font").unwrap();
        assert!(load(&path).is_none());
        save(&path, &db, snapshot()).unwrap();
        fs::write(&source, b"changed font").unwrap();
        assert!(load(&path).is_none());
        let before_scan = snapshot();
        fs::write(&source, b"changed during scan").unwrap();
        assert!(save(&path, &db, before_scan).is_none());
        save(&path, &db, snapshot()).unwrap();
        fs::write(&config, b"<fontconfig><!-- changed --></fontconfig>").unwrap();
        assert!(load(&path).is_none());
        fs::write(&path, b"broken cache").unwrap();
        assert!(load(&path).is_none());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn watches_empty_custom_dirs_optional_includes_and_symlink_subtrees() {
        let root = std::env::temp_dir().join(format!("tern-font-sources-{}", std::process::id()));
        fs::create_dir_all(root.join("custom/empty")).unwrap();
        fs::create_dir_all(root.join("external/nested")).unwrap();
        std::os::unix::fs::symlink(root.join("external"), root.join("custom/linked")).unwrap();
        // A symlink loop must not recurse indefinitely.
        std::os::unix::fs::symlink(root.join("custom"), root.join("external/loop")).unwrap();
        let config = root.join("fonts.conf");
        fs::write(&config, "<fontconfig><dir prefix=\"relative\">custom</dir><include prefix=\"relative\" ignore_missing=\"yes\">optional.conf</include></fontconfig>").unwrap();
        let discover = || {
            let mut snapshot = Snapshot::default();
            snapshot.config(&config).unwrap();
            snapshot.visited.clear();
            for dir in std::mem::take(&mut snapshot.font_dirs) {
                snapshot.fonts(&dir).unwrap();
            }
            assert!(Snapshot::unchanged(&snapshot.stamps));
            snapshot
        };
        let snapshot = discover();
        fs::write(root.join("custom/empty/new.ttf"), b"font").unwrap();
        assert!(!Snapshot::unchanged(&snapshot.stamps));
        let snapshot = discover();
        fs::write(root.join("external/nested/new.ttf"), b"font").unwrap();
        assert!(!Snapshot::unchanged(&snapshot.stamps));
        let snapshot = discover();
        fs::write(root.join("optional.conf"), "<fontconfig/>").unwrap();
        assert!(!Snapshot::unchanged(&snapshot.stamps));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "uses the host font installation; run explicitly for integration validation"]
    fn installed_fonts_round_trip() {
        let snapshot = Snapshot::system().expect("discover installed font dependencies");
        let system = FontSystem::new();
        assert!(
            Snapshot::unchanged(&snapshot.stamps),
            "font inputs changed during scan"
        );
        for face in system.db().faces() {
            if let fontdb::Source::File(path) | fontdb::Source::SharedFile(path, _) = &face.source {
                assert!(
                    snapshot.stamps.get(path).is_some_and(Option::is_some),
                    "missing source: {}",
                    path.display()
                );
            }
        }
        let path =
            std::env::temp_dir().join(format!("tern-installed-fonts-{}.json", std::process::id()));
        save(&path, system.db(), snapshot).expect("save installed fonts");
        let cached = load(&path).expect("reload installed fonts");
        assert_eq!(cached.len(), system.db().len());
        for (original, restored) in system.db().faces().zip(cached.faces()) {
            assert_eq!(original.families, restored.families);
            assert_eq!(original.index, restored.index);
        }
        fs::remove_file(path).unwrap();
    }
}
