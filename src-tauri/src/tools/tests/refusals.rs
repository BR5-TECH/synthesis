//! Refusal text, retryability, the empty answer, logging, and concurrent callers.

use super::*;

// ---------------------------------------------------------------------------
// TLC-FR-09 — refusal text (TLC-FR-09)
// ---------------------------------------------------------------------------

#[test]
fn every_refusal_message_is_plain_and_carries_no_implementation_detail() {
    // Built through a `match` over the enum so a variant added tomorrow fails
    // to compile here rather than going silently unchecked.
    let refusals: Vec<ToolRefusal> = [
        ToolRefusal::NoProjectOpen,
        ToolRefusal::InvalidArguments(skill_search::EMPTY_QUERY),
        ToolRefusal::InvalidArguments(skill_load::BLANK_NAME),
        ToolRefusal::SkillNotFound,
        ToolRefusal::SkillUnreadable,
        ToolRefusal::SkillNameNotUnique,
        ToolRefusal::SkillAmbiguous(vec![
            crate::skills::Ecosystem::Claude,
            crate::skills::Ecosystem::Codex,
        ]),
        ToolRefusal::InvalidArguments(crate::tools::EMPTY_SPEC_QUERY),
        ToolRefusal::InvalidArguments(crate::tools::PATH_BLANK),
        ToolRefusal::PathOutsideProject,
        ToolRefusal::PathThroughLink,
        ToolRefusal::FileNotFound,
        ToolRefusal::PathIsFolder,
        ToolRefusal::FileNotText,
        ToolRefusal::ConversationLocked,
        ToolRefusal::QuestionNotPosted,
        ToolRefusal::InvalidArguments(crate::tools::PROPOSAL_CONTENT_BLANK),
        ToolRefusal::InvalidArguments(crate::tools::PROPOSAL_RATIONALE_BLANK),
        ToolRefusal::InvalidArguments(crate::tools::PROPOSAL_NO_CHANGE),
        ToolRefusal::NotADraftConversation,
        ToolRefusal::ProposalPathMissing,
        ToolRefusal::ProposalPending,
        ToolRefusal::ProposalConversationLocked,
        ToolRefusal::ProposalNotRecorded,
        ToolRefusal::InvalidArguments(crate::tools::EMPTY_DRAFT_QUERY),
        // NST-FR-14: this tool's own empty-query refusal joins the group and is
        // held to exactly the same rules. Listed by name because the `match`
        // below forces exhaustiveness over *variants* alone — a new
        // `InvalidArguments` constant compiles without it and would go
        // unchecked.
        ToolRefusal::InvalidArguments(crate::tools::EMPTY_NOTE_QUERY),
        ToolRefusal::InvalidArguments(crate::tools::DRAFT_ID_BLANK),
        ToolRefusal::DraftNotFound,
        ToolRefusal::DraftInconsistent,
        ToolRefusal::DraftPromptUnreadable,
        ToolRefusal::GraduationPathOutside,
        ToolRefusal::GraduationFileNotFound,
        ToolRefusal::GraduationWorkingCopyUnavailable,
        ToolRefusal::NotEscalatableNow,
        ToolRefusal::AlreadyEscalated,
        // ESU-FR-10: the same rules, on a refusal that names which question of
        // the set it is about.
        ToolRefusal::InvalidQuestion(crate::tools::QUESTION_BLANK, 2),
        // PPC-FR-01: this tool's own refusals join the group and are held to
        // exactly the same rules.
        ToolRefusal::NotAnArtifactConversation,
        ToolRefusal::PromptProposalPathMissing,
        ToolRefusal::NotAPromptArtifact,
        ToolRefusal::PromptProposalPending,
        ToolRefusal::InvalidArguments(crate::tools::PROMPT_PROPOSAL_RATIONALE_BLANK),
        // SDT-FR-GGYO, GDT-FR-IPJG, GDT-FR-MVHY, GDT-FR-FZHF, GDT-FR-IFCA,
        // GDT-FR-HREW: the two document tools' refusals join the group and are
        // held to exactly the same rules.
        ToolRefusal::InvalidArguments(crate::tools::EMPTY_DOCUMENT_QUERY),
        ToolRefusal::InvalidArguments(crate::tools::DOCUMENT_ID_BLANK),
        ToolRefusal::InvalidArguments(crate::tools::DOCUMENT_BOTH_RANGE_FORMS),
        ToolRefusal::DocumentNotFound,
        ToolRefusal::DocumentUnavailable,
        ToolRefusal::DocumentNoText,
        // ADQ-FR-KZNV, AUC-FR-QSVN: this tool's own refusals join the group and
        // are held to exactly the same rules.
        ToolRefusal::QuestionSetPendingForComment,
        ToolRefusal::QuestionSetAlreadyPending,
        ToolRefusal::QuestionSetConversationLocked,
        ToolRefusal::QuestionSetNotRecorded,
        ToolRefusal::InvalidArguments(crate::tools::WRONG_QUESTION_SET_COUNT),
        ToolRefusal::InvalidQuestion(crate::tools::SET_QUESTION_BLANK, 2),
        ToolRefusal::InvalidQuestion(crate::tools::WRONG_OPTION_COUNT, 1),
        ToolRefusal::InvalidQuestion(crate::tools::SET_OPTION_BLANK, 3),
        ToolRefusal::InvalidQuestion(crate::tools::SET_QUESTION_TOO_LONG, 1),
        ToolRefusal::InvalidQuestion(crate::tools::SET_OPTION_TOO_LONG, 1),
        // PDC-FR-TZKQ: the same rules hold of a refusal that names which of
        // several changes it is about — the two numbers are counts of what the
        // model sent, and the sentence around them is this application's.
        ToolRefusal::InvalidArgumentsAt {
            message: crate::tools::propose_draft_changes::PROPOSAL_TEXT_NOT_FOUND,
            at: 3,
            of: 8,
        },
    ]
    .into_iter()
    .inspect(|refusal| match refusal {
        ToolRefusal::NoProjectOpen
        | ToolRefusal::InvalidArguments(_)
        | ToolRefusal::InvalidArgumentsAt { .. }
        | ToolRefusal::SkillNotFound
        | ToolRefusal::SkillUnreadable
        | ToolRefusal::SkillNameNotUnique
        | ToolRefusal::SkillAmbiguous(_)
        | ToolRefusal::PathOutsideProject
        | ToolRefusal::PathThroughLink
        | ToolRefusal::FileNotFound
        | ToolRefusal::PathIsFolder
        | ToolRefusal::FileNotText
        | ToolRefusal::ConversationLocked
        | ToolRefusal::QuestionNotPosted
        | ToolRefusal::NotADraftConversation
        | ToolRefusal::ProposalPathMissing
        | ToolRefusal::ProposalPending
        | ToolRefusal::ProposalConversationLocked
        | ToolRefusal::ProposalNotRecorded
        | ToolRefusal::DraftNotFound
        | ToolRefusal::DraftInconsistent
        | ToolRefusal::DraftPromptUnreadable
        | ToolRefusal::GraduationPathOutside
        | ToolRefusal::GraduationFileNotFound
        | ToolRefusal::GraduationWorkingCopyUnavailable
        | ToolRefusal::NotEscalatableNow
        | ToolRefusal::AlreadyEscalated
        | ToolRefusal::NotAnArtifactConversation
        | ToolRefusal::PromptProposalPathMissing
        | ToolRefusal::NotAPromptArtifact
        | ToolRefusal::InvalidQuestion(..)
        | ToolRefusal::QuestionSetPendingForComment
        | ToolRefusal::QuestionSetAlreadyPending
        | ToolRefusal::QuestionSetConversationLocked
        | ToolRefusal::QuestionSetNotRecorded
        | ToolRefusal::DocumentNotFound
        | ToolRefusal::DocumentUnavailable
        | ToolRefusal::DocumentNoText
        | ToolRefusal::PromptProposalPending => {}
    })
    .collect();

    for refusal in &refusals {
        let message = refusal.to_string();
        assert!(
            message.ends_with('.'),
            "a refusal is plain sentences: {message:?} (TLC-FR-09)",
        );
        for forbidden in [
            "ToolRefusal",
            "Bm25Indexer",
            "rig::",
            "crate::",
            "panicked",
            "stack backtrace",
            "src-tauri",
            "Err(",
        ] {
            assert!(
                !message.contains(forbidden),
                "a refusal must not carry {forbidden:?}: {message:?} (TLC-FR-09)",
            );
        }
        // No *absolute* filesystem path — which is what TLC-FR-09 forbids, and
        // is the part that leaks where this machine keeps things. A relative
        // example like 'src/main.ts' is the opposite: it shows a model the shape
        // of a valid argument, which TLC-FR-06 asks a parameter to do.
        for token in message.split_whitespace() {
            let bare = token.trim_matches(|c: char| !c.is_ascii_alphanumeric() && c != '/' && c != '\\');
            assert!(
                !bare.starts_with('/') && !bare.contains(":\\"),
                "a refusal carries no absolute filesystem path: {message:?} (TLC-FR-09)",
            );
        }
    }
}

