//! The discussion catalog (`AUI-agent-ui-surfaces.md` AUI-FR-CSOS to
//! AUI-FR-PYVB, AUI-FR-EGAB).
//!
//! The catalog is one JSON document compiled into the build: the official
//! A2UI v1.0 basic catalog of the pinned `@a2ui/web_core` package, with its
//! common types inlined, and the extension components of the application
//! (`tools/a2ui/generate_catalog.py` writes it). A component and a function
//! are checked against their schemas in it with a JSON Schema 2020-12
//! validator. The validator resolves no address: every reference points into
//! the one document.

use std::collections::HashMap;
use std::sync::OnceLock;

use jsonschema::error::ValidationErrorKind;
use serde_json::{json, Map, Value};

/// AUI-FR-CSOS: the catalog document, as it is in the resources.
pub const CATALOG_JSON: &str = include_str!("../../../resources/a2ui/discussion-catalog.json");

/// AUI-FR-CSOS: the catalog id.
pub const CATALOG_ID: &str = "synthesis:a2ui/v1";

/// AUI-FR-HOJP: the id of the first catalog, read as this one.
pub const FIRST_CATALOG_ID: &str = "synthesis:discussion/v1";

/// The parsed catalog and its compiled validators.
pub struct Catalog {
    root: Value,
    components: HashMap<String, jsonschema::Validator>,
    functions: HashMap<String, jsonschema::Validator>,
}

/// The catalog, parsed and compiled once.
pub fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| {
        // The document is a resource of the build, and a test parses it.
        let root: Value = serde_json::from_str(CATALOG_JSON).unwrap_or(Value::Null);
        let components = compile_each(&root, "components", "ComponentCommon");
        let functions = compile_each(&root, "functions", "FunctionCommon");
        Catalog { root, components, functions }
    })
}

/// One validator for each entry of `section`: the common part and the
/// entry's own schema, with no property that neither declares. The schema of
/// each is the catalog document with that pair at its root, so every `#/…`
/// reference resolves inside the document.
fn compile_each(
    root: &Value,
    section: &str,
    common: &str,
) -> HashMap<String, jsonschema::Validator> {
    let mut out = HashMap::new();
    let Some(names) = root.get(section).and_then(Value::as_object) else {
        return out;
    };
    for name in names.keys() {
        let mut schema = Map::new();
        for key in ["$defs", "components", "functions"] {
            if let Some(part) = root.get(key) {
                schema.insert(key.to_string(), part.clone());
            }
        }
        schema.insert("$schema".into(), json!("https://json-schema.org/draft/2020-12/schema"));
        schema.insert(
            "allOf".into(),
            json!([
                { "$ref": format!("#/$defs/{common}") },
                { "$ref": format!("#/{section}/{name}") },
            ]),
        );
        schema.insert("unevaluatedProperties".into(), json!(false));
        if let Ok(validator) = jsonschema::options()
            .with_draft(jsonschema::Draft::Draft202012)
            .build(&Value::Object(schema))
        {
            out.insert(name.clone(), validator);
        }
    }
    out
}

/// What kind of failure a [`SchemaError`] is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchemaFailure {
    /// A required property is absent.
    Missing,
    /// A property that the schema does not declare is present.
    Unknown,
    /// A value does not have the form that the schema declares.
    Invalid,
}

/// One failure of a value against a schema: what failed and where.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaError {
    pub failure: SchemaFailure,
    /// The property path inside the checked value, such as `action.event.name`.
    pub path: Vec<String>,
    /// What a valid value looks like, in one sentence.
    pub expected: String,
}

/// The most characters of a validator message in a refusal.
const EXPECTED_MAX_CHARS: usize = 240;

impl Catalog {
    /// AUI-FR-PYVB: the instructions for a model.
    pub fn instructions(&self) -> &str {
        self.root.get("instructions").and_then(Value::as_str).unwrap_or("")
    }

    /// AUI-FR-PMXJ: the names of the components, in catalog order.
    pub fn component_names(&self) -> Vec<&str> {
        self.section("components")
            .map(|m| m.keys().map(String::as_str).collect())
            .unwrap_or_default()
    }

    /// The schema of one component.
    pub fn component(&self, name: &str) -> Option<&Value> {
        self.section("components").and_then(|m| m.get(name))
    }

    /// AUI-FR-PXGX: the schema of one function.
    pub fn function(&self, name: &str) -> Option<&Value> {
        self.section("functions").and_then(|m| m.get(name))
    }

    /// The `$defs` of the catalog.
    pub fn defs(&self) -> Option<&Map<String, Value>> {
        self.section("$defs")
    }

    /// The whole document.
    pub fn document(&self) -> &Value {
        &self.root
    }

    fn section(&self, key: &str) -> Option<&Map<String, Value>> {
        self.root.get(key).and_then(Value::as_object)
    }

    /// Whether every entry of the catalog has a compiled validator.
    pub fn compiled(&self) -> bool {
        let count = |key: &str| self.section(key).map(Map::len).unwrap_or(0);
        self.components.len() == count("components") && self.functions.len() == count("functions")
    }

    /// AUI-FR-EGAB: check one component against its schema in the catalog.
    pub fn check_component(&self, name: &str, value: &Value) -> Result<(), SchemaError> {
        check_with(self.components.get(name), value)
    }

    /// AUI-FR-HCCU: check one function call against its schema.
    pub fn check_function(&self, name: &str, value: &Value) -> Result<(), SchemaError> {
        check_with(self.functions.get(name), value)
    }
}

fn check_with(validator: Option<&jsonschema::Validator>, value: &Value) -> Result<(), SchemaError> {
    let Some(validator) = validator else {
        return Err(SchemaError {
            failure: SchemaFailure::Invalid,
            path: Vec::new(),
            expected: "an entry that the catalog declares".into(),
        });
    };
    // The deepest error names the property most exactly. An unevaluated
    // property is reported only where nothing else failed: a part that fails
    // leaves its own properties unevaluated as well.
    let rank = |e: &jsonschema::ValidationError<'_>| {
        let depth = e.instance_path().to_string().matches('/').count();
        let primary = !matches!(e.kind(), ValidationErrorKind::UnevaluatedProperties { .. });
        (primary, depth)
    };
    let error = validator.iter_errors(value).max_by_key(|e| rank(e));
    let Some(error) = error else {
        return Ok(());
    };
    let mut path: Vec<String> = error
        .instance_path()
        .to_string()
        .split('/')
        .skip(1)
        .map(|token| token.replace("~1", "/").replace("~0", "~"))
        .collect();
    let failure = match error.kind() {
        ValidationErrorKind::Required { property } => {
            if let Some(name) = property.as_str() {
                path.push(name.to_string());
            }
            SchemaFailure::Missing
        }
        ValidationErrorKind::AdditionalProperties { unexpected }
        | ValidationErrorKind::UnevaluatedProperties { unexpected } => {
            if let Some(name) = unexpected.first() {
                path.push(name.clone());
            }
            SchemaFailure::Unknown
        }
        _ => SchemaFailure::Invalid,
    };
    let expected = match failure {
        SchemaFailure::Missing => "a value here: this property is required".to_string(),
        SchemaFailure::Unknown => "no such property in the catalog".to_string(),
        SchemaFailure::Invalid => error.to_string().chars().take(EXPECTED_MAX_CHARS).collect(),
    };
    Err(SchemaError { failure, path, expected })
}
