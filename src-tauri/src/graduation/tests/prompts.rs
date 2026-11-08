//! What the compiled-in instructions may say (`../ai/GRL-graduation-loop.md`
//! GRL-FR-GADT, GRL-FR-YIJG).
//!
//! These read the prompt files themselves rather than the loop. They exist
//! because the failure they catch is silent: an instruction that names a field
//! the task input does not carry leaves the turn with no index of its work at
//! all, and the turn answers anyway.

use super::*;

/// The six instructions this loop compiles in (GRL-FR-GADT), as source.
fn prompt_sources() -> Vec<(&'static str, &'static str)> {
    vec![
        ("work.md", include_str!("../../../../resources/prompts/graduation/work.md")),
        ("review.md", include_str!("../../../../resources/prompts/graduation/review.md")),
        ("rebase.md", include_str!("../../../../resources/prompts/graduation/rebase.md")),
        ("refusals.md", include_str!("../../../../resources/prompts/graduation/refusals.md")),
        ("merge-work.md", include_str!("../../../../resources/prompts/graduation/merge-work.md")),
        (
            "merge-review.md",
            include_str!("../../../../resources/prompts/graduation/merge-review.md"),
        ),
        ("pr-summary.md", include_str!("../../../../resources/prompts/graduation/pr-summary.md")),
    ]
}

/// GRL-FR-SNHR: every field the task input of a PR summary turn carries, with
/// the optional ones set.
fn pr_summary_input_fields() -> Vec<String> {
    let run = super::pr_summary_turn::pr_summary_run("feature/panel", PrSummaryMode::Combine);
    let mut input = driver::compose_pr_summary_input(&run).expect("a PR summary run");
    input.correction = Some("send this instead".into());
    input.to_map().keys().cloned().collect()
}

/// GRL-FR-VHIS: a review history that has an earlier pass in it, so that the
/// optional field of a work turn is present.
fn earlier_findings() -> Vec<ReviewHistoryEntry> {
    let finding = |description: &str| ReviewFinding {
        severity: ReviewSeverity::Major,
        description: description.into(),
        affected_files: Vec::new(),
        correction: "Fix it.".into(),
    };
    vec![
        ReviewHistoryEntry { pass: 1, finding: finding("earlier") },
        ReviewHistoryEntry { pass: 2, finding: finding("latest") },
    ]
}

