//! The refusals of a call with `diagrams`: each with its place, its kind, and
//! its retry flag, and each with no write and no comment (PDM-FR-RNUS,
//! PDM-FR-HOQI, PDM-FR-CEZB, PDM-FR-YKMV, PDM-FR-DWAF, PDM-FR-GFXA,
//! PDM-FR-UITL, PDM-FR-SDJP, PDM-FR-XRVZ, PDM-FR-AMCD, PDM-FR-TPYC,
//! PDM-FR-LJGN, PDM-FR-OHSB).

use super::*;
use crate::text_hunks::hunks::ContextSide;
use crate::tools::propose_draft_changes::context_too_long;

/// The refusal of `args`, with the proof that the call wrote nothing.
fn refused(f: &Fixture, args: ProposeDiagramChangesArgs) -> ToolRefusal {
    let before = f.world();
    let refusal = f.call(args).expect_err("refused");
    f.assert_untouched(&before);
    refusal
}

/// The refusal is the model's to correct.
fn assert_correctable(refusal: &ToolRefusal) {
    assert_eq!(refusal.kind(), ToolErrorKind::InvalidArgs);
    assert!(refusal.retryable(), "{refusal}");
}

// PDM-FR-RNUS, PDM-FR-UITL: a source that no Mermaid fence holds is the
// retryable refusal with the place of the change in the list the model sent.
#[test]
fn a_diagram_that_is_not_found_is_refused_with_its_place() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        let refusal = refused(
            &f,
            f.args(vec![
                change(SRC_A, SRC_A2),
                addition(SRC_B, "End line."),
                change("flowchart TD\n  Z --> Q", SRC_A3),
            ]),
        );
        assert_eq!(
            refusal,
            ToolRefusal::InvalidDiagramAt { message: DIAGRAM_NOT_FOUND, at: 3, of: 3 },
            "{subject:?}",
        );
        assert_correctable(&refusal);
        let text = refusal.to_string();
        assert!(text.starts_with("Diagram edit 3 of 3: "), "{text}");
        assert!(text.contains("character for character"), "{text}");
    }
}

// PDM-FR-RNUS, PDM-FR-UITL: an addition whose `after_text` the file does not
// hold is refused with the message that names `after_text`, and a change that
// stands beside it keeps the message that names `before`.
#[test]
fn an_addition_anchor_that_is_not_found_is_refused_with_its_place() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        let refusal = refused(
            &f,
            f.args(vec![
                change(SRC_A, SRC_A2),
                addition(SRC_B, "Not in the file."),
                change("flowchart TD\n  Z --> Q", SRC_A3),
            ]),
        );
        assert_eq!(
            refusal,
            ToolRefusal::InvalidDiagramAt { message: ADDITION_ANCHOR_NOT_FOUND, at: 2, of: 3 },
            "{subject:?}",
        );
        assert_correctable(&refusal);
        let text = refusal.to_string();
        assert!(text.starts_with("Diagram edit 2 of 3: "), "{text}");
        assert!(text.contains("`after_text`") && !text.contains("`before`"), "{text}");

        let later = refused(
            &f,
            f.args(vec![addition(SRC_B, "End line."), change("flowchart TD\n  Z --> Q", SRC_A3)]),
        );
        assert_eq!(
            later,
            ToolRefusal::InvalidDiagramAt { message: DIAGRAM_NOT_FOUND, at: 2, of: 2 },
            "{subject:?}",
        );
    }
}

// PDM-FR-RNUS, PDM-FR-UITL: an addition whose `after_text` the file holds more
// than once is refused with the message that names `after_text`, and `lead`
// or `trail` that leave one place name it.
#[test]
fn an_addition_anchor_that_is_not_unique_is_refused_with_its_place() {
    for subject in SUBJECTS {
        let f = Fixture::with_body(subject, TWICE);
        let refusal = refused(&f, f.args(vec![addition(SRC_B, "A --> B")]));
        assert_eq!(
            refusal,
            ToolRefusal::InvalidDiagramAt { message: ADDITION_ANCHOR_NOT_UNIQUE, at: 1, of: 1 },
            "{subject:?}",
        );
        assert_correctable(&refusal);
        let text = refusal.to_string();
        assert!(text.contains("`after_text`") && !text.contains("`before`"), "{text}");
        assert!(text.contains("`lead` or `trail`"), "{text}");

        let named = with_lead(addition(SRC_B, "section."), "Second ");
        f.propose(vec![named]).expect("one place is named");
    }
}

