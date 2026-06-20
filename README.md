# git-substatus

An interactive terminal UI (ratatui) that shows the git status of many repositories
at once — designed for a directory full of worktrees/clones, and for living in a tmux
side pane. It is the "next level" successor to a polling shell script: status is loaded
**in parallel** and refreshed **only when the filesystem actually changes** (no polling).

## Features

- **Recursive discovery** of git repos under a root, configurable depth.
- **Parallel scanning** (rayon) — repos fill in as they finish.
- **Event-driven refresh** — a debounced filesystem watcher (notify) re-scans *only*
  the repos that changed. Idle CPU is ~0%.
- **Pure-Rust git** via [`gix`](https://github.com/GitoxideLabs/gitoxide) for status and
  branch info (ahead/behind uses a single `git rev-list` per scan).
- **In-app diffs** — select a file to see a colored unified diff (built with
  [`imara-diff`](https://github.com/pascalkuthe/imara-diff), gix's own diff engine).
- **Navigation** — expand/collapse file lists, scroll, dirty-only toggle, fuzzy filter.
- **Custom actions** — run configurable shell commands against the selected repo
  (open a tmux pane there, launch lazygit/nvim, etc.).

## Install

```sh
cargo install --path .
```

The binary `git-substatus` lands in `~/.cargo/bin`.

## Usage

```
git-substatus [ROOT] [--depth N] [--dirty-only] [--no-watch] [--config PATH]
```

- `ROOT` — parent directory to scan (default: current dir).
- `--depth` — 1 = direct children (default), 2 = also grandchildren.
- `--dirty-only` — start with only repos that have changes.
- `--no-watch` — disable the filesystem watcher.
- `--config` — path to the actions config (default `~/.config/git-substatus/config.toml`).

### Keys

| Key | Action |
|-----|--------|
| `j` / `k` / ↓ ↑ | move (or scroll the diff pane when it's focused) |
| `l` / `Enter` / → | expand a repo; on a file, focus the diff pane |
| `h` / ← | collapse |
| `Tab` | switch focus between the list and the diff pane |
| `d` | toggle dirty-only |
| `/` | fuzzy filter by repo name (`Enter` keep, `Esc` clear) |
| `a` | actions popup |
| `r` | force rescan all |
| `g` / `G` | top / bottom |
| `Esc` | leave the diff pane, or quit from the list |
| `q` / `Ctrl-c` | quit |

## Actions

If no config exists, built-in defaults are used (tmux split, tmux window, lazygit, nvim).
Create `~/.config/git-substatus/config.toml` to customize:

```toml
[[actions]]
key = "t"
name = "tmux split here"
command = "tmux split-window -h -c {path}"
suspend = false        # spawns in tmux; the TUI keeps running

[[actions]]
key = "g"
name = "lazygit"
command = "lazygit"
cwd = "{path}"
suspend = true         # takes over this terminal; TUI is restored on exit
```

- `{path}` is replaced with the selected repo's path (shell-escaped in `command`).
- `suspend = true` for interactive programs that need this terminal; `false` for things
  that appear elsewhere (tmux panes/windows/popups).

## tmux

Open it in a 50-column right-hand pane for the current directory:

```tmux
bind C-s split-window -h -l 50 "cd '#{pane_current_path}' && git-substatus"
```
