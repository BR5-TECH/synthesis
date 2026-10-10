//! Reachability, names, descriptions, definitions, the schema, arguments, and output shape.

use super::*;

// ---------------------------------------------------------------------------
// TLC-FR-01 — nothing here is reachable from the frontend (TLC-FR-01)
// ---------------------------------------------------------------------------

#[test]
fn no_tool_is_reachable_from_the_frontend() {
    // The claim rests on this group's ABSENCE from the handler: every other
    // registration test checks the forward direction, so adding
    // `#[tauri::command]` to a tool tomorrow would turn nothing else red.
    const LIB: &str = include_str!("../../lib.rs");
    let handler = LIB
        .split_once("generate_handler![")
        .expect("generate_handler! invocation")
        .1
        .split_once("])")
        .expect("end of generate_handler!")
        .0;
    // `generate_handler!` lists function names, not module paths, so scanning
    // for "tools" would miss a registered `search_skills` entirely — and would
    // fail on an unrelated future command like `open_devtools`. The tool names
    // are what a registration would actually add.
    //
    // Compared against each entry's own function name rather than against the
    // whole text: a tool's name is routinely a prefix of an unrelated command's
    // — `read_draft` is a prefix of `read_draft_image`, which
    // `DAS-draft-assets.md` registers deliberately (DAS-FR-08) — and a
    // substring scan would read that as this group having been registered.
    let registered: Vec<&str> = handler
        .lines()
        .map(|line| line.trim().trim_end_matches(',').trim())
        .filter(|line| !line.is_empty() && !line.starts_with("//"))
        .map(|entry| entry.rsplit("::").next().unwrap_or(entry))
        .collect();
    for tool in [
        skill_search::NAME,
        skill_list::NAME,
        skill_load::NAME,
        spec_search::NAME,
        file_read::NAME,
        ask_user_comment::NAME,
        propose_draft_changes::NAME,
        draft_read::NAME,
        note_search::NAME,
        read_graduation_file::NAME,
        escalate_to_user::NAME,
    ] {
        assert!(
            !registered.contains(&tool),
            "no operation of the tools group may be a registered command, found {tool:?} \
             in the handler (TLC-FR-01)",
        );
    }

    // `search_drafts` is the one name this scan cannot read as an absence:
    // `crate::drafts` registers a Tauri command of exactly that spelling — the
    // Drafts panel's substring filter (DRS-FR-17) — and the tool of the same
    // name is a different operation on a different surface, which DST-FR-01
    // sanctions. So the assertion is that the handler entry belongs to
    // `crate::drafts` and that nothing under `tools::` is registered, rather
    // than that the string is absent.
    assert!(
        handler.contains("drafts::search_drafts"),
        "the handler's `search_drafts` is the Drafts panel's own command (DRS-FR-17)",
    );
    assert!(
        !handler.contains("tools::"),
        "no path under the tools group may be registered as a command (TLC-FR-01)",
    );

    // Comments are stripped first: these modules document the attribute they
    // must not carry, and a naive scan would match that prose and fail for the
    // opposite of the reason this test exists.
    for (name, source) in GROUP_SOURCES {
        let code: String = source
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            !code.contains("#[tauri::command]"),
            "{name} registers no Tauri command (TLC-FR-01)",
        );
        assert!(
            !code.contains(".emit("),
            "{name} emits no Tauri event of its own (TLC-FR-01)",
        );
    }
}

// ---------------------------------------------------------------------------
// TLC-FR-02 — names (TLC-FR-02)
// ---------------------------------------------------------------------------

