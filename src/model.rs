//! Core data types shared across modules.

use std::path::PathBuf;

/// One changed file within a repo, with its staged (index) and unstaged (worktree) change.
#[derive(Clone, Debug)]
pub struct FileEntry {
    pub rel_path: String,
    pub index: Change,
    pub worktree: Change,
}

impl FileEntry {
    /// Two-column porcelain-style code, e.g. "M ", " M", "A ", "??".
    pub fn xy(&self) -> [char; 2] {
        // Untracked is shown as "??" in both columns, like `git status -s`.
        if self.worktree == Change::Untracked && self.index == Change::None {
            return ['?', '?'];
        }
        [self.index.code_index(), self.worktree.code_worktree()]
    }
}

/// The kind of change on one side (index or worktree) for a path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
pub enum Change {
    None,
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
    TypeChange,
    Untracked,
    Conflict,
}

impl Change {
    /// Code for the staged (index, "X") column.
    pub fn code_index(self) -> char {
        match self {
            Change::None => ' ',
            Change::Added => 'A',
            Change::Modified => 'M',
            Change::Deleted => 'D',
            Change::Renamed => 'R',
            Change::Copied => 'C',
            Change::TypeChange => 'T',
            Change::Untracked => '?',
            Change::Conflict => 'U',
        }
    }

    /// Code for the unstaged (worktree, "Y") column. Untracked shows as '?'.
    pub fn code_worktree(self) -> char {
        match self {
            Change::Untracked => '?',
            other => other.code_index(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RepoState {
    Scanning,
    Ready,
}

/// Full status snapshot for a single repository.
#[derive(Clone, Debug)]
pub struct RepoStatus {
    pub path: PathBuf,
    pub name: String,
    pub branch: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    pub upstream: bool,
    pub entries: Vec<FileEntry>,
    pub clean: bool,
    pub error: Option<String>,
    pub state: RepoState,
}

impl RepoStatus {
    /// A placeholder shown immediately while the first scan is in flight.
    pub fn scanning(path: PathBuf) -> Self {
        let name = path
            .file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned());
        RepoStatus {
            path,
            name,
            branch: None,
            ahead: 0,
            behind: 0,
            upstream: false,
            entries: Vec::new(),
            clean: true,
            error: None,
            state: RepoState::Scanning,
        }
    }
}
