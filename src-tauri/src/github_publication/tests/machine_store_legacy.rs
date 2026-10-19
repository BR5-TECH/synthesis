//! The machine store against a legacy store, the command wiring, and the
//! refusals and deletions around it (DRS-FR-01, DRS-FR-EJBM, DRS-FR-PSCH,
//! DRS-FR-ULKN, DRS-FR-PXHW, DRS-FR-MQBT, DRS-FR-XNLP, RMS-FR-JVEC,
//! GHP-FR-RUYT, GHP-FR-TWKE).

use tauri::Manager;

use super::*;

struct Stores {
    dir: TempDir,
    machine_dir: TempDir,
}

impl Stores {
    fn new() -> Self {
        Stores { dir: project(), machine_dir: TempDir::new().unwrap() }
    }

    fn root(&self) -> crate::fs::RootFs {
        crate::fs::RootFs::for_root(self.dir.path())
    }

    fn machine(&self) -> crate::fs::RootFs {
        crate::fs::RootFs::for_root(self.machine_dir.path())
    }

    fn machine_file(&self, id: &str) -> std::path::PathBuf {
        self.machine_dir.path().join("drafts").join(id).join("publication.toml")
    }
}

fn record(number: u64, marker: &str) -> PublicationRecord {
    PublicationRecord {
        provider: "github".into(),
        repository_host: "github.com".into(),
        repository_owner: "acme".into(),
        repository_name: "widgets".into(),
        issue_number: number,
        issue_url: format!("https://github.com/acme/widgets/issues/{number}"),
        published_at: format!("2026-01-0{number}T00:00:00Z"),
        updated_at: None,
        marker: marker.into(),
        choice: None,
    }
}

fn legacy_attempt() -> PublicationAttempt {
    PublicationAttempt {
        marker: "legacy-attempt".into(),
        remote_name: "origin".into(),
        remote_url: "github.com/acme/widgets".into(),
        repository_host: "github.com".into(),
        repository_owner: "acme".into(),
        repository_name: "widgets".into(),
        state: AttemptState::Open,
        started_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-01T00:00:00Z".into(),
        choice: None,
    }
}

fn write_legacy(root: &crate::fs::RootFs, id: &str, store: &PublicationStore) {
    let path = crate::drafts::draft_publication_path(root, id).unwrap();
    root.write_toml_atomic(&path, store).unwrap();
}

// DRS-FR-ULKN / DRS-FR-JOEV: a standing attempt an earlier build left in the
// legacy store is abandoned once, and does not come back from the legacy file.
#[test]
fn a_legacy_attempt_is_abandoned_once_and_stays_gone() {
    let s = Stores::new();
    let (root, machine) = (s.root(), s.machine());
    let id = draft(&root, "widget");
    let legacy =
        PublicationStore { publication: vec![record(4, "m4")], attempt: Some(legacy_attempt()) };
    write_legacy(&root, &id, &legacy);
    let legacy_path = crate::drafts::draft_publication_path(&root, &id).unwrap();
    let legacy_bytes = std::fs::read(&legacy_path).unwrap();

    flow::clear_attempt(&root, &machine, &id).unwrap();

    let read = store::read_store(&root, &machine, &id).unwrap();
    assert_eq!(read.attempt, None);
    assert_eq!(read.publication, vec![record(4, "m4")]);
    assert_eq!(std::fs::read(&legacy_path).unwrap(), legacy_bytes);
    assert_eq!(flow::clear_attempt(&root, &machine, &id), Err(ERR_NO_ATTEMPT.to_string()));
}

// GHP-FR-WUJQ / DRS-FR-XNLP / DRS-FR-ULKN: an update of a draft that an earlier
// build published takes the current record from the legacy store and writes
// the whole history, unchanged and in order, plus its own record.
#[test]
fn an_update_of_a_legacy_publication_carries_the_whole_history() {
    let s = Stores::new();
    let (root, machine) = (s.root(), s.machine());
    let id = draft(&root, "widget");
    let history = vec![record(3, "m3"), record(4, "m4")];
    write_legacy(&root, &id, &PublicationStore { publication: history.clone(), attempt: None });

    let current = store::current_of(&store::read_store(&root, &machine, &id).unwrap()).unwrap();
    let updated =
        PublicationRecord { updated_at: Some("2026-02-01T00:00:00Z".into()), ..current.clone() };
    store::append_update(&root, &machine, &id, &current, updated.clone()).unwrap();

    let read = store::read_store(&root, &machine, &id).unwrap();
    assert_eq!(read.publication, vec![history[0].clone(), history[1].clone(), updated]);
    assert!(s.machine_file(&id).is_file());
}