/// The file after the hunks of the one standing proposal are accepted.
fn accepted(f: &Fixture) -> String {
    let doc = crate::text_hunks::hunks::HunkDocument::new(f.one().hunks);
    crate::text_hunks::hunks::apply_all(&f.body(), &doc).expect("the hunks apply")
}

// DCP-FR-WSLE, PDM-FR-RNUS: an addition anchor inside a Mermaid fence is not
// refused. The new fence goes after the closing line of that fence, so the
// fence that holds the anchor stays whole.
#[test]
fn an_addition_anchor_inside_a_mermaid_fence_goes_after_that_fence() {
    for subject in SUBJECTS {
        let f = Fixture::with_body(subject, TWICE);
        let named = with_trail(addition(SRC_B, "A --> B"), "\n```\n\nEnd line.");
        f.propose(vec![named]).expect("the anchor is found");
        let fence = "```mermaid\nflowchart TD\n  A --> B\n```";
        let new = "```mermaid\ngraph LR\n  X --> Y\n```";
        let expected = format!(
            "# Spec\n\nFirst section.\n\n{fence}\n\nSecond section.\n\n{fence}\n\n{new}\n\nEnd line.\n"
        );
        assert_eq!(accepted(&f), expected, "{subject:?}");
    }
}

// DCP-FR-WSLE, PDM-FR-RNUS: the same for a fence of another language.
#[test]
fn an_addition_anchor_inside_a_fence_of_another_language_goes_after_that_fence() {
    for subject in SUBJECTS {
        let f = Fixture::with_body(subject, OTHER_FENCE);
        f.propose(vec![addition(SRC_B, "nodes: [a, b]")]).expect("the anchor is found");
        let new = "```mermaid\ngraph LR\n  X --> Y\n```";
        let expected = format!(
            "# Spec\n\n```calm\nnodes: [a, b]\n```\n\n{new}\n\n```mermaid\nflowchart TD\n  A --> B\n```\n\nEnd line.\n"
        );
        assert_eq!(accepted(&f), expected, "{subject:?}");
    }
}

// PDM-FR-RNUS, PDM-FR-BWOH: the same two refusals for an `add` entry and a
// `change` entry of `updates`, each with the place of the update.
#[test]
fn an_addition_anchor_of_an_update_is_refused_with_its_place() {
    for subject in SUBJECTS {
        let f = Fixture::with_body(subject, TWICE);
        f.offer_text("First section.", "Opening.").expect("a text proposal stands");
        let before = f.world();
        let cases = [
            (
                vec![entry("add", None, None, Some(SRC_B), Some("Not in the file."))],
                ADDITION_ANCHOR_NOT_FOUND,
            ),
            (
                vec![entry("add", None, None, Some(SRC_B), Some("A --> B"))],
                ADDITION_ANCHOR_NOT_UNIQUE,
            ),
            (vec![entry("add", None, Some(SRC_A), Some(SRC_A2), None)], DIAGRAM_NOT_UNIQUE),
        ];
        for (updates, message) in cases {
            let refusal = f.call(f.updating(updates)).expect_err("refused");
            assert_eq!(
                refusal,
                ToolRefusal::InvalidUpdateAt { message, at: 1, of: 1 },
                "{subject:?}",
            );
            assert_correctable(&refusal);
            f.assert_untouched(&before);
        }
    }
}

// PDM-FR-YKMV: a `before` that is text and not the source of a diagram never
// reaches a diagram hunk.
#[test]
fn text_that_is_not_the_source_of_a_diagram_is_not_found() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        for before in ["Intro line.", "```mermaid", "flowchart", "# Spec"] {
            let refusal = refused(&f, f.args(vec![change(before, SRC_A2)]));
            assert_eq!(
                refusal,
                ToolRefusal::InvalidDiagramAt { message: DIAGRAM_NOT_FOUND, at: 1, of: 1 },
                "{subject:?}: {before:?}",
            );
        }
    }
}

