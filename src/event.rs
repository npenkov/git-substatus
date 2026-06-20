//! Asynchronous events delivered to the main loop from worker threads.

use crate::model::RepoStatus;
use std::path::PathBuf;

/// Events pushed onto the channel by the scan pool and the fs watcher.
/// (Keyboard input is polled directly in the main loop, so it is not here.)
pub enum AppEvent {
    /// A repo finished (re)scanning.
    RepoUpdate(RepoStatus),
    /// Debounced filesystem paths that changed.
    FsDirty(Vec<PathBuf>),
}

pub type Sender = crossbeam_channel::Sender<AppEvent>;
pub type Receiver = crossbeam_channel::Receiver<AppEvent>;