#[test]
fn every_tool_name_is_snake_case_unique_and_matches_its_definition() {
    let names: Vec<String> = definitions().into_iter().map(|d| d.name.to_string()).collect();

    // The `NAME` constant and what a provider receives are the same string.
    assert_eq!(
        names,
        vec![
            <skill_search::SkillSearchTool<tauri::test::MockRuntime> as PortableTool>::NAME
                .to_string(),
            <skill_list::SkillListTool<tauri::test::MockRuntime> as PortableTool>::NAME.to_string(),
            <skill_load::SkillLoadTool<tauri::test::MockRuntime> as PortableTool>::NAME.to_string(),
            <spec_search::SpecSearchTool<tauri::test::MockRuntime> as PortableTool>::NAME
                .to_string(),
            <file_read::FileReadTool<tauri::test::MockRuntime> as PortableTool>::NAME.to_string(),
            <draft_search::DraftSearchTool<tauri::test::MockRuntime> as PortableTool>::NAME
                .to_string(),
            <draft_read::DraftReadTool<tauri::test::MockRuntime> as PortableTool>::NAME.to_string(),
            <note_search::NoteSearchTool<tauri::test::MockRuntime> as PortableTool>::NAME
                .to_string(),
            <document_search::SearchDocumentsTool<tauri::test::MockRuntime> as PortableTool>::NAME
                .to_string(),
            <document_get::GetDocumentTool<tauri::test::MockRuntime> as PortableTool>::NAME
                .to_string(),
            <read_graduation_file::ReadGraduationFileTool<tauri::test::MockRuntime> as PortableTool>::NAME
                .to_string(),
            <ask_user_comment::AskUserCommentTool<tauri::test::MockRuntime> as PortableTool>::NAME
                .to_string(),
            <propose_draft_changes::ProposeDraftChangesTool<tauri::test::MockRuntime> as PortableTool>::NAME
                .to_string(),
                    <ask_discussion_questions::AskDiscussionQuestionsTool<tauri::test::MockRuntime> as PortableTool>::NAME
                .to_string(),
],
        "TLC-FR-02: NAME and ToolDefinition.name are identical",
    );

    for name in &names {
        assert!(
            !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit()),
            "TLC-FR-02: {name} must be lowercase snake_case",
        );
    }

    let unique: std::collections::BTreeSet<&String> = names.iter().collect();
    assert_eq!(
        unique.len(),
        names.len(),
        "TLC-FR-02: no two tools may share a name",
    );
}

// ---------------------------------------------------------------------------
// TLC-FR-03 — the description is written for a model (TLC-FR-03, TLC-FR-04)
// ---------------------------------------------------------------------------

