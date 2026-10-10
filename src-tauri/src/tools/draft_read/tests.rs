//! Tests for `RDT-read-draft-tool.md`.
//!
//! The group-wide claims of `TLC-tool-conventions.md` are swept over every tool
//! at once in `../tests.rs`. What is here is this tool's own behaviour: which
//! version it reads, what it will not read, and what it refuses.

use rig::tool::PortableTool;
use tauri::Manager;

use super::*;
use crate::drafts::DraftStatus;
use crate::logging::{Domain, LogBuffer, LogFilter, LogLevel};
use crate::tools::tests::{agent_session, block_on};

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

struct DraftFixture {
    fixture: crate::tools::tests::Fixture,
    session: String,
}

impl DraftFixture {
    fn new() -> Self {
        let fixture = crate::tools::tests::empty_project();
        let root = fixture
            .app
            .state::<crate::bm25_index::Bm25Indexer>()
            .root()
            .expect("mounted");
        let session = agent_session(&fixture.app, &root, "draft-read");
        DraftFixture { fixture, session }
    }

    fn root(&self) -> crate::fs::RootFs {
        let root = self
            .fixture
            .app
            .state::<crate::bm25_index::Bm25Indexer>()
            .root()
            .expect("mounted");
        crate::fs::RootFs::for_root(&root)
    }

    fn draft(&self, name: &str, body: &str) -> String {
        let root = self.root();
        let created = crate::drafts::create_draft_at_root(&root, Some(name))
            .expect("the scaffold completes");
        let id = created.draft.id.clone();
        let prompt = created
            .draft
            .prompt_path
            .clone()
            .expect("a created draft holds its prompt");
        crate::drafts::save_draft_file_impl(&root, &id, &prompt, body).expect("the prompt writes");
        id
    }

    fn tool(&self) -> DraftReadTool<tauri::test::MockRuntime> {
        DraftReadTool::new(self.fixture.handle(), self.session.clone())
    }

    fn logged(&self, buffer: &'static LogBuffer) -> DraftReadTool<tauri::test::MockRuntime> {
        DraftReadTool::with_buffer(self.fixture.handle(), self.session.clone(), buffer)
    }

    fn read(&self, draft_id: &str) -> Result<ReadDraftOutput, ToolRefusal> {
        block_on(self.tool().call(ReadDraftArgs {
            draft_id: draft_id.into(),
        }))
    }
}

// ---------------------------------------------------------------------------
// RDT-FR-01, RDT-FR-02: the contract surface
// ---------------------------------------------------------------------------

/// RDT-FR-01: a `rig` portable tool under the documented name.
#[test]
fn is_a_portable_tool_named_read_draft() {
    let fixture = DraftFixture::new();
    assert_eq!(NAME, "read_draft");
    assert_eq!(
        <DraftReadTool<tauri::test::MockRuntime> as PortableTool>::NAME,
        NAME
    );
    assert_eq!(
        rig::tool::tool_definition(&fixture.tool()).name,
        NAME
    );
}

/// RDT-FR-02: the fixed description, and a schema declaring `draftId` alone.
#[test]
fn definition_declares_one_parameter_and_no_other() {
    let fixture = DraftFixture::new();
    let definition = rig::tool::tool_definition(&fixture.tool());
    assert_eq!(definition.description, DESCRIPTION);

    let schema = definition.parameters;
    assert_eq!(schema["required"], serde_json::json!(["draft_id"]));
    assert_eq!(
        schema["properties"]["draft_id"]["description"],
        DRAFT_ID_DESCRIPTION
    );
    // RDT-FR-04: nothing here selects a version, and RDT-FR-06 offers no paging.
    let properties = schema["properties"].as_object().expect("an object");
    assert_eq!(properties.len(), 1, "one parameter and no other");
    for absent in [
        "version", "entryId", "entry_id", "revision", "at", "path", "offset", "limit",
        "draftId",
    ] {
        assert!(properties.get(absent).is_none(), "{absent} is not a parameter");
    }
}

