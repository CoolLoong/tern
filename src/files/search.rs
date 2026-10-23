use std::path::Path;

use crate::files::model::{Entry, EntryKind};

/// Upper bounds so a search from a huge tree cannot run away. The result cap is
/// generous for a file manager; the depth cap is what keeps a symlink loop from
/// walking forever.
const MAX_RESULTS: usize = 2000;
const MAX_DEPTH: usize = 12;

/// Case-insensitive match. A query containing `*` or `?` is treated as a glob
/// over the whole name, anything else as a substring.
pub(crate) fn matches(name: &str, query: &str) -> bool {
    let name = name.to_lowercase();
    let query = query.trim().to_lowercase();
    if query.is_empty() {
        return true;
    }
    if query.contains('*') || query.contains('?') {
        glob_match(name.as_bytes(), query.as_bytes())
    } else {
        name.contains(&query)
    }
}

/// Iterative `*` / `?` matcher. `?` matches any single byte, `*` any run —
/// byte-wise is fine here because both sides are already lowercased UTF-8 and a
/// `?` landing mid-codepoint simply fails to match, which is acceptable for a
/// file filter.
fn glob_match(name: &[u8], pattern: &[u8]) -> bool {
    let (mut n, mut p) = (0usize, 0usize);
    let (mut star, mut resume) = (None, 0usize);

    while n < name.len() {
        if p < pattern.len() && (pattern[p] == b'?' || pattern[p] == name[n]) {
            n += 1;
            p += 1;
        } else if p < pattern.len() && pattern[p] == b'*' {
            star = Some(p);
            resume = n;
            p += 1;
        } else if let Some(star_ix) = star {
            p = star_ix + 1;
            resume += 1;
            n = resume;
        } else {
            return false;
        }
    }
    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
}

/// What a walk found, and whether it stopped before covering the tree.
pub(crate) struct SearchOutcome {
    pub entries: Vec<Entry>,
    /// True when [`MAX_RESULTS`] cut the walk short before remaining entries
    /// were checked; additional matches may exist. Hitting [`MAX_DEPTH`] does
    /// not set this: that branch is skipped, and results elsewhere are still
    /// found.
    pub truncated: bool,
}

/// Walks `root`'s subtree and returns the entries whose name matches `query`.
/// Hidden names are skipped unless `show_hidden`, and directory symlinks are not
/// followed. Branches deeper than [`MAX_DEPTH`] are skipped, and the walk stops
/// once [`MAX_RESULTS`] matches are collected.
#[cfg(test)]
pub(crate) fn search(root: &Path, query: &str, show_hidden: bool) -> SearchOutcome {
    search_cancellable(
        root,
        query,
        show_hidden,
        &crate::files::jobs::Cancellation::default(),
    )
    .expect("uncancelled search")
}

