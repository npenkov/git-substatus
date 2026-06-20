//! Parallel scan orchestration via a rayon thread pool.

use std::path::PathBuf;

use crate::event::{AppEvent, Sender};
use crate::git;

/// Owns a rayon pool and spawns per-repo scans that report back over the channel.
pub struct Scanner {
    pool: rayon::ThreadPool,
    tx: Sender,
}

impl Scanner {
    pub fn new(tx: Sender) -> Self {
        let pool = rayon::ThreadPoolBuilder::new()
            .build()
            .expect("failed to build rayon pool");
        Scanner { pool, tx }
    }

    /// Queue a scan of one repo; the result arrives as `AppEvent::RepoUpdate`.
    pub fn scan(&self, path: PathBuf) {
        let tx = self.tx.clone();
        self.pool.spawn(move || {
            let status = git::scan_repo(&path);
            let _ = tx.send(AppEvent::RepoUpdate(status));
        });
    }

    /// Queue scans for every repo (initial load).
    pub fn scan_all(&self, paths: &[PathBuf]) {
        for p in paths {
            self.scan(p.clone());
        }
    }
}