#[test]
fn every_description_addresses_the_model_and_names_no_implementation() {
    for definition in definitions() {
        let text = &definition.description;
        let name = &definition.name;

        // What it does, when to use it, what it returns, what it does not.
        // The wording is each tool's own — what is asserted is that the four
        // things are *stated*, not that they are stated in one phrasing, which
        // would make a clearer sentence a test failure.
        assert!(
            text.contains("Use this") || text.contains("Use it") || text.contains("Reach for it"),
            "{name}: the description says when to reach for the tool (TLC-FR-03)",
        );
        // The phrase is looked up per tool for the reason `disclaims` below is:
        // a shared substring test would let a tool inherit a loophole rather
        // than state its own result. A bare `contains("returns")` in particular
        // is satisfied by a purely negative sentence — `read_draft`'s own
        // description carries "never returns an accepted-history snapshot" —
        // so what is pinned here is the clause that says what *does* come back.
        let states_result = match name.as_str() {
            n if n == draft_search::NAME => "Results are best first and include",
            // NST-FR-02: this one's answer to "what do I get back" says the
            // whole note comes with it, which is the property that separates it
            // from every other search here — there is no second call to make.
            n if n == note_search::NAME || n == document_search::NAME => {
                "Returns the best matches first"
            }
            // GDT-FR-GNCR: the answer to "what do I get back" is the text of the
            // document, whole unless a range was asked for.
            n if n == document_get::NAME => "Returns the whole text by default",
            n if n == draft_read::NAME => "It returns the draft's stable metadata",
            n if n == skill_list::NAME => "Each entry gives",
            // AUC's and PDC's answer to "what do I get back" is that you get
            // nothing back and why, which is the more useful thing for a model
            // to read about a tool that ends its turn.
            // ADQ's answer is the same as these two: the turn ends, so what a
            // model needs to read is that nothing comes back and why.
            n if n == ask_user_comment::NAME
                || n == propose_draft_changes::NAME
                || n == ask_discussion_questions::NAME =>
            {
                "brings you no answer back"
            }
            n if n == skill_search::NAME
                || n == skill_load::NAME
                || n == spec_search::NAME
                || n == file_read::NAME
                || n == read_graduation_file::NAME =>
            {
                "Returns"
            }
            // ESU's answer to "what do I get back" is the same shape as AUC's
            // and PDC's — nothing, and what happens instead — phrased for a run
            // that stops rather than a turn that ends.
            n if n == escalate_to_user::NAME => "The run pauses where it stands",
            other => panic!("no result phrase declared for {other} (TLC-FR-03)"),
        };
        assert!(
            text.contains(states_result),
            "{name}: the description says what the tool returns (TLC-FR-03)",
        );
        // The boundary each tool draws is its own, so the phrase is looked up
        // per tool rather than shared: asserting one sentence across all three
        // would only prove they were copied from each other.
        let disclaims = match name.as_str() {
            n if n == skill_search::NAME || n == skill_list::NAME => {
                "read that file to actually use the skill"
            }
            n if n == skill_load::NAME => "it does not carry out what the skill says",
            n if n == spec_search::NAME => "nothing that is found is changed",
            // NST-FR-19: the boundary this one draws is that finding a note
            // does not act on it — a note found here is not read out of, not
            // resolved, and not marked in any way.
            n if n == note_search::NAME => {
                "nothing that is found is read out of, changed, resolved, or deleted"
            }
            n if n == file_read::NAME => "it does not search for a file",
            // SDT-FR-JMSA: finding a document changes nothing about it.
            n if n == document_search::NAME => "Nothing that is found is changed",
            // GDT-FR-GNCR: the boundary is which files it reaches — only a
            // selected document, never a path a model composed.
            n if n == document_get::NAME => {
                "cannot read any file that is not a selected document"
            }
            n if n == ask_user_comment::NAME => "it changes nothing in the project",
            // PDC-FR-04: the boundary this one has to draw is what `content`
            // must be, because that is the one mistake it cannot detect.
            n if n == propose_draft_changes::NAME => {
                "the prompt is not rewritten unless the author accepts"
            }
            // DST-FR-04: the boundary this one draws is which *version* it
            // searches, because a model that assumed history was included would
            // recommend against a draft's own past.
            n if n == draft_search::NAME => {
                "accepted history and other snapshots are never searched"
            }
            // RDT-FR-04: the same boundary from the reading side.
            n if n == draft_read::NAME => {
                "never returns an accepted-history snapshot or another version"
            }
            // RGF-FR-01: the boundary this one draws is that it reads one named
            // file out of one working copy — a model that took it for a search,
            // or for a reach into the project it can see, would read the wrong
            // tree and never know.
            n if n == read_graduation_file::NAME => "it does not list a folder",
            // ESU-FR-03: the boundary is what the question is *not* for, which
            // is the mistake a model actually makes with it.
            n if n == escalate_to_user::NAME => "Do not use it to report progress",
            // ADQ-FR-WBQL: the boundary is which of the two asking tools this
            // is — a model that reached for it with nothing to propose would
            // be refused, and one that reached for the other with three open
            // points would spend three turns.
            n if n == ask_discussion_questions::NAME => {
                "use `ask_user_comment` instead"
            }
            other => panic!("no boundary phrase declared for {other} (TLC-FR-03)"),
        };
        assert!(
            text.contains(disclaims),
            "{name}: the description says what the tool does NOT do (TLC-FR-03)",
        );
        // The first sentence alone decides relevance, so it must name the
        // subject this tool answers about rather than open on a caveat. The
        // subject is per tool for the same reason the boundary phrase is: a
        // model picking among five tools tells them apart on what each is
        // *about*, and "skill" asserted across all five would say the group
        // answers one question.
        let subject = match name.as_str() {
            n if n == skill_search::NAME || n == skill_list::NAME || n == skill_load::NAME => {
                "skill"
            }
            n if n == spec_search::NAME => "specifications",
            n if n == file_read::NAME => "file",
            n if n == ask_user_comment::NAME => "question",
            // ADQ-FR-WBQL: its first sentence is "Put several questions to the
            // author at once, when more than one thing about this discussion is
            // unsettled and you can propose the likely answers to each." — the
            // subject is the questions, and "several" is what tells it apart
            // from `ask_user_comment`'s one on that sentence alone.
            n if n == ask_discussion_questions::NAME => "several questions",
            n if n == propose_draft_changes::NAME => "draft",
            n if n == draft_search::NAME || n == draft_read::NAME => "draft",
            n if n == note_search::NAME => "notes",
            n if n == document_search::NAME => "documents",
            n if n == document_get::NAME => "document",
            n if n == read_graduation_file::NAME => "file",
            // ESU-FR-19: its first sentence is "Stop and ask the person who
            // started this graduation what you cannot answer yourself." — the
            // subject is the asking rather than the noun, the questions
            // themselves being what the sentence after it is about.
            n if n == escalate_to_user::NAME => "ask",
            other => panic!("no subject declared for {other} (TLC-FR-03)"),
        };
        let first = text.split(". ").next().unwrap_or_default();
        assert!(
            first.contains(subject),
            "{name}: the first sentence alone must be enough to decide relevance, \
             and must name {subject:?} (TLC-FR-03)",
        );

        // TLC-FR-04: nothing a model cannot act on.
        //
        // Matched as whole words, not as substrings: a description is English
        // prose, and ordinary English contains these letter sequences —
        // "instructions" carries "struct", "original" carries "rig". A
        // substring scan flags those and says the description named a Rust
        // keyword, which is the opposite of true and trains the next author to
        // work around the test rather than to write a better description.
        let words: std::collections::HashSet<&str> = text
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .collect();
        for forbidden in [
            "rig",
            "Rust",
            "serde",
            "Bm25",
            "BM25",
            "PortableTool",
            "Tauri",
            "invoke",
            "crate",
            "struct",
        ] {
            assert!(
                !words.contains(forbidden),
                "{name}: the description must not name {forbidden:?} (TLC-FR-04)",
            );
        }
        // A specification identifier is not a word, so these stay substrings.
        for forbidden in [
            "TLC-FR", "SST-FR", "SLT-FR", "LSK-FR", "DSL-FR", "BMI-FR", ".md`",
        ] {
            assert!(
                !text.contains(forbidden),
                "{name}: the description must not name {forbidden:?} (TLC-FR-04)",
            );
        }
    }
}