#[test]
fn every_declared_failure_returns_rather_than_panicking() {
    // TLC-FR-09: every failure is an `Err`, never a panic.
    let closed = closed_project();
    let closed_handle = closed.handle().clone();
    let open = demo_project();

    // The new tools' own failures, driven against a live project so each takes
    // its real path rather than short-circuiting on the closed-project check.
    let read_root = open.app.state::<Bm25Indexer>().root().expect("mounted");
    let session = agent_session(&open.app, &read_root, "panics");
    let reader = file_read::FileReadTool::new(open.handle(), session);
    let mut extra: Vec<ToolRefusal> = Vec::new();
    for args in [
        ("", None, None),
        ("   ", None, None),
        ("nope/missing.md", None, None),
        (".claude", None, None),
        ("../outside.md", None, None),
        ("nope/missing.md", Some(-9), Some(-9)),
    ] {
        extra.push(
            block_on(reader.call(file_read::ReadFileArgs {
                path: args.0.to_string(),
                offset: args.1,
                limit: args.2,
            }))
            .unwrap_err(),
        );
    }
    extra.push(
        block_on(SpecSearchTool::new(open.handle()).call(
            spec_search::SpecificationSearchArgs {
                query: "  ".to_string(),
                limit: None,
            },
        ))
        .unwrap_err(),
    );
    extra.push(
        block_on(SpecSearchTool::new(closed_handle.clone()).call(
            spec_search::SpecificationSearchArgs {
                query: "anything".to_string(),
                limit: Some(1000),
            },
        ))
        .unwrap_err(),
    );
    for refusal in &extra {
        assert!(
            !refusal.to_string().is_empty(),
            "every refusal reaches the model as a sentence (TLC-FR-09)",
        );
    }

    let outcomes: Vec<ToolRefusal> = vec![
        block_on(
            skill_search::SkillSearchTool::new(closed_handle.clone()).call(
                skill_search::SkillSearchArgs {
                    query: "anything".to_string(),
                    limit: None,
                },
            ),
        )
        .unwrap_err(),
        block_on(skill_search::SkillSearchTool::new(open.handle()).call(
            skill_search::SkillSearchArgs {
                query: "   ".to_string(),
                limit: None,
            },
        ))
        .unwrap_err(),
        block_on(
            skill_list::SkillListTool::new(closed_handle.clone())
                .call(skill_list::SkillListArgs::default()),
        )
        .unwrap_err(),
        block_on(
            skill_load::SkillLoadTool::new(closed_handle).call(skill_load::LoadSkillArgs {
                name: "analyst".to_string(),
                ecosystem: None,
            }),
        )
        .unwrap_err(),
        block_on(
            skill_load::SkillLoadTool::new(open.handle()).call(skill_load::LoadSkillArgs {
                name: "  ".to_string(),
                ecosystem: None,
            }),
        )
        .unwrap_err(),
        block_on(
            skill_load::SkillLoadTool::new(open.handle()).call(skill_load::LoadSkillArgs {
                name: "no such skill".to_string(),
                ecosystem: None,
            }),
        )
        .unwrap_err(),
    ];
    // Each `unwrap_err` above is the assertion; what is worth stating here is
    // that these are the refusals the group declares and nothing else.
    for outcome in &outcomes {
        assert!(matches!(
            outcome,
            ToolRefusal::NoProjectOpen
                | ToolRefusal::InvalidArguments(_)
                | ToolRefusal::SkillNotFound
                | ToolRefusal::SkillUnreadable
                | ToolRefusal::SkillNameNotUnique
                | ToolRefusal::SkillAmbiguous(_)
        ));
    }
}