// PDM-FR-DWAF: a change that carries neither `before` nor `after` is a diagram
// that is not found, whatever else it carries. The tool reads no argument as a
// line number or a region.
#[test]
fn a_change_with_neither_source_is_a_diagram_that_is_not_found() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        let empty = diagram(None, None, Some("End line."));
        let blank = diagram(Some("  "), Some(""), None);
        for item in [empty, blank] {
            let refusal = refused(&f, f.args(vec![item]));
            assert_eq!(
                refusal,
                ToolRefusal::InvalidDiagramAt { message: DIAGRAM_NOT_FOUND, at: 1, of: 1 },
            );
        }
    }
}

// PDM-FR-RNUS, PDM-FR-UITL: a source that two diagrams hold is refused, and
// names `lead` and `trail` as the correction. A `lead` or a `trail` that leaves
// one diagram names it, and a context that still leaves two is refused again.
#[test]
fn a_diagram_that_is_not_unique_is_named_by_lead_or_trail() {
    for subject in SUBJECTS {
        let f = Fixture::with_body(subject, TWICE);
        let refusal = refused(&f, f.args(vec![change(SRC_A, SRC_A2)]));
        assert_eq!(
            refusal,
            ToolRefusal::InvalidDiagramAt { message: DIAGRAM_NOT_UNIQUE, at: 1, of: 1 },
            "{subject:?}",
        );
        assert_correctable(&refusal);
        assert!(refusal.to_string().contains("`lead` or `trail`"), "{refusal}");

        // A context that both diagrams share names neither.
        let both = with_lead(change(SRC_A, SRC_A2), "\n\n");
        assert!(matches!(
            refused(&f, f.args(vec![both])),
            ToolRefusal::InvalidDiagramAt { message: DIAGRAM_NOT_UNIQUE, .. }
        ));

        // `trail` names the first diagram, `lead` the second.
        let first = TWICE.find("```mermaid").expect("the first fence");
        let second = TWICE.rfind("```mermaid").expect("the second fence");
        let by_trail = with_trail(change(SRC_A, SRC_A2), "\n\nSecond section.");
        f.propose(vec![by_trail]).expect("recorded by trail");
        assert_eq!(f.one().hunks[0].anchor.hint_start, first, "{subject:?}");

        let g = Fixture::with_body(subject, TWICE);
        let by_lead = with_lead(change(SRC_A, SRC_A2), "Second section.\n\n");
        g.propose(vec![by_lead]).expect("recorded by lead");
        assert_eq!(g.one().hunks[0].anchor.hint_start, second, "{subject:?}");
        assert_eq!(g.body(), TWICE);
    }
}

// PDM-FR-HOQI: a new source that does not parse is refused with the parse
// error, and nothing is recorded. The turn is not ended, so the model corrects
// the source and calls again.
#[test]
fn a_source_that_does_not_parse_is_refused_with_the_parse_error() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        for (after, at) in
            [("this is not a diagram", 1), ("sequenceDiagram\n  A->>: oops ->>", 1), ("  \n", 1)]
        {
            let refusal = refused(&f, f.args(vec![change(SRC_A, after)]));
            let ToolRefusal::NotValidMermaid { error, at: place, of, update } = &refusal else {
                panic!("expected a parse refusal for {after:?}, got {refusal:?}");
            };
            assert_eq!((*place, *of, *update), (at, 1, false));
            assert!(!error.is_empty());
            assert!(error.chars().count() <= 501, "the error is cut to 500 characters");
            assert_correctable(&refusal);
            let text = refusal.to_string();
            assert!(text.starts_with("Diagram edit 1 of 1: "), "{text}");
            assert!(text.contains("not valid Mermaid"), "{text}");
            assert!(text.contains(error.as_str()), "the message carries the parse error");
        }

        // The place counts the list, and an addition is checked as a change is.
        let refusal = refused(
            &f,
            f.args(vec![change(SRC_A, SRC_A2), addition("not a diagram either", "End line.")]),
        );
        assert!(
            matches!(refusal, ToolRefusal::NotValidMermaid { at: 2, of: 2, .. }),
            "{refusal:?}"
        );

        // The correction succeeds on the next call.
        f.propose(vec![change(SRC_A, SRC_A2)]).expect("the corrected source");
    }
}

