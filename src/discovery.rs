//! Recursive git-repo discovery under a root directory.

use std::path::{Path, PathBuf};

/// Return the sorted list of git repositories under `root`, up to `depth` levels deep.
///
/// `depth == 1` looks at direct children of `root`; `depth == 2` also looks at
/// grandchildren, and so on. A directory counts as a repo if it contains a `.git`
/// entry (directory for normal repos, file for worktrees/submodules). Once a repo is
/// found we do not descend into it.
pub fn find_repos(root: &Path, depth: usize) -> Vec<PathBuf> {
    let mut repos = Vec::new();
    collect(root, depth, &mut repos);
    repos.sort();
    repos
}

fn is_repo(dir: &Path) -> bool {
    dir.join(".git").exists()
}

fn collect(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
    if depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        // Skip hidden dirs (e.g. .git itself, .cache) to avoid noise.
        if path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with('.'))
        {
            continue;
        }
        if is_repo(&path) {
            out.push(path);
        } else {
            collect(&path, depth - 1, out);
        }
    }
}