/// The word scan is only worth having if it still catches what it is for.
#[test]
fn the_forbidden_word_scan_catches_a_real_implementation_leak() {
    let leaky = "Search the skills. Returns a PortableTool result you can invoke.";
    let words: std::collections::HashSet<&str> = leaky
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .collect();
    assert!(words.contains("PortableTool") && words.contains("invoke"));
    // ...and does not fire on the English that made it word-aware.
    let clean = "Read one skill's full instructions and follow the original steps.";
    let words: std::collections::HashSet<&str> = clean
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .collect();
    assert!(!words.contains("struct") && !words.contains("rig"));
}

// ---------------------------------------------------------------------------
// TLC-FR-05 — definitions are fixed application data (TLC-FR-05, TLC-FR-15)
// ---------------------------------------------------------------------------

#[test]
fn definitions_are_identical_across_instances_and_project_states() {
    let open = demo_project();
    let closed = closed_project();

    let a = rig::tool::tool_definition(&skill_search::SkillSearchTool::new(open.handle()));
    let b = rig::tool::tool_definition(&skill_search::SkillSearchTool::new(open.handle()));
    let c = rig::tool::tool_definition(&skill_search::SkillSearchTool::new(
        closed.handle().clone(),
    ));

    assert_eq!(a.description, b.description);
    assert_eq!(a.description, c.description);
    assert_eq!(a.parameters, b.parameters);
    assert_eq!(
        a.parameters, c.parameters,
        "TLC-FR-05: a definition is compiled-in data, not derived from project state",
    );
    // Byte-identical as well as equal: two equal JSON objects can hold their
    // fields in different orders, and a provider sees the bytes.
    assert_eq!(a.parameters.to_string(), c.parameters.to_string());

    let l1 = rig::tool::tool_definition(&skill_list::SkillListTool::new(open.handle()));
    let l2 = rig::tool::tool_definition(&skill_list::SkillListTool::new(
        closed.handle().clone(),
    ));
    assert_eq!(l1.description, l2.description);
    assert_eq!(l1.parameters, l2.parameters);

    let k1 = rig::tool::tool_definition(&skill_load::SkillLoadTool::new(open.handle()));
    let k2 =
        rig::tool::tool_definition(&skill_load::SkillLoadTool::new(closed.handle().clone()));
    assert_eq!(k1.description, k2.description);
    assert_eq!(k1.parameters, k2.parameters);
}

// ---------------------------------------------------------------------------
// TLC-FR-06 — the parameter schema (TLC-FR-06)
// ---------------------------------------------------------------------------

