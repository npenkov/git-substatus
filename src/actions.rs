//! User-configurable shell actions run against the selected repo.

use std::path::{Path, PathBuf};
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
            command: "tmux split-window -h -c {path}".into(),
            cwd: None,
            suspend: false,
        },
        Action {
            key: "w".into(),
            name: "tmux window here".into(),
            command: "tmux new-window -c {path}".into(),
            cwd: None,
            suspend: false,
        },
        Action {
            key: "g".into(),
            name: "lazygit".into(),
            command: "lazygit".into(),
            cwd: Some("{path}".into()),
            suspend: true,
        },
        Action {
            key: "v".into(),
            name: "nvim".into(),
            command: "nvim".into(),
            cwd: Some("{path}".into()),
            suspend: true,
        },
    ]
}

/// Build the `Command` for an action against `repo_path`, without running it.
fn build(action: &Action, repo_path: &Path) -> Command {
    let path_str = repo_path.to_string_lossy().to_string();
    let escaped = shell_words::quote(&path_str).into_owned();
    let cmd_str = action.command.replace("{path}", &escaped);
    let cwd = action
        .cwd
        .as_ref()
        .map(|c| c.replace("{path}", &path_str))
        .unwrap_or_else(|| path_str.clone());

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());
    let mut cmd = Command::new(shell);
    cmd.arg("-c").arg(cmd_str).current_dir(cwd);
    cmd
}

/// Run a non-suspending action: spawn detached with stdio nulled so it can't draw
/// over our TUI (right for tmux split/new-window/popup).
pub fn spawn_detached(action: &Action, repo_path: &Path) -> std::io::Result<()> {
    let mut cmd = build(action, repo_path);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    cmd.spawn().map(|_| ())
}

/// Build a suspending command (interactive program that owns this terminal).
/// The caller is responsible for restoring/re-initialising the terminal around it.
pub fn build_suspending(action: &Action, repo_path: &Path) -> Command {
    build(action, repo_path)
}
