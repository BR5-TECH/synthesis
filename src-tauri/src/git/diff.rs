//! Diff wire shapes, unified-diff formatting, and the whole-file
//! revisions read (`../../specifications/core/GTC-git.md` GTC-FR-16).

use std::cell::RefCell;
use std::path::Path;

use git2::{DiffFormat, Repository};
use serde::{Deserialize, Serialize};

use crate::changes::{self, ERR_NO_MERGE_BASE};


// ---------------------------------------------------------------------------
// Wire shapes
// ---------------------------------------------------------------------------

/// What a `get_diff` call is asking about.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DiffScope {
    /// One project-relative path, working tree (staged and unstaged alike)
    /// against `HEAD` — the Changes panel's Uncommitted comparison.
    #[serde(rename_all = "camelCase")]
    Path {
        path: String,
        /// A renamed entry's pre-rename path. Both halves of a rename must be
        /// in view or libgit2 cannot pair them, and the diff would read as a
        /// whole-file addition — contradicting the `+n −n` the panel row that
        /// opened the tab already showed (CHC-FR-09).
        #[serde(default)]
        previous_path: Option<String>,
    },
    /// Everything currently staged — the Git panel's pre-commit review.
    Staged,
    /// One project-relative path against the merge-base of the current branch
    /// and `target_branch` — the Changes panel's Branch comparison.
    #[serde(rename_all = "camelCase")]
    Branch {
        path: String,
        target_branch: String,
        #[serde(default)]
        previous_path: Option<String>,
    },
}

/// One line of a unified diff.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffLine {
    /// `"add"`, `"del"`, or `"context"`.
    pub kind: String,
    /// Line number in the pre-image, absent for an added line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_lineno: Option<u32>,
    /// Line number in the post-image, absent for a removed line.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub new_lineno: Option<u32>,
    /// The line's text, without its trailing newline and without the leading
    /// `+`/`-`/space marker (which `kind` already carries).
    pub content: String,
}

/// One `@@` hunk.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffHunk {
    pub header: String,
    pub lines: Vec<DiffLine>,
}

/// A typed diff payload: unified-diff hunks for text, and an untouched-binary
/// marker for non-text content (GTC contract surface).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffPayload {
    /// Non-text content — the caller renders a marker instead of hunks, and
    /// `hunks` is empty.
    pub is_binary: bool,
    pub hunks: Vec<DiffHunk>,
}

/// Both sides of a comparison for one file, complete rather than windowed to
/// the changed regions (GTC-FR-16).
///
/// `None` on a side means the comparison has no version of the file there — an
/// added file has no `old`, a deleted one has no `new`. That is deliberately
/// distinct from `Some("")`, which is a file that exists and is empty; the
/// caller renders the two differently (`../ui/DFV-diff-viewer.md` DFV-FR-15).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRevisions {
    pub old: Option<String>,
    pub new: Option<String>,
    /// Binary content — neither text is returned. Agrees with the marker
    /// `get_diff` returns for the same path, so the two operations never
    /// disagree about what a path is (GTC-FR-16).
    pub is_binary: bool,
}

impl DiffPayload {
    fn binary() -> DiffPayload {
        DiffPayload {
            is_binary: true,
            hunks: Vec::new(),
        }
    }

    /// The payload for a path with nothing to show. Production reaches this
    /// shape through `format_diff` over an empty diff; it is spelled out here
    /// for the test that pins that equivalence.
    #[cfg(test)]
    pub(crate) fn empty() -> DiffPayload {
        DiffPayload {
            is_binary: false,
            hunks: Vec::new(),
        }
    }
}

// ---------------------------------------------------------------------------
// Formatting (pure over a libgit2 diff)
// ---------------------------------------------------------------------------

/// Classify a libgit2 line origin. Origins outside the unified-diff body
/// (`F` file header, `H` hunk header, binary markers) never reach here.
pub fn line_kind(origin: char) -> Option<&'static str> {
    match origin {
        '+' => Some("add"),
        '-' => Some("del"),
        ' ' => Some("context"),
        // `>`/`<` mark an added/removed trailing newline; they carry no content
        // the reader can act on, so they are folded into context-free silence.
        _ => None,
    }
}

