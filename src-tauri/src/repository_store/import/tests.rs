//! Tests for the import pass (`RMS-repository-machine-storage.md`).

use super::*;

use crate::fs::RootFs;

struct Fixture {
    _worktree_dir: tempfile::TempDir,
    _store_dir: tempfile::TempDir,
    worktree: RootFs,
    store: RootFs,
}

impl Fixture {
    fn new() -> Fixture {
        let worktree_dir = tempfile::TempDir::new().expect("worktree");
        let store_dir = tempfile::TempDir::new().expect("store");
        let worktree = RootFs::for_root(worktree_dir.path());
        let store = RootFs::for_root(store_dir.path());
        Fixture {
            worktree,
            store,
            _worktree_dir: worktree_dir,
            _store_dir: store_dir,
        }
    }

    /// A committed file of the worktree, as an earlier build left it.
    fn legacy(&self, rel: &str, body: &str) {
        let path = self.worktree.path().join(rel);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(path, body).expect("write");
    }

    fn store_text(&self, rel: &str) -> Option<String> {
        std::fs::read_to_string(self.store.path().join(rel)).ok()
    }

    fn run(&self) -> ImportOutcome {
        import_legacy_storage(&self.store, &self.worktree)
    }

    /// A draft the worktree still holds, which is what a statistics log has to
    /// belong to before the pass imports it (DRS-FR-ZLBK).
    fn draft(&self, id: &str) {
        self.legacy(
            &format!(".synthesis/drafts/{id}/draft.toml"),
            &format!(
                "id = \"{id}\"\nname = \"one\"\nprompt_path = \"one.md\"\n\
                 status = \"active\"\ncreated_at = \"2026-01-01T00:00:00Z\"\n\
                 updated_at = \"2026-01-01T00:00:00Z\"\n"
            ),
        );
    }
}

/// A generated-shaped draft id (`DRS-draft-storage.md` DRS-FR-02).
const DRAFT: &str = "1a2b3c4d5e6-0001-deadbeef";