/// Every `input.<name>` a document names.
fn fields_named(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for (index, _) in text.match_indices("input.") {
        let rest = &text[index + "input.".len()..];
        let name: String =
            rest.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
        if !name.is_empty() {
            out.push(name);
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Every field the task input of a **merge run** carries for one part, with the
/// optional ones set so none is skipped (GRL-FR-TXEB).
fn merge_input_fields(part: &str) -> Vec<String> {
    let mut run = super::merge_input::bare_merge_run();
    run.checkpoint.pass = 1;
    run.checkpoint.loop_instruction = Some("findings".into());
    run.checkpoint.review_history = earlier_findings();
    run.checkpoint.agent_account = Some("what the work turn said".into());
    run.checkpoint.changed_paths = vec!["src/call.rs".into()];
    run.escalation = Some(GraduationEscalation {
        reason: "why".into(),
        questions: Vec::new(),
        origin: GraduationEscalationOrigin::Work,
        raised_at: "t0".into(),
        resume: None,
    });
    let mut input = driver::compose_input(&run, part, driver::PURPOSE_GENERATE);
    input.correction = Some("send this instead".into());
    // GRL-FR-CHDT: the gate sets `check_correction` on a work part alone.
    if part == driver::PART_WORK {
        input.check_correction = Some("fix the checks".into());
    }
    // GRL-FR-FESZ: the review sets `checks` once its checkout stands, as it
    // sets `correction`, so the review part is given it here.
    if part == driver::PART_REVIEW {
        input.checks = Some(driver::checks_folder::REPORT_PATH.into());
    }
    input.to_map().keys().cloned().collect()
}

/// Every field the graduation task input carries, with the optional ones set so
/// none is skipped.
fn graduation_input_fields(part: &str) -> Vec<String> {
    let mut run = GraduationRun {
        id: "g1".into(),
        stream_id: "w1".into(),
        stream_name: "editor".into(),
        direct_target: None,
        target_hold: None,
        project_key: "p".into(),
        state: GraduationRunState::Queued,
        standing_work: StandingWork::default(),
        standing_work_message: None,
        standing_work_outcome: None,
        input: CapturedGraduationInput {
            draft_id: "d1".into(),
            draft_name: "draft".into(),
            prompt: "do it".into(),
            prompt_checksum: "sum".into(),
            captured_at: "t0".into(),
        },
        base_commit: None,
        commits: Vec::new(),
        auto_start: true,
        dispatched: false,
        archived: false,
        archived_at: None,
        work_turns: 0,
        review_turns: 0,
        turn_time_used_ms: 0,
        rest: None,
        logs: crate::graduation::logs::GraduationLogIndexes::default(),
        checkpoint: GraduationCheckpoint::default(),
        observability: GraduationObservability::default(),
        escalation: None,
        blocker: None,
        interruption: None,
        restarted_from_run_id: None,
        failure: None,
        merge: None,
        pr_summary: None,
        run_summary: None,
        landings: Vec::new(),
        reverts: Vec::new(),
        created_at: "t0".into(),
        updated_at: "t0".into(),
    };
    run.checkpoint.pass = 1;
    run.checkpoint.loop_instruction = Some("findings".into());
    // GRL-FR-VHIS: the history is skipped when it is empty, so one earlier
    // finding stands here and the work part names it.
    run.checkpoint.review_history = earlier_findings();
    // GRL-FR-DNKA: an optional field is skipped when it is absent, so every one
    // a prompt may name is filled here — otherwise the check below reads a
    // field a turn is genuinely given as one that does not exist.
    run.checkpoint.agent_account = Some("what the work turn said".into());
    run.escalation = Some(GraduationEscalation {
        reason: "why".into(),
        questions: Vec::new(),
        origin: GraduationEscalationOrigin::Work,
        raised_at: "t0".into(),
        resume: None,
    });
    let mut input = driver::compose_input(&run, part, driver::PURPOSE_GENERATE);
    input.correction = Some("send this instead".into());
    // GRL-FR-CHDT: the gate sets `check_correction` on a work part alone.
    if part == driver::PART_WORK {
        input.check_correction = Some("fix the checks".into());
    }
    // GRL-FR-FESZ: the review sets `checks` once its checkout stands, as it
    // sets `correction`, so the review part is given it here.
    if part == driver::PART_REVIEW {
        input.checks = Some(driver::checks_folder::REPORT_PATH.into());
    }
    input.to_map().keys().cloned().collect()
}

// GRL-FR-GADT, GRL-FR-OTRH, GRL-FR-SLQF, GRL-FR-MRVK, GRL-FR-TXEB: every field an
// instruction names is one its turn is actually given. A name that resolves to nothing leaves the turn with no index
// of the work it was asked to judge.
#[test]
fn every_input_field_a_prompt_names_exists() {
    // GRL-FR-DNKA: the fields differ by part — the review is given the work
    // turn's account and the work turn is not — so each instruction is held to
    // what **its own** turn carries. One combined list would let a prompt name a
    // field another part's turn receives and this check would not notice.
    let work = graduation_input_fields(driver::PART_WORK);
    let review = graduation_input_fields(driver::PART_REVIEW);
    let merge = crate::streams::merge_input_field_names();
    let merge_work = merge_input_fields(driver::PART_WORK);
    let merge_review = merge_input_fields(driver::PART_REVIEW);
    let pr_summary = pr_summary_input_fields();

    for (name, source) in prompt_sources() {
        // The semantic merge turn carries its own input, composed by the merge
        // rather than by this loop.
        let carried: &[String] = match name {
            "rebase.md" => &merge,
            "review.md" => &review,
            // GRL-FR-SLQF, GRL-FR-MRVK, GRL-FR-TXEB: a merge run's two turns are
            // given the merge context and are held to their own input.
            "merge-work.md" => &merge_work,
            "merge-review.md" => &merge_review,
            // GRL-FR-SNHR: the PR summary turn is held to its own input.
            "pr-summary.md" => &pr_summary,
            _ => &work,
        };
        for field in fields_named(source) {
            assert!(
                carried.iter().any(|known| known == &field),
                "{name} names `input.{field}`, which its turn is not given. \
                 The fields it carries are {carried:?}",
            );
        }
    }
}

// GRB-FR-KMXT: the author's decisions reach the turn as structured input, and
// an input field no instruction names is one the turn is never told to read.
// The field and the wording that reads it move together or this fails.
#[test]
fn the_rebase_prompt_reads_the_authors_decisions() {
    let source = prompt_sources()
        .into_iter()
        .find(|(name, _)| *name == "rebase.md")
        .map(|(_, source)| source)
        .expect("the rebase instruction");

    assert!(
        crate::streams::merge_input_field_names().iter().any(|field| field == "author_decisions"),
        "the merge carries the author's decisions",
    );
    assert!(
        fields_named(source).iter().any(|field| field == "author_decisions"),
        "and rebase.md tells the turn to read them",
    );
}

// GRL-FR-CKBL, GRL-FR-DNKA: the review turn is told the work turn's account is
// there, and told to check what it claims against the change set.
//
// The account reaching the input accomplishes nothing on its own: before this,
// the field was carried on the failure path and `review.md` never named it, so
// no review ever read it. The delivery decision is the one the account settles,
// so this asserts both that the field is named among the material the turn is
// given and that the decision reaches for it.
#[test]
fn the_review_prompt_reads_the_work_turns_account() {
    let source = prompt_sources()
        .into_iter()
        .find(|(name, _)| *name == "review.md")
        .map(|(_, source)| source)
        .expect("the review instruction");

    // GRL-FR-CKBL: named among **the material the turn is given**, which is the
    // half that tells the reviewer the field exists at all. Found in that list
    // rather than anywhere in the file: a mention in the delivery bullet alone
    // would leave a reviewer reading the given material with no idea it is
    // there.
    let given = source
        .lines()
        .skip_while(|line| !line.starts_with("- `input.prompt`"))
        .take_while(|line| line.starts_with("- "))
        .find(|line| line.contains("`input.agent_account`"))
        .expect("GRL-FR-CKBL: review.md names the account among what the turn is given");

    // GRL-FR-DNKA: and says whose words it is.
    let lower = given.to_lowercase();
    assert!(
        lower.contains("work turn") && lower.contains("own account"),
        "GRL-FR-DNKA: the line says whose words the account is: {given}",
    );
    // GRL-FR-NVBZ: a claim rather than evidence, and the line says what settles
    // it instead. Asserted on the **claim** it must carry rather than on an
    // exact phrase, so rewording the sentence does not fail a passing reviewer.
    assert!(lower.contains("claim"), "GRL-FR-NVBZ: the line calls the account a claim: {given}",);
    assert!(
        lower.contains("working copy"),
        "GRL-FR-NVBZ: and names the working copy as what says whether it is true: {given}",
    );

    // GRL-FR-CKBL: the delivery decision is what checks it against the change
    // set. Both halves asserted on the one line, so a mention that does not
    // reach the decision fails.
    let delivery = source
        .lines()
        .find(|line| line.starts_with("- **Delivery.**"))
        .expect("the delivery decision");
    assert!(
        delivery.contains("input.agent_account"),
        "GRL-FR-CKBL: the delivery decision reads the account: {delivery}",
    );
    assert!(
        delivery.contains("against the change set"),
        "GRL-FR-CKBL: and checks it against the change set rather than believing it: {delivery}",
    );
}

// GRL-FR-GADT: an instruction that cites a specification cites one that exists.
// A citation to a retired document is how a prompt outlives the design it was
// written for.
#[test]
fn every_specification_a_prompt_cites_exists() {
    let root =
        Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("the repository root").to_path_buf();
    for (name, source) in prompt_sources() {
        for (index, _) in source.match_indices("specifications/") {
            let rest = &source[index..];
            let cited: String =
                rest.chars().take_while(|c| !c.is_whitespace() && *c != '`' && *c != ')').collect();
            if !cited.ends_with(".md") {
                continue;
            }
            assert!(
                root.join(&cited).is_file(),
                "{name} cites {cited}, which is not in the corpus",
            );
        }
    }
}

// GRL-FR-YIJG: a prompt's instruction text is its content with every authoring
// comment removed. An agent never reads the requirement names.
#[test]
fn no_authoring_comment_reaches_an_agent() {
    for (name, source) in prompt_sources() {
        let instruction = crate::prompts::instruction_text(source);
        assert!(
            !instruction.contains("<!--") && !instruction.contains("-->"),
            "{name} carries an authoring comment into its instruction",
        );
        assert!(
            !instruction.contains("-FR-"),
            "{name} names a requirement in the text an agent reads",
        );
        assert!(!instruction.trim().is_empty(), "{name} is empty");
    }
}

// GRL-FR-TVXI: the correction a refused review is asked again with is a section
// the loop can actually find.
#[test]
fn the_review_refusal_names_the_shape_a_verdict_must_take() {
    let correction = driver::prompt_section(&driver::REFUSALS, driver::REFUSAL_REVIEW_RESULT)
        .expect("refusals.md holds the review_result correction");
    for required in ["verdict", "rationale", "findings", "severity", "correction"] {
        assert!(correction.contains(required), "the correction does not name `{required}`",);
    }
}

// GRL-FR-MZHT, GRL-FR-XSFA: every correctable refusal has its section in
// `refusals.md`, and each section says that the work stands and that no file
// changes. A refusal the agent cannot repair has none.
#[test]
fn every_correctable_refusal_has_a_correction_that_keeps_the_work() {
    use crate::tools::agent_exec::protocol::EnvelopeInvalid;
    for section in driver::corrections::correction_sections() {
        let text = driver::prompt_section(&driver::REFUSALS, section)
            .unwrap_or_else(|| panic!("refusals.md holds the {section} correction"));
        assert!(text.contains("change no file"), "{section} says to change no file");
        assert!(text.contains("work you did stands"), "{section} says that the work stands");
        assert!(text.contains("Send your answer again"), "{section} asks for the answer again");
    }
    for (correctable, section) in [
        (EnvelopeInvalid::NotFound, "answer_missing"),
        (EnvelopeInvalid::Malformed, "answer_malformed"),
        (EnvelopeInvalid::ProtocolVersion, "answer_malformed"),
        (EnvelopeInvalid::MultipleDocuments, "answer_several"),
        (EnvelopeInvalid::SummaryEmpty, "answer_summary_empty"),
        (EnvelopeInvalid::Exclusivity, "answer_outcome_mismatch"),
        (EnvelopeInvalid::EscalationShape, "answer_escalation_shape"),
        (EnvelopeInvalid::TooLarge, "answer_too_large"),
    ] {
        assert_eq!(driver::corrections::correction_section(correctable.as_str()), Some(section));
    }
    for not_correctable in [
        EnvelopeInvalid::NotUtf8,
        EnvelopeInvalid::OutputTruncated,
        EnvelopeInvalid::VendorRunFailed,
        EnvelopeInvalid::SessionMismatch,
    ] {
        assert!(driver::corrections::correction_section(not_correctable.as_str()).is_none());
    }
}

// GRL-FR-XSFA: the work instructions tell a turn to act on `correction`.
#[test]
fn every_instruction_tells_a_turn_to_act_on_a_correction() {
    for (name, text) in [
        ("work.md", driver::WORK_PROMPT.as_str()),
        ("merge-work.md", driver::MERGE_WORK_PROMPT.as_str()),
        ("review.md", driver::REVIEW_PROMPT.as_str()),
        ("merge-review.md", driver::MERGE_REVIEW_PROMPT.as_str()),
        ("pr-summary.md", driver::PR_SUMMARY_PROMPT.as_str()),
    ] {
        assert!(text.contains("`input.correction`"), "{name} names `input.correction`");
    }
}

// GRL-FR-TZKR: `pr-summary.md` directs the turn to the repository's pull
// request template, skills and agent instructions, says that it changes no
// file, and holds one task for each mode.
#[test]
fn the_pr_summary_prompt_names_the_template_both_modes_and_no_change() {
    let text = driver::PR_SUMMARY_PROMPT.as_str();
    for required in [
        ".github/PULL_REQUEST_TEMPLATE.md",
        "skills",
        "agent instructions",
        "change no file",
        "## When the mode is `diff`",
        "## When the mode is `combine`",
        "`input.pr_summary.merge_base`",
        "`input.pr_summary.head_tip`",
        "`input.pr_summary.inputs`",
        "\"title\"",
        "\"description\"",
    ] {
        assert!(text.contains(required), "pr-summary.md does not say `{required}`");
    }
    // GRL-FR-UNKD: the turn has no answer to a question, so it asks none.
    assert!(!text.contains("escalate_to_user"), "pr-summary.md offers no escalation");
}

// GRL-FR-FRNJ: `review.md` asks a `ready` verdict for the run summary, and
// directs the turn to the repository's template, skills and agent
// instructions for its shape.
#[test]
fn the_review_prompt_asks_a_ready_verdict_for_a_run_summary() {
    let text = driver::REVIEW_PROMPT.as_str();
    for required in [
        "\"summary\"",
        "A `revise` verdict carries **no** summary",
        ".github/PULL_REQUEST_TEMPLATE.md",
        "skills",
        "agent instructions",
        "**one line**",
    ] {
        assert!(text.contains(required), "review.md does not say `{required}`");
    }
}

// GRL-FR-VDCP, GRL-FR-MQFB: the two work instructions run narrow checks while
// working, the full checks once at the end, each as a command of its own,
// record them in the check folder, and run a long command in the foreground.
#[test]
fn the_work_instructions_record_the_full_checks_once() {
    for (name, source) in prompt_sources() {
        if !matches!(name, "work.md" | "merge-work.md") {
            continue;
        }
        let text = crate::prompts::instruction_text(source);
        for needed in [
            "run the narrow checks",
            "run the project's **full** checks **once**",
            ".synthesis-checks/logs/<name>.log",
            ".synthesis-checks/report.md",
            "the command, its exit status, its start and end time, and its summary line",
            "**Run a long command in the foreground**",
            "Never start a background job and wait for it with a sleep loop",
            "After your last edit",
            "Run each full check as a command of its own",
            "invent none",
        ] {
            assert!(text.contains(needed), "{name} does not say `{needed}`");
        }
    }
}

// GRL-FR-CNMP: the two work instructions add the entry of each check as soon as
// it ends, record a check that did not end as not finished, and keep one entry
// for each check.
#[test]
fn the_work_instructions_record_each_check_as_it_ends() {
    for (name, source) in prompt_sources() {
        if !matches!(name, "work.md" | "merge-work.md") {
            continue;
        }
        let text = crate::prompts::instruction_text(source);
        for needed in [
            "**As soon as a check ends, add its entry to `.synthesis-checks/report.md`**",
            "Do not keep the entries for the end.",
            "A check that did not end gets an entry that says it is not finished, with its start time.",
            "when you run a check again, replace its earlier entry",
        ] {
            assert!(text.contains(needed), "{name} does not say `{needed}`");
        }
        assert!(!text.contains("Write `.synthesis-checks/report.md` with one entry"), "{name}");
    }
}

// GRL-FR-MQFB, GRL-FR-ZHHX: the two work instructions ask for the longest
// timeout, say that the turn ends at the answer, and say that a command the tool
// moved to the background is stopped, recorded as not finished and named.
#[test]
fn the_work_instructions_wait_for_no_command_in_the_background() {
    for (name, source) in prompt_sources() {
        if !matches!(name, "work.md" | "merge-work.md") {
            continue;
        }
        let text = crate::prompts::instruction_text(source);
        for needed in [
            "with the longest timeout your tool allows",
            "**Your turn ends at your answer.**",
            "Nothing tells you after your answer that a background job ended",
            "**If your tool moves a command to the background**",
            "do not wait for it. Stop it, record its check as not finished, and name that check in your summary.",
        ] {
            assert!(text.contains(needed), "{name} does not say `{needed}`");
        }
        // The old reason said that a background job outlives the answer, which
        // the turn-ends rule now contradicts.
        for gone in
            ["with a timeout long enough for it", "keeps your session open after your answer"]
        {
            assert!(!text.contains(gone), "{name} still says `{gone}`");
        }
    }
}

// GRL-FR-QEOT: both review instructions ask a correction about a check for a
// foreground run, and no review instruction offers a background run.
#[test]
fn a_review_correction_asks_for_a_foreground_run() {
    for (name, source) in prompt_sources() {
        if !matches!(name, "review.md" | "merge-review.md") {
            continue;
        }
        let text = crate::prompts::instruction_text(source);
        assert!(
            text.contains("A `correction` about a check asks for that check to run in the foreground. It never asks the work turn to run a check in the background or to wait for a background job."),
            "{name} does not ask for a foreground run"
        );
        // The one sentence above is the only place a review instruction speaks
        // of the background, so no other text can offer a background run.
        assert_eq!(text.matches("background").count(), 2, "{name}");
    }
}

// GRL-FR-RTNC, GRL-FR-PNLQ: both review instructions run no check and verify the
// record the work turn wrote.
#[test]
fn the_review_instructions_verify_the_record_and_run_no_check() {
    for (name, source) in prompt_sources() {
        if !matches!(name, "review.md" | "merge-review.md") {
            continue;
        }
        let text = crate::prompts::instruction_text(source);
        for needed in [
            "**You run no check command.**",
            "`input.checks`",
            "Where `input.checks` is `missing`, return `revise`",
            "A required check that the record does not hold is a finding",
            "A check that failed, timed out, did not run or is recorded as not finished is a finding",
            "A record that does not agree with its logs is a finding",
            "is not a finding by itself",
        ] {
            assert!(text.contains(needed), "{name} does not say `{needed}`");
        }
        assert!(!text.contains("You may run the project's own build"), "{name}");
    }
}

// GRL-FR-LKLE: no instruction names a command of one project.
#[test]
fn no_instruction_names_a_command_of_one_project() {
    for (name, source) in prompt_sources() {
        let text = crate::prompts::instruction_text(source);
        for command in [
            "task specs",
            "task tests",
            "task lint",
            "pnpm",
            "cargo",
            "vitest",
            "npm run",
            "npm test",
            "go test",
            "pytest",
            "BASH_",
            "run_in_background",
            "BashOutput",
        ] {
            assert!(!text.contains(command), "{name} names `{command}`");
        }
    }
}

// GRL-FR-CKBL: the review's checks decision reads the record and runs nothing.
#[test]
fn the_review_decides_on_the_checks_by_reading_the_record() {
    let (_, source) = prompt_sources()
        .into_iter()
        .find(|(name, _)| *name == "review.md")
        .expect("the review instruction");
    let text = crate::prompts::instruction_text(source);
    assert!(text.contains(
        "- **Checks.** Whether the check record shows that every check the project requires passed"
    ));
    assert!(text.contains("and not by running anything"));
}
