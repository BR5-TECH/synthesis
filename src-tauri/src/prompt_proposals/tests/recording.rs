//! Tests for the recording of a prompt change proposal (PCP-FR-01 … PCP-FR-09).

use super::*;

// ---------------------------------------------------------------------------
// Recording (PCP-FR-01 … PCP-FR-09)
// ---------------------------------------------------------------------------

#[test]
fn a_recording_writes_two_files_beside_the_project_and_touches_no_artifact() {
    // PCP-FR-01, PCP-FR-02, PCP-FR-09.
    let f = Fixture::new();
    let proposal = f.pending();

    assert_eq!(
        f.folder_files(),
        vec![
            format!("{}.content", proposal.id),
            format!("{}.toml", proposal.id),
        ],
        "a proposal at rest is a record and a document, and nothing else",
    );
    assert_eq!(
        std::fs::read_to_string(f.dir().join(format!("{}.content", proposal.id))).unwrap(),
        PROPOSED,
        "the content file holds the proposed text byte-for-byte",
    );
    assert_eq!(f.artifact_body(), ORIGINAL, "the artifact is untouched");
    // PCP-FR-02: registered through `ensure_gitignored` when the folder is
    // created, so `git status` never sees a candidate. The entry is **anchored**
    // (FSA-FR-08): unanchored it would also exclude every draft's own
    // `proposals/` folder, which is committed draft-owned storage (per
    // `DRS-draft-storage.md` DRS-FR-04, `DCP-draft-change-proposals.md`
    // DCP-FR-02).
    let ignore = std::fs::read_to_string(f.root.path().join(".synthesis/.gitignore"))
        .expect("the ignore file");
    assert!(
        ignore.lines().any(|line| line.trim() == "/proposals/"),
        "the proposals folder is gitignored: {ignore:?}",
    );
}

#[test]
fn the_folder_comes_into_being_with_the_first_write_that_needs_it() {
    // PCP-FR-01: a project nobody has proposed a change in holds no such folder.
    let f = Fixture::new();
    assert!(!f.dir().exists(), "no folder before the first proposal");
    f.pending();
    assert!(f.dir().is_dir());
}

#[test]
fn an_artifact_carries_one_pending_proposal_at_a_time() {
    // PCP-FR-04, PCP-FR-07.
    let f = Fixture::new();
    let first = f.pending();
    let before = f.discussion().comments.len();

    assert_eq!(
        f.propose("# Review\n\nAnother version entirely.\n"),
        Err(RecordRefusal::ProposalPending),
    );
    assert_eq!(
        f.folder_files().len(),
        2,
        "no second candidate was written",
    );
    assert_eq!(
        f.discussion().comments.len(),
        before,
        "the conversation gained no line",
    );

    f.decline(&first.id).expect("declined");
    f.propose("# Review\n\nAnother version entirely.\n")
        .expect("a decided proposal frees the slot");

    // …and the rule is per artifact.
    let other = "prompts/other.md";
    write_file(&f.root, other, "# Other\n");
    assign(
        &f.root,
        &[(PROMPT, ArtifactType::Prompt), (other, ArtifactType::Prompt)],
    );
    f.propose_against(other, "# Other\n\nBetter.\n")
        .expect("a second artifact has its own slot");
}

#[test]
fn no_tauri_command_records_a_proposal() {
    // PCP-FR-05: the recording path is internal, so no frontend call
    // can forge a proposal or attribute one to an agent.
    const LIB: &str = include_str!("../../lib.rs");
    assert!(
        !LIB.contains("record_prompt_proposal"),
        "the recording path is absent from the handler registry",
    );
    const MODULE: &str = include_str!("../commands.rs");
    let commands: Vec<&str> = MODULE
        .split("#[tauri::command]")
        .skip(1)
        .filter_map(|tail| tail.split("pub fn ").nth(1))
        .filter_map(|tail| tail.split('<').next())
        .filter_map(|name| name.split('(').next())
        .collect();
    assert_eq!(
        commands,
        vec![
            "list_prompt_change_proposals",
            "load_prompt_change_proposal_content",
            "save_prompt_change_proposal_candidate",
            "apply_prompt_change_proposal",
            "decline_prompt_change_proposal",
            "complete_prompt_change_decision",
        ],
        "these six and no route that records or accepts an agent participant",
    );
}

