//! The agent tool names and the argument aliases of
//! `PVC-product-vocabulary.md` (PVC-FR-RWDM, PVC-FR-SQAF).

use super::*;

// PVC-FR-RWDM: the agent tools carry exactly the names of the Tool names
// table.
#[test]
fn the_tools_carry_the_names_of_the_vocabulary() {
    let names = [
        (draft_search::NAME, "search_changes"),
        (draft_read::NAME, "read_change"),
        (crate::tools::propose_draft_changes::NAME, "propose_change_edits"),
        (crate::tools::decide_draft_changes::NAME, "decide_change_edits"),
        (crate::tools::propose_new_draft::NAME, "propose_new_change"),
        (read_graduation_file::NAME, "read_implementation_file"),
        (crate::tools::propose_change_edits::NAME, "propose_openspec_change_edits"),
        (crate::tools::decide_change_edits::NAME, "decide_openspec_change_edits"),
        (crate::tools::propose_new_change::NAME, "propose_new_openspec_change"),
        (crate::tools::request_change_action::NAME, "request_openspec_change_action"),
    ];
    for (name, expected) in names {
        assert_eq!(name, expected);
    }
}

// PVC-FR-RWDM: no tool definition of this group names a draft or a
// graduation, and a tool that acts on a change of a draft has no `openspec`.
#[test]
fn no_definition_carries_an_internal_word() {
    for definition in definitions() {
        let name = definition.name.as_str();
        assert!(!name.contains("draft"), "{name}");
        assert!(!name.contains("graduation"), "{name}");
    }
    for name in [
        draft_search::NAME,
        draft_read::NAME,
        crate::tools::propose_draft_changes::NAME,
        crate::tools::decide_draft_changes::NAME,
        crate::tools::propose_new_draft::NAME,
    ] {
        assert!(name.contains("change") && !name.contains("openspec"), "{name}");
    }
}

// PVC-FR-SQAF: `read_change` and `measure_document` take `change_id`, and
// still accept `draft_id` and the camelCase spellings.
#[test]
fn the_change_id_has_its_aliases() {
    for key in ["change_id", "draft_id", "changeId", "draftId"] {
        let read: draft_read::ReadDraftArgs =
            serde_json::from_value(serde_json::json!({ key: "d-1" })).expect(key);
        assert_eq!(read.draft_id, "d-1", "{key}");
        let measure: measure_document::MeasureDocumentArgs =
            serde_json::from_value(serde_json::json!({ key: "d-1" })).expect(key);
        assert_eq!(measure.draft_id.as_deref(), Some("d-1"), "{key}");
    }
}

// PVC-FR-SQAF: the schema that a model receives names `change_id` alone.
#[test]
fn the_schemas_name_change_id_alone() {
    for schema in [draft_read::parameters(), measure_document::parameters()] {
        let properties = schema["properties"].as_object().expect("an object");
        assert!(properties.contains_key("change_id"), "{schema}");
        for alias in ["draft_id", "draftId", "changeId"] {
            assert!(!properties.contains_key(alias), "{alias} in {schema}");
        }
        assert!(!schema.to_string().contains("draft_id"), "{schema}");
    }
}

// PVC-FR-CUAT: no description and no parameter description of a tool of this
// group calls a change a "draft", or an implementation a "graduation".
#[test]
fn no_definition_describes_itself_with_an_internal_word() {
    for definition in definitions() {
        for text in crate::prompts::vocabulary_tests::definition_texts(&definition) {
            let words = crate::prompts::vocabulary_tests::internal_words(&text);
            assert!(words.is_empty(), "{} uses {words:?} in {text:?}", definition.name);
        }
    }
}