#[test]
fn every_parameter_schema_is_a_documented_json_schema_object() {
    for definition in definitions() {
        let schema = &definition.parameters;
        let name = &definition.name;

        assert_eq!(
            schema.get("type").and_then(|t| t.as_str()),
            Some("object"),
            "{name}: the parameter document is a JSON Schema object (TLC-FR-06)",
        );

        let properties = schema
            .get("properties")
            .and_then(|p| p.as_object())
            .unwrap_or_else(|| panic!("{name}: a properties map is required (TLC-FR-06)"));
        let required: Vec<&str> = schema
            .get("required")
            .and_then(|r| r.as_array())
            .expect("a required list")
            .iter()
            .filter_map(|v| v.as_str())
            .collect();

        for (parameter, declared) in properties {
            let description = declared
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or_default();
            assert!(
                !description.is_empty(),
                "{name}.{parameter}: every parameter carries its own description (TLC-FR-06)",
            );
        }

        // `required` names exactly the parameters that have no default.
        for parameter in &required {
            assert!(
                properties.contains_key(*parameter),
                "{name}: required names {parameter}, which is not a declared property",
            );
        }
    }
}

#[test]
fn a_bounded_parameter_states_its_default_and_range_in_its_own_description() {
    // TLC-FR-06: a model reads the description and does not reliably infer a
    // constraint from a schema keyword.
    let schema = skill_search::parameters();
    let limit = schema
        .pointer("/properties/limit/description")
        .and_then(|d| d.as_str())
        .expect("limit is documented");

    assert!(
        limit.contains(&skill_search::DEFAULT_LIMIT.to_string()),
        "the default is stated in the description: {limit:?}",
    );
    // Asserted as the phrase rather than as `contains("1")`, which "Defaults
    // to 10" already satisfies and which therefore could not fail.
    assert!(
        limit.contains(&format!(
            "below {} or above {}",
            skill_search::MIN_LIMIT,
            skill_search::MAX_LIMIT
        )),
        "both bounds are stated in the description: {limit:?}",
    );
}

// ---------------------------------------------------------------------------
// TLC-FR-07 — forgiving arguments (TLC-FR-07, TLC-FR-10)
// ---------------------------------------------------------------------------

#[test]
fn an_absent_optional_takes_its_default_and_an_out_of_range_value_is_clamped() {
    let absent: skill_search::SkillSearchArgs =
        serde_json::from_value(serde_json::json!({ "query": "anything" })).unwrap();
    assert_eq!(
        skill_search::normalize_limit(absent.limit),
        skill_search::DEFAULT_LIMIT as usize,
        "an absent optional takes its documented default (TLC-FR-07)",
    );

    assert_eq!(skill_search::normalize_limit(Some(0)), 1);
    assert_eq!(skill_search::normalize_limit(Some(-5)), 1);
    assert_eq!(skill_search::normalize_limit(Some(1000)), 25);
    assert_eq!(skill_search::normalize_limit(Some(7)), 7);
}

#[test]
fn an_unrecognised_field_is_ignored_by_every_tool() {
    // TLC-FR-07: the arguments were composed by a model, and a stray field is
    // not a question worth spending a turn on.
    let search: skill_search::SkillSearchArgs = serde_json::from_value(serde_json::json!({
        "query": "review a specification",
        "ecosystem": "claude",
        "verbose": true,
    }))
    .expect("an unrecognised field is ignored (TLC-FR-07)");
    assert_eq!(search.query, "review a specification");

    serde_json::from_value::<skill_list::SkillListArgs>(serde_json::json!({ "filter": "x" }))
        .expect("an unrecognised field is ignored (TLC-FR-07)");

    let load: skill_load::LoadSkillArgs = serde_json::from_value(serde_json::json!({
        "name": "analyst",
        "version": 2,
    }))
    .expect("an unrecognised field is ignored (TLC-FR-07)");
    assert_eq!(load.name, "analyst");
    assert_eq!(load.ecosystem, None);
}

#[test]
fn an_absent_required_parameter_does_not_decode() {
    // TLC-FR-07: guessing one would answer a question the model did not ask.
    // `rig` decodes `Args` before `call`, so this is the failure a model sees,
    // and it arrives as the `InvalidArgs` of TLC-FR-10.
    let decoded =
        serde_json::from_value::<skill_search::SkillSearchArgs>(serde_json::json!({ "limit": 5 }));
    assert!(decoded.is_err(), "a missing required parameter is a refusal");

    let decoded = serde_json::from_value::<skill_load::LoadSkillArgs>(
        serde_json::json!({ "ecosystem": "claude" }),
    );
    assert!(decoded.is_err(), "a missing required parameter is a refusal");
}

