//! The discussion catalog (AUI-FR-CSOS to AUI-FR-PYVB).

use serde_json::{json, Value};

use crate::agent_surfaces::catalog::{catalog, ANNOTATION_KEYWORDS, APPLIED_KEYWORDS, CATALOG_ID};

/// Every key of a schema object that is a keyword and not a property name.
fn keywords_of(schema: &Value, out: &mut Vec<String>) {
    match schema {
        Value::Object(map) => {
            for (key, value) in map {
                out.push(key.clone());
                match key.as_str() {
                    "properties" | "$defs" => {
                        if let Some(children) = value.as_object() {
                            children.values().for_each(|child| keywords_of(child, out));
                        }
                    }
                    "const" | "enum" | "required" | "description" | "returnType" | "title"
                    | "pattern" | "type" => {}
                    _ => keywords_of(value, out),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|item| keywords_of(item, out)),
        _ => {}
    }
}

// AUI-FR-CSOS: the catalog document parses, and it carries its id.
#[test]
fn the_catalog_parses_and_carries_its_id() {
    let document = catalog().document();
    assert_eq!(document["catalogId"], CATALOG_ID);
    assert_eq!(document["$id"], CATALOG_ID);
    assert_eq!(CATALOG_ID, "synthesis:discussion/v1");
}

// AUI-FR-PMXJ: the catalog declares exactly the components of the spec.
#[test]
fn the_catalog_declares_exactly_the_components_of_the_spec() {
    let mut names = catalog().component_names();
    names.sort_unstable();
    let mut expected = [
        "Row",
        "Column",
        "List",
        "Card",
        "Tabs",
        "Divider",
        "Text",
        "Icon",
        "Button",
        "TextField",
        "CheckBox",
        "ChoicePicker",
        "Slider",
        "DateTimeInput",
        "Table",
        "ChoiceCard",
        "ConfirmCard",
        "ComparisonCard",
        "ChecklistCard",
    ];
    expected.sort_unstable();
    assert_eq!(names, expected);
    for excluded in ["Modal", "Image", "Video", "AudioPlayer"] {
        assert!(catalog().component(excluded).is_none(), "{excluded} is not in the catalog");
    }
}

// AUI-FR-PXGX: the catalog declares exactly the functions of the spec.
#[test]
fn the_catalog_declares_exactly_the_functions_of_the_spec() {
    let functions = catalog().document()["functions"].as_object().expect("functions");
    let mut names: Vec<&str> = functions.keys().map(String::as_str).collect();
    names.sort_unstable();
    let mut expected = [
        "formatString",
        "formatNumber",
        "formatDate",
        "pluralize",
        "required",
        "length",
        "numeric",
        "and",
        "or",
        "not",
        "@index",
    ];
    expected.sort_unstable();
    assert_eq!(names, expected);
    for excluded in ["openUrl", "regex", "email", "formatCurrency"] {
        assert!(catalog().function(excluded).is_none(), "{excluded} is not in the catalog");
    }
}

// AUI-FR-PYVB: the catalog holds instructions for a model.
#[test]
fn the_catalog_holds_instructions() {
    let instructions = catalog().instructions();
    assert!(instructions.contains("root"));
    assert!(instructions.contains("@path"));
    // AUI-FR-PYVB: short, and naming the skill that holds the rest.
    assert!(instructions.contains("`a2ui` skill"));
    assert!(instructions.len() < 1_500, "{} characters", instructions.len());
}

// AUI-FR-EGAB: the schema reader applies every keyword the catalog uses, so
// no rule of the catalog is ignored.
#[test]
fn the_catalog_uses_only_keywords_the_reader_applies() {
    let mut keywords = Vec::new();
    let document = catalog().document();
    for section in ["components", "functions", "$defs"] {
        for schema in document[section].as_object().expect("section").values() {
            keywords_of(schema, &mut keywords);
        }
    }
    for keyword in keywords {
        assert!(
            APPLIED_KEYWORDS.contains(&keyword.as_str())
                || ANNOTATION_KEYWORDS.contains(&keyword.as_str()),
            "the reader does not apply `{keyword}`"
        );
    }
}

// AUI-FR-EGAB: every component carries the common properties and refuses an
// undeclared one.
#[test]
fn every_component_is_closed_and_names_its_type() {
    for name in catalog().component_names() {
        let schema = catalog().component(name).expect("schema");
        assert_eq!(schema["additionalProperties"], json!(false), "{name}");
        assert_eq!(schema["properties"]["component"]["const"], json!(name));
        let required = schema["required"].as_array().expect("required");
        assert!(required.contains(&json!("id")) && required.contains(&json!("component")));
    }
}
