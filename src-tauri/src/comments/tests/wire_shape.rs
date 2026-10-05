//! The shape the frontend receives.

use super::*;

// -- Wire shape ---------------------------------------------------------

#[test]
fn the_wire_shape_is_the_one_the_contract_surface_declares() {
    let thread = Discussion {
        id: "t1".into(),
        target: artifact_target("specs/a.md"),
        fragment_target: Some(FragmentTarget::in_artifact("specs/a.md", 40, 61, "the first session")),
        comments: vec![Comment {
            id: "c1".into(),
            author: Participant::Human {
                login: "raver119".into(),
                display_name: Some("Demo Author".into()),
                email: None,
            },
            body: "hi".into(),
            quotes: vec![CommentQuote {
                comment_id: "c0".into(),
                excerpt: "e".into(),
            }],
            attachments: vec![
                Attachment::Blob {
                    digest: "a3f9".into(),
                    media_type: "image/png".into(),
                    filename: "diff.png".into(),
                    bytes: 81234,
                },
                Attachment::Url {
                    url: "https://example.test/spec.png".into(),
                    media_type: "image/png".into(),
                    label: Some("spec v2".into()),
                },
            ],
            created_at: "2026-01-01T00:00:00Z".into(),
        }],
        locked: false,
        resolved: true,
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-02T00:00:00Z".into(),
    };
    let json = serde_json::to_value(&thread).unwrap();
    assert_eq!(json["target"]["kind"], "artifact");
    assert_eq!(json["target"]["artifactId"], "specs/a.md");
    assert_eq!(json["fragmentTarget"]["owner"]["artifactId"], "specs/a.md");
    assert_eq!(json["fragmentTarget"]["path"], "specs/a.md");
    assert_eq!(json["fragmentTarget"]["start"], 40);
    assert_eq!(json["fragmentTarget"]["quote"], "the first session");
    for removed in ["scope", "kind", "artifactId", "draftId", "noteId", "anchor"] {
        assert!(json.get(removed).is_none(), "{removed} is not a field of the unified record");
    }
    assert_eq!(json["comments"][0]["author"]["kind"], "human");
    assert_eq!(json["comments"][0]["author"]["displayName"], "Demo Author");
    assert!(
        json["comments"][0]["author"].get("email").is_none(),
        "an absent email is absent on the wire, not null"
    );
    assert_eq!(json["comments"][0]["quotes"][0]["commentId"], "c0");
    assert_eq!(json["createdAt"], "2026-01-01T00:00:00Z");
    assert_eq!(json["resolved"], true);

    // A whole-target discussion carries an explicit null, never an absent field.
    let whole = Discussion {
        fragment_target: None,
        ..thread
    };
    let whole_json = serde_json::to_value(&whole).unwrap();
    assert!(whole_json["fragmentTarget"].is_null());
    assert!(whole_json.as_object().unwrap().contains_key("fragmentTarget"));

    let agent_json = serde_json::to_value(agent("claude_code", "claude")).unwrap();
    assert_eq!(agent_json["kind"], "agent");
    assert_eq!(agent_json["agentId"], "claude_code");
}

/// DQA-FR-KDVU, CMS-FR-GNTB: the field names an answer entry arrives under.
///
/// The three answer fields are optional, so a name the frontend and the backend
/// spell differently no longer fails to deserialize — it decodes to `None` and
/// reads as an incomplete set. The names are therefore pinned here, in the
/// camelCase the frontend sends (`../../../src/types/comments.ts`).
#[test]
fn an_answer_entry_decodes_from_the_names_the_frontend_sends() {
    let chosen: QuestionAnswer = serde_json::from_value(serde_json::json!({
        "questionPosition": 1,
        "optionPosition": 2,
        "optionValue": "two",
        "note": "a note",
    }))
    .unwrap();
    assert_eq!(chosen.question_position, 1);
    assert_eq!(chosen.option_position, Some(2));
    assert_eq!(chosen.option_value.as_deref(), Some("two"));
    assert_eq!(chosen.own_answer, None);
    assert_eq!(chosen.note.as_deref(), Some("a note"));

    let own: QuestionAnswer = serde_json::from_value(serde_json::json!({
        "questionPosition": 3,
        "ownAnswer": "three, one per layer",
    }))
    .unwrap();
    assert_eq!(own.question_position, 3);
    assert_eq!(own.option_position, None);
    assert_eq!(own.option_value, None);
    assert_eq!(own.own_answer.as_deref(), Some("three, one per layer"));
    assert_eq!(own.note, None);

    // And an absent field is absent on the wire rather than null, so a set the
    // author answered in their own words carries no empty option.
    let json = serde_json::to_value(&own).unwrap();
    assert_eq!(json["ownAnswer"], "three, one per layer");
    for absent in ["optionPosition", "optionValue", "note"] {
        assert!(json.get(absent).is_none(), "{absent} should be absent");
    }
}

#[test]
fn an_event_line_is_one_flat_json_object_carrying_its_type() {
    // The on-disk shape is the contract an agent writing lines would target,
    // so the key names are pinned here rather than left to serde's defaults.
    let event = Event {
        v: 1,
        event_id: "e1".into(),
        thread_id: "t1".into(),
        at: "2026-01-01T00:00:00Z".into(),
        by: human("raver119"),
        body: EventBody::ThreadOpened {
            artifact_path: "specs/a.md".into(),
            anchor: legacy_anchor(0, 3, "abc"),
        },
    };
    let line = serde_json::to_string(&event).unwrap();
    assert!(!line.contains('\n'), "one event is one line");
    let json: serde_json::Value = serde_json::from_str(&line).unwrap();
    assert_eq!(json["v"], 1);
    assert_eq!(json["eventId"], "e1");
    assert_eq!(json["threadId"], "t1");
    assert_eq!(json["type"], "thread_opened");
    assert_eq!(json["artifactPath"], "specs/a.md");
    assert_eq!(json["by"]["kind"], "human");

    // And it round-trips.
    let decoded: Event = serde_json::from_str(&line).unwrap();
    assert_eq!(decoded, event);
}
