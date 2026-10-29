//! Tests for `lead` and `trail` on a draft's changes (PDC-FR-WCXD,
//! PDC-FR-DMTH, PDC-FR-YSKV, DCP-FR-JGCD).

use super::*;

const REPEATED: &str = "# Spec\n\nKeep it short.\n\n## Notes\n\nKeep it short.\n";

fn change(before: &str, after: &str, lead: Option<&str>, trail: Option<&str>) -> HunkArg {
    HunkArg {
        kind: None,
        before: Some(before.into()),
        after: Some(after.into()),
        after_text: None,
        lead: lead.map(Into::into),
        trail: trail.map(Into::into),
        note: None,
    }
}

fn with(hunks: Vec<HunkArg>) -> ProposeDraftChangesArgs {
    ProposeDraftChangesArgs { path: FILE.into(), rationale: RATIONALE.into(), revises: None, hunks }
}

impl Fixture {
    fn seed(&self, body: &str) {
        crate::drafts::save_draft_file_impl(&self.root(), &self.draft_id, FILE, body)
            .expect("seed");
    }
}

// PDC-FR-WCXD, DCP-FR-JGCD: `trail` names one copy of a repeated text; the
// recorded change holds `before` alone, and the prompt is not touched.
#[test]
fn a_trail_records_a_change_to_one_copy_of_a_repeated_text() {
    let f = Fixture::new();
    f.seed(REPEATED);
    f.call(with(vec![change("Keep it short.", "Keep it brief.", None, Some("\n\n## Notes"))]))
        .expect("recorded");
    let hunks = f.hunks_of_the_one_proposal();
    assert_eq!(hunks[0].before_text(), "Keep it short.");
    assert_eq!(hunks[0].anchor.hint_start, "# Spec\n\n".len());
    assert_eq!(f.file_body(), REPEATED);
}

// PDC-FR-YSKV, PDC-FR-TZKQ: a repeated text is refused with its place and the
// correction, with no context and with context that still repeats.
#[test]
fn a_repeated_text_is_refused_with_its_place_and_names_lead_and_trail() {
    let f = Fixture::new();
    f.seed(REPEATED);
    for second in [
        change("Keep it short.", "x", None, None),
        change("Keep it short.", "x", Some("\n\n"), Some("\n")),
    ] {
        let refusal = f
            .call(with(vec![change("# Spec", "# Rules", None, None), second]))
            .expect_err("refused");
        assert_eq!(
            refusal,
            ToolRefusal::InvalidArgumentsAt { message: PROPOSAL_TEXT_NOT_UNIQUE, at: 2, of: 2 },
        );
        assert!(refusal.retryable());
        let text = refusal.to_string();
        assert!(text.starts_with("Change 2 of 2"), "{text}");
        assert!(text.contains("`lead` or `trail`"), "{text}");
        assert!(f.proposals().is_empty());
    }
}

// PDC-FR-DMTH: a `lead` over 200 characters is refused with its place, and
// nothing is recorded.
#[test]
fn a_context_over_its_bound_is_refused_with_its_place() {
    let f = Fixture::new();
    let long = "x".repeat(201);
    let refusal = f
        .call(with(vec![change("The original.", "Better.", Some(&long), None)]))
        .expect_err("refused");
    assert_eq!(
        refusal,
        ToolRefusal::InvalidArgumentsAt { message: PROPOSAL_CONTEXT_TOO_LONG, at: 1, of: 1 },
    );
    assert_eq!(refusal.kind(), ToolErrorKind::InvalidArgs);
    assert!(refusal.retryable());
    assert!(f.proposals().is_empty());
}

// PDC-FR-DMTH, PDC-FR-GMWR: a revision is placed by the same rule, so a long
// context is the same refusal there.
#[test]
fn a_revision_with_a_context_over_its_bound_is_the_same_refusal() {
    let f = Fixture::new();
    f.call(good()).expect("recorded");
    let id = f.hunks_of_the_one_proposal()[0].id.clone();
    let mut args = with(vec![change(ORIGINAL, PROPOSED, None, Some(&"y".repeat(201)))]);
    args.revises = Some(id);
    let refusal = f.call(args).expect_err("refused");
    assert_eq!(refusal, ToolRefusal::InvalidArguments(PROPOSAL_CONTEXT_TOO_LONG));
}

