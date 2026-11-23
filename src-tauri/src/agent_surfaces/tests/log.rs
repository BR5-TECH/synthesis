//! The message log (AUI-FR-TYBX, AUI-FR-JYKT, AUI-FR-HOJP).

use serde_json::json;

use crate::agent_surfaces::log::{create_surface, fold};
use crate::agent_surfaces::*;

// AUI-FR-TYBX: the current state is the fold of the log: updateComponents
// replaces or adds by id, and updateDataModel sets or removes at its path.
#[test]
fn the_current_state_is_the_fold_of_the_log() {
    let messages = vec![
        create_surface(
            "s-1",
            &[
                json!({"id":"root","component":"Column","children":["a"]}),
                json!({"id":"a","component":"Text","text":"one"}),
            ],
            &json!({"n": 1, "keep": true}),
        ),
        json!({"version":"1.0","updateComponents":{"surfaceId":"s-1","components":[
            {"id":"a","component":"Text","text":"two"},
            {"id":"b","component":"Text","text":"new"}
        ]}}),
        json!({"version":"1.0","updateDataModel":{"surfaceId":"s-1","path":"/deep/x","value":5}}),
        json!({"version":"1.0","updateDataModel":{"surfaceId":"s-1","path":"/keep"}}),
    ];
    let state = fold(&messages);
    assert_eq!(state.components.len(), 3);
    assert_eq!(state.components[1]["text"], "two");
    assert_eq!(state.components[2]["id"], "b");
    assert_eq!(state.data_model, json!({"n": 1, "deep": {"x": 5}}));
    let replaced = fold(&[
        messages[0].clone(),
        json!({"version":"1.0","updateDataModel":{"surfaceId":"s-1","value":{"all":"new"}}}),
    ]);
    assert_eq!(replaced.data_model, json!({"all": "new"}));
}

// AUI-FR-TYBX: the first message names the surface and the catalog.
#[test]
fn the_log_starts_with_create_surface() {
    let message =
        create_surface("s-9", &[json!({"id":"root","component":"Text","text":"x"})], &json!({}));
    assert_eq!(message["version"], "1.0");
    assert_eq!(message["createSurface"]["surfaceId"], "s-9");
    assert_eq!(message["createSurface"]["catalogId"], CATALOG_ID);
}

// AUI-FR-JYKT / AUI-FR-HOJP: a record of the first shape and catalog reads as
// one createSurface of the new catalog, with the common properties renamed.
#[test]
fn a_record_of_the_first_shape_is_read_as_a_log() {
    let stored = json!({
        "id": "s-1",
        "discussionId": "t-1",
        "target": {"kind": "draft", "draftId": "d-1"},
        "commentId": "c-1",
        "catalogId": "synthesis:discussion/v1",
        "components": [
            {"id":"root","component":"Button","child":"l","action":{"event":{"name":"go"}},
             "@checks":[{"condition":{"@call":"required","args":{"value":{"@path":"/x"}}},"message":"Fill it."}],
             "@accessibility":{"label":"Go"}},
            {"id":"l","component":"Text","text":"Go"}
        ],
        "dataModel": {"x": ""},
        "authoredBy": {"kind": "agent", "agentId": "a", "handle": "arch"},
        "createdAt": "2026-01-01T00:00:00Z",
        "state": "open"
    });
    let surface: AgentSurface = serde_json::from_value(stored).expect("reads");
    assert_eq!(surface.catalog_id, CATALOG_ID);
    assert_eq!(surface.messages.len(), 1);
    let current = surface.current();
    assert_eq!(current.data_model, json!({"x": ""}));
    assert!(current.components[0].get("checks").is_some());
    assert!(current.components[0].get("accessibility").is_some());
    assert!(current.components[0].get("@checks").is_none());
    validate_surface(&current.components, &current.data_model).expect("valid in the new catalog");
    // It is written back in the new shape.
    let wire = serde_json::to_value(&surface).unwrap();
    assert!(wire.get("components").is_none());
    assert_eq!(wire["messages"][0]["createSurface"]["surfaceId"], "s-1");
}