#[test]
fn a_recording_posts_one_comment_carrying_one_prompt_proposal_attachment() {
    // PCP-FR-05, CMS-FR-66.
    let f = Fixture::new();
    let before = f.discussion().comments.len();
    let proposal = f.pending();

    let thread = f.discussion();
    assert_eq!(thread.comments.len(), before + 1);
    let posted = thread.comments.last().expect("the agent's comment");
    assert_eq!(posted.id, proposal.comment_id);
    assert_eq!(
        posted.body, "The instructions bury the important step.",
        "the body is the rationale and nothing else",
    );
    assert_eq!(posted.attachments.len(), 1);
    match &posted.attachments[0] {
        comments::Attachment::PromptProposal {
            proposal_id,
            artifact_id,
            path,
        } => {
            assert_eq!(proposal_id, &proposal.id);
            assert_eq!(artifact_id, PROMPT);
            assert_eq!(path, PROMPT);
        }
        other => panic!("expected a prompt proposal reference, got {other:?}"),
    }
    assert_eq!(posted.author, agent(), "posted through the agent write path");
    assert_eq!(proposal.thread_id, f.thread_id);
}

#[test]
fn a_recording_whose_append_fails_leaves_nothing_behind() {
    // PCP-FR-06: a locked thread is the append that cannot happen.
    let f = Fixture::new();
    f.lock();
    let before = f.discussion().comments.len();
    assert_eq!(f.propose(PROPOSED), Err(RecordRefusal::ThreadLocked));
    assert!(
        f.folder_files().is_empty(),
        "neither the record nor the candidate remains",
    );
    assert_eq!(f.discussion().comments.len(), before);
}

#[test]
fn a_recording_whose_candidate_cannot_be_written_appends_nothing() {
    // PCP-FR-06 second clause / PCP-FR-06: a call that could not write the
    // candidate appends no comment, and the folder holds nothing from it.
    let f = Fixture::new();
    let before = f.discussion().comments.len();
    let synthesis = f.root.path().join(".synthesis");
    let restore = std::fs::metadata(&synthesis).expect("dir").permissions();
    let mut sealed = restore.clone();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        sealed.set_mode(0o500);
    }
    #[cfg(not(unix))]
    sealed.set_readonly(true);
    std::fs::set_permissions(&synthesis, sealed).expect("seal");

    let refused = f.propose(PROPOSED);
    std::fs::set_permissions(&synthesis, restore).expect("unseal");

    assert_eq!(refused, Err(RecordRefusal::NotRecorded));
    assert_eq!(f.discussion().comments.len(), before, "no comment was appended");
    assert!(f.folder_files().is_empty());
    assert_eq!(f.artifact_body(), ORIGINAL);
}

#[test]
fn two_recordings_racing_one_artifact_leave_exactly_one_proposal() {
    // PCP-FR-04: the check and the writes that follow it are **serialised per
    // artifact**, because several turns are in flight at once (AGC-FR-03) and
    // two agents asked about one file can otherwise both pass the check —
    // leaving the project holding two undecided proposals, of which only one is
    // reachable from any surface while the other holds the slot.
    let f = Fixture::new();
    let outcomes: Vec<Result<PromptChangeProposal, RecordRefusal>> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..2)
            .map(|n| {
                let handle = f.handle();
                let root = f.root.clone();
                let thread_id = f.thread_id.clone();
                scope.spawn(move || {
                    record_prompt_proposal(
                        &handle,
                        &root,
                        &root,
                        NewPromptProposal {
                            artifact_id: PROMPT,
                            content: &format!("# Review\n\nVersion {n}.\n"),
                            rationale: "why",
                            agent: &agent(),
                            thread_id: &thread_id,
                        },
                    )
                })
            })
            .collect();
        handles.into_iter().map(|h| h.join().expect("joined")).collect()
    });

    let recorded: Vec<&PromptChangeProposal> =
        outcomes.iter().filter_map(|o| o.as_ref().ok()).collect();
    assert_eq!(recorded.len(), 1, "exactly one was recorded: {outcomes:?}");
    assert!(
        outcomes
            .iter()
            .any(|o| o == &Err(RecordRefusal::ProposalPending)),
        "and the other found the slot taken: {outcomes:?}",
    );
    assert_eq!(
        list_proposals_impl(&f.root, PROMPT).len(),
        1,
        "so the artifact holds one proposal and not two",
    );
    assert_eq!(
        f.discussion().comments.len(),
        2,
        "and the conversation gained one line",
    );
}

