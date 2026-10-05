//! Publishing, retry recovery, and the append-only history (GHP-FR-OWLB,
//! GHP-FR-IEQC, GHP-FR-TMBK, GHP-FR-HRUN, GHP-FR-YPGL, GHP-FR-JAWD,
//! GHP-FR-UZMX, GHP-FR-QLDF, GHP-FR-VNCS).

use super::*;

struct Fixture {
    _dir: TempDir,
    root: crate::fs::RootFs,
    id: String,
    github: FakeGithub,
}

fn fixture(prompt: &str) -> Fixture {
    let dir = project();
    let root = crate::fs::RootFs::for_root(dir.path());
    let id = draft(&root, "Widget window");
    save_prompt(&root, &id, prompt);
    Fixture { _dir: dir, root, id, github: FakeGithub::publishable() }
}

impl Fixture {
    fn attempt(&self, marker: &str) -> PublicationAttempt {
        flow::open_attempt(&self.root, &self.id, &origin(), marker.to_string()).unwrap()
    }

    fn publish(&self, attempt: &PublicationAttempt) -> Result<PublicationOutcome, String> {
        flow::publish_with_attempt(&self.root, &self.id, attempt, "secret", &self.github)
    }

    fn store(&self) -> PublicationStore {
        store::read_store(&self.root, &self.id).unwrap()
    }
}

/// GHP-FR-OWLB / GHP-FR-IEQC / GHP-FR-JAWD: every attempt searches for its
/// marker first; finding nothing, it creates the issue and appends exactly one
/// record carrying the provider, repository, issue, instant, and marker.
#[test]
fn a_publication_searches_before_it_creates_and_appends_one_record() {
    let f = fixture("Do the thing.\n");
    let attempt = f.attempt("pub-1");
    let outcome = f.publish(&attempt).unwrap();

    assert_eq!(
        f.github.calls(),
        vec![Call::Search("acme/widgets:pub-1".into()), Call::Create("acme/widgets".into())]
    );
    let PublicationOutcome::Published { record } = outcome else { panic!("published") };
    assert_eq!(record.provider, "github");
    assert_eq!(record.repository_owner, "acme");
    assert_eq!(record.repository_name, "widgets");
    assert_eq!(record.issue_number, 1);
    assert_eq!(record.issue_url, "https://github.com/acme/widgets/issues/1");
    assert_eq!(record.marker, "pub-1");
    assert!(!record.published_at.is_empty());

    let store = f.store();
    assert_eq!(store.attempt, None);
    assert_eq!(store.publication.len(), 1);
}

/// GHP-FR-OWLB: a retry whose create already landed finds the marker and
/// creates no second issue. GHP-FR-TMBK: an issue already matching the latest
/// title and body is reused.
#[test]
fn a_retry_after_a_lost_response_reuses_the_issue_rather_than_duplicating_it() {
    let f = fixture("Do the thing.\n");
    let attempt = f.attempt("pub-1");
    // The first create landed on GitHub; the response never came back, so the
    // attempt is still standing.
    let content = flow::issue_content(&f.root, &f.id, "pub-1").unwrap();
    f.github.create_issue("s", "acme", "widgets", &content.title, &content.body, &Default::default()).unwrap();
    f.github.calls.lock().unwrap().clear();

    let outcome = f.publish(&attempt).unwrap();
    assert_eq!(f.github.calls(), vec![Call::Search("acme/widgets:pub-1".into())]);
    let PublicationOutcome::Published { record } = outcome else { panic!("published") };
    assert_eq!(record.issue_number, 1);
    assert_eq!(f.github.issues.lock().unwrap().len(), 1);
}

/// GHP-FR-IEQC: a search that cannot answer creates nothing and returns a typed
/// error. GHP-FR-VNCS: the attempt stays standing so it can be retried.
#[test]
fn a_search_that_cannot_answer_creates_nothing_and_keeps_the_attempt() {
    let f = fixture("Do the thing.\n");
    let attempt = f.attempt("pub-1");
    *f.github.search_error.lock().unwrap() = Some(ERR_GITHUB_UNREACHABLE.to_string());

    assert_eq!(f.publish(&attempt), Err(ERR_GITHUB_UNREACHABLE.to_string()));
    assert!(!f.github.calls().contains(&Call::Create("acme/widgets".into())));
    let store = f.store();
    assert_eq!(store.attempt.unwrap().state, AttemptState::Open);
    assert!(store.publication.is_empty());
}

/// GHP-FR-VNCS: a create that fails appends no record and leaves the attempt
/// recoverable.
#[test]
fn a_failed_create_appends_nothing_and_leaves_the_attempt_recoverable() {
    let f = fixture("Do the thing.\n");
    let attempt = f.attempt("pub-1");
    *f.github.create_error.lock().unwrap() = Some(ERR_ISSUE_CREATE_FAILED.to_string());

    assert_eq!(f.publish(&attempt), Err(ERR_ISSUE_CREATE_FAILED.to_string()));
    let store = f.store();
    assert!(store.publication.is_empty());
    assert_eq!(store.attempt.unwrap().marker, "pub-1");
}

