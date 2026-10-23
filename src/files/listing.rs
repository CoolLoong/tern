//! Immutable directory snapshots and cancellable background work.
//! A view sorts/filters indices, never copies the file names and metadata.

use std::{collections::HashMap, io, sync::Arc};

use super::jobs::Cancellation;
#[cfg(test)]
use super::jobs::JobSlot;

use crate::files::model::{Entry, SortSpec, VisibleRow, compare_visible_rows};

#[derive(Clone, Copy)]
pub(crate) enum Origin {
    Directory,
    Trash,
    Recent,
}

#[derive(Default)]
pub(crate) struct RowData {
    rows: Vec<VisibleRow>,
    by_id: HashMap<String, usize>,
}

pub(crate) type RowSource = Arc<RowData>;

impl RowData {
    /// Run on a worker: trash metadata and the ID index are prepared here too.
    pub(crate) fn from_entries(
        entries: Vec<Entry>,
        origin: Origin,
        cancel: &Cancellation,
    ) -> io::Result<RowSource> {
        let mut rows = Vec::with_capacity(entries.len());
        let mut by_id = HashMap::with_capacity(entries.len());
        for entry in entries {
            cancel.check()?;
            let mut row = VisibleRow::from(entry);
            row.original_path = match origin {
                Origin::Directory => None,
                Origin::Trash => {
                    crate::files::operations::trashed_original(std::path::Path::new(&row.id))
                }
                Origin::Recent => std::path::Path::new(&row.id)
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned()),
            };
            by_id.insert(row.id.clone(), rows.len());
            rows.push(row);
        }
        cancel.check()?;
        Ok(Arc::new(Self { rows, by_id }))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ViewOptions {
    pub(crate) sort: SortSpec,
    pub(crate) show_hidden: bool,
    pub(crate) filter: Option<Arc<super::filter::FileFilter>>,
}

#[derive(Clone, Default)]
pub(crate) struct RowView {
    source: RowSource,
    order: Arc<[usize]>,
    positions: Arc<[usize]>,
    pub(crate) options: ViewOptions,
}

impl RowView {
    pub(crate) fn prepare(
        source: RowSource,
        options: ViewOptions,
        cancel: &Cancellation,
    ) -> io::Result<Self> {
        cancel.check()?;
        let mut order = Vec::with_capacity(source.rows.len());
        for (index, row) in source.rows.iter().enumerate() {
            cancel.check()?;
            if (options.show_hidden || !crate::files::fs::is_hidden_name(&row.name))
                && (row.is_dir
                    || options
                        .filter
                        .as_ref()
                        .is_none_or(|filter| filter.matches(&row.name)))
            {
                order.push(index);
            }
        }
        order.sort_unstable_by(|&a, &b| {
            compare_visible_rows(&source.rows[a], &source.rows[b], options.sort)
                .then_with(|| source.rows[a].id.cmp(&source.rows[b].id))
        });
        cancel.check()?;
        let mut positions = vec![usize::MAX; source.rows.len()];
        for (position, &index) in order.iter().enumerate() {
            cancel.check()?;
            positions[index] = position;
        }
        Ok(Self {
            source,
            order: order.into(),
            positions: positions.into(),
            options,
        })
    }

    pub(crate) fn source(&self) -> RowSource {
        self.source.clone()
    }
    pub(crate) fn len(&self) -> usize {
        self.order.len()
    }
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.order.is_empty()
    }
    pub(crate) fn first(&self) -> Option<&VisibleRow> {
        self.get(0)
    }
    pub(crate) fn get(&self, index: usize) -> Option<&VisibleRow> {
        self.order.get(index).map(|&index| &self.source.rows[index])
    }
    pub(crate) fn iter(&self) -> impl ExactSizeIterator<Item = &VisibleRow> + DoubleEndedIterator {
        self.order.iter().map(|&index| &self.source.rows[index])
    }
    pub(crate) fn position(&self, id: &str) -> Option<usize> {
        let index = *self.source.by_id.get(id)?;
        let position = self.positions[index];
        (position != usize::MAX).then_some(position)
    }
    pub(crate) fn contains(&self, id: &str) -> bool {
        self.position(id).is_some()
    }
}

