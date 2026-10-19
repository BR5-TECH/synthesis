//! The publication store in the repository machine store: one history for a
//! draft across every worktree, the legacy store inside the draft's folder,
//! and the import pass that carries it over (DRS-FR-EJBM, DRS-FR-PSCH,
//! DRS-FR-WBTA, DRS-FR-ULKN, DRS-FR-PXHW, RMS-FR-TCAF,
//! RMS-FR-MVDU, RMS-FR-JVEC, RMS-FR-SUAK, GHP-FR-KZAP).
//!
//! Each worktree here is its own temporary project and the machine store is a
//! third directory, so a test sees exactly what a worktree switch sees: the
//! draft reaches the second worktree through Git (a copy of its committed
//! files), and its publication store does not.

use super::*;

/// A second worktree that holds the draft as Git would deliver it: the
/// committed draft storage alone (`draft.toml` and `files/`), and none of the
/// private storage.
fn worktree_with_draft_of(from: &crate::fs::RootFs, id: &str) -> TempDir {
    let other = project();
    let src = crate::drafts::draft_dir(from, id).unwrap();
    let dest = other.path().join(".synthesis/drafts").join(id);
    std::fs::create_dir_all(dest.join("files")).unwrap();
    std::fs::copy(src.join("draft.toml"), dest.join("draft.toml")).unwrap();
    for entry in std::fs::read_dir(src.join("files")).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), dest.join("files").join(entry.file_name())).unwrap();
    }
    other
}

fn issue_numbers(listed: &crate::drafts::DraftHierarchy, id: &str) -> Option<u64> {
    let row = listed.drafts.iter().find(|d| d.id == id).expect("listed");
    row.publication.as_ref().map(|p| p.issue_number)
}

fn listed_from(
    root: &crate::fs::RootFs,
    machine: &crate::fs::RootFs,
) -> crate::drafts::DraftHierarchy {
    let mut listed = crate::drafts::list_drafts_impl(root);
    crate::drafts::attach_publication(root, machine, &mut listed.drafts);
    listed
}

/// A legacy store, as an earlier build wrote it inside the draft's folder.
fn write_legacy(root: &crate::fs::RootFs, id: &str, number: u64) {
    let path = crate::drafts::draft_publication_path(root, id).unwrap();
    let store = PublicationStore {
        publication: vec![PublicationRecord {
            provider: "github".into(),
            repository_host: "github.com".into(),
            repository_owner: "acme".into(),
            repository_name: "widgets".into(),
            issue_number: number,
            issue_url: format!("https://github.com/acme/widgets/issues/{number}"),
            published_at: "2026-01-01T00:00:00Z".into(),
            updated_at: None,
            marker: format!("legacy-{number}"),
            choice: None,
        }],
        attempt: None,
    };
    root.write_toml_atomic(&path, &store).unwrap();
}

// DRS-FR-WBTA / RMS-FR-TCAF: a publication made in one worktree reads in
// another worktree of the same repository, and the marker shows there.
#[test]
fn a_publication_reads_the_same_from_every_worktree() {
    let first = project();
    let machine_dir = TempDir::new().unwrap();
    let machine = crate::fs::RootFs::for_root(machine_dir.path());
    let a = crate::fs::RootFs::for_root(first.path());
    let id = draft(&a, "widget");
    let attempt = flow::open_attempt(&a, &machine, &id, &origin(), "pub-1".into()).unwrap();
    let issue = IssueRef {
        number: 7,
        url: "https://github.com/acme/widgets/issues/7".into(),
        ..Default::default()
    };
    store::complete_attempt(&a, &machine, &id, flow::record_of(&attempt, &issue)).unwrap();

    let second = worktree_with_draft_of(&a, &id);
    let b = crate::fs::RootFs::for_root(second.path());
    assert_eq!(store::read_store(&b, &machine, &id).unwrap().publication.len(), 1);
    assert_eq!(issue_numbers(&listed_from(&b, &machine), &id), Some(7));
    assert!(
        !crate::drafts::draft_publication_path(&b, &id).unwrap().exists(),
        "nothing was copied into the second worktree"
    );

    // And back: an attempt the second worktree opens reads from the first.
    flow::open_attempt(&b, &machine, &id, &origin(), "pub-2".into()).unwrap();
    let from_a = store::read_store(&a, &machine, &id).unwrap();
    assert_eq!(from_a.attempt.map(|a| a.marker), Some("pub-2".to_string()));
    assert_eq!(from_a.publication.len(), 1);
}