pub(crate) fn search_cancellable(
    root: &Path,
    query: &str,
    show_hidden: bool,
    cancel: &crate::files::jobs::Cancellation,
) -> std::io::Result<SearchOutcome> {
    cancel.check()?;
    let mut out = Vec::new();
    let mut truncated = false;
    let mut stack = vec![(root.to_path_buf(), 0usize)];

    while let Some((dir, depth)) = stack.pop() {
        cancel.check()?;
        if out.len() >= MAX_RESULTS {
            // The result cap is what makes the list a prefix.
            truncated = true;
            break;
        }
        if depth > MAX_DEPTH {
            // Skip this branch only. Real trees (build output, vendored deps)
            // routinely exceed the cap in one branch, and abandoning the whole
            // walk there would silently drop every other directory.
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        let mut entries = entries;
        loop {
            cancel.check()?;
            let Some(entry) = entries.next() else { break };
            let Ok(entry) = entry else { continue };
            if out.len() >= MAX_RESULTS {
                break;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if !show_hidden && crate::files::fs::is_hidden_name(&name) {
                continue;
            }
            let path = entry.path();
            cancel.check()?;
            let Ok(meta) = entry.metadata() else {
                continue;
            };
            let is_dir = meta.is_dir();
            // `metadata()` follows symlinks, so a link to a directory would look
            // like one. Descend only into real directories to avoid cycles.
            let descend = is_dir && !entry.file_type().map(|t| t.is_symlink()).unwrap_or(true);

            if matches(&name, query) {
                out.push(entry_from(&path, name.clone(), &meta));
                if out.len() == MAX_RESULTS {
                    // The current directory or queued subdirectories still
                    // contain unvisited entries, so report the partial walk.
                    truncated = descend || !stack.is_empty() || entries.next().is_some();
                    break;
                }
            }
            if descend {
                stack.push((path, depth + 1));
            }
        }
    }

    // The browser prepares its chosen sort order once, on this same worker
    // path. Sorting by modification time here would immediately be discarded.
    cancel.check()?;
    Ok(SearchOutcome {
        entries: out,
        truncated,
    })
}

fn entry_from(path: &Path, name: String, meta: &std::fs::Metadata) -> Entry {
    let kind = if meta.is_dir() {
        EntryKind::Dir
    } else if meta.file_type().is_symlink() {
        EntryKind::Symlink
    } else if meta.is_file() {
        EntryKind::File
    } else {
        EntryKind::Other
    };
    let size = (!meta.is_dir()).then_some(meta.len());
    let modified = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_secs());
    Entry::new_with_meta(
        path.to_string_lossy(),
        name,
        kind,
        size,
        modified,
        Some(crate::files::fs::format_perms(meta)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_cancelled_search_does_not_walk_or_return_partial_results() {
        let token = crate::files::jobs::Cancellation::default();
        token.cancel();
        let result = search_cancellable(Path::new("/not-opened"), "*", true, &token);
        assert!(matches!(result, Err(error) if error.kind() == std::io::ErrorKind::Interrupted));
    }

    #[test]
    fn substring_is_case_insensitive() {
        assert!(matches("README.md", "readme"));
        assert!(matches("readme.md", "README"));
        assert!(!matches("readme.md", "zzz"));
    }

    #[test]
    fn glob_covers_star_and_question() {
        assert!(matches("a.txt", "*.txt"));
        assert!(matches("a.txt", "a?txt"));
        assert!(matches("report-2026.txt", "report-*.txt"));
        assert!(matches("anything", "*"));
        assert!(!matches("a.md", "*.txt"));
        assert!(!matches("abcdef", "a?c"));
        // A trailing star must not require more input.
        assert!(matches("ab", "ab*"));
    }

    #[test]
    fn deep_branch_is_skipped_without_abandoning_the_walk() {
        // One deep branch must not hide results that live elsewhere: real trees
        // (build output, vendored deps) exceed the depth cap routinely.
        let root = std::env::temp_dir().join(format!("tern-depth-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);

        let mut deep = root.clone();
        for level in 0..(MAX_DEPTH + 3) {
            deep = deep.join(format!("l{level}"));
        }
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::write(deep.join("buried.txt"), b"x").unwrap();
        std::fs::write(root.join("shallow.txt"), b"x").unwrap();

        let found = search(&root, "txt", false);
        let names: Vec<&str> = found.entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, vec!["shallow.txt"], "the shallow hit survives");
        assert!(!found.truncated, "nothing was capped");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn result_cap_is_reported_as_truncated() {
        let root = std::env::temp_dir().join(format!("tern-cap-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        for ix in 0..(MAX_RESULTS + 5) {
            std::fs::write(root.join(format!("hit{ix}.txt")), b"x").unwrap();
        }

        let found = search(&root, "hit", false);
        assert_eq!(found.entries.len(), MAX_RESULTS);
        assert!(found.truncated, "the cap must be visible to the caller");

        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn search_walks_subdirectories_and_skips_hidden() {
        let root = std::env::temp_dir().join(format!("tern-search-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("deep/deeper")).unwrap();
        std::fs::write(root.join("hit.txt"), b"x").unwrap();
        std::fs::write(root.join("deep/hit.txt"), b"x").unwrap();
        std::fs::write(root.join("deep/.hidden-hit.txt"), b"x").unwrap();

        let found = search(&root, "hit", false);
        assert_eq!(
            found.entries.len(),
            2,
            "both copies found, hidden one skipped"
        );
        assert!(!found.truncated);

        let globbed = search(&root, "*.txt", false);
        assert_eq!(globbed.entries.len(), 2);

        let with_hidden = search(&root, "hit", true);
        assert_eq!(with_hidden.entries.len(), 3);

        std::fs::remove_dir_all(&root).ok();
    }
}