// ---------------------------------------------------------------------------
// TLC-FR-11, TLC-FR-13 / TLC-FR-11 — the shared refusal and retryability
// ---------------------------------------------------------------------------

#[test]
fn no_open_project_reads_identically_from_every_tool() {
    // TLC-FR-11, TLC-FR-13.
    let app = closed_project();
    let handle = app.handle().clone();
    let handle2 = handle.clone();

    let search = block_on(skill_search::SkillSearchTool::new(handle.clone()).call(
        skill_search::SkillSearchArgs {
            query: "anything".to_string(),
            limit: None,
        },
    ))
    .unwrap_err();
    let list = block_on(
        skill_list::SkillListTool::new(handle.clone()).call(skill_list::SkillListArgs::default()),
    )
    .unwrap_err();
    // LSK-FR-16: the no-project refusal rather than the unknown-skill one — a
    // model told "no skill by that name" would conclude the project lacks a
    // skill it has.
    let load = block_on(
        skill_load::SkillLoadTool::new(handle.clone()).call(skill_load::LoadSkillArgs {
            name: "analyst".to_string(),
            ecosystem: None,
        }),
    )
    .unwrap_err();
    let specs = block_on(SpecSearchTool::new(handle.clone()).call(
        spec_search::SpecificationSearchArgs {
            query: "anything".to_string(),
            limit: None,
        },
    ))
    .unwrap_err();
    // RFT-FR-17: no root to resolve a path against, so the shared refusal
    // rather than the not-found one a model would read as "the file is gone".
    let read = block_on(
        file_read::FileReadTool::new(handle, "no-session").call(file_read::ReadFileArgs {
            path: "src/a.ts".to_string(),
            offset: None,
            limit: None,
        }),
    )
    .unwrap_err();

    // AUC-FR-13: the root this tool holds was bound when its turn began, and a
    // turn outlives the project closing under it — so it gates per call like
    // every other tool here rather than trusting what it was constructed with.
    let ask = block_on(
        ask_user_comment::AskUserCommentTool::new(
            handle2,
            crate::agent_conversations::OwnedRoots { worktree: crate::fs::RootFs::for_root(&std::env::temp_dir()), store: crate::fs::RootFs::for_root(&std::env::temp_dir()) },
            crate::agent_conversations::ConversationOrigin::stub_artifact("no-project", "a.md", true),
            crate::comments::Participant::Agent {
                agent_id: "no-project".into(),
                handle: "no-project".into(),
                model: None,
                title: None,
            },
            std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        )
        .call(ask_user_comment::AskUserCommentArgs {
            question: "which way?".to_string(),
            options: None,
        }),
    )
    .unwrap_err();

    assert_eq!(search, ToolRefusal::NoProjectOpen);
    assert_eq!(ask, ToolRefusal::NoProjectOpen);
    assert_eq!(ask.to_string(), NO_PROJECT_OPEN);
    assert_eq!(list, ToolRefusal::NoProjectOpen);
    assert_eq!(load, ToolRefusal::NoProjectOpen);
    assert_eq!(specs, ToolRefusal::NoProjectOpen);
    assert_eq!(read, ToolRefusal::NoProjectOpen);
    assert_eq!(
        search.to_string(),
        list.to_string(),
        "TLC-FR-13: the message is identical across every tool that produces it",
    );
    assert_eq!(load.to_string(), list.to_string());
    assert_eq!(search.to_string(), NO_PROJECT_OPEN);

    for refusal in [&search, &list, &load, &ask] {
        let error = refusal.to_execution_error();
        assert_eq!(error.kind(), ToolErrorKind::NotFound, "TLC-FR-10");
        assert_eq!(error.retryable(), Some(false), "TLC-FR-11");
        assert_eq!(error.message(), NO_PROJECT_OPEN);
    }
}

