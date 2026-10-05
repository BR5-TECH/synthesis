//! Tests for the comments storage module.
//!
//! The shared harness lives here; each topic file holds the tests for one
//! group of requirements, in the order the module itself states them.

use super::*;

fn temp_root() -> tempfile::TempDir {
    tempfile::TempDir::new().unwrap()
}

fn human(login: &str) -> Participant {
    Participant::Human {
        login: login.into(),
        display_name: None,
        email: None,
    }
}

fn agent(id: &str, handle: &str) -> Participant {
    Participant::Agent {
        agent_id: id.into(),
        handle: handle.into(),
        model: None,
        title: None,
    }
}

/// A fragment position. The owner and the path are filled in by whatever the
/// fragment is opened on.
fn anchor(start: usize, end: usize, quote: &str) -> FragmentTarget {
    FragmentTarget::in_artifact("", start, end, quote)
}

/// A passage position as a legacy log line stores it.
fn legacy_anchor(start: usize, end: usize, quote: &str) -> LegacyAnchor {
    LegacyAnchor {
        start,
        end,
        quote: quote.into(),
    }
}

fn open(root: &crate::fs::RootFs, artifact: &str, a: FragmentTarget, body: &str, at: &str) -> Discussion {
    open_artifact_fragment_in(root, artifact, a, body.into(), Vec::new(), &human("raver119"), at).unwrap()
}

/// The fragment move of an artifact's fragment log, for tests of the move itself.
fn reanchor_reporting(
    root: &crate::fs::RootFs,
    artifact: &str,
    id: &str,
    position: FragmentTarget,
    by: &Participant,
    at: &str,
) -> Result<(Discussion, bool), String> {
    reanchor_to(root, ThreadRef::artifact(artifact), id, position, by, at)
}

fn reanchor_in(
    root: &crate::fs::RootFs,
    artifact: &str,
    id: &str,
    position: FragmentTarget,
    by: &Participant,
    at: &str,
) -> Result<Discussion, String> {
    reanchor_reporting(root, artifact, id, position, by, at).map(|(d, _)| d)
}

/// A fragment move where only the range and the quote are given: the owner is
/// the discussion's own, which is what a surface sends.
fn reanchor_to(
    root: &crate::fs::RootFs,
    log: ThreadRef<'_>,
    id: &str,
    position: FragmentTarget,
    by: &Participant,
    at: &str,
) -> Result<(Discussion, bool), String> {
    let (owner, path) = match log.find(root, id) {
        Ok(d) => (d.target.clone(), d.fragment_path().to_string()),
        Err(_) => (artifact_target(""), String::new()),
    };
    let moved = FragmentTarget {
        owner,
        path,
        ..position
    };
    move_fragment_to(root, log, id, moved, by, at)
}

/// Which log a legacy locator pair names, for tests of the log lookup.
fn target_of<'a>(
    root: &crate::fs::RootFs,
    artifact_id: Option<&'a str>,
    draft_id: Option<&'a str>,
    note_id: Option<&'a str>,
    thread_id: &str,
) -> Result<ThreadRef<'a>, String> {
    if let Some(note_id) = note_id {
        return Ok(ThreadRef::note_discussion(note_id));
    }
    match (artifact_id, draft_id) {
        (Some(file_rel), Some(draft_id)) => Ok(ThreadRef::draft_file(draft_id, file_rel)),
        (None, Some(draft_id)) => Ok(ThreadRef::discussion(draft_id)),
        (Some(artifact_id), None) => {
            let anchored = ThreadRef::artifact(artifact_id);
            if anchored.find(root, thread_id).is_ok() {
                return Ok(anchored);
            }
            let discussion = ThreadRef::artifact_discussion(artifact_id);
            if discussion.find(root, thread_id).is_ok() {
                return Ok(discussion);
            }
            Err(ERR_DISCUSSION_NOT_FOUND.to_string())
        }
        (None, None) => Err(ERR_DISCUSSION_NOT_FOUND.to_string()),
    }
}

// -- attachment helpers (CMS-FR-42 … CMS-FR-50) --