// PDC-FR-18: the refusal is logged under its own code with the place, and no
// text of the change.
#[test]
fn a_context_refusal_is_logged_with_its_code_and_place() {
    static BUFFER: LogBuffer = LogBuffer::new();
    BUFFER.clear();
    let f = Fixture::new();
    let long = "secret".repeat(40);
    block_on(f.tool().with_buffer(&BUFFER).call(with(vec![change(
        "The original.",
        "Better.",
        Some(&long),
        None,
    )])))
    .expect_err("refused");
    let records = records(&BUFFER, Domain::Ai);
    let warn = records.iter().find(|r| r.level == LogLevel::Warn).expect("the refusal");
    let fields = serde_json::to_value(&warn.fields).expect("fields");
    assert_eq!(fields["refusal"], "context_too_long");
    assert_eq!(fields["refusalAt"], 1);
    assert!(!fields.to_string().contains("secret"), "no context text: {fields}");
}

// PDC-FR-02: the description and the parameter text tell the model about
// `lead`, `trail`, and reading only the parts it changes.
#[test]
fn the_definition_names_lead_trail_and_ranged_reads() {
    for text in [DESCRIPTION, HUNKS_DESCRIPTION] {
        assert!(text.contains("`lead`") && text.contains("`trail`"), "{text}");
        assert!(text.contains("200 characters"), "{text}");
    }
    assert!(DESCRIPTION.contains("`offset` and `limit` on `read_draft`"));
    let schema = parameters();
    let item = &schema["properties"]["hunks"]["items"]["properties"];
    assert_eq!(item["lead"]["description"], LEAD_DESCRIPTION);
    assert_eq!(item["trail"]["description"], TRAIL_DESCRIPTION);
}

fn ada() -> crate::comments::Participant {
    crate::comments::Participant::Human { login: "ada".into(), display_name: None, email: None }
}

// PDC-FR-WCXD, DCP-FR-JGCD: accepting a change recorded with `lead` changes the
// copy the lead named and no other.
#[test]
fn accepting_a_change_with_a_lead_changes_only_the_named_copy() {
    let f = Fixture::new();
    f.seed(REPEATED);
    f.call(with(vec![change("Keep it short.", "Keep it brief.", Some("## Notes\n\n"), None)]))
        .expect("recorded");
    let proposal = f.proposals().remove(0);
    let hunk = f.hunks_of_the_one_proposal().remove(0);
    crate::draft_proposals::accept_hunk_for_test(
        &f.handle(),
        &f.root(),
        &f.root(),
        &proposal.id,
        &hunk.id,
        &ada(),
    )
    .expect("accepted");
    assert_eq!(f.file_body(), "# Spec\n\nKeep it short.\n\n## Notes\n\nKeep it brief.\n");
}

// PDC-FR-WCXD: an insertion is placed by `lead` + `after_text` + `trail`, so a
// repeated `after_text` takes its new text after the copy the context names.
#[test]
fn an_insertion_with_lead_and_trail_is_placed_after_the_named_copy() {
    let f = Fixture::new();
    f.seed(REPEATED);
    let insertion = HunkArg {
        kind: None,
        before: None,
        after: Some("\nAnd say why.".into()),
        after_text: Some("Keep it short.".into()),
        lead: Some("## Notes\n\n".into()),
        trail: Some("\n".into()),
        note: None,
    };
    f.call(with(vec![insertion])).expect("recorded");
    let proposal = f.proposals().remove(0);
    let hunk = f.hunks_of_the_one_proposal().remove(0);
    crate::draft_proposals::accept_hunk_for_test(
        &f.handle(),
        &f.root(),
        &f.root(),
        &proposal.id,
        &hunk.id,
        &ada(),
    )
    .expect("accepted");
    assert_eq!(
        f.file_body(),
        "# Spec\n\nKeep it short.\n\n## Notes\n\nKeep it short.\nAnd say why.\n"
    );
}