/// Render a libgit2 diff into the wire payload. A single `print` pass walks
/// hunk headers and their lines in order, so hunks come out in file order with
/// their lines attached.
pub(crate) fn format_diff(diff: &git2::Diff<'_>) -> Result<DiffPayload, String> {
    let hunks: RefCell<Vec<DiffHunk>> = RefCell::new(Vec::new());
    let is_binary = RefCell::new(false);

    diff.print(DiffFormat::Patch, |delta, _hunk, line| {
        if delta.flags().is_binary() || line.origin() == 'B' {
            *is_binary.borrow_mut() = true;
            return true;
        }
        let mut hunks = hunks.borrow_mut();
        match line.origin() {
            // A hunk header opens a new hunk; its content is the `@@ … @@` line.
            'H' => {
                let header = String::from_utf8_lossy(line.content())
                    .trim_end_matches('\n')
                    .to_string();
                hunks.push(DiffHunk {
                    header,
                    lines: Vec::new(),
                });
            }
            'F' => {} // the file header is the caller's business, not ours
            origin => {
                let Some(kind) = line_kind(origin) else {
                    return true;
                };
                // libgit2 emits hunk headers before their lines, but guard
                // anyway so a body line can never be dropped.
                if hunks.is_empty() {
                    hunks.push(DiffHunk {
                        header: String::new(),
                        lines: Vec::new(),
                    });
                }
                let content = String::from_utf8_lossy(line.content())
                    .trim_end_matches('\n')
                    .to_string();
                let entry = hunks.last_mut().expect("a hunk was just ensured");
                entry.lines.push(DiffLine {
                    kind: kind.to_string(),
                    old_lineno: line.old_lineno(),
                    new_lineno: line.new_lineno(),
                    content,
                });
            }
        }
        true
    })
    .map_err(|e| format!("failed to format diff: {e}"))?;

    if is_binary.into_inner() {
        return Ok(DiffPayload::binary());
    }
    Ok(DiffPayload {
        is_binary: false,
        hunks: hunks.into_inner(),
    })
}

/// Repository-relative pathspec for a project-relative path.
pub(crate) fn repo_pathspec(prefix: &str, path: &str) -> String {
    if prefix.is_empty() {
        path.to_string()
    } else {
        format!("{prefix}/{path}")
    }
}

// ---------------------------------------------------------------------------
// Diff computation
// ---------------------------------------------------------------------------

/// GTC `get_diff(scope)`. Returns unified-diff hunks for text and an untouched-
/// binary marker for non-text content. A path with nothing to show yields an
/// empty payload rather than an error, so a Diff tab whose file was just
/// reverted renders "no changes" instead of failing (CHG-FR-22).
pub fn diff_for_scope(root: &Path, scope: &DiffScope) -> Result<DiffPayload, String> {
    let repo = changes::open_repo(root)?;
    let prefix = changes::project_prefix(&repo, root);

    match scope {
        DiffScope::Path {
            path,
            previous_path,
        } => {
            let mut opts = file_options(&prefix, path, previous_path.as_deref());
            let tree = head_tree(&repo);
            let mut diff = repo
                .diff_tree_to_workdir_with_index(tree.as_ref(), Some(&mut opts))
                .map_err(|e| format!("failed to diff working tree: {e}"))?;
            changes::find_renames(&mut diff)?;
            format_diff(&diff)
        }
        DiffScope::Staged => {
            let mut opts = changes::diff_options();
            changes::scope_to_project(&mut opts, &prefix);
            // The staged set is the index against HEAD; untracked files are not
            // part of it.
            opts.include_untracked(false).recurse_untracked_dirs(false);
            let tree = head_tree(&repo);
            let mut diff = repo
                .diff_tree_to_index(tree.as_ref(), None, Some(&mut opts))
                .map_err(|e| format!("failed to diff the staged set: {e}"))?;
            changes::find_renames(&mut diff)?;
            format_diff(&diff)
        }
        DiffScope::Branch {
            path,
            target_branch,
            previous_path,
        } => {
            let base_tree = merge_base_tree(&repo, target_branch)?;
            let mut opts = file_options(&prefix, path, previous_path.as_deref());
            let mut diff = repo
                .diff_tree_to_workdir_with_index(Some(&base_tree), Some(&mut opts))
                .map_err(|e| format!("failed to diff against merge base: {e}"))?;
            changes::find_renames(&mut diff)?;
            format_diff(&diff)
        }
    }
}