// ---------------------------------------------------------------------------
// TLC-FR-08 — output shape (TLC-FR-08)
// ---------------------------------------------------------------------------

#[test]
fn every_output_is_an_object_carrying_its_collection_under_a_named_field() {
    let fixture = demo_project();
    let handle = fixture.handle();

    let search = block_on(skill_search::SkillSearchTool::new(handle.clone()).call(
        skill_search::SkillSearchArgs {
            query: "review a specification".to_string(),
            limit: None,
        },
    ))
    .unwrap();
    let value = serde_json::to_value(&search).unwrap();
    assert!(value.is_object(), "never a bare array (TLC-FR-08)");
    assert!(
        value.get("skills").map(|s| s.is_array()).unwrap_or(false),
        "the collection sits under a named field (TLC-FR-08)",
    );

    let list =
        block_on(skill_list::SkillListTool::new(handle).call(skill_list::SkillListArgs::default()))
            .unwrap();
    let value = serde_json::to_value(&list).unwrap();
    assert!(value.is_object());
    assert!(value.get("skills").map(|s| s.is_array()).unwrap_or(false));

    // A `Debug` rendering would carry the Rust type name; the JSON does not.
    let rendered = serde_json::to_string(&value).unwrap();
    assert!(!rendered.contains("SkillEntry") && !rendered.contains("SkillListOutput"));
}

#[test]
fn a_document_shaped_output_is_one_unwrapped_text_block() {
    // TLC-FR-08, LSK-FR-12. Asserted through `IntoToolOutput` rather than
    // through the returned `String`, because the requirement is about the
    // content block a model receives and the conversion is where a wrapper
    // would appear. `as_json()` returning `None` is the half that fails if the
    // output type is ever changed to a struct.
    let fixture = collision_project();
    let text = block_on(
        skill_load::SkillLoadTool::new(fixture.handle()).call(skill_load::LoadSkillArgs {
            name: "analyst".to_string(),
            ecosystem: None,
        }),
    )
    .unwrap();

    let output = rig::tool::IntoToolOutput::into_tool_output(text.clone()).unwrap();
    assert_eq!(
        output.as_text(),
        Some(text.as_str()),
        "TLC-FR-08: a document is one text block holding it as written",
    );
    assert!(
        output.as_json().is_none(),
        "TLC-FR-08: a document is not a JSON object wrapping it in a string field",
    );
    // The escaping this shape exists to avoid: a wrapper would turn the body's
    // line breaks into `\n` on the way to the model.
    assert!(
        text.contains('\n') && !output.render().contains("\\n"),
        "the body reaches the model unescaped",
    );
}

/// TLC-FR-08, the declared exception: a document that cannot be acted on apart
/// from its metadata returns as *data* carrying the document under a named
/// field (TLC-FR-08, RDT-FR-05).
///
/// The mirror image of the test above, and asserted through the same conversion
/// for the same reason — the content block a model receives is where the shape
/// is actually decided. `read_draft` is the one tool in this group taking it.
#[test]
fn the_one_data_with_document_output_is_a_json_object_carrying_its_document() {
    let dir = TempDir::new().unwrap();
    let fixture = mounted(dir);
    let root = fixture
        .app
        .state::<crate::bm25_index::Bm25Indexer>()
        .root()
        .expect("mounted");
    let session = agent_session(&fixture.app, &root, "data-with-document");
    let root_fs = crate::fs::RootFs::for_root(&root);
    let created =
        crate::drafts::create_draft_at_root(&root_fs, Some("shaped")).expect("creates");
    let body = "# Plan\n\nA prompt whose line breaks must survive.\n";
    crate::drafts::save_draft_file_impl(&root_fs, &created.draft.id, "shaped.md", body)
        .expect("writes");

    let result = block_on(
        draft_read::DraftReadTool::new(fixture.handle(), session).call(
            draft_read::ReadDraftArgs {
                draft_id: created.draft.id.clone(),
            },
        ),
    )
    .expect("reads");

    let output = rig::tool::IntoToolOutput::into_tool_output(result.clone()).unwrap();
    let json = output
        .as_json()
        .expect("TLC-FR-08: the declared exception returns data, not a text block");
    assert!(
        output.as_text().is_none(),
        "TLC-FR-08: this one is data rather than an unwrapped document",
    );
    // The document sits under a named field beside the metadata it cannot be
    // acted on without — which is the whole justification for the exception.
    assert_eq!(json["content"], serde_json::json!(body));
    assert_eq!(json["draft_id"], serde_json::json!(created.draft.id));
    assert!(json.get("name").is_some() && json.get("status").is_some());
}