pub(crate) fn load_directory(
    dir: &str,
    origin: Origin,
    options: ViewOptions,
    cancel: &Cancellation,
) -> io::Result<RowView> {
    let entries = match origin {
        Origin::Recent => crate::files::recent::read_entries(cancel)?,
        _ => crate::files::fs::read_dir_cancellable(dir, cancel)?,
    };
    RowView::prepare(
        RowData::from_entries(entries, origin, cancel)?,
        options,
        cancel,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::model::{EntryKind, SortColumn, SortDirection};

    #[test]
    fn views_share_strings_and_match_all_sort_orders() {
        let entries = vec![
            Entry::new_with_meta(
                "/a",
                "a",
                EntryKind::File,
                Some(9),
                Some(3),
                Some("rw".into()),
            ),
            Entry::new_with_meta("/z", "z", EntryKind::Dir, None, Some(5), None),
            Entry::new_with_meta("/.hidden", ".hidden", EntryKind::File, None, None, None),
            Entry::new_with_meta("/b", "b", EntryKind::File, Some(2), Some(4), None),
        ];
        let name_pointer = entries[0].name.as_ptr();
        let token = Cancellation::default();
        let source = RowData::from_entries(entries, Origin::Directory, &token).unwrap();
        assert_eq!(
            source.rows[0].name.as_ptr(),
            name_pointer,
            "entry strings are moved into the snapshot"
        );
        for sort in [SortColumn::Name, SortColumn::Size, SortColumn::Modified] {
            for direction in [SortDirection::Asc, SortDirection::Desc] {
                for show_hidden in [false, true] {
                    let options = ViewOptions {
                        sort: SortSpec {
                            column: sort,
                            direction,
                        },
                        show_hidden,
                        ..Default::default()
                    };
                    let view = RowView::prepare(source.clone(), options.clone(), &token).unwrap();
                    let mut expected: Vec<_> = source
                        .rows
                        .iter()
                        .filter(|r| show_hidden || !r.name.starts_with('.'))
                        .collect();
                    expected.sort_by(|a, b| compare_visible_rows(a, b, options.sort));
                    assert_eq!(view.iter().collect::<Vec<_>>(), expected);
                    assert!(Arc::ptr_eq(&source, &view.source));
                    let row = view.get(view.position("/a").unwrap()).unwrap();
                    assert_eq!(
                        row.name.as_ptr(),
                        name_pointer,
                        "sorting never copies names"
                    );
                    assert_eq!(view.contains("/.hidden"), show_hidden);
                    assert_eq!(view.first().unwrap().kind, EntryKind::Dir);
                    assert!(Arc::ptr_eq(&view.order, &view.clone().order));
                }
            }
        }
    }

    #[test]
    fn superseding_or_dropping_jobs_cancels_workers_and_rejects_stale_results() {
        let mut job = JobSlot::default();
        let first = job.start();
        let second = job.start();
        assert_eq!(
            first.check().unwrap_err().kind(),
            io::ErrorKind::Interrupted
        );
        assert!(!job.finish(&first));
        assert!(job.finish(&second));
        assert!(!job.finish(&second), "a completion is applied at most once");
        let closing = job.start();
        drop(job);
        assert!(closing.check().is_err());
    }

    #[test]
    fn cancelled_prepare_never_publishes_partial_rows() {
        let token = Cancellation::default();
        let source = RowData::from_entries(
            vec![Entry::new("/x", "x", EntryKind::File)],
            Origin::Recent,
            &token,
        )
        .unwrap();
        assert_eq!(source.rows[0].original_path.as_deref(), Some("/"));
        token.cancel();
        assert!(RowView::prepare(source, ViewOptions::default(), &token).is_err());
        assert!(RowData::from_entries(Vec::new(), Origin::Directory, &token).is_err());
    }
}