/// The text of every string literal of `source`, outside its comments.
///
/// A small Rust lexer: it skips line and block comments, reads raw and normal
/// string literals, and tells a char literal from a lifetime, so that `'"'`
/// does not open a string.
fn string_literals(source: &str) -> Vec<String> {
    let chars: Vec<char> = source.chars().collect();
    let mut found = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '/' if chars.get(i + 1) == Some(&'/') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '/' if chars.get(i + 1) == Some(&'*') => {
                i += 2;
                while i + 1 < chars.len() && !(chars[i] == '*' && chars[i + 1] == '/') {
                    i += 1;
                }
                i += 2;
            }
            'r' if matches!(chars.get(i + 1), Some('"') | Some('#'))
                && (i == 0 || !(chars[i - 1].is_alphanumeric() || chars[i - 1] == '_')) =>
            {
                let mut j = i + 1;
                let mut hashes = 0;
                while chars.get(j) == Some(&'#') {
                    hashes += 1;
                    j += 1;
                }
                if chars.get(j) != Some(&'"') {
                    i += 1;
                    continue;
                }
                let start = j + 1;
                let mut end = start;
                while end < chars.len() {
                    if chars[end] == '"' && (1..=hashes).all(|k| chars.get(end + k) == Some(&'#')) {
                        break;
                    }
                    end += 1;
                }
                found.push(chars[start..end.min(chars.len())].iter().collect());
                i = end + 1 + hashes;
            }
            '"' => {
                let mut text = String::new();
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    if chars[i] == '\\' {
                        i += 1;
                    }
                    if let Some(c) = chars.get(i) {
                        text.push(*c);
                    }
                    i += 1;
                }
                found.push(text);
                i += 1;
            }
            '\'' => {
                // A char literal is `'x'` or an escape such as `'\''`. Any
                // other quote opens a lifetime or a label.
                if chars.get(i + 1) == Some(&'\\') {
                    i += 2;
                    while i < chars.len() && chars[i] != '\'' {
                        i += 1;
                    }
                    i += 1;
                } else if chars.get(i + 2) == Some(&'\'') {
                    i += 3;
                } else {
                    i += 1;
                }
            }
            _ => i += 1,
        }
    }
    found
}

/// Every Rust source file below `dir` that is not a test file.
fn production_sources(dir: &std::path::Path) -> Vec<(String, String)> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(dir).expect("a readable source folder") {
        let path = entry.expect("an entry").path();
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if path.is_dir() {
            if !(name == "tests" || name.ends_with("_tests")) {
                found.extend(production_sources(&path));
            }
        } else if name.ends_with(".rs") && name != "tests.rs" && !name.ends_with("_tests.rs") {
            let text = std::fs::read_to_string(&path).expect("a readable source file");
            // An inline test module ends the production part of a file.
            let production = text.split("\n#[cfg(test)]\nmod tests").next().unwrap_or_default();
            found.push((path.display().to_string(), production.to_string()));
        }
    }
    found
}

// PVC-FR-CUAT: no text of a tool or of the input of a conversation turn calls
// a change a "draft", or an implementation a "graduation". The check reads each
// string literal that holds a sentence: every refusal message, every
// description, and every line that a builder writes into a turn. A GitHub
// draft pull request and an identifier in backticks are allowed.
#[test]
fn no_refusal_and_no_turn_input_text_uses_an_internal_word() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = production_sources(&src.join("tools"));
    files.push(("tools.rs".into(), include_str!("../../tools.rs").into()));
    files.extend(production_sources(&src.join("agent_conversations")));
    assert!(files.iter().any(|(path, _)| path.ends_with("refusals.rs")), "the refusals are read");
    let mut sentences = 0;
    for (path, source) in &files {
        for literal in string_literals(source) {
            // A sentence holds a space between two words; a name, a key, or a
            // code holds none.
            if literal.split_whitespace().count() < 3 {
                continue;
            }
            sentences += 1;
            let words = crate::prompts::vocabulary_tests::internal_words(&literal);
            assert!(words.is_empty(), "{path} uses {words:?} in {literal:?}");
        }
    }
    assert!(sentences > 100, "the check read {sentences} sentences");
}

// PVC-FR-CUAT: the lexer reads a string literal, a raw string, and an escaped
// quote, and skips a comment, a char literal, and a lifetime.
#[test]
fn the_literal_scan_reads_strings_and_skips_comments() {
    let source = "// \"no\"\nfn f<'a>(x: &'a str) { let q = '\"'; let s = \"one \\\"two\\\"\"; \
                  /* \"no\" */ let r = r#\"raw \"x\"\"#; }";
    assert_eq!(string_literals(source), vec!["one \"two\"".to_string(), "raw \"x\"".to_string()]);
}

// PVC-FR-SQAF, TLC-FR-14: a call that gives both `change_id` and its alias
// `draft_id` names one argument twice. It does not decode, so the dispatcher
// refuses it with the retryable "arguments undecodable" refusal before the tool
// runs. Neither value wins, whatever the order of the keys.
#[test]
fn a_call_with_change_id_and_draft_id_does_not_decode() {
    for both in [
        serde_json::json!({ "change_id": "d-1", "draft_id": "d-2" }),
        serde_json::json!({ "draft_id": "d-2", "change_id": "d-1" }),
        serde_json::json!({ "changeId": "d-1", "draft_id": "d-2" }),
    ] {
        let read = serde_json::from_value::<draft_read::ReadDraftArgs>(both.clone());
        assert!(read.unwrap_err().to_string().contains("duplicate field"), "{both}");
        let measure = serde_json::from_value::<measure_document::MeasureDocumentArgs>(both.clone());
        assert!(measure.unwrap_err().to_string().contains("duplicate field"), "{both}");
    }
}