#[test]
fn a_correctable_argument_is_retryable_and_a_missing_project_is_not() {
    // TLC-FR-11. Set explicitly rather than inherited: `ToolErrorKind`'s own
    // default calls `InvalidArgs` unretryable, which is the opposite of what
    // this group means by it.
    let correctable = ToolRefusal::InvalidArguments(skill_search::EMPTY_QUERY).to_execution_error();
    assert_eq!(correctable.kind(), ToolErrorKind::InvalidArgs);
    assert_eq!(correctable.retryable(), Some(true));

    let permanent = ToolRefusal::NoProjectOpen.to_execution_error();
    assert_eq!(permanent.kind(), ToolErrorKind::NotFound);
    assert_eq!(permanent.retryable(), Some(false));
    assert!(
        permanent.message().contains("do not retry"),
        "TLC-FR-11: the message says so as well as the flag",
    );

    // TLC-FR-11: the flag is independent of the kind. Both of these are
    // `NotFound`, and they differ because one names material the model chose
    // and the other names a state no argument reaches.
    let chosen = ToolRefusal::SkillNotFound.to_execution_error();
    assert_eq!(chosen.kind(), ToolErrorKind::NotFound);
    assert_eq!(
        chosen.retryable(),
        Some(true),
        "TLC-FR-11: a different name reaches a skill this project does have",
    );
    assert_eq!(
        ToolRefusal::SkillUnreadable.to_execution_error().retryable(),
        Some(true),
    );
}