// PDM-FR-CEZB: a new source with a line that would end the fence is refused,
// for a change and for an addition.
#[test]
fn a_fence_line_in_the_new_source_is_refused() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        for source in ["flowchart TD\n  A --> B\n```\nmore", "flowchart TD\n  A --> B\n~~~"] {
            let refusal = refused(&f, f.args(vec![change(SRC_A, source)]));
            assert_eq!(
                refusal,
                ToolRefusal::InvalidDiagramAt { message: FENCE_IN_SOURCE, at: 1, of: 1 },
            );
            assert_correctable(&refusal);
        }
        let refusal = refused(&f, f.args(vec![addition("flowchart TD\n```", "End line.")]));
        assert_eq!(
            refusal,
            ToolRefusal::InvalidDiagramAt { message: FENCE_IN_SOURCE, at: 1, of: 1 },
        );
    }
}

// PDM-FR-GFXA: a change whose `after` equals its `before` is the refusal that
// nothing changes, and so is a call whose changes together alter nothing. White
// space at the end of a source does not count as a change.
#[test]
fn a_change_that_alters_nothing_is_refused() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        for after in [SRC_A, "flowchart TD\n  A --> B\n", "flowchart TD\r\n  A --> B  "] {
            let refusal = refused(&f, f.args(vec![change(SRC_A, after)]));
            assert_eq!(refusal, ToolRefusal::InvalidArguments(PROPOSAL_NO_CHANGE), "{after:?}");
            assert_correctable(&refusal);
        }
    }
}

// PDM-FR-UITL: a `lead`, a `trail`, or both over 200 characters are three
// refusals. Each names its place and the context to shorten.
#[test]
fn a_context_over_its_bound_is_refused_with_its_place() {
    let long = "x".repeat(201);
    let cases = [
        (Some(long.as_str()), None, ContextSide::Lead),
        (None, Some(long.as_str()), ContextSide::Trail),
        (Some(long.as_str()), Some(long.as_str()), ContextSide::Both),
    ];
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        for (lead, trail, side) in cases {
            let mut item = change(SRC_A, SRC_A2);
            item.lead = lead.map(str::to_string);
            item.trail = trail.map(str::to_string);
            let refusal = refused(&f, f.args(vec![item]));
            assert_eq!(
                refusal,
                ToolRefusal::InvalidDiagramAt { message: context_too_long(side), at: 1, of: 1 },
                "{subject:?}: {side:?}",
            );
            assert_correctable(&refusal);
            assert!(refusal.to_string().contains("200 characters"), "{refusal}");
        }
    }
}

// PDM-FR-UITL: two changes that cover one diagram are refused, with the place
// of the later one.
#[test]
fn two_changes_of_one_diagram_overlap() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        let refusal = refused(&f, f.args(vec![change(SRC_A, SRC_A2), removal(SRC_A)]));
        assert_eq!(
            refusal,
            ToolRefusal::InvalidDiagramAt { message: PROPOSAL_HUNKS_OVERLAP, at: 2, of: 2 },
            "{subject:?}",
        );
        assert_correctable(&refusal);
    }
}

// PDM-FR-SDJP: an empty `diagrams` list is the refusal that there are no
// changes, and a call that carries both forms is the refusal that names one
// form. Nothing is written for either.
#[test]
fn an_empty_list_and_both_forms_are_refused() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        let refusal = refused(&f, f.args(Vec::new()));
        assert_eq!(refusal, ToolRefusal::InvalidArguments(PROPOSAL_NO_HUNKS));
        assert_correctable(&refusal);

        let mut both = f.args(vec![change(SRC_A, SRC_A2)]);
        both.updates = Some(vec![entry("withdraw", Some("h1"), None, None, None)]);
        let refusal = refused(&f, both);
        assert_eq!(refusal, ToolRefusal::InvalidArguments(NOT_ONE_FORM));
        assert_correctable(&refusal);

        let refusal = refused(&f, f.updating(Vec::new()));
        assert_eq!(refusal, ToolRefusal::InvalidArguments(UPDATE_NONE));
        assert_correctable(&refusal);
    }
}

