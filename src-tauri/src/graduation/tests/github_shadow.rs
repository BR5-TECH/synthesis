//! A GitHub-shadow draft graduates like any other draft (GSU-FR-TTAY,
//! GRD-FR-JGEC, and `DRS-draft-storage.md` DRS-FR-JYIO).

use super::*;

impl Fixture {
    /// A GitHub-shadow draft in the project, as a claim creates it.
    fn shadow_draft(&self, title: &str, body: &str) -> String {
        let access = self.app.state::<crate::fs::FsAccessState>().get().expect("an instance");
        let root = crate::fs::RootFs::new(self.root(), access);
        let link = crate::drafts::GithubIssueLink {
            repository_host: "github.com".into(),
            repository_owner: "acme".into(),
            repository_name: "widgets".into(),
            issue_number: 4,
            issue_url: "https://github.com/acme/widgets/issues/4".into(),
            project_node_id: "PVT_1".into(),
            claim_state: crate::drafts::GithubClaimState::Claimed,
        };
        crate::drafts::create_github_shadow_draft(&root, title, body, link)
            .expect("a shadow draft")
            .draft
            .id
    }

    fn resolved_status(&self, draft_id: &str) -> crate::drafts::DraftStatus {
        let access = self.app.state::<crate::fs::FsAccessState>().get().expect("an instance");
        let root = crate::fs::RootFs::new(self.root(), access);
        let mut record = crate::drafts::draft_record(&root, draft_id).expect("the record");
        crate::drafts::resolve_graduated_status(&self.app, &mut record);
        record.status
    }
}

/// GSU-FR-TTAY / DRS-FR-JYIO: a shadow draft is admitted on the terms of any
/// other draft, its prompt is captured whole, and a refused start leaves it
/// `github_shadow` with nothing changed.
#[test]
fn a_shadow_draft_is_admitted_to_graduation() {
    let fx = Fixture::new();
    fx.allow_execution();
    let stream = fx.stream("editor");
    let draft = fx.shadow_draft("Fix the login page", "Make the login page load.\n");

    // A refused start: the stream does not exist.
    let refused = crate::graduation::start_graduation(
        fx.app.clone(),
        draft.clone(),
        "no-such-stream".into(),
        StandingWork::Keep,
        None,
    );
    assert!(refused.is_err());
    assert_eq!(fx.resolved_status(&draft), crate::drafts::DraftStatus::GithubShadow);
    // GSU-FR-RNOM: a refused start asks for no graduation-start commit.
    let root = crate::fs::RootFs::for_root(fx.root());
    let graduation_events = || {
        crate::storage_floor::commit::pending_events(&root)
            .into_iter()
            .filter(|(id, event, _)| {
                id == &draft
                    && *event == crate::storage_floor::commit::DraftEvent::GraduationStarted
            })
            .map(|(_, _, message)| message)
            .collect::<Vec<_>>()
    };
    assert!(graduation_events().is_empty(), "a refused start commits nothing");
    let queue = crate::graduation::list_graduation_queue(fx.app.clone()).expect("the queue");
    assert!(queue.runs.is_empty(), "a refused start made no run");

    fx.hold_stream(&stream.id);
    let run = crate::graduation::start_graduation(
        fx.app.clone(),
        draft.clone(),
        stream.id.clone(),
        StandingWork::Keep,
        None,
    )
    .expect("a shadow draft starts like any other");
    assert_eq!(run.input.draft_id, draft);
    assert_eq!(run.input.prompt, "Make the login page load.\n");
    // GSU-FR-RNOM / PST-FR-YWXF: an enqueued start asks for one
    // graduation-start commit, named for the draft.
    assert_eq!(graduation_events(), vec!["draft: graduate \"Fix the login page\"".to_string()]);
    // DRS-FR-JYIO: the graduation lock applies to it unchanged.
    assert!(crate::graduation::require_unlocked_draft(&fx.app, &draft).is_err());
}

/// GRD-FR-JGEC / DRS-FR-EZDB: a run of a shadow draft that commits makes it
/// report `graduated`; once that run is discarded it reports `github_shadow`
/// again, never `active`.
#[test]
fn a_committed_run_makes_a_shadow_draft_report_graduated() {
    let fx = Fixture::new();
    let stream = fx.stream("editor");
    let draft = fx.shadow_draft("Task", "Do it.\n");
    let mut run = fx.enqueue_for(&stream, "Do it.\n", &draft);
    run.commits = vec!["abc123".into()];
    run.state = GraduationRunState::Completed;
    crate::graduation::save_run(&fx.app, &mut run).expect("saved");
    assert_eq!(fx.resolved_status(&draft), crate::drafts::DraftStatus::Graduated);

    run.state = GraduationRunState::Discarded;
    crate::graduation::save_run(&fx.app, &mut run).expect("saved");
    assert_eq!(fx.resolved_status(&draft), crate::drafts::DraftStatus::GithubShadow);
    // The link is untouched by the run.
    let access = fx.app.state::<crate::fs::FsAccessState>().get().expect("an instance");
    let root = crate::fs::RootFs::new(fx.root(), access);
    assert!(crate::drafts::is_github_shadow(&root, &draft));
}