/// GHP-FR-OWLB: a retry always sends the draft's **latest** name and prompt.
/// GHP-FR-HRUN: where the found issue differs, the attempt moves to
/// `awaiting_choice` and nothing is edited or created.
#[test]
fn a_retry_over_an_edited_draft_asks_rather_than_editing() {
    let f = fixture("Do the thing.\n");
    let attempt = f.attempt("pub-1");
    let stale = flow::issue_content(&f.root, &f.id, "pub-1").unwrap();
    f.github.create_issue("s", "acme", "widgets", &stale.title, &stale.body, &Default::default()).unwrap();

    save_prompt(&f.root, &f.id, "Do the other thing.\n");
    f.github.calls.lock().unwrap().clear();

    let outcome = f.publish(&attempt).unwrap();
    let PublicationOutcome::RecoveryRequired { issue_number, issue_url, marker, mismatches } = outcome else {
        panic!("recovery")
    };
    assert_eq!(issue_number, 1);
    assert_eq!(issue_url, "https://github.com/acme/widgets/issues/1");
    assert_eq!(marker, "pub-1");
    assert_eq!(mismatches, vec!["body".to_string()]);
    assert_eq!(f.github.calls(), vec![Call::Search("acme/widgets:pub-1".into())]);
    assert_eq!(f.store().attempt.unwrap().state, AttemptState::AwaitingChoice);
    assert!(f.store().publication.is_empty());
}

/// GHP-FR-YPGL: **Update existing issue** edits that issue to the latest title
/// and body, keeps the marker, and records exactly one history entry.
#[test]
fn update_existing_edits_the_issue_and_records_one_entry() {
    let f = fixture("Do the thing.\n");
    let attempt = f.attempt("pub-1");
    let stale = flow::issue_content(&f.root, &f.id, "pub-1").unwrap();
    f.github.create_issue("s", "acme", "widgets", &stale.title, &stale.body, &Default::default()).unwrap();
    save_prompt(&f.root, &f.id, "Do the other thing.\n");
    crate::drafts::rename_draft_impl(&f.root, &f.id, "Widget window v2").unwrap();
    f.github.calls.lock().unwrap().clear();

    let outcome =
        flow::update_existing(&f.root, &f.id, &attempt, "secret", &f.github).unwrap();
    let PublicationOutcome::Published { record } = outcome else { panic!("published") };
    assert_eq!(record.issue_number, 1);
    assert_eq!(record.marker, "pub-1");
    assert!(f.github.calls().contains(&Call::Update(1)));
    assert!(!f.github.calls().contains(&Call::Create("acme/widgets".into())));

    let issue = f.github.issues.lock().unwrap()[0].clone();
    assert_eq!(issue.title, "Widget window v2");
    assert!(issue.body.contains("Do the other thing."));
    assert!(issue.body.ends_with("<!-- synthesis-publication-marker: pub-1 -->"));

    let store = f.store();
    assert_eq!(store.publication.len(), 1);
    assert_eq!(store.attempt, None);
}

/// GHP-FR-QLDF / GHP-FR-UZMX: a deliberate re-publication takes a new marker,
/// creates a new issue, and appends a record without touching the earlier one.
#[test]
fn a_deliberate_republication_creates_a_second_issue_and_keeps_the_first_record() {
    let f = fixture("Do the thing.\n");
    let first = f.attempt("pub-1");
    f.publish(&first).unwrap();
    let before = f.store().publication[0].clone();

    save_prompt(&f.root, &f.id, "Now with more detail.\n");
    let second = f.attempt("pub-2");
    f.publish(&second).unwrap();

    let store = f.store();
    assert_eq!(store.publication.len(), 2);
    assert_eq!(store.publication[0], before);
    assert_eq!(store.publication[1].issue_number, 2);
    assert_eq!(store.publication[1].marker, "pub-2");
    assert_eq!(store::current_of(&store).unwrap().issue_number, 2);
    assert_eq!(f.github.issues.lock().unwrap().len(), 2);
}

/// GHP-FR-VNCS: an edit that fails appends no record and leaves the attempt
/// standing in `awaiting_choice`, so the choice can be answered again.
#[test]
fn a_failed_edit_appends_nothing_and_keeps_the_attempt() {
    let f = fixture("Do the thing.\n");
    let attempt = f.attempt("pub-1");
    let stale = flow::issue_content(&f.root, &f.id, "pub-1").unwrap();
    f.github.create_issue("s", "acme", "widgets", &stale.title, &stale.body, &Default::default()).unwrap();
    save_prompt(&f.root, &f.id, "Do the other thing.\n");
    f.publish(&attempt).unwrap();
    assert_eq!(f.store().attempt.unwrap().state, AttemptState::AwaitingChoice);

    // The issue is gone from under the choice — the update cannot find it, and
    // the create it falls back to is refused.
    f.github.issues.lock().unwrap().clear();
    *f.github.create_error.lock().unwrap() = Some(ERR_ISSUE_CREATE_FAILED.to_string());
    assert_eq!(
        flow::update_existing(&f.root, &f.id, &attempt, "secret", &f.github),
        Err(ERR_ISSUE_CREATE_FAILED.to_string())
    );
    let store = f.store();
    assert!(store.publication.is_empty());
    assert_eq!(store.attempt.unwrap().marker, "pub-1");
}

/// GHP-FR-QLDF: a deliberate re-publication takes a marker of its own, and two
/// markers are never the same value.
#[test]
fn every_attempt_takes_a_marker_of_its_own() {
    let markers: std::collections::HashSet<String> =
        (0..64).map(|_| flow::new_marker()).collect();
    assert_eq!(markers.len(), 64);
    assert!(markers.iter().all(|m| m.starts_with("pub-")));
    // And the marker line is the fixed format the issue body carries.
    let one = markers.iter().next().unwrap();
    assert_eq!(
        flow::marker_line(one),
        format!("<!-- synthesis-publication-marker: {one} -->")
    );
}

