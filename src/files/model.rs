#[cfg(test)]
use std::collections::HashMap;

#[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
pub enum EntryKind {
    Dir,
    File,
    Symlink,
    Other,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entry {
    pub path: String,
    pub name: String,
    pub kind: EntryKind,
    pub size: Option<u64>,
    pub modified: Option<u64>,
    pub perms: Option<String>,
}

impl Entry {
    #[cfg(test)]
    pub fn new(path: impl Into<String>, name: impl Into<String>, kind: EntryKind) -> Self {
        Self::new_with_meta(path, name, kind, None, None, None)
    }

    pub fn new_with_meta(
        path: impl Into<String>,
        name: impl Into<String>,
        kind: EntryKind,
        size: Option<u64>,
        modified: Option<u64>,
        perms: Option<String>,
    ) -> Self {
        Self {
            path: path.into(),
            name: name.into(),
            kind,
            size,
            modified,
            perms,
        }
    }
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SortColumn {
    Name,
    Size,
    Modified,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum SortDirection {
    Asc,
    Desc,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct SortSpec {
    pub column: SortColumn,
    pub direction: SortDirection,
}

impl Default for SortSpec {
    fn default() -> Self {
        Self {
            column: SortColumn::Name,
            direction: SortDirection::Asc,
        }
    }
}

/// Original full-copy projection, kept as a regression reference.
#[cfg(test)]
#[derive(Clone, Debug, Default)]
pub struct TreeState {
    pub root: String,
    root_entry: Option<Entry>,
    entries: HashMap<String, Entry>,
}

#[cfg(test)]
impl TreeState {
    pub fn new(root: Entry) -> Self {
        Self {
            root: root.path.clone(),
            root_entry: Some(root),
            entries: HashMap::new(),
        }
    }

    pub fn upsert_children(&mut self, parent_id: &str, children: Vec<Entry>) {
        if parent_id != self.root {
            return;
        }
        self.entries = children
            .into_iter()
            .map(|entry| (entry.path.clone(), entry))
            .collect();
    }

    pub fn visible_rows_sorted(&self, sort: SortSpec) -> Vec<VisibleRow> {
        let mut rows = Vec::with_capacity(self.entries.len() + 1);
        if let Some(root) = &self.root_entry {
            rows.push(VisibleRow::from_entry(root));
        }
        let mut entries: Vec<_> = self.entries.values().collect();
        entries.sort_by(|a, b| compare_entries(a, b, sort));
        rows.extend(entries.into_iter().map(VisibleRow::from_entry));
        rows
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VisibleRow {
    pub id: String,
    pub name: String,
    pub kind: EntryKind,
    pub size: Option<u64>,
    pub modified: Option<u64>,
    pub perms: Option<String>,
    pub is_dir: bool,
    /// Where a trashed entry came from; `None` everywhere else.
    pub original_path: Option<String>,
}

impl VisibleRow {
    #[cfg(test)]
    pub(crate) fn from_entry(entry: &Entry) -> Self {
        Self {
            id: entry.path.clone(),
            name: entry.name.clone(),
            kind: entry.kind,
            size: entry.size,
            modified: entry.modified,
            perms: entry.perms.clone(),
            is_dir: entry.kind == EntryKind::Dir,
            original_path: None,
        }
    }
}

impl From<Entry> for VisibleRow {
    fn from(entry: Entry) -> Self {
        Self {
            id: entry.path,
            name: entry.name,
            kind: entry.kind,
            size: entry.size,
            modified: entry.modified,
            perms: entry.perms,
            is_dir: entry.kind == EntryKind::Dir,
            original_path: None,
        }
    }
}

pub fn breadcrumbs_for_path(path: &str) -> Vec<(String, String)> {
    let path = path.trim();
    if crate::files::recent::is_recent(path) {
        return vec![(
            "Recent".to_string(),
            crate::files::recent::RECENT_URI.to_string(),
        )];
    }
    if path.is_empty() || path == "." {
        return vec![(".".to_string(), ".".to_string())];
    }
    if path == "/" {
        return vec![("/".to_string(), "/".to_string())];
    }

    let is_abs = path.starts_with('/');
    let normalized = path.trim_end_matches('/');
    let parts: Vec<&str> = normalized.split('/').filter(|p| !p.is_empty()).collect();

    if parts.is_empty() {
        return if is_abs {
            vec![("/".to_string(), "/".to_string())]
        } else {
            vec![(".".to_string(), ".".to_string())]
        };
    }

    let mut out = Vec::new();
    if is_abs {
        out.push(("/".to_string(), "/".to_string()));
        let mut acc = String::new();
        for part in parts {
            if acc.is_empty() {
                acc = format!("/{part}");
            } else {
                acc = format!("{acc}/{part}");
            }
            out.push((part.to_string(), acc.clone()));
        }
    } else {
        let mut acc = String::new();
        for part in parts {
            if acc.is_empty() {
                acc = part.to_string();
            } else {
                acc = format!("{acc}/{part}");
            }
            out.push((part.to_string(), acc.clone()));
        }
    }

    out
}

pub fn display_name_for_dir(path: &str) -> String {
    let path = path.trim();
    if path.is_empty() || path == "." {
        return ".".to_string();
    }
    if path == "/" {
        return "/".to_string();
    }
    if crate::files::recent::is_recent(path) {
        return "Recent".to_string();
    }

    let normalized = path.trim_end_matches('/');
    let parts: Vec<&str> = normalized.split('/').filter(|p| !p.is_empty()).collect();
    parts.last().map(|s| s.to_string()).unwrap_or_else(|| {
        if path.starts_with('/') {
            "/".to_string()
        } else {
            ".".to_string()
        }
    })
}

#[cfg(test)]
pub fn compare_entries(a: &Entry, b: &Entry, sort: SortSpec) -> std::cmp::Ordering {
    compare_rows(
        (a.kind, &a.name, a.size, a.modified),
        (b.kind, &b.name, b.size, b.modified),
        sort,
    )
}

/// Same ordering for a row that is already projected for display, so a search
/// result list can be sorted by the column headers too.
pub fn compare_visible_rows(a: &VisibleRow, b: &VisibleRow, sort: SortSpec) -> std::cmp::Ordering {
    compare_rows(
        (a.kind, &a.name, a.size, a.modified),
        (b.kind, &b.name, b.size, b.modified),
        sort,
    )
}

/// Directories first, then the chosen column, with the name as the tiebreak.
fn compare_rows(
    a: (EntryKind, &String, Option<u64>, Option<u64>),
    b: (EntryKind, &String, Option<u64>, Option<u64>),
    sort: SortSpec,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    let (a_kind, a_name, a_size, a_modified) = a;
    let (b_kind, b_name, b_size, b_modified) = b;

    let kind_rank = |k: EntryKind| match k {
        EntryKind::Dir => 0u8,
        _ => 1u8,
    };
    let dir_rank = kind_rank(a_kind).cmp(&kind_rank(b_kind));
    if dir_rank != Ordering::Equal {
        return dir_rank;
    }

    let ord = match sort.column {
        SortColumn::Name => a_name.cmp(b_name),
        SortColumn::Size => opt_u64_cmp(a_size, b_size).then_with(|| a_name.cmp(b_name)),
        SortColumn::Modified => {
            opt_u64_cmp(a_modified, b_modified).then_with(|| a_name.cmp(b_name))
        }
    };

    match sort.direction {
        SortDirection::Asc => ord,
        SortDirection::Desc => ord.reverse(),
    }
}

fn opt_u64_cmp(a: Option<u64>, b: Option<u64>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(a), Some(b)) => a.cmp(&b),
        (Some(_), None) => std::cmp::Ordering::Greater,
        (None, Some(_)) => std::cmp::Ordering::Less,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, name: &str, kind: EntryKind) -> Entry {
        Entry::new(path, name, kind)
    }

    #[test]
    fn sorts_dirs_before_files_and_by_name() {
        let mut tree = TreeState::new(entry("/", "/", EntryKind::Dir));
        tree.upsert_children(
            "/",
            vec![
                entry("/a.txt", "a.txt", EntryKind::File),
                entry("/Zdir", "Zdir", EntryKind::Dir),
                entry("/m.txt", "m.txt", EntryKind::File),
                entry("/adir", "adir", EntryKind::Dir),
            ],
        );
        let rows = tree.visible_rows_sorted(SortSpec::default());
        let names: Vec<&str> = rows.iter().skip(1).map(|r| r.name.as_str()).collect();
        assert_eq!(names, vec!["Zdir", "adir", "a.txt", "m.txt"]);
    }

    #[test]
    fn only_root_children_are_listed() {
        let mut tree = TreeState::new(entry("/", "/", EntryKind::Dir));
        tree.upsert_children(
            "/",
            vec![
                entry("/b", "b", EntryKind::Dir),
                entry("/a", "a", EntryKind::Dir),
            ],
        );
        tree.upsert_children("/a", vec![entry("/a/f", "f", EntryKind::File)]);
        let rows = tree.visible_rows_sorted(SortSpec::default());
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].id, "/");
        assert_eq!(rows[1].id, "/a");
        assert_eq!(rows[2].id, "/b");
    }

    #[test]
    fn breadcrumbs_for_absolute_paths() {
        let crumbs = breadcrumbs_for_path("/home/user/Documents");
        assert_eq!(crumbs.len(), 4);
        assert_eq!(crumbs[0], ("/".to_string(), "/".to_string()));
        assert_eq!(
            crumbs[3],
            ("Documents".to_string(), "/home/user/Documents".to_string())
        );
    }

    #[test]
    fn display_name_for_dir_uses_basename() {
        assert_eq!(display_name_for_dir("/home/user/Documents"), "Documents");
        assert_eq!(display_name_for_dir("/"), "/");
    }

    #[test]
    fn modified_sort_then_name() {
        let a = Entry::new_with_meta("/a", "a", EntryKind::File, None, Some(10), None);
        let b = Entry::new_with_meta("/b", "b", EntryKind::File, None, Some(5), None);
        let c = Entry::new_with_meta("/c", "c", EntryKind::File, None, Some(5), None);
        let sort = SortSpec {
            column: SortColumn::Modified,
            direction: SortDirection::Asc,
        };
        let mut entries = [a, b, c];
        entries.sort_by(|x, y| compare_entries(x, y, sort));
        assert_eq!(
            entries.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            vec!["b", "c", "a"]
        );
    }

    #[test]
    fn row_and_entry_orderings_agree() {
        // Search results are sorted as rows, directory listings as entries; the
        // two must not drift apart.
        let entries = vec![
            Entry::new_with_meta("/z", "z", EntryKind::File, Some(10), Some(1), None),
            Entry::new_with_meta("/d", "d", EntryKind::Dir, None, Some(9), None),
            Entry::new_with_meta("/a", "a", EntryKind::File, Some(10), Some(1), None),
        ];
        let rows: Vec<VisibleRow> = entries.iter().map(VisibleRow::from_entry).collect();

        for column in [SortColumn::Name, SortColumn::Size, SortColumn::Modified] {
            for direction in [SortDirection::Asc, SortDirection::Desc] {
                let sort = SortSpec { column, direction };
                let mut by_entry = entries.clone();
                by_entry.sort_by(|x, y| compare_entries(x, y, sort));
                let mut by_row = rows.clone();
                by_row.sort_by(|x, y| compare_visible_rows(x, y, sort));
                assert_eq!(
                    by_entry.iter().map(|e| e.name.clone()).collect::<Vec<_>>(),
                    by_row.iter().map(|r| r.name.clone()).collect::<Vec<_>>(),
                    "{column:?} {direction:?}"
                );
            }
        }
    }
}
