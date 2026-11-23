//! The actions of the cards (AUI-FR-KBVY, AUI-FR-XCXV, AUI-FR-WCRS,
//! AUI-FR-KJPZ).

use serde_json::{json, Value};

use super::validate::{checklist_card, choice_card, comparison_card, confirm_card};
use crate::agent_surfaces::actions::accept;
use crate::agent_surfaces::*;
use crate::comments::{DiscussionTarget, Participant};

fn surface_of(card: Value) -> AgentSurface {
    let mut card = card;
    card["id"] = json!("root");
    AgentSurface {
        id: "s-1".into(),
        discussion_id: "t-1".into(),
        target: DiscussionTarget::Draft { draft_id: "d-1".into() },
        comment_id: "c-1".into(),
        catalog_id: CATALOG_ID.into(),
        components: vec![card],
        data_model: json!({}),
        authored_by: Participant::Agent {
            agent_id: "a".into(),
            handle: "arch".into(),
            model: None,
            title: None,
        },
        created_at: "2026-01-01T00:00:00Z".into(),
        state: SurfaceState::Open,
        submission: None,
    }
}

fn action(name: &str, context: Value) -> SurfaceAction {
    SurfaceAction {
        name: name.into(),
        source_component_id: "root".into(),
        context,
        data_model: json!({}),
    }
}

// AUI-FR-XCXV / AUI-FR-WCRS: a choice sends option values, at most one where it
// is not multiple, and the body names the options by their labels.
#[test]
fn a_choice_card_sends_option_values() {
    let surface = surface_of(choice_card());
    assert_eq!(
        accept(&surface, &action("choose", json!({"selected": ["b"]}))).unwrap(),
        "Sent “Send”.\n- selected: B"
    );
    assert_eq!(
        accept(&surface, &action("choose", json!({"selected": [], "other": "Neither, use C."})))
            .unwrap(),
        "Sent “Send”.\n- selected: none\n- other: Neither, use C."
    );
    for bad in [
        json!({"selected": ["a", "b"]}),
        json!({"selected": ["z"]}),
        json!({"selected": []}),
        json!({"selected": [], "other": "  "}),
        json!({"selected": ["a"], "extra": 1}),
        json!({"other": "x"}),
        json!({"selected": "a"}),
    ] {
        assert_eq!(
            accept(&surface, &action("choose", bad.clone())).unwrap_err(),
            ERR_INVALID_CONTEXT,
            "{bad}"
        );
    }
    let mut multiple = choice_card();
    multiple["multiple"] = json!(true);
    multiple["allowOther"] = json!(false);
    multiple["sendLabel"] = json!("Use these");
    let surface = surface_of(multiple);
    assert_eq!(
        accept(&surface, &action("choose", json!({"selected": ["a", "b"]}))).unwrap(),
        "Sent “Use these”.\n- selected: A, B"
    );
    assert_eq!(
        accept(&surface, &action("choose", json!({"selected": ["a"], "other": "x"}))).unwrap_err(),
        ERR_INVALID_CONTEXT,
        "no own answer without allowOther"
    );
    assert_eq!(
        accept(&surface, &action("choose", json!({"selected": ["a", "a"]}))).unwrap_err(),
        ERR_INVALID_CONTEXT
    );
}

// AUI-FR-XCXV / AUI-FR-WCRS / AUI-FR-KBVY: a confirmation sends either of its
// two names with no values, and the body names the pressed button.
#[test]
fn a_confirm_card_sends_either_name() {
    let surface = surface_of(confirm_card());
    assert_eq!(accept(&surface, &action("go", json!({}))).unwrap(), "Sent “Go”.");
    assert_eq!(accept(&surface, &action("stop", json!({}))).unwrap(), "Sent “Stop”.");
    assert_eq!(accept(&surface, &action("other", json!({}))).unwrap_err(), ERR_UNKNOWN_ACTION);
    assert_eq!(accept(&surface, &action("go", json!({"x": 1}))).unwrap_err(), ERR_INVALID_CONTEXT);
}

// AUI-FR-XCXV / AUI-FR-WCRS: a comparison sends one option value.
#[test]
fn a_comparison_card_sends_one_pick() {
    let surface = surface_of(comparison_card());
    assert_eq!(
        accept(&surface, &action("pick_one", json!({"picked": "y"}))).unwrap(),
        "Sent “Pick”.\n- picked: Y"
    );
    assert_eq!(
        accept(&surface, &action("pick_one", json!({"picked": "z"}))).unwrap_err(),
        ERR_INVALID_CONTEXT
    );
    assert_eq!(accept(&surface, &action("pick_one", json!({}))).unwrap_err(), ERR_INVALID_CONTEXT);
    let mut no_pick = comparison_card();
    no_pick.as_object_mut().unwrap().remove("pick");
    assert_eq!(
        accept(&surface_of(no_pick), &action("pick_one", json!({"picked": "x"}))).unwrap_err(),
        ERR_UNKNOWN_ACTION
    );
}

// AUI-FR-XCXV / AUI-FR-WCRS: a checklist sends its checked values, also none.
#[test]
fn a_checklist_card_sends_the_checked_values() {
    let surface = surface_of(checklist_card());
    assert_eq!(
        accept(&surface, &action("keep", json!({"checked": ["q", "p"]}))).unwrap(),
        "Sent “Send”.\n- checked: Q, P"
    );
    assert_eq!(
        accept(&surface, &action("keep", json!({"checked": []}))).unwrap(),
        "Sent “Send”.\n- checked: none"
    );
    assert_eq!(
        accept(&surface, &action("keep", json!({"checked": ["r"]}))).unwrap_err(),
        ERR_INVALID_CONTEXT
    );
}

// AUI-FR-KBVY: an action names its card, and a card of another id is unknown.
#[test]
fn an_action_names_its_card() {
    let surface = surface_of(checklist_card());
    let mut elsewhere = action("keep", json!({"checked": []}));
    elsewhere.source_component_id = "other".into();
    assert_eq!(accept(&surface, &elsewhere).unwrap_err(), ERR_UNKNOWN_ACTION);
}

// AUI-FR-KJPZ: every action is bounded, the card actions too.
#[test]
fn a_card_action_is_bounded() {
    let surface = surface_of(choice_card());
    let mut big = action("choose", json!({"selected": [], "other": "x"}));
    big.data_model = json!({"big": "a".repeat(32 * 1024)});
    assert_eq!(accept(&surface, &big).unwrap_err(), ERR_INVALID_CONTEXT);
    let mut not_object = action("choose", json!({"selected": ["a"]}));
    not_object.data_model = json!([]);
    assert_eq!(accept(&surface, &not_object).unwrap_err(), ERR_INVALID_CONTEXT);
}