// ---------------------------------------------------------------------------
// TLC-FR-12 — an empty answer is a success (TLC-FR-12)
// ---------------------------------------------------------------------------

#[test]
fn nothing_matched_and_nothing_present_are_successes_not_refusals() {
    let fixture = demo_project();
    let handle = fixture.handle();

    let search = block_on(skill_search::SkillSearchTool::new(handle).call(
        skill_search::SkillSearchArgs {
            query: "zzzzz nonexistent terminology".to_string(),
            limit: None,
        },
    ))
    .expect("nothing matched is an answer, not a failure (TLC-FR-12)");
    assert!(search.skills.is_empty());

    let empty = empty_project();
    let list = block_on(
        skill_list::SkillListTool::new(empty.handle()).call(skill_list::SkillListArgs::default()),
    )
    .expect("a project with no skills is a fact, not a failure (TLC-FR-12)");
    assert!(list.skills.is_empty());
}

// ---------------------------------------------------------------------------
// TLC-FR-14 — logging (TLC-FR-14)
// ---------------------------------------------------------------------------

static TLC_BUFFER: LogBuffer = LogBuffer::new();

fn records(buffer: &LogBuffer, domain: Domain) -> Vec<crate::logging::LogRecord> {
    buffer
        .query(
            &LogFilter {
                min_level: LogLevel::Debug,
                domains: vec![domain],
                ..LogFilter::default()
            },
            None,
            1000,
        )
        .unwrap()
        .records
}

#[test]
fn a_call_reports_under_both_domains_and_leaks_nothing() {
    let fixture = demo_project();
    let handle = fixture.handle();
    TLC_BUFFER.clear();

    // One success and one refusal, from the same tool.
    let query = "review a specification for contradictions";
    let ok = block_on(
        skill_search::SkillSearchTool::with_buffer(handle.clone(), &TLC_BUFFER).call(
            skill_search::SkillSearchArgs {
                query: query.to_string(),
                limit: None,
            },
        ),
    )
    .unwrap();
    assert!(!ok.skills.is_empty(), "the fixture matches this query");

    block_on(
        skill_search::SkillSearchTool::with_buffer(handle, &TLC_BUFFER).call(
            skill_search::SkillSearchArgs {
                query: String::new(),
                limit: None,
            },
        ),
    )
    .unwrap_err();

    let ai = records(&TLC_BUFFER, Domain::Ai);
    let backend = records(&TLC_BUFFER, Domain::Backend);
    assert_eq!(
        ai.len(),
        2,
        "one INFO for the call and one WARN for the refusal (TLC-FR-14)",
    );
    assert_eq!(ai, backend, "TLC-FR-14: both domains return the same records");

    assert_eq!(ai[0].level, LogLevel::Info);
    assert_eq!(ai[0].message, "tool call completed");
    assert_eq!(ai[0].fields.get("tool").unwrap(), "search_skills");
    assert_eq!(ai[1].level, LogLevel::Warn);
    assert_eq!(ai[1].message, "tool call refused");
    assert_eq!(ai[1].fields.get("reason").unwrap(), "invalid_arguments");

    // TLC-FR-14's exception is an argument a tool's own spec names, and this
    // tool's spec names none — so the refusal record is the convention's three
    // fields and the success record carries no argument either. Pinning the key
    // sets is what keeps a change to the shared helpers, made for the one tool
    // that does name an argument, from widening what the other four record.
    assert_eq!(sorted_keys(&ai[1]), vec!["reason", "retryable", "tool"]);
    assert_eq!(sorted_keys(&ai[0]), vec!["matches", "tool"]);
    assert!(!ai[0].fields.contains_key("query"));
    assert!(!ai[1].fields.contains_key("query"));

    // Nothing the model sent, nothing the tool returned, nothing from a file.
    let serialised = serde_json::to_string(&ai).unwrap();
    for leaked in [
        query,
        "analyst",
        "Review a specification for internal contradictions",
        ".claude/skills",
        "sarcophagus",
    ] {
        assert!(
            !serialised.contains(leaked),
            "a record must not carry {leaked:?} (TLC-FR-14)",
        );
    }
}

static TLC_EXTRA_BUFFER: LogBuffer = LogBuffer::new();