// ---------------------------------------------------------------------------
// RDT-FR-03 … RDT-FR-04: which version, and which files
// ---------------------------------------------------------------------------

/// RDT-FR-03: the live prompt as it stands on disk, before any index pass has
/// observed it.
#[test]
fn reads_the_newest_persisted_prompt_without_the_index() {
    let fixture = DraftFixture::new();
    let id = fixture.draft("fresh", "# Plan\n\nthe first text\n");
    fixture.fixture.reindex();

    crate::drafts::save_draft_file_impl(
        &fixture.root(),
        &id,
        "fresh.md",
        "# Plan\n\nthe newly saved text\n",
    )
    .expect("the prompt writes");
    // Deliberately no reindex: the drafts index still holds the older text.

    let output = fixture.read(&id).expect("reads");
    assert_eq!(output.content, "# Plan\n\nthe newly saved text\n");
    assert!(!output.content.contains("the first text"));
}

/// RDT-FR-04, RDT-FR-10: the live prompt and never a history snapshot, and no argument
/// reaches one.
#[test]
fn never_returns_a_history_snapshot() {
    let fixture = DraftFixture::new();
    let id = fixture.draft("versioned", "# Plan\n\nthe live text\n");
    let dir = crate::drafts::draft_dir(&fixture.root(), &id).expect("resolves");

    // Two entries whose payloads differ from the live prompt.
    for (entry, body) in [("e1", "the original text"), ("e2", "the superseded text")] {
        std::fs::write(
            dir.join("history").join(format!("{entry}.snapshot")),
            format!("# Plan\n\n{body}\n"),
        )
        .expect("write");
        std::fs::write(
            dir.join("history").join(format!("{entry}.toml")),
            format!("id = \"{entry}\"\n"),
        )
        .expect("write");
    }

    let output = fixture.read(&id).expect("reads");
    assert_eq!(output.content, "# Plan\n\nthe live text\n");
    assert!(!output.content.contains("original"));
    assert!(!output.content.contains("superseded"));

    // RDT-FR-10: an entry id, a prompt path, and a version selector each name no
    // draft, so each is the unknown-draft refusal rather than a snapshot.
    for not_an_id in ["e1", "e2", "versioned.md", "files/versioned.md", "e1.snapshot"] {
        assert_eq!(
            fixture.read(not_an_id).unwrap_err(),
            ToolRefusal::DraftNotFound,
            "{not_an_id:?} must not resolve to anything"
        );
    }
}

/// RDT-FR-04: a proposal candidate, a comment log, and a conversation log are
/// none of them reachable.
#[test]
fn never_returns_sibling_storage() {
    let fixture = DraftFixture::new();
    let id = fixture.draft("surrounded", "# Plan\n\nthe live text\n");
    let dir = crate::drafts::draft_dir(&fixture.root(), &id).expect("resolves");
    std::fs::write(dir.join("proposals").join("p1.md"), "the candidate text\n").expect("write");
    std::fs::write(
        dir.join("proposals").join("p.content"),
        "the comment text\n",
    )
    .expect("write");
    std::fs::write(dir.join("conversation.jsonl"), "the conversation text\n").expect("write");

    let output = fixture.read(&id).expect("reads");
    assert_eq!(output.content, "# Plan\n\nthe live text\n");
    for leaked in ["candidate", "comment", "conversation"] {
        assert!(!output.content.contains(leaked));
    }
}

// ---------------------------------------------------------------------------
// RDT-FR-05, RDT-FR-07 … RDT-FR-08: the result's shape
// ---------------------------------------------------------------------------