// DRS-FR-WBTA: the removal of the worktree that wrote the store leaves the
// store as it is, so a restart that opens another worktree still reads it.
#[test]
fn a_publication_survives_the_removal_of_the_worktree_that_wrote_it() {
    let first = project();
    let machine_dir = TempDir::new().unwrap();
    let machine = crate::fs::RootFs::for_root(machine_dir.path());
    let a = crate::fs::RootFs::for_root(first.path());
    let id = draft(&a, "widget");
    flow::open_attempt(&a, &machine, &id, &origin(), "pub-1".into()).unwrap();
    let second = worktree_with_draft_of(&a, &id);
    drop(first);

    let b = crate::fs::RootFs::for_root(second.path());
    let read = store::read_store(&b, &machine, &id).unwrap();
    assert_eq!(read.attempt.map(|a| a.marker), Some("pub-1".to_string()));
}

// DRS-FR-EJBM: an id that names no draft in the active worktree reaches no
// store, even where the machine store holds one for it.
#[test]
fn a_draft_missing_from_the_active_worktree_reaches_no_store() {
    let first = project();
    let machine_dir = TempDir::new().unwrap();
    let machine = crate::fs::RootFs::for_root(machine_dir.path());
    let a = crate::fs::RootFs::for_root(first.path());
    let id = draft(&a, "widget");
    flow::open_attempt(&a, &machine, &id, &origin(), "pub-1".into()).unwrap();

    let empty = project();
    let b = crate::fs::RootFs::for_root(empty.path());
    assert_eq!(store::read_store(&b, &machine, &id), Err(ERR_DRAFT_NOT_FOUND.to_string()));
    assert_eq!(
        store::write_store(&b, &machine, &id, &PublicationStore::default()),
        Err(ERR_DRAFT_NOT_FOUND.to_string())
    );
    assert_eq!(store::read_store(&a, &machine, "../escape"), Err(ERR_DRAFT_NOT_FOUND.into()));
}

// DRS-FR-ULKN / DRS-FR-PSCH: a legacy store answers where the machine store
// holds none, and the next write carries its records into the machine store
// without touching the legacy file.
#[test]
fn a_legacy_store_is_read_until_the_first_write_carries_it_over() {
    let dir = project();
    let machine_dir = TempDir::new().unwrap();
    let machine = crate::fs::RootFs::for_root(machine_dir.path());
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "widget");
    write_legacy(&root, &id, 4);
    let legacy = crate::drafts::draft_publication_path(&root, &id).unwrap();
    let legacy_bytes = std::fs::read(&legacy).unwrap();

    assert_eq!(store::read_store(&root, &machine, &id).unwrap().publication.len(), 1);
    assert_eq!(issue_numbers(&listed_from(&root, &machine), &id), Some(4));

    flow::open_attempt(&root, &machine, &id, &origin(), "pub-2".into()).unwrap();
    let machine_file = machine_dir.path().join("drafts").join(&id).join("publication.toml");
    assert!(machine_file.is_file());
    let read = store::read_store(&root, &machine, &id).unwrap();
    assert_eq!(read.publication[0].issue_number, 4, "the legacy record came along");
    assert_eq!(read.attempt.map(|a| a.marker), Some("pub-2".to_string()));
    assert_eq!(std::fs::read(&legacy).unwrap(), legacy_bytes, "the legacy file is untouched");
}

// DRS-FR-PSCH / DRS-FR-GWNI: a machine-store file that does not parse is the
// typed error, is not answered from the legacy store, and is not rewritten.
#[test]
fn an_unparseable_machine_store_is_not_answered_from_the_legacy_store() {
    let dir = project();
    let machine_dir = TempDir::new().unwrap();
    let machine = crate::fs::RootFs::for_root(machine_dir.path());
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "widget");
    write_legacy(&root, &id, 4);
    let machine_file = machine_dir.path().join("drafts").join(&id).join("publication.toml");
    std::fs::create_dir_all(machine_file.parent().unwrap()).unwrap();
    std::fs::write(&machine_file, "<<<<<<< HEAD\nnot toml\n").unwrap();

    assert_eq!(store::read_store(&root, &machine, &id), Err(ERR_STORE_WRITE_FAILED.into()));
    let listed = listed_from(&root, &machine);
    let row = listed.drafts.iter().find(|d| d.id == id).unwrap();
    assert!(row.publication.is_none());
    assert!(row.publication_unreadable);
    assert_eq!(std::fs::read_to_string(&machine_file).unwrap(), "<<<<<<< HEAD\nnot toml\n");
}

