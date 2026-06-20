//! Filesystem watching: one recursive watch per repo, debounced, mapped back to
//! the owning repo so only changed repos get re-scanned.

use std::path::PathBuf;
use std::time::Duration;

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
                let paths: Vec<PathBuf> =
                    events.into_iter().flat_map(|e| e.event.paths).collect();
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