/// RDT-FR-05, RDT-FR-07: a structured record carrying exactly the five documented fields.
#[test]
fn output_is_a_record_of_exactly_five_fields() {
    let fixture = DraftFixture::new();
    let id = fixture.draft("shape", "# Plan\n\nbody\n");

    let json = serde_json::to_value(fixture.read(&id).expect("reads")).expect("serialises");
    let mut keys: Vec<_> = json.as_object().expect("an object").keys().cloned().collect();
    keys.sort();
    assert_eq!(keys, vec!["content", "draft_id", "name", "prompt_path", "status"]);
    assert_eq!(json["draft_id"], serde_json::json!(id));
    assert_eq!(json["name"], serde_json::json!("shape"));
    assert_eq!(json["prompt_path"], serde_json::json!("shape.md"));
    // RDT-FR-05, RDT-FR-07: `status` serialises as the wire spelling a model reads, not as
    // a `Debug` rendering of the enum.
    assert_eq!(json["status"], serde_json::json!("active"));
    assert_eq!(json["content"], serde_json::json!("# Plan\n\nbody\n"));
}

/// RDT-FR-06, TLC-FR-07: the complete prompt, whatever its size, with no paging parameter
/// having any effect.
#[test]
fn returns_the_complete_prompt() {
    let fixture = DraftFixture::new();
    let body = format!("# Plan\n\n{}\n", "a long line of prompt text. ".repeat(80_000));
    assert!(body.len() > 2_000_000, "a prompt worth calling large");
    let id = fixture.draft("large", &body);

    let output = fixture.read(&id).expect("reads");
    assert_eq!(output.content, body, "byte-for-byte, with no truncation");

    // TLC-FR-07: an `offset` and a `limit` carried over from `read_file` are
    // ignored, and the complete prompt still comes back.
    let args: ReadDraftArgs = serde_json::from_value(serde_json::json!({
        "draftId": id,
        "offset": 5,
        "limit": 1,
    }))
    .expect("unknown fields are ignored");
    let paged = block_on(fixture.tool().call(args)).expect("reads");
    assert_eq!(paged.content, body);
}

/// RDT-FR-07: no field about how the author is managing the draft.
#[test]
fn omits_the_management_fields() {
    let fixture = DraftFixture::new();
    let id = fixture.draft("managed", "# Plan\n\nbody\n");
    let json = serde_json::to_value(fixture.read(&id).expect("reads")).expect("serialises");
    for absent in [
        "destinationRoot",
        "destination_root",
        "createdAt",
        "created_at",
        "updatedAt",
        "updated_at",
        "folder",
        "build",
        "hasPendingProposal",
        "inconsistent",
    ] {
        assert!(json.get(absent).is_none(), "{absent} is omitted");
    }
    // TLC-FR-07: a model that reached for the camelCase spelling of the one
    // multi-word argument in this group is forgiven rather than failed at the
    // decode, which would never even reach a clean `InvalidArgs`.
    let aliased: ReadDraftArgs =
        serde_json::from_value(serde_json::json!({ "draftId": "abc" })).expect("the alias decodes");
    assert_eq!(aliased.draft_id, "abc");
}

/// RDT-FR-08: every retained status reads on identical terms, and a rename or an
/// archive is reflected at once.
#[test]
fn reads_every_status_and_reports_the_current_record() {
    let fixture = DraftFixture::new();
    let active = fixture.draft("still active", "# Plan\n\nactive body\n");
    let archived = fixture.draft("retired", "# Plan\n\narchived body\n");
    crate::drafts::set_draft_status_impl(&fixture.root(), &archived, DraftStatus::Archived)
        .expect("archives");

    assert_eq!(fixture.read(&active).expect("reads").status, DraftStatus::Active);
    let a = fixture.read(&archived).expect("an archived draft reads identically");
    assert_eq!(a.status, DraftStatus::Archived);
    assert_eq!(a.content, "# Plan\n\narchived body\n");

    crate::drafts::rename_draft_impl(&fixture.root(), &active, "renamed now").expect("renames");
    crate::drafts::set_draft_status_impl(&fixture.root(), &active, DraftStatus::Archived)
        .expect("archives");
    let after = fixture.read(&active).expect("reads");
    assert_eq!(after.name, "renamed now");
    assert_eq!(after.status, DraftStatus::Archived);
    assert_eq!(after.prompt_path, "renamed now.md", "the rename moved the file");
    assert_eq!(after.content, "# Plan\n\nactive body\n", "content is untouched");
}