// ---------------------------------------------------------------------------
// TLC-FR-14 — the shape of arguments that did not decode, and never a value
// ---------------------------------------------------------------------------

/// A schema declaring the names these tests expect to read back.
fn shape_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "path": { "type": "string" },
            "rationale": { "type": "string" },
            "hunks": {
                "type": "array",
                "items": {
                    "type": "object",
                    "properties": {
                        "kind": { "type": "string" },
                        "before": { "type": "string" },
                        "after": { "type": "string" },
                    },
                },
            },
        },
    })
}

fn shape_of(value: serde_json::Value) -> String {
    crate::tools::argument_shape(&value, &shape_schema())
}

// TLC-FR-14: the shape names the fields the model sent and the kind of value
// each holds, so a reader can see which one was wrong. A field the schema does
// not declare is the mark and never the name — `line` here is the mistake, and
// that it was made is the whole of what a reader needs.
//
// The fields are read by name, whatever order the object holds them in, so
// they read back in that order rather than in the order they were written.
#[test]
fn an_argument_shape_names_the_fields_and_the_kinds_and_no_value() {
    let shape = shape_of(serde_json::json!({
        "path": "prompt.md",
        "rationale": "because",
        "hunks": [{ "kind": "replace", "before": "old", "line": 12 }],
    }));
    assert_eq!(
        shape,
        "{hunks:[{before:string,kind:string,?:number}](1),path:string,rationale:string}",
    );
}

// TLC-FR-14: no value of any kind reaches the shape — not a path, not a
// sentence, not a line of the document the model was reading.
#[test]
fn an_argument_shape_carries_no_value_the_model_composed() {
    let secret = "THE WHOLE OF THE AUTHORS DOCUMENT";
    let shape = shape_of(serde_json::json!({
        "path": "specifications/ui/EDT-editor.md",
        "rationale": secret,
        "hunks": [{ "before": "## Intent", "after": "## Intent\n\nRewritten." }],
    }));
    for value in [secret, "specifications", "EDT-editor.md", "## Intent"] {
        assert!(!shape.contains(value), "the shape carried {value:?}: {shape}");
    }
}

// TLC-FR-14: a name reaches a record only where the tool's own schema declares
// it. This is the rule rather than how a name is spelled, because the values
// this application must never write down are spelled exactly as a field name
// is: letters and digits within any length a field name keeps. A model that put
// one of them in a key position repeats none of it.
#[test]
fn a_key_the_schema_does_not_declare_is_never_repeated() {
    let keys = [
        // The shape every issued key has, which no rule reading the key alone
        // tells from a field name: a prefix, an underscore, and forty
        // characters of letters and digits.
        "abc_0123456789abcdef0123456789abcdef0123",
        // A commit, an identifier, a login, a file.
        "0123456789abcdef0123456789abcdef01234567",
        "3f2504e0-4f89-11d3-9a0c-0305e82c3301",
        "raver119",
        "prompt.md",
        // A line of prose, and a name spelled in another script.
        "The author asked me to rewrite the Intent",
        "путь",
        // And the empty key.
        "",
    ];
    for key in keys {
        let shape = shape_of(serde_json::json!({ key: "x" }));
        assert_eq!(shape, "{?:string}", "the shape repeated the key {key:?}");
    }
}

// TLC-FR-14: two keys the schema does not declare are two marks. The mark says a
// field was invented and says nothing about which, so it tells one from another
// no more than it repeats either.
#[test]
fn two_keys_the_schema_does_not_declare_are_both_marks() {
    assert_eq!(
        shape_of(serde_json::json!({ "aaa": 1, "bbb": 2 })),
        "{?:number,?:number}",
    );
}