// PDM-FR-SDJP, PDM-FR-BWOH: `updates` where no proposal is open is refused, and
// the message sends the model to `diagrams`.
#[test]
fn updates_with_no_open_proposal_send_the_model_to_diagrams() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        let refusal =
            refused(&f, f.updating(vec![entry("withdraw", Some("h1"), None, None, None)]));
        assert_eq!(refusal, ToolRefusal::InvalidArguments(NO_OPEN_PROPOSAL));
        assert_correctable(&refusal);
        assert!(refusal.to_string().contains("`diagrams`"), "{refusal}");
    }
}

// PDM-FR-XRVZ: a blank rationale and a rationale with no spoken part are two
// distinct retryable refusals, and nothing is recorded for either. The text of
// the blank one is the one of the text tool of the origin.
#[test]
fn a_blank_rationale_and_a_missing_spoken_part_are_refused() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        let mut blank = f.args(vec![change(SRC_A, SRC_A2)]);
        blank.rationale = "  \n ".into();
        let blank = refused(&f, blank);
        let message = match subject {
            Subject::Draft => PROPOSAL_RATIONALE_BLANK,
            Subject::Artifact => PROMPT_PROPOSAL_RATIONALE_BLANK,
        };
        assert_eq!(blank, ToolRefusal::InvalidArguments(message));

        let mut untagged = f.args(vec![change(SRC_A, SRC_A2)]);
        untagged.rationale = "No tag here.".into();
        let untagged = refused(&f, untagged);
        assert_eq!(untagged, ToolRefusal::InvalidArguments(RATIONALE_NO_SPOKEN_PART));

        assert_correctable(&blank);
        assert_correctable(&untagged);
        assert_ne!(blank.to_string(), untagged.to_string());
    }
}

// PDM-FR-LJGN: a path that names no file the origin holds is the refusal of the
// text tool of that origin, and nothing is recorded or created.
#[test]
fn a_path_that_names_no_file_is_refused() {
    let f = Fixture::new(Subject::Draft);
    for path in ["missing.md", "../../etc/passwd", "/etc/passwd", "a/../../b"] {
        let mut args = f.args(vec![change(SRC_A, SRC_A2)]);
        args.path = path.into();
        let refusal = refused(&f, args);
        assert_eq!(refusal, ToolRefusal::ProposalPathMissing, "{path:?}");
        assert_eq!(refusal.kind(), ToolErrorKind::NotFound);
        assert!(refusal.retryable());
    }

    let f = Fixture::new(Subject::Artifact);
    for path in ["prompts/missing.md", "/etc/hosts", "../outside.md", "prompts"] {
        let mut args = f.args(vec![change(SRC_A, SRC_A2)]);
        args.path = path.into();
        let refusal = refused(&f, args);
        assert_eq!(refusal, ToolRefusal::PromptProposalPathMissing, "{path:?}");
        assert_eq!(refusal.kind(), ToolErrorKind::NotFound);
        assert!(refusal.retryable());
    }
    assert!(!f.path.join("outside.md").exists());
}

// PDM-FR-LJGN: on an artifact origin a file that is not a prompt or a project
// document is refused as the text tool refuses it.
#[test]
fn a_file_that_is_not_a_prompt_is_refused() {
    let f = Fixture::new(Subject::Artifact);
    let mut args = f.args(vec![change(SRC_A, SRC_A2)]);
    args.path = "notes/plain.md".into();
    let refusal = refused(&f, args);
    assert_eq!(refusal, ToolRefusal::NotAPromptArtifact);
    assert_correctable(&refusal);
}

