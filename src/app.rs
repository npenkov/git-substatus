//! Application state and input handling.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;

use crate::actions::Action;
use crate::diff::{self, DiffLine};
use crate::event::AppEvent;
use crate::model::RepoStatus;
use crate::scan::Scanner;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    List,
    Detail,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Filter,
    Actions,
}

/// A visible row in the left pane.
#[derive(Clone, Copy)]
pub enum Row {
    Repo(usize),
    File { repo: usize, file: usize },
}

/// What the detail pane should show for the current selection.
pub enum Target<'a> {
    None,
    Repo(&'a RepoStatus),
    File { repo: &'a RepoStatus, rel: &'a str },
}

pub struct App {
    pub root: PathBuf,
    pub repos: Vec<RepoStatus>,
    pub expanded: HashSet<PathBuf>,

    pub selected: usize,
    pub list_state: ListState,
    pub focus: Focus,
    pub show_detail: bool,
    pub detail_scroll: u16,

    pub dirty_only: bool,
    pub filter: String,
    pub input_mode: InputMode,

    pub actions: Vec<Action>,
    diff_cache: HashMap<(PathBuf, String), Vec<DiffLine>>,

    scanning: HashSet<PathBuf>,
    pending: HashSet<PathBuf>,

    pub should_quit: bool,
    pub suspend_request: Option<(Action, PathBuf)>,
    pub status_msg: Option<String>,
    pub redraw: bool,
}

impl App {
    pub fn new(root: PathBuf, repos: Vec<RepoStatus>, actions: Vec<Action>, dirty_only: bool) -> Self {
        let scanning = repos.iter().map(|r| r.path.clone()).collect();
        let mut app = App {
            root,
            repos,
            expanded: HashSet::new(),
            selected: 0,
            list_state: ListState::default(),
            focus: Focus::List,
            show_detail: false,
            detail_scroll: 0,
            dirty_only,
            filter: String::new(),
            input_mode: InputMode::Normal,
            actions,
            diff_cache: HashMap::new(),
            scanning,
            pending: HashSet::new(),
            should_quit: false,
            suspend_request: None,
            status_msg: None,
            redraw: true,
        };
        app.list_state.select(Some(0));
        app
    }

    // ---- async events -----------------------------------------------------

    pub fn on_event(&mut self, ev: AppEvent, scanner: &Scanner) {
        match ev {
            AppEvent::RepoUpdate(status) => self.apply_update(status, scanner),
            AppEvent::FsDirty(paths) => self.on_fs_dirty(paths, scanner),
        }
        self.redraw = true;
    }

    fn apply_update(&mut self, status: RepoStatus, scanner: &Scanner) {
        let path = status.path.clone();
        // Invalidate cached diffs for this repo.
        self.diff_cache.retain(|(p, _), _| p != &path);

        if let Some(slot) = self.repos.iter_mut().find(|r| r.path == path) {
            *slot = status;
        } else {
            self.repos.push(status);
            self.repos.sort_by(|a, b| a.path.cmp(&b.path));
        }
        self.scanning.remove(&path);
        // If changes landed while scanning, scan once more.
        if self.pending.remove(&path) {
            self.scanning.insert(path.clone());
            scanner.scan(path);
        }
    }

    fn on_fs_dirty(&mut self, paths: Vec<PathBuf>, scanner: &Scanner) {
        let mut to_scan: HashSet<PathBuf> = HashSet::new();
        for p in paths {
            if let Some(root) = self.owning_repo(&p) {
                to_scan.insert(root);
            }
        }
        for root in to_scan {
            if self.scanning.contains(&root) {
                self.pending.insert(root); // coalesce
            } else {
                self.scanning.insert(root.clone());
                scanner.scan(root);
            }
        }
    }

    /// Longest repo-root prefix of `path`.
    fn owning_repo(&self, path: &std::path::Path) -> Option<PathBuf> {
        self.repos
            .iter()
            .filter(|r| path.starts_with(&r.path))
            .max_by_key(|r| r.path.as_os_str().len())
            .map(|r| r.path.clone())
    }