fn event(event_id: &str, body: &str) -> String {
    format!(r#"{{"v":1,"event_id":"{event_id}","body":"{body}"}}"#)
}

/// RMS-FR-JVEC: the committed conversation and statistics logs both reach the
/// store, under the same names they had.
#[test]
fn rms_the_pass_imports_the_committed_logs_into_the_store() {
    let f = Fixture::new();
    f.legacy(".synthesis/comments/abc.jsonl", &format!("{}\n", event("e1", "one")));
    f.legacy(
        ".synthesis/comments/notes/n1/discussion.jsonl",
        &format!("{}\n", event("e2", "two")),
    );
    f.draft(DRAFT);
    f.legacy(&format!(".synthesis/statistics/{DRAFT}.jsonl"), &format!("{}\n", event("e3", "three")));

    let outcome = f.run();

    assert_eq!(outcome.lines, 3);
    assert!(f.store_text("comments/abc.jsonl").expect("log").contains("e1"));
    assert!(f
        .store_text("comments/notes/n1/discussion.jsonl")
        .expect("note log")
        .contains("e2"));
    assert!(f.store_text(&format!("statistics/{DRAFT}.jsonl")).expect("stats").contains("e3"));
}

/// RMS-FR-SUAK: additive and idempotent — a second pass over the same folders
/// writes nothing, and a line the store already holds is never doubled.
#[test]
fn rms_the_pass_is_idempotent() {
    let f = Fixture::new();
    f.draft(DRAFT);
    f.legacy(
        &format!(".synthesis/statistics/{DRAFT}.jsonl"),
        &format!("{}\n{}\n", event("e1", "one"), event("e2", "two")),
    );

    assert_eq!(f.run().lines, 2);
    assert_eq!(f.run(), ImportOutcome::default(), "a second pass adds nothing");

    let text = f.store_text(&format!("statistics/{DRAFT}.jsonl")).expect("stats");
    assert_eq!(text.lines().filter(|l| l.contains("\"e1\"")).count(), 1);
    assert_eq!(text.lines().filter(|l| l.contains("\"e2\"")).count(), 1);
}

/// RMS-FR-SUAK: a line the store recorded itself is not imported a second time,
/// and a line the store does not hold is appended beside it.
#[test]
fn rms_the_pass_appends_only_what_the_store_does_not_hold() {
    let f = Fixture::new();
    std::fs::create_dir_all(f.store.path().join("statistics")).expect("dirs");
    std::fs::write(
        f.store.path().join(format!("statistics/{DRAFT}.jsonl")),
        format!("{}\n", event("e1", "one")),
    )
    .expect("seed");
    f.draft(DRAFT);
    f.legacy(
        &format!(".synthesis/statistics/{DRAFT}.jsonl"),
        &format!("{}\n{}\n", event("e1", "one"), event("e2", "two")),
    );

    assert_eq!(f.run().lines, 1);
    let text = f.store_text(&format!("statistics/{DRAFT}.jsonl")).expect("stats");
    assert_eq!(text.lines().count(), 2);
    assert_eq!(text.lines().filter(|l| l.contains("\"e1\"")).count(), 1);
}

/// RMS-FR-SUAK: an attachment is written once, named by the digest of its own
/// bytes, and a `.gitattributes` reaches no store.
#[test]
fn rms_the_pass_carries_attachments_and_leaves_git_artifacts_behind() {
    let f = Fixture::new();
    f.legacy(".synthesis/comments/attachments/deadbeef", "PNG-ish");
    f.legacy(".synthesis/comments/.gitattributes", "*.jsonl merge=union\n");

    let outcome = f.run();

    assert_eq!(outcome.files, 1);
    assert_eq!(
        f.store_text("comments/attachments/deadbeef").as_deref(),
        Some("PNG-ish")
    );
    assert!(
        f.store_text("comments/.gitattributes").is_none(),
        "a Git artifact belongs to no store"
    );
    assert_eq!(f.run(), ImportOutcome::default(), "an attachment is written once");
}

/// RMS-FR-XRPT: the pass removes nothing. Every committed file stays exactly as
/// it was, so the author removes it in a commit of their own.
#[test]
fn rms_the_pass_removes_nothing_from_the_project() {
    let f = Fixture::new();
    f.legacy(".synthesis/comments/abc.jsonl", &format!("{}\n", event("e1", "one")));
    f.draft(DRAFT);
    f.legacy(&format!(".synthesis/statistics/{DRAFT}.jsonl"), &format!("{}\n", event("e2", "two")));
    f.legacy(".synthesis/comments/attachments/deadbeef", "bytes");

    f.run();

    for rel in [
        ".synthesis/comments/abc.jsonl".to_string(),
        format!(".synthesis/statistics/{DRAFT}.jsonl"),
        ".synthesis/comments/attachments/deadbeef".to_string(),
    ] {
        assert!(
            f.worktree.path().join(&rel).is_file(),
            "{rel} is left exactly as it was"
        );
    }
    assert!(
        std::fs::read_to_string(f.worktree.path().join(".synthesis/comments/abc.jsonl"))
            .expect("log")
            .contains("e1")
    );
}

/// RMS-FR-JVEC: a draft's committed review folder is imported under that
/// draft's stable id, wherever in the hierarchy the draft is filed.
#[test]
fn rms_the_pass_imports_a_drafts_review_under_its_stable_id() {
    let f = Fixture::new();
    let draft = DRAFT;
    f.legacy(
        &format!(".synthesis/drafts/UI/{draft}/draft.toml"),
        &format!("id = \"{draft}\"\nname = \"one\"\nprompt_path = \"one.md\"\nstatus = \"active\"\ncreated_at = \"2026-01-01T00:00:00Z\"\nupdated_at = \"2026-01-01T00:00:00Z\"\n"),
    );
    f.legacy(
        &format!(".synthesis/drafts/UI/{draft}/comments/discussion.jsonl"),
        &format!("{}\n", event("e9", "nine")),
    );

    let outcome = f.run();

    assert_eq!(outcome.lines, 1);
    assert!(f
        .store_text(&format!("drafts/{draft}/comments/discussion.jsonl"))
        .expect("draft log")
        .contains("e9"));
}

/// RMS-FR-JVEC: a project that committed nothing has nothing to import, and the
/// pass creates no folder for it.
#[test]
fn rms_a_project_with_no_legacy_folder_imports_nothing() {
    let f = Fixture::new();
    assert_eq!(f.run(), ImportOutcome::default());
    assert!(!f.store.path().join("comments").exists());
    assert!(!f.store.path().join("statistics").exists());
}

/// `DRS-draft-storage.md` DRS-FR-ZLBK: deleting a draft takes its statistics
/// log, and success means it is gone. The committed copy stays in the worktree
/// because the pass removes nothing (RMS-FR-XRPT), so the pass must not carry
/// it back.
#[test]
fn rms_the_pass_does_not_resurrect_a_deleted_drafts_statistics() {
    let f = Fixture::new();
    // A log for a draft the worktree no longer holds, beside one it does.
    f.draft(DRAFT);
    f.legacy(
        &format!(".synthesis/statistics/{DRAFT}.jsonl"),
        &format!("{}\n", event("kept", "one")),
    );
    f.legacy(
        ".synthesis/statistics/1a2b3c4d5e6-0002-deadbeef.jsonl",
        &format!("{}\n", event("gone", "two")),
    );

    assert_eq!(f.run().lines, 1, "the live draft's log alone");
    assert!(f
        .store_text(&format!("statistics/{DRAFT}.jsonl"))
        .expect("the live draft's log")
        .contains("kept"));
    assert!(
        f.store_text("statistics/1a2b3c4d5e6-0002-deadbeef.jsonl").is_none(),
        "a deleted draft's account is not carried back",
    );
    // RMS-FR-XRPT: and the committed copy is still exactly where it was, for
    // the author to remove in a commit of their own.
    assert!(f
        .worktree
        .path()
        .join(".synthesis/statistics/1a2b3c4d5e6-0002-deadbeef.jsonl")
        .is_file());
}
