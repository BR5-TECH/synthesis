//! The key input of a tool call (CVL-FR-DUUO, CVL-FR-IQZS).
//!
//! The one argument that identifies a call, so that a surface can show what the
//! agent searched for or read. It goes into the turn record alone: no log
//! record carries it (AGC-FR-SEZK).

/// CVL-FR-IQZS: the longest key input a report carries, in characters.
pub(super) const KEY_INPUT_MAX_CHARS: usize = 200;

/// CVL-FR-DUUO: the arguments that identify a call of each tool, in the order
/// they are tried. A tool that this table does not name has no key input.
fn key_arguments(tool: &str) -> &'static [&'static str] {
    match tool {
        "search_specifications"
        | "search_drafts"
        | "search_notes"
        | "search_documents"
        | "search_skills" => &["query"],
        "read_file" => &["path"],
        "measure_document" => &["path", "draft_id"],
        "read_draft" => &["draft_id"],
        "get_document" => &["id"],
        "load_skill" => &["name"],
        "propose_draft_changes" | "propose_prompt_changes" => &["path"],
        crate::tools::web_search::ENTRY_TYPE => &["query"],
        crate::tools::web_fetch::ENTRY_TYPE => &["url"],
        _ => &[],
    }
}

/// CVL-FR-DUUO: the key input of a call of `tool` with these arguments, or
/// `None` where the tool has none or the argument is absent, empty, or not text.
pub(super) fn key_input_of(tool: &str, arguments: &serde_json::Value) -> Option<String> {
    key_arguments(tool)
        .iter()
        .filter_map(|name| arguments.get(*name).and_then(serde_json::Value::as_str))
        .map(str::trim)
        .find(|value| !value.is_empty())
        .map(cut_key_input)
}

/// CVL-FR-DUUO: the same, for a provider-native call, whose arguments the
/// provider reports as JSON text. Text that is not a JSON object has no key
/// input.
pub(super) fn native_key_input_of(tool: &str, arguments: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(arguments).ok()?;
    key_input_of(tool, &value)
}

/// CVL-FR-IQZS: cut a trimmed value at 200 characters, counted by character. A
/// cut value ends in `…`.
pub(super) fn cut_key_input(value: &str) -> String {
    let value = value.trim();
    if value.chars().count() <= KEY_INPUT_MAX_CHARS {
        return value.to_string();
    }
    let mut cut: String = value.chars().take(KEY_INPUT_MAX_CHARS - 1).collect();
    cut.push('…');
    cut
}

#[cfg(test)]
mod tests {
    use super::*;

    // CVL-FR-DUUO: each tool of the table gives the argument the table names.
    #[test]
    fn each_tool_gives_the_argument_that_identifies_its_call() {
        let cases = [
            ("search_specifications", serde_json::json!({ "query": "tabs", "limit": 3 }), "tabs"),
            ("search_drafts", serde_json::json!({ "query": "drafts" }), "drafts"),
            ("search_notes", serde_json::json!({ "query": "notes" }), "notes"),
            ("search_documents", serde_json::json!({ "query": "docs" }), "docs"),
            ("search_skills", serde_json::json!({ "query": "skills" }), "skills"),
            ("read_file", serde_json::json!({ "path": "src/a.rs" }), "src/a.rs"),
            ("measure_document", serde_json::json!({ "path": "doc.md" }), "doc.md"),
            ("measure_document", serde_json::json!({ "draft_id": "d-1" }), "d-1"),
            ("read_draft", serde_json::json!({ "draft_id": "d-2" }), "d-2"),
            ("get_document", serde_json::json!({ "id": "doc-9" }), "doc-9"),
            ("load_skill", serde_json::json!({ "name": "engineer" }), "engineer"),
            ("propose_draft_changes", serde_json::json!({ "path": "p.md" }), "p.md"),
            ("propose_prompt_changes", serde_json::json!({ "path": "q.md" }), "q.md"),
        ];
        for (tool, arguments, expected) in cases {
            assert_eq!(key_input_of(tool, &arguments).as_deref(), Some(expected), "{tool}");
        }
        assert_eq!(
            native_key_input_of("openrouter:web_search", r#"{"query":"rust 1.95"}"#).as_deref(),
            Some("rust 1.95"),
        );
        assert_eq!(
            native_key_input_of("openrouter:web_fetch", r#"{"url":"https://example.com/a"}"#)
                .as_deref(),
            Some("https://example.com/a"),
        );
    }

    // CVL-FR-DUUO: a tool the table does not name, and an argument that is
    // absent, empty, or not text, give no key input.
    #[test]
    fn a_call_with_nothing_to_identify_it_has_no_key_input() {
        assert_eq!(key_input_of("list_skills", &serde_json::json!({})), None);
        assert_eq!(key_input_of("ask_user_comment", &serde_json::json!({ "question": "q" })), None);
        assert_eq!(key_input_of("read_file", &serde_json::json!({})), None);
        assert_eq!(key_input_of("read_file", &serde_json::json!({ "path": "   " })), None);
        assert_eq!(key_input_of("read_file", &serde_json::json!({ "path": 7 })), None);
        assert_eq!(native_key_input_of("openrouter:web_search", "not json"), None);
    }

    // CVL-FR-IQZS: the value is trimmed and cut at 200 characters, counted by
    // character, and a cut value ends in an ellipsis.
    #[test]
    fn a_long_key_input_is_cut_at_two_hundred_characters() {
        let exact = "é".repeat(KEY_INPUT_MAX_CHARS);
        assert_eq!(cut_key_input(&exact), exact);
        let long = format!("  {}  ", "ж".repeat(KEY_INPUT_MAX_CHARS + 50));
        let cut = cut_key_input(&long);
        assert_eq!(cut.chars().count(), KEY_INPUT_MAX_CHARS);
        assert!(cut.ends_with('…'));
        assert!(cut.starts_with('ж'));
        assert_eq!(
            key_input_of("search_notes", &serde_json::json!({ "query": "  padded  " })).as_deref(),
            Some("padded"),
        );
    }
}
