mod actions;
mod app;
mod cli;
mod diff;
mod discovery;
mod event;
mod git;
mod model;
mod scan;
mod ui;
mod watcher;

use std::time::Duration;

use clap::Parser;
use ratatui::crossterm::event as cev;
use ratatui::crossterm::event::{Event, KeyEventKind};
use ratatui::DefaultTerminal;

use crate::app::App;
use crate::scan::Scanner;

fn main() -> anyhow::Result<()> {
    let args = cli::Args::parse();
    let root = args.root.canonicalize().unwrap_or(args.root.clone());

    let repos = discovery::find_repos(&root, args.depth);
    if repos.is_empty() {
        eprintln!("no git repos found under {}", root.display());
        return Ok(());
    }

    let (tx, rx) = crossbeam_channel::unbounded::<event::AppEvent>();
    let scanner = Scanner::new(tx.clone());

    // Initial parallel load.
    scanner.scan_all(&repos);

    // Filesystem watching (kept alive for the program's lifetime).
    let _watcher = if args.no_watch {
        None
    } else {
        watcher::spawn(&repos, tx.clone())
    };

    let actions = actions::load(args.config.clone());
    let mut app = App::new(root, app::initial_repos(&repos), actions, args.dirty_only);

    let mut terminal = ratatui::init();
    let result = run(&mut terminal, &mut app, &scanner, &rx);
    ratatui::restore();
    result
}

fn run(
    terminal: &mut DefaultTerminal,
    app: &mut App,
    scanner: &Scanner,
    rx: &event::Receiver,
) -> anyhow::Result<()> {
    terminal.draw(|f| ui::render(f, app))?;

    loop {
        // 1. Poll for keyboard input (also acts as the redraw tick).
        if cev::poll(Duration::from_millis(120))? {
            match cev::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    app.on_key(key, scanner);
                }
                Event::Resize(_, _) => app.redraw = true,
                _ => {}
            }
        }

        // 2. Drain async events from the scan pool / watcher.
        while let Ok(ev) = rx.try_recv() {
            app.on_event(ev, scanner);
        }

        // 3. Handle a deferred suspend action (interactive program in this terminal).
        if let Some((action, path)) = app.suspend_request.take() {
            run_suspended(terminal, &action, &path)?;
            app.redraw = true;
        }

        if app.should_quit {
            break;
        }

        // 4. Redraw only when something changed.
        if app.redraw {
            terminal.draw(|f| ui::render(f, app))?;
            app.redraw = false;
        }
    }
    Ok(())
}

/// Leave the TUI, run an interactive command in `path`, then restore the TUI.
fn run_suspended(
    terminal: &mut DefaultTerminal,
    action: &actions::Action,
    path: &std::path::Path,
) -> anyhow::Result<()> {
    ratatui::restore();
    let status = actions::build_suspending(action, path).status();
    // Re-enter the alternate screen / raw mode no matter how the child exited.
    *terminal = ratatui::init();
    terminal.clear()?;
    if let Err(e) = status {
        eprintln!("action '{}' failed: {e}", action.name);
    }
    Ok(())
}