/// RDT-FR-09, RDT-FR-10: the prompt path is informational, and the id
/// `search_drafts` returns loads unchanged.
#[test]
fn prompt_path_is_informational_and_the_id_round_trips() {
    let fixture = DraftFixture::new();
    let id = fixture.draft("round trip", "# Plan\n\nteardown\n");
    fixture.fixture.reindex();

    let matches = block_on(
        crate::tools::draft_search::DraftSearchTool::new(
            fixture.fixture.handle(),
            fixture.session.clone(),
        )
        .call(crate::tools::draft_search::DraftSearchArgs {
            query: "teardown".into(),
            limit: None,
        }),
    )
    .expect("searches")
    .drafts;
    assert_eq!(matches.len(), 1);

    // RDT-FR-10: the id carries forward unchanged.
    let output = fixture.read(&matches[0].draft_id).expect("reads");
    assert_eq!(output.draft_id, id);

    // RDT-FR-09: the prompt path is not a `read_file` path to this prompt.
    let access = fixture
        .fixture
        .app
        .state::<crate::fs::FsAccessState>()
        .agent_session(&fixture.session)
        .expect("a session is open");
    assert!(
        crate::tools::file_read::read(
            &access,
            fixture.root().path(),
            &crate::tools::file_read::ReadFileArgs {
                path: output.prompt_path.clone(),
                offset: None,
                limit: None,
            },
        )
        .is_err(),
        "a draft-relative path names no project file"
    );
}

// ---------------------------------------------------------------------------
// RDT-FR-11 … RDT-FR-14: refusals
// ---------------------------------------------------------------------------

/// RDT-FR-11: a blank id is the retryable `InvalidArgs` refusal.
#[test]
fn blank_id_refuses_as_invalid_arguments() {
    let fixture = DraftFixture::new();
    for id in ["", "   ", "\n\t"] {
        let error = fixture.read(id).expect_err("a blank id refuses");
        assert_eq!(error, ToolRefusal::InvalidArguments(DRAFT_ID_BLANK));
        let execution = error.to_execution_error();
        assert_eq!(execution.kind(), rig::tool::ToolErrorKind::InvalidArgs);
        assert_eq!(execution.retryable(), Some(true));
    }
}

/// RDT-FR-12: an id no draft carries is the retryable `NotFound` refusal.
#[test]
fn unknown_id_refuses_as_not_found() {
    let fixture = DraftFixture::new();
    fixture.draft("present", "# Plan\n\nbody\n");

    for id in [
        "01JQZ0000000000000000000AA",
        "not-a-draft-id",
        "../../etc/passwd",
    ] {
        let error = fixture.read(id).expect_err("an unknown id refuses");
        assert_eq!(error, ToolRefusal::DraftNotFound, "for {id:?}");
        let execution = error.to_execution_error();
        assert_eq!(execution.kind(), rig::tool::ToolErrorKind::NotFound);
        assert_eq!(execution.retryable(), Some(true), "a different id reaches a draft that exists");
    }
}

/// RDT-FR-13: an inconsistent draft is the retryable `Other` refusal, nothing is
/// repaired, and a valid draft still reads.
#[test]
fn inconsistent_draft_refuses_without_repairing_anything() {
    let fixture = DraftFixture::new();
    let broken = fixture.draft("broken", "# Plan\n\nbody\n");
    let fine = fixture.draft("fine", "# Plan\n\nother body\n");
    let dir = crate::drafts::draft_dir(&fixture.root(), &broken).expect("resolves");
    std::fs::write(dir.join("files").join("intruder.md"), "# second\n").expect("write");

    let before = crate::tools::tests::tree_snapshot(&dir);
    let error = fixture.read(&broken).expect_err("an inconsistent draft refuses");
    assert_eq!(error, ToolRefusal::DraftInconsistent);
    let execution = error.to_execution_error();
    assert_eq!(execution.kind(), rig::tool::ToolErrorKind::Other);
    assert_eq!(execution.retryable(), Some(true), "naming another draft is what helps");
    assert_eq!(
        before,
        crate::tools::tests::tree_snapshot(&dir),
        "nothing deleted, moved, rewritten, or chosen as the prompt"
    );
    // TLC-FR-09: no part of the draft reaches the message.
    let message = error.to_string();
    assert!(!message.contains("intruder"));
    assert!(!message.contains("body"));

    assert!(fixture.read(&fine).is_ok(), "a different draft still reads");
}