/// Diff options narrowed to one file — and, for a rename, to both of its paths.
///
/// The project prefix is deliberately *not* added: `DiffOptions::pathspec`
/// appends, and libgit2 matches a path against any entry, so a `prefix` entry
/// alongside a file entry would widen the diff back to the whole project and a
/// Diff tab would render every changed file at once.
pub(crate) fn file_options(prefix: &str, path: &str, previous_path: Option<&str>) -> git2::DiffOptions {
    let mut opts = changes::diff_options();
    opts.pathspec(repo_pathspec(prefix, path));
    if let Some(previous) = previous_path {
        opts.pathspec(repo_pathspec(prefix, previous));
    }
    opts
}

pub(crate) fn head_tree(repo: &Repository) -> Option<git2::Tree<'_>> {
    repo.head().ok()?.peel_to_commit().ok()?.tree().ok()
}

/// The tree at the merge-base of the current branch and `target_branch`,
/// resolved exactly as `changes::branch_change_set` resolves it (CHC-FR-04) so
/// a Diff tab and the panel row that opened it agree on the comparison.
pub(crate) fn merge_base_tree<'r>(
    repo: &'r Repository,
    target_branch: &str,
) -> Result<git2::Tree<'r>, String> {
    let target = changes::resolve_branch_commit(repo, target_branch)?;
    let head = repo
        .head()
        .ok()
        .and_then(|h| h.peel_to_commit().ok())
        .map(|c| c.id())
        .ok_or_else(|| ERR_NO_MERGE_BASE.to_string())?;
    let base = repo
        .merge_base(head, target)
        .map_err(|_| ERR_NO_MERGE_BASE.to_string())?;
    repo.find_commit(base)
        .and_then(|c| c.tree())
        .map_err(|_| ERR_NO_MERGE_BASE.to_string())
}

// ---------------------------------------------------------------------------
// File revisions (GTC-FR-16)
// ---------------------------------------------------------------------------

/// `get_file_revisions` answers about one file. The staged *set* names no
/// single path, so it is not a scope this operation can serve.
pub const ERR_SCOPE_NOT_A_FILE: &str = "scope does not name a file";

/// libgit2 decides binary by scanning the leading bytes for a NUL. Matching its
/// window is what keeps `get_file_revisions` and `get_diff` from disagreeing
/// about a path (GTC-FR-16).
pub(crate) const BINARY_SCAN_LIMIT: usize = 8000;

pub(crate) fn looks_binary(bytes: &[u8]) -> bool {
    bytes.iter().take(BINARY_SCAN_LIMIT).any(|b| *b == 0)
}

/// `-diff` in `.gitattributes` makes a path binary to Git whatever its bytes
/// say, and `get_diff` honours it through libgit2's delta flags. Consulting the
/// same attribute is the other half of the agreement GTC-FR-16 requires: a path
/// marked `-diff` must not come back as renderable text here while `get_diff`
/// reports it binary.
pub(crate) fn marked_binary_by_attributes(repo: &Repository, spec: &str) -> bool {
    matches!(
        repo.get_attr(Path::new(spec), "diff", git2::AttrCheckFlags::default())
            .map(git2::AttrValue::from_string),
        Ok(git2::AttrValue::False)
    )
}

/// Line terminators are a filter, not content.
///
/// `get_diff` compares the *filtered* working-directory text against the blob,
/// so under `core.autocrlf` or an `eol=crlf` attribute it reports no change for
/// a file whose blob is LF and whose checkout is CRLF. Reading the raw bytes on
/// both sides — which is all libgit2 exposes here — would make this operation
/// call that same file wholly rewritten, and every line of a Windows checkout
/// would render as changed under a diff that showed nothing. Normalising both
/// sides is what keeps the two operations describing one comparison
/// (GTC-FR-16), and it costs the reader nothing: a terminator is not something
/// any of the rendering modes shows.
pub(crate) fn normalize_eol(text: String) -> String {
    if text.contains('\r') {
        text.replace("\r\n", "\n")
    } else {
        text
    }
}

