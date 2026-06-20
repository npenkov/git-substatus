//! Per-repo status scanning via gix.

use std::collections::BTreeMap;
use std::path::Path;

use crate::model::{Change, FileEntry, RepoState, RepoStatus};

/// Scan a single repository and return its status. Never panics: any failure is
/// captured into `RepoStatus.error` so the UI can show it inline.
pub fn scan_repo(path: &Path) -> RepoStatus {
    let name = path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string_lossy().into_owned());

    let mut status = RepoStatus {
        path: path.to_path_buf(),
        name,
        branch: None,
        ahead: 0,
        behind: 0,
        upstream: false,
        entries: Vec::new(),
        clean: true,
        error: None,
        state: RepoState::Ready,
    };

    match scan_inner(path) {
        Ok(data) => {
            status.branch = data.branch;
            status.entries = data.entries;
            status.clean = status.entries.is_empty();
            status.ahead = data.ahead;
            status.behind = data.behind;
            status.upstream = data.upstream;
        }
        Err(e) => status.error = Some(e.to_string()),
    }
    status
}

struct ScanData {
    branch: Option<String>,
    entries: Vec<FileEntry>,
    ahead: usize,
    behind: usize,
    upstream: bool,
}

fn scan_inner(path: &Path) -> anyhow::Result<ScanData> {
    let repo = gix::open(path)?;

    let branch = current_branch(&repo);
    let entries = changed_files(&repo)?;
    let (ahead, behind, upstream) = ahead_behind(path);

    Ok(ScanData {
        branch,
        entries,
        ahead,
        behind,
        upstream,
    })
}

/// Short branch name, or short commit id if detached.
fn current_branch(repo: &gix::Repository) -> Option<String> {
    if let Ok(Some(name)) = repo.head_name() {
        return Some(name.shorten().to_string());
    }
    // Detached HEAD: show the short object id.
    repo.head_id()
        .ok()
        .map(|id| id.to_hex_with_len(7).to_string())
}

/// Collect changed files, merging the staged (tree↔index) and unstaged
/// (index↔worktree) sides per path into a single `FileEntry`.
fn changed_files(repo: &gix::Repository) -> anyhow::Result<Vec<FileEntry>> {
    use gix::status::index_worktree::Item as IwItem;
    use gix::status::Item;

    let mut map: BTreeMap<String, FileEntry> = BTreeMap::new();

    let platform = repo
        .status(gix::progress::Discard)?
        .untracked_files(gix::status::UntrackedFiles::Collapsed);

    for item in platform.into_iter(None)? {
        let item = item?;
        let rela = item.location().to_string();
        let entry = map.entry(rela.clone()).or_insert_with(|| FileEntry {
            rel_path: rela,
            index: Change::None,
            worktree: Change::None,
        });

        match item {
            // Staged side: difference between HEAD tree and the index.
            Item::TreeIndex(change) => {
                entry.index = tree_index_change(&change);
            }
            // Unstaged / untracked side: difference between index and worktree.
            Item::IndexWorktree(iw) => {
                if let IwItem::Modification { status, .. } = &iw {
                    use gix::status::plumbing::index_as_worktree::{Change as C, EntryStatus};
                    entry.worktree = match status {
                        EntryStatus::Conflict { .. } => Change::Conflict,
                        EntryStatus::IntentToAdd => Change::Added,
                        EntryStatus::NeedsUpdate(_) => Change::None,
                        EntryStatus::Change(c) => match c {
                            C::Removed => Change::Deleted,
                            C::Type { .. } => Change::TypeChange,
                            C::Modification { .. } | C::SubmoduleModification(_) => Change::Modified,
                        },
                    };
                } else {
                    // DirectoryContents (untracked) or Rewrite.
                    entry.worktree = Change::Untracked;
                }
            }
        }
    }

    Ok(map.into_values().filter(|e| !e.is_clean()).collect())
}

fn tree_index_change(change: &gix::diff::index::Change) -> Change {
    use gix::diff::index::ChangeRef;
    match change {
        ChangeRef::Addition { .. } => Change::Added,
        ChangeRef::Deletion { .. } => Change::Deleted,
        ChangeRef::Modification { .. } => Change::Modified,
        ChangeRef::Rewrite { copy, .. } => {
            if *copy {
                Change::Copied
            } else {
                Change::Renamed
            }
        }
    }
}

impl FileEntry {
    fn is_clean(&self) -> bool {
        self.index == Change::None && self.worktree == Change::None
    }
}

/// Ahead/behind counts vs. the upstream tracking branch.
///
/// gix has no single high-level helper for this, so we shell out to
/// `git rev-list --left-right --count @{u}...HEAD` (one cheap call per scan).
/// Returns (ahead, behind, has_upstream).
fn ahead_behind(path: &Path) -> (usize, usize, bool) {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["rev-list", "--left-right", "--count", "@{u}...HEAD"])
        .output();

    match out {
        Ok(o) if o.status.success() => {
            let s = String::from_utf8_lossy(&o.stdout);
            let mut it = s.split_whitespace();
            let behind = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
            let ahead = it.next().and_then(|x| x.parse().ok()).unwrap_or(0);
            (ahead, behind, true)
        }
        // No upstream configured (or other failure): not an error to surface.
        _ => (0, 0, false),
    }
}

