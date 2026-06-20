//! User-configurable shell actions run against the selected repo.

use std::path::PathBuf;
use std::process::{Command, Stdio};

use serde::Deserialize;

/// One configurable action, e.g. "open lazygit here".
#[derive(Clone, Debug, Deserialize)]
pub struct Action {
    /// Single-character trigger key shown in the actions popup.
    pub key: String,
    /// Human-readable label.
    pub name: String,
    /// Shell command; `{path}` is replaced with the (shell-escaped) repo path.
    pub command: String,
    /// Working directory; `{path}` allowed. Defaults to the repo path.
    #[serde(default)]
    pub cwd: Option<String>,
    /// If true, the TUI is suspended while the command runs in this terminal.
    #[serde(default)]
    pub suspend: bool,
}

impl Action {
    pub fn key_char(&self) -> Option<char> {
        self.key.chars().next()
    }
}

/// Substitution context derived from the currently selected item.
///
/// - `path`: the selected item — the file's absolute path when a file is selected,
///   otherwise the repo root.
/// - `dir`: a directory to run in — the file's parent directory when a file is
///   selected, otherwise the repo root. Always a valid directory (default `cwd`).
/// - `repo`: the repo root.
#[derive(Clone, Debug)]
pub struct Ctx {
    pub path: PathBuf,
    pub dir: PathBuf,
    pub repo: PathBuf,
}

impl Ctx {
    /// What the action targets, for display in the popup title.
    pub fn label(&self) -> String {
        self.path.display().to_string()
    }
}

#[derive(Deserialize, Default)]
struct Config {
    #[serde(default)]
    actions: Vec<Action>,
}

/// Load actions from `path` (or the default location). Falls back to built-in
/// defaults if no config file exists or it can't be parsed.
pub fn load(path: Option<PathBuf>) -> Vec<Action> {
    let path = path.or_else(default_config_path);
    if let Some(p) = path
        && let Ok(text) = std::fs::read_to_string(&p)
        && let Ok(cfg) = toml::from_str::<Config>(&text)
        && !cfg.actions.is_empty()
    {
        return cfg.actions;
    }
    defaults()
}

fn default_config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("git-substatus").join("config.toml"))
}

/// Built-in actions used when there is no config file.
pub fn defaults() -> Vec<Action> {
    vec![
        Action {
            key: "t".into(),
            name: "tmux split here".into(),
            command: "tmux split-window -h -c {dir}".into(),
            cwd: None,
            suspend: false,
        },
        Action {
            key: "w".into(),
            name: "tmux window here".into(),
            command: "tmux new-window -c {dir}".into(),
            cwd: None,
            suspend: false,
        },
        Action {
            key: "g".into(),
            name: "lazygit".into(),
            command: "lazygit".into(),
            cwd: Some("{repo}".into()),
            suspend: true,
        },
        Action {
            // Opens the selected file, or the repo dir if a repo row is selected.
            key: "v".into(),
            name: "nvim".into(),
            command: "nvim {path}".into(),
            cwd: Some("{dir}".into()),
            suspend: true,
        },
    ]
}

/// Build the `Command` for an action against the selection `ctx`, without running it.
///
/// In `command`, `{path}`/`{dir}`/`{repo}` are substituted shell-escaped; in `cwd`
/// they are substituted raw (it is a path, not parsed by a shell). `cwd` defaults to
/// `{dir}` — the directory of the selected item.
fn build(action: &Action, ctx: &Ctx) -> Command {
    let path = ctx.path.to_string_lossy();
    let dir = ctx.dir.to_string_lossy();
    let repo = ctx.repo.to_string_lossy();

    let q = |s: &str| shell_words::quote(s).into_owned();
    let cmd_str = action
        .command
        .replace("{path}", &q(&path))
        .replace("{dir}", &q(&dir))
        .replace("{repo}", &q(&repo));

    let cwd = action
        .cwd
        .as_deref()
        .unwrap_or("{dir}")
        .replace("{path}", &path)
        .replace("{dir}", &dir)
        .replace("{repo}", &repo);

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    let mut cmd = Command::new(shell);
    cmd.arg("-c").arg(cmd_str).current_dir(cwd);
    cmd
}

/// Run a non-suspending action: spawn detached with stdio nulled so it can't draw
/// over our TUI (right for tmux split/new-window/popup).
pub fn spawn_detached(action: &Action, ctx: &Ctx) -> std::io::Result<()> {
    let mut cmd = build(action, ctx);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd.spawn().map(|_| ())
}

/// Build a suspending command (interactive program that owns this terminal).
/// The caller is responsible for restoring/re-initialising the terminal around it.
pub fn build_suspending(action: &Action, ctx: &Ctx) -> Command {
    build(action, ctx)
}