    pub fn rescan_all(&mut self, scanner: &Scanner) {
        for r in &self.repos {
            if self.scanning.insert(r.path.clone()) {
                scanner.scan(r.path.clone());
            }
        }
        self.status_msg = Some("rescanning all…".into());
    }

    // ---- view model -------------------------------------------------------

    /// Indices of repos passing the current filters, in display order.
    pub fn visible_repos(&self) -> Vec<usize> {
        let needle = self.filter.to_lowercase();
        self.repos
            .iter()
            .enumerate()
            .filter(|(_, r)| !self.dirty_only || !r.clean || r.error.is_some())
            .filter(|(_, r)| needle.is_empty() || r.name.to_lowercase().contains(&needle))
            .map(|(i, _)| i)
            .collect()
    }

    /// Flattened rows (repo headers + expanded file rows).
    pub fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        for ri in self.visible_repos() {
            rows.push(Row::Repo(ri));
            if self.expanded.contains(&self.repos[ri].path) {
                for fi in 0..self.repos[ri].entries.len() {
                    rows.push(Row::File { repo: ri, file: fi });
                }
            }
        }
        rows
    }

    pub fn current_row(&self) -> Option<Row> {
        self.rows().get(self.selected).copied()
    }

    pub fn current_target(&self) -> Target<'_> {
        match self.current_row() {
            Some(Row::Repo(ri)) => Target::Repo(&self.repos[ri]),
            Some(Row::File { repo, file }) => Target::File {
                repo: &self.repos[repo],
                rel: &self.repos[repo].entries[file].rel_path,
            },
            None => Target::None,
        }
    }

    /// Diff for the currently selected file (cached). Empty if not a file row.
    pub fn current_diff(&mut self) -> Vec<DiffLine> {
        let (path, rel) = match self.current_row() {
            Some(Row::File { repo, file }) => (
                self.repos[repo].path.clone(),
                self.repos[repo].entries[file].rel_path.clone(),
            ),
            _ => return Vec::new(),
        };
        let key = (path.clone(), rel.clone());
        if let Some(d) = self.diff_cache.get(&key) {
            return d.clone();
        }
        let d = diff::diff_for_file(&path, &rel);
        self.diff_cache.insert(key, d.clone());
        d
    }

    pub fn scan_count(&self) -> usize {
        self.scanning.len()
    }

    // ---- input ------------------------------------------------------------

    pub fn on_key(&mut self, key: KeyEvent, scanner: &Scanner) {
        self.redraw = true;
        self.status_msg = None;
        match self.input_mode {
            InputMode::Filter => self.on_key_filter(key),
            InputMode::Actions => self.on_key_actions(key),
            InputMode::Normal => self.on_key_normal(key, scanner),
        }
    }

    fn on_key_normal(&mut self, key: KeyEvent, scanner: &Scanner) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('c') if ctrl => self.should_quit = true,
            KeyCode::Char('q') => self.should_quit = true,
            // Esc closes the detail panel if open; otherwise quits.
            KeyCode::Esc => {
                if self.show_detail {
                    self.show_detail = false;
                    self.focus = Focus::List;
                } else {
                    self.should_quit = true;
                }
            }
            KeyCode::Tab => self.toggle_detail(),
            KeyCode::Char('d') => {
                self.dirty_only = !self.dirty_only;
                self.clamp_selection();
            }
            KeyCode::Char('r') => self.rescan_all(scanner),
            KeyCode::Char('/') => {
                self.input_mode = InputMode::Filter;
            }
            KeyCode::Char('a') => {
                if !self.actions.is_empty() {
                    self.input_mode = InputMode::Actions;
                }
            }
            KeyCode::Char('g') => self.select(0),
            KeyCode::Char('G') => self.select(self.rows().len().saturating_sub(1)),
            KeyCode::Down | KeyCode::Char('j') => self.move_down(),
            KeyCode::Up | KeyCode::Char('k') => self.move_up(),
            KeyCode::Enter | KeyCode::Char('l') | KeyCode::Right => self.expand_or_focus(),
            KeyCode::Char('h') | KeyCode::Left => self.collapse(),
            _ => {}
        }
    }

    fn on_key_filter(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => {
                self.filter.clear();
                self.input_mode = InputMode::Normal;
                self.clamp_selection();
            }
            KeyCode::Enter => self.input_mode = InputMode::Normal,
            KeyCode::Backspace => {
                self.filter.pop();
                self.clamp_selection();
            }
            KeyCode::Char(c) => {
                self.filter.push(c);
                self.select(0);
            }
            _ => {}
        }
    }

    fn on_key_actions(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.input_mode = InputMode::Normal,
            KeyCode::Char(c) => {
                if let Some(action) = self.actions.iter().find(|a| a.key_char() == Some(c)).cloned() {
                    self.input_mode = InputMode::Normal;
                    self.run_action(action);
                }
            }
            _ => {}
        }
    }

    fn run_action(&mut self, action: Action) {
        let Some(repo) = self.selected_repo_path() else {
            return;
        };
        if action.suspend {
            // Defer to main, which owns the terminal.
            self.suspend_request = Some((action, repo));
        } else {
            match crate::actions::spawn_detached(&action, &repo) {
                Ok(()) => self.status_msg = Some(format!("ran: {}", action.name)),
                Err(e) => self.status_msg = Some(format!("action failed: {e}")),
            }
        }
    }

    /// The repo path for the current selection (header or file row).
    pub fn selected_repo_path(&self) -> Option<PathBuf> {
        match self.current_row() {
            Some(Row::Repo(ri)) => Some(self.repos[ri].path.clone()),
            Some(Row::File { repo, .. }) => Some(self.repos[repo].path.clone()),
            None => None,
        }
    }

    /// Show/hide the detail panel. Showing it moves focus there; hiding returns to the list.
    fn toggle_detail(&mut self) {
        self.show_detail = !self.show_detail;
        self.focus = if self.show_detail {
            Focus::Detail
        } else {
            Focus::List
        };
    }

    fn move_down(&mut self) {
        if self.focus == Focus::Detail {
            self.detail_scroll = self.detail_scroll.saturating_add(1);
        } else {
            self.select(self.selected.saturating_add(1));
        }
    }

    fn move_up(&mut self) {
        if self.focus == Focus::Detail {
            self.detail_scroll = self.detail_scroll.saturating_sub(1);
        } else {
            self.select(self.selected.saturating_sub(1));
        }
    }

    fn select(&mut self, idx: usize) {
        let len = self.rows().len();
        self.selected = if len == 0 { 0 } else { idx.min(len - 1) };
        self.list_state.select(Some(self.selected));
        self.detail_scroll = 0;
    }

    fn clamp_selection(&mut self) {
        let len = self.rows().len();
        if self.selected >= len {
            self.selected = len.saturating_sub(1);
        }
        self.list_state.select(Some(self.selected));
    }

    fn expand_or_focus(&mut self) {
        match self.current_row() {
            Some(Row::Repo(ri)) => {
                let path = self.repos[ri].path.clone();
                if self.repos[ri].entries.is_empty() {
                    return;
                }
                if !self.expanded.insert(path.clone()) {
                    self.expanded.remove(&path);
                }
                self.clamp_selection();
            }
            Some(Row::File { .. }) => {
                // Opening a file's diff reveals the detail panel.
                self.show_detail = true;
                self.focus = Focus::Detail;
            }
            None => {}
        }
    }

    fn collapse(&mut self) {
        match self.current_row() {
            Some(Row::Repo(ri)) => {
                let path = self.repos[ri].path.clone();
                self.expanded.remove(&path);
            }
            Some(Row::File { repo, .. }) => {
                // Jump back to the repo header and collapse it.
                let path = self.repos[repo].path.clone();
                self.expanded.remove(&path);
                if let Some(pos) = self
                    .rows()
                    .iter()
                    .position(|r| matches!(r, Row::Repo(ri) if *ri == repo))
                {
                    self.select(pos);
                }
            }
            None => {}
        }
        self.focus = Focus::List;
    }
}

/// Build an initial App with `scanning` placeholders so repos show up immediately.
pub fn initial_repos(paths: &[PathBuf]) -> Vec<RepoStatus> {
    paths
        .iter()
        .map(|p| RepoStatus::scanning(p.clone()))
        .collect()
}