// DRS-FR-PSCH: where both stores hold a file, the machine store answers alone.
#[test]
fn the_machine_store_answers_over_a_legacy_store() {
    let s = Stores::new();
    let (root, machine) = (s.root(), s.machine());
    let id = draft(&root, "widget");
    write_legacy(
        &root,
        &id,
        &PublicationStore { publication: vec![record(4, "m4")], attempt: None },
    );
    store::write_store(
        &root,
        &machine,
        &id,
        &PublicationStore { publication: vec![record(9, "m9")], attempt: None },
    )
    .unwrap();

    assert_eq!(store::read_store(&root, &machine, &id).unwrap().publication, vec![record(9, "m9")]);
    let mut listed = crate::drafts::list_drafts_impl(&root);
    crate::drafts::attach_publication(&root, &machine, &mut listed.drafts);
    let row = listed.drafts.iter().find(|d| d.id == id).unwrap();
    assert_eq!(row.publication.as_ref().map(|p| p.issue_number), Some(9));
}

// DRS-FR-PXHW / DRS-FR-ULKN: a listing writes nothing. A legacy store that does
// not parse costs its own row the record and no other row anything.
#[test]
fn a_listing_writes_nothing_and_an_unparseable_legacy_store_costs_its_row_alone() {
    let s = Stores::new();
    let (root, machine) = (s.root(), s.machine());
    let plain = draft(&root, "plain");
    let broken = draft(&root, "broken");
    let fine = draft(&root, "fine");
    let path = crate::drafts::draft_publication_path(&root, &broken).unwrap();
    root.write_text_atomic(&path, "<<<<<<< HEAD\nnot toml\n").unwrap();
    write_legacy(
        &root,
        &fine,
        &PublicationStore { publication: vec![record(4, "m4")], attempt: None },
    );

    let mut listed = crate::drafts::list_drafts_impl(&root);
    assert!(crate::drafts::attach_publication_from(&root, Ok(machine), &mut listed.drafts));

    let row = |id: &str| listed.drafts.iter().find(|d| d.id == id).unwrap();
    assert!(row(&broken).publication_unreadable);
    assert!(row(&broken).publication.is_none());
    assert!(!row(&fine).publication_unreadable);
    assert_eq!(row(&fine).publication.as_ref().map(|p| p.issue_number), Some(4));
    assert!(row(&plain).publication.is_none());
    assert!(!s.machine_dir.path().join("drafts").exists(), "a listing creates no file");
}

// DRS-FR-PXHW: a machine store that cannot be resolved costs every row its
// record, marks none of them unreadable, and leaves the listing whole.
#[test]
fn an_unresolved_machine_store_costs_every_row_its_record_and_nothing_else() {
    let s = Stores::new();
    let root = s.root();
    let id = draft(&root, "widget");
    write_legacy(
        &root,
        &id,
        &PublicationStore { publication: vec![record(4, "m4")], attempt: None },
    );
    let mut listed = crate::drafts::list_drafts_impl(&root);

    let attached = crate::drafts::attach_publication_from(
        &root,
        Err(crate::repository_store::ERR_STORE_UNAVAILABLE.to_string()),
        &mut listed.drafts,
    );

    assert!(!attached);
    assert_eq!(listed.drafts.len(), 1);
    assert!(listed.drafts.iter().all(|d| d.publication.is_none() && !d.publication_unreadable));
}

// DRS-FR-EJBM: a write refused for a draft the active worktree does not hold
// leaves the machine store as it was; an id that would climb out of the store
// is refused by every route.
#[test]
fn a_refused_write_leaves_the_store_and_a_crafted_id_reaches_nothing() {
    let s = Stores::new();
    let (root, machine) = (s.root(), s.machine());
    let id = draft(&root, "widget");
    flow::open_attempt(&root, &machine, &id, &origin(), "pub-1".into()).unwrap();
    let before = std::fs::read(s.machine_file(&id)).unwrap();

    let empty = project();
    let other = crate::fs::RootFs::for_root(empty.path());
    assert!(store::write_store(&other, &machine, &id, &PublicationStore::default()).is_err());
    assert_eq!(std::fs::read(s.machine_file(&id)).unwrap(), before);

    for crafted in ["../escape", "a/b", ""] {
        assert_eq!(
            store::write_store(&root, &machine, crafted, &PublicationStore::default()),
            Err(ERR_DRAFT_NOT_FOUND.to_string())
        );
        assert_eq!(store::delete_store(&machine, crafted), Err(ERR_DRAFT_NOT_FOUND.to_string()));
    }
}

