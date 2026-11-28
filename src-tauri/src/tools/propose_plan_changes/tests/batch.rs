//! PPT-FR-FFML, PPT-FR-ELJI, PPT-FR-MSQT, PPT-FR-DGAA, PPT-FR-KOCG: a batch of
//! rows is one complete step, and the call reports every refused row at once.

use super::*;

/// Ten rows of one step: an epic, its features and tasks under it, two
/// changes to plan issues, and a new draft.
fn step_of_ten() -> Value {
    json!([
        create("epic", "Payments"),
        { "key": "f1", "op": "create_issue", "title": "Card capture", "parent_row": "epic" },
        { "key": "f2", "op": "create_issue", "title": "Refunds", "parent_row": "epic" },
        { "key": "t1", "op": "create_issue", "title": "Tokenize", "parent_row": "f1" },
        { "key": "t2", "op": "create_issue", "title": "Store tokens", "parent_row": "f1" },
        { "key": "t3", "op": "create_issue", "title": "Refund API", "parent_row": "f2" },
        { "key": "t4", "op": "create_issue", "title": "Refund UI", "parent_row": "f2" },
        modify("m1", 2, json!({ "title": "Feature, renamed" })),
        modify("m2", 3, json!({ "status": "Done" })),
        draft("d1"),
    ])
}

// PPT-FR-FFML: the description asks for one complete step of 5 to 10 rows and
// never for a step of a few rows.
#[test]
fn the_description_asks_for_one_complete_step() {
    assert!(DESCRIPTION.contains("every row that the step needs, up to 20"));
    assert!(DESCRIPTION.contains("Do not split one step into several calls of a few rows"));
    assert!(!DESCRIPTION.contains("Do not propose all work at once"));
}

// PPT-FR-PMQL, PPT-FR-UZEK, PPT-FR-FFML: a step of ten mixed rows with a chain
// of parent rows is one proposal and one comment.
#[test]
fn a_step_of_ten_rows_is_one_proposal() {
    let f = Fixture::new();
    let comments = f.plan.comments();
    let out = f.rows(step_of_ten()).unwrap();
    assert_eq!(out, json!({ "proposed": true, "rows": 10 }));
    assert_eq!(f.proposals().len(), 1);
    assert_eq!(f.proposals()[0].rows.len(), 10);
    assert_eq!(f.plan.comments(), comments + 1);
    assert!(f.plan.h.fake.writes().is_empty());
}

// PPT-FR-ELJI: rows 2, 5, and 9 break rules of their own. The call is one
// refusal that lists each of them in order, and nothing is recorded.
#[test]
fn several_refused_rows_make_one_refusal_that_lists_each() {
    let f = Fixture::new();
    let comments = f.plan.comments();
    let mut rows = step_of_ten();
    rows[1]["title"] = json!("  ");
    rows[4] = json!({ "key": "t2", "op": "create_issue", "title": "T", "type": "Invented" });
    rows[8] = modify("m2", 3, json!({ "status": "Nowhere" }));
    let refusal = refused(f.rows(rows));
    assert_eq!(refusal.kind, ToolErrorKind::InvalidArgs);
    assert!(refusal.retryable);
    assert_eq!(refusal.code, rows::ROWS_REFUSED);
    let lines: Vec<&str> = refusal.message.lines().collect();
    assert_eq!(
        lines,
        [
            "3 rows were refused. Correct each one and send the call again. Nothing was proposed.",
            "Row 2: Give the row a title.",
            "Row 5: The Type Invented is not offered by the repository.",
            "Row 9: The Status Nowhere is not an option of the Project.",
        ]
    );
    assert_eq!(rows::refused_count(&refusal), 3);
    assert!(f.proposals().is_empty());
    assert_eq!(f.plan.comments(), comments);
    assert!(!f.carded());
}

// PPT-FR-ELJI: a refusal of the shape and a refusal of the facts of the plan
// are reported together.
#[test]
fn shape_and_fact_refusals_are_reported_together() {
    let f = Fixture::new();
    let refusal = refused(f.rows(json!([
        { "key": "a", "op": "new_change", "name": "n" },
        create("b", "B"),
        modify("c", 77, json!({ "title": "T" })),
    ])));
    assert_eq!(refusal.code, rows::ROWS_REFUSED);
    assert!(refusal.message.contains("Row 1: Give the full prompt of the new change in `body`."));
    assert!(refusal.message.contains("Row 3: Issue number 77 is not part of the delivery plan."));
    assert!(!refusal.message.contains("Row 2"));
}

