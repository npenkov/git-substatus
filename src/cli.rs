//! Command-line arguments.

use std::path::PathBuf;

use clap::Parser;

/// Interactive multi-repo git status TUI.
#[derive(Parser, Debug, Clone)]
#[command(name = "git-substatus", version, about)]
pub struct Args {
    /// Parent directory containing the repos to scan.
    #[arg(default_value = ".")]
    pub root: PathBuf,

    /// How deep to look for repos: 1 = direct children, 2 = grandchildren too.
    #[arg(short, long, default_value_t = 1)]
    pub depth: usize,

    /// Only show repos with changes.
    #[arg(long)]
    pub dirty_only: bool,

    /// Disable filesystem watching (no auto-refresh).
    #[arg(long)]
    pub no_watch: bool,

    /// Path to the actions config file (default: ~/.config/git-substatus/config.toml).
    #[arg(long)]
    pub config: Option<PathBuf>,
}