// DRS-FR-MQBT / DRS-FR-21: a store that cannot be removed fails the deletion
// before the draft's folder goes, and the same call repeated completes it.
#[test]
fn a_failed_store_removal_keeps_the_draft_and_a_retry_completes() {
    let s = Stores::new();
    let (root, machine) = (s.root(), s.machine());
    let id = draft(&root, "widget");
    // A non-empty folder where the store file would be cannot be removed by
    // the store's own non-recursive removal.
    std::fs::create_dir_all(s.machine_file(&id).join("blocker")).unwrap();

    assert!(crate::drafts::delete_draft_impl(&root, &machine, &id).is_err());
    assert!(crate::drafts::draft_dir(&root, &id).is_ok(), "the draft is still there");

    std::fs::remove_dir_all(s.machine_file(&id)).unwrap();
    crate::drafts::delete_draft_impl(&root, &machine, &id).expect("the retry completes");
    assert!(crate::drafts::draft_dir(&root, &id).is_err());
}

// DRS-FR-01 / DRS-FR-ULKN: a legacy store moves with its draft between
// folders and still answers there.
#[test]
fn a_legacy_store_moves_with_its_draft() {
    let s = Stores::new();
    let (root, machine) = (s.root(), s.machine());
    let id = draft(&root, "widget");
    write_legacy(
        &root,
        &id,
        &PublicationStore { publication: vec![record(4, "m4")], attempt: None },
    );
    crate::drafts::create_drafts_folder_impl(&root, "", "UI").unwrap();

    crate::drafts::move_draft_to_folder_impl(&root, &id, "UI").unwrap();

    let moved = crate::drafts::draft_publication_path(&root, &id).unwrap();
    assert!(moved.starts_with(s.dir.path().join(".synthesis/drafts/UI")));
    assert_eq!(store::read_store(&root, &machine, &id).unwrap().publication, vec![record(4, "m4")]);
}

// RMS-FR-JVEC: the import pass reaches a draft filed in a drafts folder.
#[test]
fn the_import_pass_reaches_a_filed_draft() {
    let s = Stores::new();
    let (root, machine) = (s.root(), s.machine());
    crate::drafts::create_drafts_folder_impl(&root, "", "UI").unwrap();
    let id = crate::drafts::create_draft_impl(&root, Some("filed"), Some("UI")).unwrap().draft.id;
    write_legacy(
        &root,
        &id,
        &PublicationStore { publication: vec![record(4, "m4")], attempt: None },
    );

    let outcome = crate::repository_store::import_legacy_storage(&machine, &root);

    assert_eq!(outcome.files, 1);
    assert!(s.machine_file(&id).is_file());
}

// GHP-FR-RUYT / DRS-FR-EJBM: a publication through the command writes its
// attempt and record into the machine store, and nothing into the project.
#[test]
fn a_command_publication_lands_in_the_machine_store() {
    let s = super::publication_commands::session();
    let outcome = s.publish(PublicationChoiceInput::default());
    assert!(matches!(outcome, Ok(PublicationOutcome::Published { .. })), "{outcome:?}");

    let file = s.store_dir.path().join("drafts").join(&s.id).join("publication.toml");
    assert!(file.is_file());
    assert!(!s.dir.path().join("drafts").exists(), "the project root holds no store");
    assert!(!crate::drafts::draft_publication_path(&s.root(), &s.id).unwrap().exists());
    assert_eq!(s.store().publication.len(), 1);
}

// GHP-FR-TWKE: every publication command that reaches the store refuses with
// `publication_store_write_failed` where the store cannot be used, before any
// GitHub request and with nothing written.
#[test]
fn every_publication_command_refuses_an_unusable_machine_store() {
    let s = super::publication_commands::session();
    let blocker = s.dir.path().join("not-a-directory");
    std::fs::write(&blocker, "a file where the store would be\n").unwrap();
    s.handle().state::<crate::project::ProjectState>().set_store(blocker);
    let h = s.handle();
    let id = || s.id.clone();
    let refused = Err(ERR_STORE_WRITE_FAILED.to_string());

    let gp = |name: &str, result: Result<(), String>| assert_eq!(result, refused, "{name}");
    gp("get", crate::github_publication::get_draft_publication_impl(&h, id()).map(|_| ()));
    gp("publish", s.publish(PublicationChoiceInput::default()).map(|_| ()));
    gp("retry", crate::github_publication::retry_draft_publication_impl(&h, id()).map(|_| ()));
    gp(
        "resolve",
        crate::github_publication::resolve_draft_publication_conflict_impl(
            &h,
            id(),
            RecoveryChoice::UpdateExisting,
        )
        .map(|_| ()),
    );
    gp(
        "cancel conflict",
        crate::github_publication::cancel_draft_publication_conflict_impl(&h, id()),
    );
    gp(
        "cancel attempt",
        crate::github_publication::cancel_draft_publication_attempt_impl(&h, id()),
    );
    assert_eq!(
        crate::github_publication::update::update_draft_on_github_impl(&h, id(), false).map(|_| ()),
        refused,
        "update"
    );
    assert!(s.fake.calls().is_empty(), "no GitHub call");
    assert!(!crate::drafts::draft_publication_path(&s.root(), &s.id).unwrap().exists());
}