// TLC-FR-14 / LGC-FR-08: the shape is bounded in length, so a record keeps the
// tool and the reason however large the argument set was — the record's own
// ceiling is 16 KB and this is 301 characters at its widest.
#[test]
fn an_argument_shape_is_bounded_in_length() {
    // Names the schema declares, so nothing is collapsed to a mark and the
    // bound is what stops the string rather than the mark being shorter.
    let names: Vec<String> = (0..40).map(|i| format!("field{i:0>34}")).collect();
    let properties: serde_json::Map<String, serde_json::Value> = names
        .iter()
        .map(|n| (n.clone(), serde_json::json!({ "type": "string" })))
        .collect();
    let schema = serde_json::json!({ "properties": properties });
    let arguments: serde_json::Map<String, serde_json::Value> = names
        .iter()
        .map(|n| (n.clone(), serde_json::json!("x")))
        .collect();
    let shape = crate::tools::argument_shape(&serde_json::Value::Object(arguments), &schema);
    let counted = shape.chars().count();
    assert_eq!(counted, 301, "the bound did not stop it: {counted} chars");
    assert!(shape.ends_with('…'), "a stopped shape says it was stopped");
}

// TLC-FR-14: depth is bounded too, so a deeply nested argument set costs a
// record no more than a flat one — and an argument set nested past any depth
// worth reading is answered rather than followed down.
#[test]
fn an_argument_shape_stops_at_a_fixed_depth() {
    assert_eq!(
        shape_of(serde_json::json!({ "a": { "b": { "c": { "d": "too deep" } } } })),
        "{?:{?:{?:{…}}}}",
    );
    // An array spends a level as an object does, which is the shape a change
    // list filled wrongly actually takes.
    assert_eq!(
        shape_of(serde_json::json!({ "hunks": [[[["x"]]]] })),
        "{hunks:[[[…](1)](1)](1)}",
    );
    // And the cap holds however far past it the argument set is nested, which
    // is what stops the walk rather than the shape of any one entry.
    let mut deep = serde_json::json!("x");
    for _ in 0..200 {
        deep = serde_json::json!({ "a": deep });
    }
    assert_eq!(shape_of(deep), "{?:{?:{?:{…}}}}");
}

// TLC-FR-14: the containers, empty and at the top level. A provider that hands
// the arguments as one string rather than as an object is worth telling apart
// from one that sent an object with nothing in it.
#[test]
fn an_argument_shape_answers_for_every_container() {
    assert_eq!(shape_of(serde_json::json!({})), "{}");
    assert_eq!(shape_of(serde_json::json!({ "hunks": [], "path": {} })), "{hunks:[](0),path:{}}");
    assert_eq!(shape_of(serde_json::json!([{ "path": "x" }])), "[{path:string}](1)");
    assert_eq!(shape_of(serde_json::json!("{\"path\":\"x\"}")), "string");
    assert_eq!(shape_of(serde_json::Value::Null), "null");
}

// TLC-FR-14: a list is reported by its length and by its first entry alone. A
// list the model filled wrongly is wrong the same way all the way down, and one
// entry names the mistake without the record growing with the list.
#[test]
fn a_list_is_reported_by_its_length_and_its_first_entry() {
    assert_eq!(
        shape_of(serde_json::json!({ "hunks": [1, "A SECRET", { "kind": "add" }] })),
        "{hunks:[number](3)}",
    );
}

// TLC-FR-14: the fields are read by name, whatever order the object holds them
// in, so an argument set wider than the bound is reported by its first fields
// by name rather than by the fields the model wrote first.
#[test]
fn a_wide_argument_set_is_reported_by_its_first_fields_by_name() {
    let names: Vec<String> = (0..14).map(|index| format!("f{index:02}")).collect();
    let mut properties = serde_json::Map::new();
    for name in &names {
        properties.insert(name.clone(), serde_json::json!({ "type": "string" }));
    }
    let schema = serde_json::json!({ "type": "object", "properties": properties });
    let mut sent = serde_json::Map::new();
    for name in names.iter().rev() {
        sent.insert(name.clone(), serde_json::json!("x"));
    }
    let shape = crate::tools::argument_shape(&serde_json::Value::Object(sent), &schema);
    let expected = names[..12]
        .iter()
        .map(|name| format!("{name}:string"))
        .collect::<Vec<_>>()
        .join(",");
    assert_eq!(shape, format!("{{{expected},…}}"));
}