/// RDT-FR-18: a prompt that is present and structurally sound but does not
/// decode as text is its own refusal, and emphatically **not** "no draft exists
/// with that ID".
///
/// The draft is there, its `files/` holds exactly one file, and the model's id
/// was right — so the two refusals either side of this one would each say
/// something false about it.
#[test]
fn an_unreadable_prompt_is_neither_missing_nor_inconsistent() {
    let fixture = DraftFixture::new();
    let id = fixture.draft("binary", "# Plan\n\nreadable for now\n");
    let dir = crate::drafts::draft_dir(&fixture.root(), &id).expect("resolves");
    // Invalid UTF-8 in the one file the draft holds. The file set is still the
    // single prompt DRS-FR-11 requires.
    std::fs::write(dir.join("files").join("binary.md"), [0xff, 0xfe, 0x00, 0x9f]).expect("write");

    let error = fixture.read(&id).expect_err("an unreadable prompt refuses");
    assert_eq!(error, ToolRefusal::DraftPromptUnreadable);
    let execution = error.to_execution_error();
    assert_eq!(execution.kind(), rig::tool::ToolErrorKind::Other);
    assert_eq!(execution.retryable(), Some(true));

    // The two neighbouring sentences would each be a falsehood here.
    let message = error.to_string();
    assert_ne!(message, crate::tools::DRAFT_NOT_FOUND);
    assert_ne!(message, crate::tools::DRAFT_INCONSISTENT);
    assert!(!message.contains("No draft exists"));
    // TLC-FR-09 / RFT-FR-15: not one byte of the file reaches the message.
    assert!(message.is_ascii(), "no byte of the prompt in the refusal");

    // And the draft still resolves as a record — which is what makes "no draft
    // exists with that ID" the wrong answer.
    assert!(crate::drafts::draft_record(&fixture.root(), &id).is_ok());
}

/// DST-FR-13 / RDT-FR-18 together: an unreadable prompt is never offered by
/// `search_drafts` in the first place, so the two tools cannot disagree.
///
/// A prompt that does not decode contributes no chunks to any index
/// (BMI-FR-09), so there is no hit for the search to resolve and no row the
/// model could carry to a `read_draft` that then refuses.
#[test]
fn an_unreadable_prompt_is_never_offered_by_search() {
    let fixture = DraftFixture::new();
    let ok = fixture.draft("fine", "# Plan\n\nteardown here\n");
    let bad = fixture.draft("broken", "# Plan\n\nteardown here too\n");
    fixture.fixture.reindex();

    let dir = crate::drafts::draft_dir(&fixture.root(), &bad).expect("resolves");
    std::fs::write(dir.join("files").join("broken.md"), [0xff, 0xfe, 0x00]).expect("write");
    fixture.fixture.reindex();

    let matches = block_on(
        crate::tools::draft_search::DraftSearchTool::new(
            fixture.fixture.handle(),
            fixture.session.clone(),
        )
        .call(crate::tools::draft_search::DraftSearchArgs {
            query: "teardown".into(),
            limit: None,
        }),
    )
    .expect("searches")
    .drafts;
    assert_eq!(matches.len(), 1, "only the readable draft is offered");
    assert_eq!(matches[0].draft_id, ok);
}