// RMS-FR-JVEC / RMS-FR-SUAK / RMS-FR-MVDU: the import pass copies a legacy
// store into `drafts/<id>/publication.toml`, adds nothing on a second pass,
// and leaves the legacy file where it is (RMS-FR-XRPT).
#[test]
fn the_import_pass_copies_a_legacy_store_once() {
    let dir = project();
    let machine_dir = TempDir::new().unwrap();
    let machine = crate::fs::RootFs::for_root(machine_dir.path());
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "widget");
    write_legacy(&root, &id, 4);
    let legacy = crate::drafts::draft_publication_path(&root, &id).unwrap();

    let first = crate::repository_store::import_legacy_storage(&machine, &root);
    assert_eq!(first.files, 1);
    let machine_file = machine_dir.path().join("drafts").join(&id).join("publication.toml");
    assert_eq!(std::fs::read(&machine_file).unwrap(), std::fs::read(&legacy).unwrap());
    assert!(legacy.is_file(), "the pass removes nothing");

    let second = crate::repository_store::import_legacy_storage(&machine, &root);
    assert!(second.is_empty());

    // The draft now reads the same from a worktree that never held the legacy
    // file.
    let other = worktree_with_draft_of(&root, &id);
    let b = crate::fs::RootFs::for_root(other.path());
    assert_eq!(issue_numbers(&listed_from(&b, &machine), &id), Some(4));
}

// RMS-FR-SUAK: a machine store already there is the newer one and is kept, and
// a legacy store that does not parse is not copied.
#[test]
fn the_import_pass_keeps_an_existing_store_and_skips_an_unparseable_one() {
    let dir = project();
    let machine_dir = TempDir::new().unwrap();
    let machine = crate::fs::RootFs::for_root(machine_dir.path());
    let root = crate::fs::RootFs::for_root(dir.path());
    let kept = draft(&root, "kept");
    let broken = draft(&root, "broken");
    flow::open_attempt(&root, &machine, &kept, &origin(), "pub-1".into()).unwrap();
    let machine_file = machine_dir.path().join("drafts").join(&kept).join("publication.toml");
    let before = std::fs::read(&machine_file).unwrap();
    write_legacy(&root, &kept, 4);
    let broken_legacy = crate::drafts::draft_publication_path(&root, &broken).unwrap();
    root.write_text_atomic(&broken_legacy, "<<<<<<< HEAD\nnot toml\n").unwrap();

    let outcome = crate::repository_store::import_legacy_storage(&machine, &root);
    assert_eq!(outcome.files, 0);
    assert_eq!(std::fs::read(&machine_file).unwrap(), before);
    assert!(!machine_dir.path().join("drafts").join(&broken).join("publication.toml").exists());
}

// GHP-FR-KZAP / DRS-FR-PSCH: attempts that race for one draft in this process
// are serialized, so exactly one of them stands and every other one is
// refused, whatever order the threads run in.
#[test]
fn racing_attempts_leave_exactly_one_standing() {
    let dir = project();
    let machine_dir = TempDir::new().unwrap();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "widget");
    let results: Vec<Result<PublicationAttempt, String>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..8)
            .map(|n| {
                let (path, machine_path, id) =
                    (dir.path().to_path_buf(), machine_dir.path().to_path_buf(), id.clone());
                scope.spawn(move || {
                    let root = crate::fs::RootFs::for_root(&path);
                    let machine = crate::fs::RootFs::for_root(&machine_path);
                    flow::open_attempt(&root, &machine, &id, &origin(), format!("pub-{n}"))
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().unwrap()).collect()
    });

    let opened: Vec<_> = results.iter().filter_map(|r| r.as_ref().ok()).collect();
    assert_eq!(opened.len(), 1, "exactly one attempt stands");
    assert!(results
        .iter()
        .filter_map(|r| r.as_ref().err())
        .all(|code| code == ERR_ATTEMPT_IN_PROGRESS));
    let machine = crate::fs::RootFs::for_root(machine_dir.path());
    let standing = store::read_store(&root, &machine, &id).unwrap().attempt.unwrap();
    assert_eq!(standing.marker, opened[0].marker);
}