// PDM-FR-TPYC: a turn whose origin is neither a draft conversation nor an
// artifact conversation is the refusal that no further call will succeed.
#[test]
fn a_conversation_that_is_not_about_a_document_refuses_for_good() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        for origin in [
            ConversationOrigin::stub_note(f.thread_id.clone(), "note-1"),
            ConversationOrigin::stub_map(f.thread_id.clone()),
        ] {
            let before = f.world();
            let refusal =
                block_on(f.tool_for(origin, agent()).call(f.args(vec![change(SRC_A, SRC_A2)])))
                    .expect_err("refused");
            assert_eq!(refusal, ToolRefusal::NotADocumentConversation);
            assert_eq!(refusal.kind(), ToolErrorKind::PermissionDenied);
            assert!(!refusal.retryable(), "no argument makes a note a document");
            assert!(refusal.to_string().contains("asking again will not succeed"));
            f.assert_untouched(&before);
        }
    }
}

// PDM-FR-AMCD: a resolved conversation is the refusal that no further call will
// succeed, and neither a proposal nor a comment is produced. The same holds for
// a join and for an update.
#[test]
fn a_resolved_conversation_refuses_for_good_and_records_nothing() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        f.resolve_conversation();
        let refusal = refused(&f, f.args(vec![change(SRC_A, SRC_A2)]));
        assert_eq!(refusal, ToolRefusal::ProposalConversationResolved, "{subject:?}");
        assert_eq!(refusal.kind(), ToolErrorKind::PermissionDenied);
        assert!(!refusal.retryable());
        assert!(refusal.to_string().contains("will not succeed"));
    }
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        f.propose(vec![change(SRC_A, SRC_A2)]).expect("a proposal stands");
        f.proposed.store(false, Ordering::SeqCst);
        f.resolve_conversation();
        let join = refused(&f, f.args(vec![addition(SRC_B, "End line.")]));
        assert_eq!(join, ToolRefusal::ProposalConversationResolved, "{subject:?}");
        let id = f.one().hunks[0].id.clone();
        let update = refused(&f, f.updating(vec![entry("withdraw", Some(&id), None, None, None)]));
        assert_eq!(update, ToolRefusal::ProposalConversationResolved, "{subject:?}");
    }
}

// PDM-FR-OHSB: with no project open the call gives the shared refusal.
#[test]
fn with_no_project_open_it_gives_the_shared_refusal() {
    let closed = closed_project();
    // A real directory, because building a `RootFs` over one that does not
    // exist panics, and what is under test is the refusal.
    let elsewhere = TempDir::new().expect("tempdir");
    for origin in [
        ConversationOrigin::stub_draft("t", "d", false),
        ConversationOrigin::stub_artifact("t", PROMPT, false),
    ] {
        let tool = ProposeDiagramChangesTool::new(
            closed.handle().clone(),
            crate::agent_conversations::OwnedRoots {
                worktree: crate::fs::RootFs::for_root(elsewhere.path()),
                store: crate::fs::RootFs::for_root(elsewhere.path()),
            },
            origin,
            agent(),
            Arc::new(AtomicBool::new(false)),
        );
        let args = ProposeDiagramChangesArgs {
            path: DRAFT_FILE.into(),
            rationale: RATIONALE.into(),
            updates: None,
            diagrams: vec![change(SRC_A, SRC_A2)],
        };
        let refusal = block_on(tool.call(args)).expect_err("refused");
        assert_eq!(refusal, ToolRefusal::NoProjectOpen);
        assert_eq!(refusal.kind(), ToolErrorKind::NotFound);
        assert!(!refusal.retryable());
        assert_eq!(refusal.to_string(), crate::tools::NO_PROJECT_OPEN);
    }
}

// PDM-FR-JLYU: a refusal does not end the turn. A call that recorded nothing
// leaves the flag clear, so the loop runs on and the model corrects the call.
#[test]
fn a_refusal_leaves_the_turn_running() {
    for subject in SUBJECTS {
        let f = Fixture::new(subject);
        f.propose(vec![change(SRC_A, "not a diagram")]).expect_err("refused");
        assert!(!f.proposed.load(Ordering::SeqCst));
        f.propose(vec![change(SRC_A, SRC_A2)]).expect("corrected");
        assert!(f.proposed.load(Ordering::SeqCst));
    }
}