#[test]
fn a_tools_own_fields_fill_in_around_the_conventions_and_never_displace_them() {
    let app = closed_project();
    let handle = app.handle().clone();
    TLC_EXTRA_BUFFER.clear();

    // A caller naming all three reserved fields, which is the case the helper's
    // shape exists to make harmless: what the convention says a record carries
    // is not a tool's to overwrite (TLC-FR-14).
    crate::tools::log_tool_refusal_with(
        &handle,
        &TLC_EXTRA_BUFFER,
        "read_file",
        &ToolRefusal::FileNotFound,
        crate::log_fields! {
            "tool" => "impostor",
            "reason" => "hijacked",
            "retryable" => false,
            "path" => "src/a.ts",
        },
    );
    // And the plain helper, which must still be the three fields alone.
    crate::tools::log_tool_refusal(
        &handle,
        &TLC_EXTRA_BUFFER,
        "list_skills",
        &ToolRefusal::NoProjectOpen,
    );

    let ai = records(&TLC_EXTRA_BUFFER, Domain::Ai);
    assert_eq!(ai.len(), 2);
    assert_eq!(ai, records(&TLC_EXTRA_BUFFER, Domain::Backend));

    assert_eq!(sorted_keys(&ai[0]), vec!["path", "reason", "retryable", "tool"]);
    assert_eq!(ai[0].fields["tool"], serde_json::json!("read_file"));
    assert_eq!(ai[0].fields["reason"], serde_json::json!("file_not_found"));
    assert_eq!(ai[0].fields["retryable"], serde_json::json!(true));
    assert_eq!(
        ai[0].fields["path"],
        serde_json::json!("src/a.ts"),
        "a field the convention does not claim is carried through",
    );

    assert_eq!(ai[1].message, "tool call refused");
    assert_eq!(
        sorted_keys(&ai[1]),
        vec!["reason", "retryable", "tool"],
        "delegating changed nothing about the default record",
    );
}

static TLC_SUCCESS_BUFFER: LogBuffer = LogBuffer::new();

#[test]
fn a_tools_own_fields_cannot_displace_the_tool_name_on_a_success() {
    let app = closed_project();
    let handle = app.handle().clone();
    TLC_SUCCESS_BUFFER.clear();

    // The success helper's counterpart of the case above. It matters because a
    // tool naming an argument of its own (SPS-FR-19, RFT-FR-20) builds its
    // fields and hands them over as one map, so `tool` must be the convention's
    // whatever the map arrived carrying (TLC-FR-14).
    crate::tools::log_tool_success(
        &handle,
        &TLC_SUCCESS_BUFFER,
        "search_specifications",
        crate::log_fields! {
            "tool" => "impostor",
            "matches" => 3,
            "query" => "teardown",
        },
    );

    let ai = records(&TLC_SUCCESS_BUFFER, Domain::Ai);
    assert_eq!(ai.len(), 1);
    assert_eq!(ai, records(&TLC_SUCCESS_BUFFER, Domain::Backend));
    assert_eq!(ai[0].message, "tool call completed");
    assert_eq!(
        ai[0].fields["tool"],
        serde_json::json!("search_specifications"),
        "the caller's own `tool` did not survive (TLC-FR-14)",
    );
    assert_eq!(sorted_keys(&ai[0]), vec!["matches", "query", "tool"]);
}

// ---------------------------------------------------------------------------
// TLC-FR-15 — one instance serves concurrent callers (TLC-FR-15)
// ---------------------------------------------------------------------------

#[test]
fn one_instance_serves_two_callers_without_either_observing_the_other() {
    let fixture = demo_project();
    let tool = std::sync::Arc::new(skill_search::SkillSearchTool::new(fixture.handle()));

    let a = std::sync::Arc::clone(&tool);
    let b = std::sync::Arc::clone(&tool);
    let one = std::thread::spawn(move || {
        block_on(a.call(skill_search::SkillSearchArgs {
            query: "review a specification".to_string(),
            limit: None,
        }))
        .unwrap()
    });
    let two = std::thread::spawn(move || {
        block_on(b.call(skill_search::SkillSearchArgs {
            query: "deploy a container image to production".to_string(),
            limit: None,
        }))
        .unwrap()
    });

    let one = one.join().unwrap();
    let two = two.join().unwrap();
    assert_eq!(one.skills.first().map(|s| s.name.as_str()), Some("analyst"));
    assert_eq!(two.skills.first().map(|s| s.name.as_str()), Some("deployer"));
}