/// RDT-FR-12, second clause: an id belonging to a draft in a *different*
/// worktree is the unknown-draft refusal.
///
/// The one unknown-id case that could plausibly succeed by accident, because it
/// is the only one where a real draft directory exists somewhere on disk.
#[test]
fn a_draft_of_another_worktree_is_not_found() {
    let elsewhere = DraftFixture::new();
    let foreign = elsewhere.draft("foreign", "# Plan\n\nanother worktree\n");
    assert!(elsewhere.read(&foreign).is_ok(), "it reads in its own worktree");

    let fixture = DraftFixture::new();
    fixture.draft("local", "# Plan\n\nthis worktree\n");
    let error = fixture.read(&foreign).expect_err("a foreign draft is not found here");
    assert_eq!(error, ToolRefusal::DraftNotFound);
    // And nothing was created under this worktree's drafts root in the attempt.
    let strays = std::fs::read_dir(fixture.root().path().join(".synthesis/drafts"))
        .expect("the drafts root exists")
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy() == foreign)
        .count();
    assert_eq!(strays, 0, "no directory was composed from the id");
}

/// RDT-FR-06: an empty prompt is a success carrying empty content.
#[test]
fn an_empty_prompt_reads_as_empty_content() {
    let fixture = DraftFixture::new();
    let id = fixture.draft("blank", "");
    let output = fixture.read(&id).expect("an empty prompt is not a failure");
    assert_eq!(output.content, "");
    assert_eq!(output.prompt_path, "blank.md");
}

/// RDT-FR-14: with no project open, the shared refusal.
#[test]
fn no_open_project_refuses() {
    let app = crate::tools::tests::closed_project();
    let tool = DraftReadTool::new(app.handle().clone(), "closed");
    let error = block_on(tool.call(ReadDraftArgs {
        draft_id: "anything".into(),
    }))
    .expect_err("a closed project refuses");
    assert_eq!(error, ToolRefusal::NoProjectOpen);
    let execution = error.to_execution_error();
    assert_eq!(execution.kind(), rig::tool::ToolErrorKind::NotFound);
    assert_eq!(execution.retryable(), Some(false), "no argument opens a project");
}

/// RDT-FR-15: a draft created since the last pass reads, and one deleted since a
/// search named it refuses.
#[test]
fn answers_from_disk_rather_than_from_the_index() {
    let fixture = DraftFixture::new();
    let id = fixture.draft("unindexed", "# Plan\n\nbody\n");
    // No pass has ever seen this draft.
    assert_eq!(
        fixture.read(&id).expect("reads anyway").content,
        "# Plan\n\nbody\n"
    );

    fixture.fixture.reindex();
    crate::drafts::delete_draft_impl(&fixture.root(), &fixture.root(), &id).expect("deletes");
    // The index still holds it; the tool does not.
    assert_eq!(fixture.read(&id).unwrap_err(), ToolRefusal::DraftNotFound);
}

// ---------------------------------------------------------------------------
// RDT-FR-16, RDT-FR-17: read-only posture and logging
// ---------------------------------------------------------------------------

/// RDT-FR-16: reading changes nothing, `updated_at` included.
#[test]
fn is_read_only() {
    let fixture = DraftFixture::new();
    let a = fixture.draft("one", "# Plan\n\nbody\n");
    let b = fixture.draft("two", "# Plan\n\nother\n");
    let root = fixture.root();
    let updated_before = crate::drafts::draft_record(&root, &a).expect("resolves").updated_at;

    let before = crate::tools::tests::tree_snapshot(root.path());
    for _ in 0..5 {
        let _ = fixture.read(&a);
        let _ = fixture.read(&b);
        let _ = fixture.read("nope");
    }
    assert_eq!(
        before,
        crate::tools::tests::tree_snapshot(root.path()),
        "no prompt, record, history, proposal, comment log, or conversation changed"
    );
    assert_eq!(
        crate::drafts::draft_record(&root, &a).expect("resolves").updated_at,
        updated_before,
        "reading is not activity on the draft"
    );
}

static RDT_BUFFER: LogBuffer = LogBuffer::new();

