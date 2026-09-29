//! Filesystem watching: one recursive watch per repo, debounced, mapped back to
//! the owning repo so only changed repos get re-scanned.

use std::path::PathBuf;
use std::time::Duration;

use notify::event::{AccessKind, AccessMode, EventKind};
use notify::RecursiveMode;
use notify_debouncer_full::{new_debouncer_opt, DebounceEventResult, Debouncer, NoCache};

use crate::event::{AppEvent, Sender};

/// Keep the returned debouncer alive for as long as watching is desired; dropping
/// it stops all watches.
///
/// We use `NoCache` rather than the default file-id cache: the default walks every
/// watched tree on setup to record inode ids (≈4.4s over 40 repos with node_modules
/// here), which we don't need — we only care *that* a repo changed, not file identity.
/// `NoCache` makes recursive-watch setup ≈180× faster (tens of ms).
pub type Watcher = Debouncer<notify::RecommendedWatcher, NoCache>;

/// Start watching every repo root recursively. Debounced batches of changed paths
/// are forwarded as `AppEvent::FsDirty`. Returns `None` if the watcher could not be
/// created (watching is then simply disabled).
pub fn spawn(repos: &[PathBuf], tx: Sender) -> Option<Watcher> {
    let mut debouncer = new_debouncer_opt::<_, notify::RecommendedWatcher, NoCache>(
        Duration::from_millis(400),
        None,
        move |result: DebounceEventResult| {
            if let Ok(events) = result {
                let paths: Vec<PathBuf> = events
                    .into_iter()
                    .filter(|e| is_mutation(&e.event.kind))
                    .flat_map(|e| e.event.paths)
                    .collect();
                if !paths.is_empty() {
                    let _ = tx.send(AppEvent::FsDirty(paths));
                }
            }
        },
        NoCache,
        notify::Config::default(),
    )
    .ok()?;

    for repo in repos {
        // A failed watch on one repo shouldn't kill the others.
        let _ = debouncer.watch(repo, RecursiveMode::Recursive);
    }
    Some(debouncer)
}

/// Whether an event can change `git status`. On Linux, notify's inotify backend
/// subscribes to `IN_OPEN`/`IN_CLOSE_NOWRITE`, so merely *reading* a tree reports
/// `Access` events — including the reads our own scan does (gix opens every
/// directory for the untracked walk). Forwarding those makes each scan trigger the
/// next one, an endless rescan loop. FSEvents on macOS never reports reads.
fn is_mutation(kind: &EventKind) -> bool {
    match kind {
        EventKind::Access(AccessKind::Close(AccessMode::Write)) => true,
        EventKind::Access(_) => false,
        _ => true,
    }
}