fn b64(bytes: &[u8]) -> String {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

fn inline_png(filename: &str, bytes: &[u8]) -> AttachmentInput {
    AttachmentInput::Inline {
        media_type: "image/png".into(),
        filename: filename.into(),
        data: b64(bytes),
    }
}

fn url_png(url: &str, label: Option<&str>) -> AttachmentInput {
    AttachmentInput::Url {
        url: url.into(),
        media_type: "image/png".into(),
        label: label.map(str::to_string),
    }
}

fn attachments_dir(root: &Path) -> PathBuf {
    comments_dir(root).join(ATTACHMENTS_SUBDIR)
}

fn stored_files(root: &Path) -> Vec<String> {
    let dir = attachments_dir(root);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

fn only_attachments(thread: &Discussion) -> Vec<&Attachment> {
    thread.comments.iter().flat_map(|c| &c.attachments).collect()
}

/// Create the artifact itself. `follow_rename` enumerates what moved by
/// walking the renamed path, so a test that renames must have real files.
fn touch(root: &Path, rel: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(path, b"x").unwrap();
}

/// Rename on disk, then follow it — the order `library.rs` uses.
fn rename_and_follow(root: &crate::fs::RootFs, old_rel: &str, new_rel: &str) -> usize {
    let from = root.join(old_rel);
    let to = root.join(new_rel);
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    if from.exists() {
        std::fs::rename(&from, &to).unwrap();
    }
    follow_rename(root, root, old_rel, new_rel)
}

fn log_lines(root: &Path, artifact: &str) -> Vec<String> {
    let path = log_path(root, artifact).unwrap();
    std::fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .map(|l| l.to_string())
        .collect()
}

fn artifact_target(artifact_id: &str) -> DiscussionTarget {
    DiscussionTarget::Artifact {
        artifact_id: artifact_id.to_string(),
    }
}

fn open_artifact_discussion(root: &crate::fs::RootFs, artifact_id: &str, body: &str, at: &str) -> Discussion {
    open_discussion_in(
        root,
        root,
        &artifact_target(artifact_id),
        None,
        body.into(),
        Vec::new(),
        &human("raver119"),
        at,
    )
    .unwrap()
}

// -- Discussions (CMS-FR-53 … CMS-FR-59) --------------------------------

/// A real draft on disk, because `open_discussion_thread` refuses a
/// `draft_id` naming none (CMS-FR-57) and every discussion test needs one.
/// A draft **filed in a folder**, deliberately: at the drafts root a
/// composed path and a resolved one are the same path, so a draft-scoped
/// test built on a root-level draft cannot tell CMS-FR-37 from the bug it
/// replaced. Every draft-scoped scenario here is therefore filed.
fn make_draft(root: &crate::fs::RootFs, name: &str) -> String {
    crate::drafts::create_drafts_folder_impl(root, "", "UI").ok();
    crate::drafts::create_drafts_folder_impl(root, "UI", "Components").ok();
    crate::drafts::create_draft_impl(root, Some(name), Some("UI/Components"))
        .unwrap()
        .draft
        .id
}

fn open_discussion(root: &crate::fs::RootFs, draft_id: &str, body: &str, at: &str) -> Discussion {
    open_discussion_in(
        root,
        root,
        &DiscussionTarget::Draft {
            draft_id: draft_id.to_string(),
        },
        None,
        body.into(),
        Vec::new(),
        &human("raver119"),
        at,
    )
    .unwrap()
}

fn discussion_log_lines(root: &crate::fs::RootFs, draft_id: &str) -> Vec<String> {
    std::fs::read_to_string(discussion_log_path(root, draft_id).unwrap())
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

mod log_and_fold;
mod quoting_lock_resolve;
mod reanchor_and_ordering;
mod renames;
mod agents_and_titles;
mod agent_write_path;
mod errors_and_lifecycle;
mod wire_shape;
mod attachments;
mod project_wide_read;
mod artifact_discussions;
mod unified_model;
mod attachment_read;
mod proposal_reference;
mod note_discussions;
mod question_bodies;
mod question_sets;
