//! The discussion catalog (AUI-FR-CSOS to AUI-FR-PYVB, AUI-FR-MAMC).

use serde_json::{json, Value};

use crate::agent_surfaces::catalog::{catalog, CATALOG_ID};

/// AUI-FR-PMXJ: the components of the official A2UI v1.0 basic catalog.
const BASIC_COMPONENTS: [&str; 18] = [
    "Text",
    "Image",
    "Icon",
    "Video",
    "AudioPlayer",
    "Row",
    "Column",
    "List",
    "Card",
    "Tabs",
    "Modal",
    "Divider",
    "Button",
    "TextField",
    "CheckBox",
    "ChoicePicker",
    "Slider",
    "DateTimeInput",
];

/// AUI-FR-PMXJ: the extension components of the application.
const EXTENSIONS: [&str; 5] =
    ["Table", "ChoiceCard", "ConfirmCard", "ComparisonCard", "ChecklistCard"];

/// AUI-FR-PXGX: the functions of the official A2UI v1.0 basic catalog.
const BASIC_FUNCTIONS: [&str; 14] = [
    "required",
    "regex",
    "length",
    "numeric",
    "email",
    "formatString",
    "formatNumber",
    "formatCurrency",
    "formatDate",
    "pluralize",
    "openUrl",
    "and",
    "or",
    "not",
];

/// Every `$ref` under `value`.
fn refs_of(value: &Value, out: &mut Vec<String>) {
    match value {
        Value::Object(map) => {
            if let Some(Value::String(r)) = map.get("$ref") {
                out.push(r.clone());
            }
            map.values().for_each(|v| refs_of(v, out));
        }
        Value::Array(items) => items.iter().for_each(|v| refs_of(v, out)),
        _ => {}
    }
}

// AUI-FR-CSOS: the catalog document parses, carries its id, names the official
// catalog it is based on, and compiles a validator for every entry.
#[test]
fn the_catalog_parses_and_compiles() {
    let document = catalog().document();
    assert_eq!(document["catalogId"], CATALOG_ID);
    assert_eq!(document["$id"], CATALOG_ID);
    assert_eq!(CATALOG_ID, "synthesis:a2ui/v1");
    assert_eq!(
        document["basedOn"],
        "https://a2ui.org/specification/v1_0/catalogs/basic/catalog.json"
    );
    assert!(catalog().compiled(), "every component and function has a validator");
}

// AUI-FR-CSOS: every reference points into the one document, so validation
// reaches no address.
#[test]
fn every_reference_is_local_and_resolves() {
    let document = catalog().document();
    let mut refs = Vec::new();
    refs_of(document, &mut refs);
    assert!(!refs.is_empty());
    for r in refs {
        let pointer = r.strip_prefix('#').unwrap_or_else(|| panic!("{r} is not local"));
        assert!(document.pointer(pointer).is_some(), "{r} resolves");
    }
}

// AUI-FR-PMXJ: the official basic components and the extensions, and no other.
#[test]
fn the_catalog_declares_exactly_the_components_of_the_spec() {
    let mut names = catalog().component_names();
    names.sort_unstable();
    let mut expected: Vec<&str> =
        BASIC_COMPONENTS.iter().chain(EXTENSIONS.iter()).copied().collect();
    expected.sort_unstable();
    assert_eq!(names, expected);
}

// AUI-FR-PXGX: the official basic functions, and no other.
#[test]
fn the_catalog_declares_exactly_the_functions_of_the_spec() {
    let functions = catalog().document()["functions"].as_object().expect("functions");
    let mut names: Vec<&str> = functions.keys().map(String::as_str).collect();
    names.sort_unstable();
    let mut expected = BASIC_FUNCTIONS;
    expected.sort_unstable();
    assert_eq!(names, expected);
}

// AUI-FR-MAMC: the TextField has no `obscured` variant, and keeps the others.
#[test]
fn the_text_field_has_no_obscured_variant() {
    let text = catalog().component("TextField").expect("TextField").to_string();
    assert!(!text.contains("obscured"));
    for variant in ["shortText", "longText", "number"] {
        assert!(text.contains(variant), "{variant}");
    }
}

// AUI-FR-PYVB: short instructions that name the skill with the rest.
#[test]
fn the_catalog_holds_instructions() {
    let instructions = catalog().instructions();
    assert!(instructions.contains("root"));
    assert!(instructions.contains("@path"));
    assert!(instructions.contains("`a2ui` skill"));
    assert!(instructions.contains("project:"));
    assert!(instructions.len() < 1_800, "{} characters", instructions.len());
}

// AUI-FR-EGAB: the official component shape is checked: the common `id` and
// `accessibility`, the official property names, and nothing else.
#[test]
fn a_component_is_checked_against_its_official_schema() {
    let c = catalog();
    c.check_component(
        "Text",
        &json!({"id":"t","component":"Text","text":"Hi","accessibility":{"label":"Hello"}}),
    )
    .expect("valid");
    let missing = c.check_component("Text", &json!({"id":"t","component":"Text"})).unwrap_err();
    assert_eq!(missing.path, ["text"]);
    let unknown = c
        .check_component("Text", &json!({"id":"t","component":"Text","text":"x","html":"<b>"}))
        .unwrap_err();
    assert_eq!(unknown.path, ["html"]);
    c.check_component(
        "Image",
        &json!({"id":"i","component":"Image","url":"https://example.com/a.png"}),
    )
    .expect("an image");
    c.check_component(
        "Modal",
        &json!({"id":"m","component":"Modal","trigger":"open","content":"body"}),
    )
    .expect("a modal");
}
