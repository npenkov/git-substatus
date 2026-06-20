//! Self-contained diffs: old content from gix (HEAD blob), new content from the
//! worktree, unified hunks produced by imara-diff.

use std::path::Path;

use imara_diff::{Algorithm, BasicLineDiffPrinter, Diff, InternedInput, UnifiedDiffConfig};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffKind {
    Header,
    Hunk,
    Add,
    Del,
    Context,
}

#[derive(Clone, Debug)]
pub struct DiffLine {
    pub kind: DiffKind,
    pub text: String,
}

/// Compute a worktree-vs-HEAD unified diff for one file, like `git diff HEAD -- <file>`.
/// Untracked/added files render as all-additions; deleted files as all-deletions.
pub fn diff_for_file(repo_path: &Path, rel: &str) -> Vec<DiffLine> {
    let old = head_blob(repo_path, rel).unwrap_or_default();
    let new = std::fs::read(repo_path.join(rel)).unwrap_or_default();

    if old == new {
        return vec![DiffLine {
            kind: DiffKind::Header,
            text: format!("{rel} (no content change)"),
        }];
    }

    if is_binary(&old) || is_binary(&new) {
        return vec![DiffLine {
            kind: DiffKind::Header,
            text: format!("{rel} (binary)"),
        }];
    }

    let old_s = String::from_utf8_lossy(&old);
    let new_s = String::from_utf8_lossy(&new);

    let input = InternedInput::new(old_s.as_ref(), new_s.as_ref());
    let mut diff = Diff::compute(Algorithm::Histogram, &input);
    diff.postprocess_lines(&input);

    let unified = diff
        .unified_diff(
            &BasicLineDiffPrinter(&input.interner),
            UnifiedDiffConfig::default(),
            &input,
        )
        .to_string();

    let mut out = vec![DiffLine {
        kind: DiffKind::Header,
        text: rel.to_string(),
    }];
    for line in unified.lines() {
        let kind = match line.as_bytes().first() {
            Some(b'@') => DiffKind::Hunk,
            Some(b'+') => DiffKind::Add,
            Some(b'-') => DiffKind::Del,
            _ => DiffKind::Context,
        };
        out.push(DiffLine {
            kind,
            text: line.to_string(),
        });
    }
    out
}

/// Read the blob bytes for `rel` from the repo's HEAD tree, if present.
fn head_blob(repo_path: &Path, rel: &str) -> Option<Vec<u8>> {
    let repo = gix::open(repo_path).ok()?;
    let tree = repo.head_tree().ok()?;
    let entry = tree.lookup_entry_by_path(Path::new(rel)).ok()??;
    let obj = entry.object().ok()?;
    Some(obj.data.clone())
}

fn is_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(8000).any(|&b| b == 0)
}