/// RDT-FR-17: the records name the tool, the id, and the byte count, and carry
/// no returned field.
#[test]
fn logs_the_draft_id_and_no_returned_material() {
    let fixture = DraftFixture::new();
    let id = fixture.draft("SecretName", "# Plan\n\nconfidential prompt body\n");
    RDT_BUFFER.clear();

    let tool = fixture.logged(&RDT_BUFFER);
    let output = block_on(tool.call(ReadDraftArgs {
        draft_id: id.clone(),
    }))
    .expect("reads");
    block_on(tool.call(ReadDraftArgs {
        draft_id: "  missing-draft  ".into(),
    }))
    .expect_err("refuses");

    // Querying either domain returns the same two records — a tool call is
    // work the backend performs because a model asked for it, and a reader
    // filtering on either should see it.
    let of_domain = |domain: Domain| {
        RDT_BUFFER
            .query(
                &LogFilter {
                    min_level: LogLevel::Debug,
                    domains: vec![domain],
                    ..LogFilter::default()
                },
                None,
                1000,
            )
            .expect("the buffer answers")
            .records
    };
    let records = of_domain(Domain::Ai);
    assert_eq!(records.len(), 2, "one success and one refusal");
    assert_eq!(
        records,
        of_domain(Domain::Backend),
        "both domains return the same two records",
    );
    let success = records
        .iter()
        .find(|r| r.level == LogLevel::Info)
        .expect("an INFO record");
    assert_eq!(success.fields["tool"], serde_json::json!(NAME));
    assert_eq!(success.fields["draft_id"], serde_json::json!(id));
    assert_eq!(
        success.fields["bytes"],
        serde_json::json!(output.content.len())
    );
    // Pin the exact key set, so a field added to the shared helpers for another
    // tool cannot silently widen what this one records.
    assert_eq!(
        crate::tools::tests::sorted_keys(success),
        vec!["bytes", "draft_id", "tool"],
    );

    let refusal = records
        .iter()
        .find(|r| r.level == LogLevel::Warn)
        .expect("a WARN record");
    assert_eq!(refusal.fields["reason"], serde_json::json!("draft_not_found"));
    assert_eq!(
        crate::tools::tests::sorted_keys(refusal),
        vec!["draft_id", "reason", "retryable", "tool"],
    );
    assert_eq!(
        refusal.fields["draft_id"],
        serde_json::json!("  missing-draft  "),
        "as the model composed it, padding and all"
    );

    // RDT-FR-17: no prompt, name, status, or prompt path in any record.
    for record in &records {
        let blob = serde_json::to_string(&record.fields).expect("serialises") + &record.message;
        for leaked in [
            "SecretName",
            "confidential",
            "prompt body",
            "# Plan",
            ".md",
            "active",
        ] {
            assert!(
                !blob.contains(leaked),
                "{leaked:?} leaked into a log record: {blob}"
            );
        }
    }
}

static RDT_BOUND_BUFFER: LogBuffer = LogBuffer::new();

/// RDT-FR-17: a pathological id is recorded as its first 512 characters and an
/// ellipsis, so the record keeps the tool name and the reason.
#[test]
fn logs_a_bounded_draft_id() {
    let fixture = DraftFixture::new();
    RDT_BOUND_BUFFER.clear();

    block_on(fixture.logged(&RDT_BOUND_BUFFER).call(ReadDraftArgs {
        draft_id: "x".repeat(20_000),
    }))
    .expect_err("refuses");

    let records = RDT_BOUND_BUFFER.query(&LogFilter { min_level: LogLevel::Debug, ..LogFilter::default() }, None, 1000)
        .expect("the buffer answers")
        .records;
    let record = records.first().expect("a record");
    let logged = record.fields["draft_id"].as_str().expect("a string");
    assert_eq!(logged.chars().count(), 513, "512 characters and the ellipsis");
    assert!(logged.ends_with('…'));
    assert_eq!(record.fields["tool"], serde_json::json!(NAME));
    assert_eq!(record.fields["reason"], serde_json::json!("draft_not_found"));
}
