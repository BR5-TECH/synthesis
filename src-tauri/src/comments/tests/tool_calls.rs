//! The tool list of a delivered agent answer (CMS-FR-HRDN).

use super::*;

fn call(tool: &str, key_input: Option<&str>, state: SavedToolState) -> SavedToolCall {
    SavedToolCall {
        tool: tool.into(),
        key_input: key_input.map(str::to_string),
        state,
        advisor: None,
    }
}

fn answer(
    root: &crate::fs::RootFs,
    thread: &str,
    id: &str,
    tool_calls: Vec<SavedToolCall>,
) -> Discussion {
    append_agent_answer_with(
        root,
        ThreadRef { scope: LogScope::Artifact, file_rel: "specs/a.md" },
        thread,
        id.into(),
        AnswerContent { body: "Done.".into(), attachments: Vec::new(), tool_calls },
        Stamp { by: &agent("a1", "arch"), at: "2026-01-02T00:00:00Z" },
    )
    .unwrap()
}

// CMS-FR-HRDN: the list is written in the append of the comment and folds back
// as it was written, advisor names included.
#[test]
fn a_tool_list_is_written_with_the_comment_and_folds_back() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let t = open(root, "specs/a.md", anchor(0, 3, "abc"), "@arch look", "2026-01-01T00:00:00Z");
    let mut advised = call("search_drafts", Some("query"), SavedToolState::Failed);
    advised.advisor = Some("sec".into());
    let updated = answer(
        root,
        &t.id,
        "c-answer",
        vec![call("read_file", Some("specs/a.md"), SavedToolState::Done), advised.clone()],
    );
    let saved = &updated.comments[1].tool_calls;
    assert_eq!(saved.len(), 2);
    assert_eq!(saved[0].tool, "read_file");
    assert_eq!(saved[0].advisor, None);
    assert_eq!(saved[1], advised);

    let raw = std::fs::read_to_string(log_path(root, "specs/a.md").unwrap()).unwrap();
    assert_eq!(raw.lines().count(), 2, "the comment and its list are one line");
    let line: serde_json::Value = serde_json::from_str(raw.lines().last().unwrap()).unwrap();
    assert_eq!(line["toolCalls"][1]["advisor"], "sec");
    assert_eq!(line["toolCalls"][0]["state"], "done");
    assert!(line["toolCalls"][0].get("advisor").is_none());
}

// CMS-FR-HRDN: a comment without a list writes no field, and a line without
// the field folds to an empty list.
#[test]
fn a_comment_without_a_tool_list_carries_no_field() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let t = open(root, "specs/a.md", anchor(0, 3, "abc"), "@arch look", "2026-01-01T00:00:00Z");
    let updated = answer(root, &t.id, "c-answer", Vec::new());
    assert!(updated.comments[1].tool_calls.is_empty());
    let raw = std::fs::read_to_string(log_path(root, "specs/a.md").unwrap()).unwrap();
    assert!(!raw.contains("toolCalls"));
    let wire = serde_json::to_value(&updated.comments[1]).unwrap();
    assert!(wire.get("toolCalls").is_none());
}

// CMS-FR-HRDN: a key input is cut at 200 characters.
#[test]
fn a_saved_key_input_is_cut_at_two_hundred_characters() {
    let dir = temp_root();
    let root = &crate::fs::RootFs::for_root(dir.path());
    let t = open(root, "specs/a.md", anchor(0, 3, "abc"), "@arch look", "2026-01-01T00:00:00Z");
    let long = "é".repeat(300);
    let updated =
        answer(root, &t.id, "c-answer", vec![call("web_search", Some(&long), SavedToolState::Done)]);
    let input = updated.comments[1].tool_calls[0].key_input.clone().unwrap();
    assert_eq!(input.chars().count(), SAVED_KEY_INPUT_LIMIT);
}