/// The blob a tree holds at `spec`, or `None` when that revision has no such
/// entry — the file was added, or the path names a directory.
pub(crate) fn blob_at(repo: &Repository, tree: &git2::Tree<'_>, spec: &str) -> Option<Vec<u8>> {
    let entry = tree.get_path(Path::new(spec)).ok()?;
    let object = entry.to_object(repo).ok()?;
    let blob = object.as_blob()?;
    Some(blob.content().to_vec())
}

/// GTC `get_file_revisions(scope)` — both sides of the comparison for one file,
/// complete rather than windowed to the changed regions.
///
/// The scope already names the path (that is how `get_diff`'s scope is shaped
/// here), so the path is not passed a second time: two sources for one path
/// could disagree, and a Diff tab would then diff one file against another.
///
/// GTC-FR-16: the new side is the file as it stands in the active worktree's
/// working directory — for both the uncommitted and the branch comparison,
/// which is what `get_diff` compares against too — and the old side is the blob
/// the base revision holds. A side the comparison has no version for comes back
/// `None` rather than as an empty string.
pub fn file_revisions_for_scope(
    root: &crate::fs::RootFs,
    scope: &DiffScope,
) -> Result<FileRevisions, String> {
    let repo = changes::open_repo(root)?;
    let prefix = changes::project_prefix(&repo, root);

    let (path, previous_path, base_tree) = match scope {
        DiffScope::Path {
            path,
            previous_path,
        } => (path, previous_path.as_deref(), head_tree(&repo)),
        DiffScope::Branch {
            path,
            target_branch,
            previous_path,
        } => (
            path,
            previous_path.as_deref(),
            Some(merge_base_tree(&repo, target_branch)?),
        ),
        DiffScope::Staged => return Err(ERR_SCOPE_NOT_A_FILE.to_string()),
    };

    // A renamed entry's old side lives at its pre-rename path; without that the
    // old revision reads as absent and the whole file renders as an addition,
    // contradicting the `+n −n` the row that opened the tab showed (CHC-FR-09).
    let old_spec = repo_pathspec(&prefix, previous_path.unwrap_or(path));
    let new_spec = repo_pathspec(&prefix, path);
    let old_bytes = base_tree
        .as_ref()
        .and_then(|tree| blob_at(&repo, tree, &old_spec));

    // The path arrives from the frontend, so it is resolved under the content
    // root rather than joined blindly: a `..` segment must not reach a file
    // outside the project, and an attempt to is an error rather than a quiet
    // "this revision has no such file".
    let absolute = crate::fs::resolve_under(root.path(), path).map_err(|e| e.to_string())?;
    // Through the helper: `resolve_under` is syntactic, so on its own it lets a
    // symlink inside the root serve another checkout's bytes into a diff.
    let new_bytes = match root.read_bytes(&absolute) {
        Ok(bytes) => Some(bytes),
        // Absent is what a deletion looks like. Every *other* failure — a
        // permission denial, a path that is now a directory — is a failure to
        // read, and reporting it as `new: null` would tell the reader the
        // comparison deleted their file.
        Err(crate::fs::FsError::NotFound { .. }) => None,
        Err(e) => return Err(format!("failed to read {path}: {e}")),
    };

    let is_binary = old_bytes.as_deref().map(looks_binary).unwrap_or(false)
        || new_bytes.as_deref().map(looks_binary).unwrap_or(false)
        || marked_binary_by_attributes(&repo, &new_spec)
        || marked_binary_by_attributes(&repo, &old_spec);
    if is_binary {
        return Ok(FileRevisions {
            old: None,
            new: None,
            is_binary: true,
        });
    }

    let decode = |bytes: Vec<u8>| normalize_eol(String::from_utf8_lossy(&bytes).into_owned());
    Ok(FileRevisions {
        old: old_bytes.map(decode),
        new: new_bytes.map(decode),
        is_binary: false,
    })
}