// PPT-FR-ELJI: one refused row keeps the refusal of that row, unchanged.
#[test]
fn one_refused_row_keeps_its_own_refusal() {
    let f = Fixture::new();
    let mut rows = step_of_ten();
    rows[3]["title"] = json!("");
    let refusal = refused(f.rows(rows));
    assert_eq!(refusal.code, "blank_title");
    assert_eq!(refusal.message, rows::BLANK_TITLE);
    assert_eq!(rows::refused_count(&refusal), 1);
}

// PPT-FR-MSQT: a rule of the call as a whole refuses at once, and lists no
// row.
#[test]
fn a_rule_of_the_whole_call_refuses_before_the_rows() {
    let f = Fixture::new();
    f.plan.h.fake.lock().calls.clear();
    let mut rows = step_of_ten();
    rows[9]["key"] = json!("epic");
    rows[1]["title"] = json!("");
    let refusal = refused(f.rows(rows));
    assert_eq!(refusal.message, rows::BAD_ROWS);
    let too_many: Vec<Value> = (0..21).map(|i| draft(&format!("d{i}"))).collect();
    assert_eq!(refused(f.rows(json!(too_many))).message, rows::BAD_ROWS);
    assert!(f.plan.h.fake.calls().is_empty(), "nothing is read from GitHub");
}

// PPT-FR-KOCG: several refused updates make one refusal that names the place
// of each.
#[test]
fn several_refused_updates_make_one_refusal() {
    let f = Fixture::new();
    f.rows(json!([draft("a"), draft("b")])).unwrap();
    let before = f.proposals();
    let refusal = refused(f.call(json!({
        "rationale": RATIONALE,
        "updates": [
            { "action": "withdraw", "key": "z" },
            { "action": "change", "key": "a", "row": { "op": "new_change", "name": "", "body": "b" } },
            { "action": "explode", "key": "b" },
        ]
    })));
    assert_eq!(refusal.code, rows::ROWS_REFUSED);
    let lines: Vec<&str> = refusal.message.lines().skip(1).collect();
    assert_eq!(lines.len(), 3, "{}", refusal.message);
    assert!(lines[0].starts_with("Update 1 of 3: the key z names no row"));
    assert_eq!(lines[1], "Update 2 of 3: Give the new change a name.");
    assert!(lines[2].starts_with("Update 3 of 3: give `action` and `key`"));
    assert_eq!(f.proposals(), before);
}

// PPT-FR-DGAA: the WARN record of a **rows refused** refusal carries the count
// of refused rows.
#[test]
fn a_rows_refused_record_carries_the_count() {
    static BUFFER: crate::logging::LogBuffer = crate::logging::LogBuffer::new();
    let f = Fixture::new();
    let args = json!({ "rationale": RATIONALE, "rows": [create("a", ""), create("b", " ")] });
    refused(run(&f.tool().with_buffer(&BUFFER), args));
    let records = BUFFER
        .query(&LogFilter { min_level: LogLevel::Warn, ..LogFilter::default() }, None, 100)
        .unwrap()
        .records;
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].fields["reason"], rows::ROWS_REFUSED);
    assert_eq!(records[0].fields["rowsRefused"], 2);
}

// PPT-FR-MSQT: a card that waits, and the card of the turn, refuse the call
// before any row is checked.
#[test]
fn the_rules_of_the_turn_refuse_before_the_rows() {
    let f = Fixture::new();
    f.rows(json!([draft("a")])).unwrap();
    let open = refused(f.rows(json!([create("x", ""), create("y", " ")])));
    assert_eq!(open.code, "card_open");
    f.decide_all();
    f.in_one_turn();
    f.rows(json!([draft("b")])).unwrap();
    let carded = refused(f.rows(json!([create("x", ""), create("y", " ")])));
    assert_eq!(carded.code, "one_card_a_turn");
    let both = refused(
        f.call(json!({ "rationale": RATIONALE, "rows": [create("x", "")], "updates": [] })),
    );
    assert_eq!(both.code, "not_one_form");
}

// PPT-FR-ELJI, PPT-FR-UZEK: where every row breaks a rule of its own shape,
// GitHub is not read.
#[test]
fn rows_that_all_break_their_shape_read_nothing() {
    let f = Fixture::new();
    f.plan.h.fake.lock().calls.clear();
    let no_number = json!({ "key": "m", "op": "modify_issue", "title": "T" });
    let refusal = refused(f.rows(json!([create("x", ""), no_number])));
    assert_eq!(refusal.code, rows::ROWS_REFUSED);
    assert!(f.plan.h.fake.calls().is_empty(), "{:?}", f.plan.h.fake.calls());
}