#[test]
fn a_recording_refuses_a_target_that_is_missing_or_not_a_prompt() {
    // PCP-FR-07, PCP-FR-08.
    let f = Fixture::new();
    write_file(&f.root, "specifications/ui/EDT-editor.md", "# Editor\n");
    write_file(&f.root, "notes/plain.md", "just text\n");
    assign(
        &f.root,
        &[
            (PROMPT, ArtifactType::Prompt),
            ("specifications/ui/EDT-editor.md", ArtifactType::Spec),
        ],
    );

    assert_eq!(
        f.propose_against("specifications/ui/EDT-editor.md", "# Editor\n\nBetter.\n"),
        Err(RecordRefusal::NotAPromptArtifact),
        "a file of another type",
    );
    assert_eq!(
        f.propose_against("notes/plain.md", "different\n"),
        Err(RecordRefusal::NotAPromptArtifact),
        "and a file the scan surfaces with no resolved type",
    );
    assert_eq!(
        f.propose_against("prompts/gone.md", "anything\n"),
        Err(RecordRefusal::ArtifactNotFound),
    );
    assert!(
        !f.root.path().join("prompts/gone.md").exists(),
        "PCP-FR-08: no route here creates a file",
    );
    assert!(f.folder_files().is_empty());

    // A candidate differing from the artifact only in its line endings changes
    // nothing on disk, so it is nothing to decide.
    let crlf = ORIGINAL.replace('\n', "\r\n");
    assert_eq!(f.propose(&crlf), Err(RecordRefusal::NoChange));
}

#[test]
fn an_empty_candidate_is_a_change_like_any_other() {
    // PCP-FR-14 / PCP-FR-07, PCP-FR-09, PCP-FR-13.
    let f = Fixture::new();
    let proposal = f.propose("").expect("an empty candidate is recorded");
    let content = f.dir().join(format!("{}.content", proposal.id));
    assert!(content.is_file());
    assert_eq!(std::fs::metadata(&content).unwrap().len(), 0);
    assert_eq!(proposal.state, PromptProposalState::Pending);
    assert_eq!(f.artifact_body(), ORIGINAL);

    let loaded = load_content_impl(&f.root, &proposal.id).expect("loaded");
    assert_eq!(loaded.content, "");
    assert_eq!(loaded.checksum, fs::sha256_bytes(b""));

    f.accept(&proposal.id).expect("accepted");
    assert_eq!(f.artifact_body(), "", "the file is emptied");
    assert!(
        f.root.path().join(PROMPT).is_file(),
        "and still exists as a file of the project",
    );

    // …and an empty candidate against an already-empty prompt is the ordinary
    // no-change refusal rather than a rule of its own.
    assert_eq!(f.propose(""), Err(RecordRefusal::NoChange));
}

#[test]
fn a_candidate_is_kept_verbatim_and_the_write_settles_the_line_endings() {
    // PCP-FR-09, PCP-FR-13.
    let f = Fixture::new();
    let odd = "# Review   \n\n\t  indented\n\nno final newline";
    let proposal = f.propose(odd).expect("recorded");
    assert_eq!(load_content_impl(&f.root, &proposal.id).unwrap().content, odd);

    // The project's convention is what the write applies, and it applies it to
    // the artifact rather than to the candidate.
    crate::project_settings::save_project_config_to(
        &f.root,
        crate::project_settings::ProjectConfig {
            line_endings: crate::project_settings::LineEndings::Crlf,
            draft_template: None,
                    ..Default::default()
        },
    )
    .expect("convention");
    f.accept(&proposal.id).expect("accepted");
    let landed = f.artifact_body();
    assert!(landed.contains("# Review   \r\n"), "trailing space survives");
    assert!(landed.contains("\t  indented"), "indentation survives");
    assert!(!landed.ends_with('\n'), "a missing final newline survives");
    assert_eq!(
        load_content_impl(&f.root, &proposal.id).unwrap().content,
        odd,
        "the candidate file is still byte-for-byte what it was",
    );
}

#[test]
fn a_list_returns_every_proposal_most_recently_created_first() {
    // PCP-FR-10.
    let f = Fixture::new();
    let mut ids = Vec::new();
    for n in 0..4 {
        let proposal = f.propose(&format!("# Review {n}\n")).expect("recorded");
        // `created_at` is second-granular, so four proposals recorded inside one
        // second would tie and the order would be the tie-break's rather than
        // the one under test. Restamped so the sort key is actually exercised.
        let mut record = read_proposal(&f.root, &proposal.id).expect("record");
        record.created_at = format!("2026-01-0{}T00:00:00Z", n + 1);
        f.root
            .write_toml_atomic(f.dir().join(format!("{}.toml", proposal.id)), &record)
            .expect("restamp");
        ids.push(proposal.id.clone());
        if n < 3 {
            f.decline(&proposal.id).expect("declined");
        }
    }
    let listed: Vec<String> = f.list().into_iter().map(|p| p.id).collect();
    ids.reverse();
    assert_eq!(listed, ids, "most recently created first, all four states");
    // PCP-FR-10: reads records and never a `.content` file, so a long history
    // costs a list no more than a single proposal does.
    const MODULE: &str = include_str!("../../prompt_proposals.rs");
    let list_body = MODULE
        .split("pub fn list_proposals_impl")
        .nth(1)
        .expect("the list")
        .split("\n}")
        .next()
        .expect("its body");
    assert!(
        !list_body.contains("CONTENT_EXT"),
        "a list reads no candidate: {list_body}",
    );
}
