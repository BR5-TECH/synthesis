//! The discussion catalog (`AUI-agent-ui-surfaces.md` AUI-FR-CSOS to
//! AUI-FR-PYVB) and the subset of JSON Schema that it is written in.
//!
//! The catalog is one JSON document compiled into the build. The frontend
//! renders from the same file, so the two sides can not declare different
//! components. The schema reader below knows only the keywords that the
//! catalog uses. A test walks the catalog and fails on any other keyword, so
//! no keyword is ever ignored by accident.

use std::sync::OnceLock;

use serde_json::{Map, Value};

/// AUI-FR-CSOS: the catalog document, as it is in the resources.
pub const CATALOG_JSON: &str = include_str!("../../../resources/a2ui/discussion-catalog.json");

/// AUI-FR-CSOS: the catalog id.
pub const CATALOG_ID: &str = "synthesis:discussion/v1";

/// The keywords that the schema reader applies.
#[cfg(test)]
pub(crate) const APPLIED_KEYWORDS: [&str; 16] = [
    "$ref",
    "type",
    "enum",
    "const",
    "properties",
    "required",
    "additionalProperties",
    "items",
    "minItems",
    "maxItems",
    "minimum",
    "minLength",
    "maxLength",
    "pattern",
    "oneOf",
    "allOf",
];

/// The keywords that only describe, and that the reader does not apply.
#[cfg(test)]
pub(crate) const ANNOTATION_KEYWORDS: [&str; 3] = ["description", "returnType", "title"];

/// The parsed catalog.
pub struct Catalog {
    root: Value,
}

/// The catalog, parsed once.
pub fn catalog() -> &'static Catalog {
    static CATALOG: OnceLock<Catalog> = OnceLock::new();
    CATALOG.get_or_init(|| Catalog {
        // The document is a resource of the build, and a test parses it.
        root: serde_json::from_str(CATALOG_JSON).unwrap_or(Value::Null),
    })
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

    /// The whole document, for the tool's parameter schema (SSF-FR-FKWS).
    pub fn document(&self) -> &Value {
        &self.root
    }

    fn section(&self, key: &str) -> Option<&Map<String, Value>> {
        self.root.get(key).and_then(Value::as_object)
    }

    /// Check `value` against `schema`. The first failure is returned.
    pub fn check(&self, schema: &Value, value: &Value) -> Result<(), SchemaError> {
        let mut path = Vec::new();
        self.check_at(schema, value, &mut path, 0)
    }

    fn resolve<'a>(&'a self, schema: &'a Value) -> &'a Value {
        let mut current = schema;
        // A reference names a `$defs` entry. A chain of them ends quickly, and
        // the bound keeps a broken document from looping.
        for _ in 0..16 {
            let Some(reference) = current.get("$ref").and_then(Value::as_str) else {
                return current;
            };
            let name = reference.strip_prefix("#/$defs/").unwrap_or(reference);
            match self.defs().and_then(|defs| defs.get(name)) {
                Some(next) => current = next,
                None => return &Value::Null,
            }
        }
        current
    }

    fn check_at(
        &self,
        schema: &Value,
        value: &Value,
        path: &mut Vec<String>,
        depth: usize,
    ) -> Result<(), SchemaError> {
        if depth > 64 {
            return Err(fail(path, "a value that is not nested so deeply"));
        }
        let schema = self.resolve(schema);
        let Some(rules) = schema.as_object() else {
            return Err(fail(path, "a value that the catalog declares"));
        };
        if let Some(options) = rules.get("oneOf").and_then(Value::as_array) {
            return self.check_one_of(options, value, path, depth);
        }
        if let Some(parts) = rules.get("allOf").and_then(Value::as_array) {
            for part in parts {
                self.check_at(part, value, path, depth + 1)?;
            }
        }
        if let Some(expected) = rules.get("const") {
            if value != expected {
                return Err(fail(path, &format!("the value {expected}")));
            }
        }
        if let Some(options) = rules.get("enum").and_then(Value::as_array) {
            if !options.contains(value) {
                return Err(fail(path, &format!("one of {}", list(options))));
            }
        }
        if let Some(kind) = rules.get("type") {
            if !type_matches(kind, value) {
                return Err(fail(path, &format!("a value of type {}", type_name(kind))));
            }
        }
        check_bounds(rules, value, path)?;
        if let Some(object) = value.as_object() {
            self.check_object(rules, object, path, depth)?;
        }
        if let (Some(items), Some(array)) = (rules.get("items"), value.as_array()) {
            for (index, item) in array.iter().enumerate() {
                path.push(index.to_string());
                self.check_at(items, item, path, depth + 1)?;
                path.pop();
            }
        }
        Ok(())
    }

    fn check_one_of(
        &self,
        options: &[Value],
        value: &Value,
        path: &mut Vec<String>,
        depth: usize,
    ) -> Result<(), SchemaError> {
        // An object that carries `@path` or `@call` is meant as a binding or a
        // call, so its own failure is the useful one to report.
        let mut nearest = None;
        for option in options {
            match self.check_at(option, value, path, depth + 1) {
                Ok(()) => return Ok(()),
                Err(error) => {
                    let meant = self.meant_as(option, value);
                    if meant || nearest.is_none() {
                        nearest = Some((meant, error));
                    }
                    if meant {
                        break;
                    }
                }
            }
        }
        match nearest {
            Some((true, error)) => Err(error),
            _ => Err(fail(path, &self.forms(options))),
        }
    }

    /// Whether `value` was meant as `option`: a binding or a call it names.
    fn meant_as(&self, option: &Value, value: &Value) -> bool {
        let Some(object) = value.as_object() else {
            return false;
        };
        let required = self.resolve(option).get("required").and_then(Value::as_array);
        let Some(required) = required else {
            return false;
        };
        required
            .iter()
            .filter_map(Value::as_str)
            .any(|key| key.starts_with('@') && object.contains_key(key))
    }

    /// The forms of a `oneOf`, for a refusal.
    fn forms(&self, options: &[Value]) -> String {
        let names: Vec<String> = options
            .iter()
            .map(|option| match option.get("$ref").and_then(Value::as_str) {
                Some(reference) => match reference.rsplit('/').next() {
                    Some("DataBinding") => "a binding {\"@path\": \"/pointer\"}".to_string(),
                    Some("FunctionCall") => {
                        "a call {\"@call\": \"<function>\", \"args\": {…}}".to_string()
                    }
                    Some(other) => other.to_string(),
                    None => reference.to_string(),
                },
                None => match option.get("type") {
                    Some(kind) => format!("a {}", type_name(kind)),
                    None => "a value".to_string(),
                },
            })
            .collect();
        format!("one of: {}", names.join(", "))
    }

    fn check_object(
        &self,
        rules: &Map<String, Value>,
        object: &Map<String, Value>,
        path: &mut Vec<String>,
        depth: usize,
    ) -> Result<(), SchemaError> {
        let properties = rules.get("properties").and_then(Value::as_object);
        if let Some(required) = rules.get("required").and_then(Value::as_array) {
            for key in required.iter().filter_map(Value::as_str) {
                if !object.contains_key(key) {
                    path.push(key.to_string());
                    let mut error = fail(path, "a value here: this property is required");
                    error.failure = SchemaFailure::Missing;
                    path.pop();
                    return Err(error);
                }
            }
        }
        for (key, item) in object {
            path.push(key.clone());
            if let Some(schema) = properties.and_then(|p| p.get(key)) {
                self.check_at(schema, item, path, depth + 1)?;
            } else {
                match rules.get("additionalProperties") {
                    Some(Value::Bool(false)) => {
                        let known = properties
                            .map(|p| p.keys().map(String::as_str).collect::<Vec<_>>().join(", "))
                            .unwrap_or_default();
                        let mut error =
                            fail(path, &format!("no such property; the properties are {known}"));
                        error.failure = SchemaFailure::Unknown;
                        path.pop();
                        return Err(error);
                    }
                    Some(schema @ Value::Object(_)) => {
                        self.check_at(schema, item, path, depth + 1)?;
                    }
                    _ => {}
                }
            }
            path.pop();
        }
        Ok(())
    }
}

fn fail(path: &[String], expected: &str) -> SchemaError {
    SchemaError {
        failure: SchemaFailure::Invalid,
        path: path.to_vec(),
        expected: expected.to_string(),
    }
}

fn list(options: &[Value]) -> String {
    options.iter().map(Value::to_string).collect::<Vec<_>>().join(", ")
}

fn type_name(kind: &Value) -> String {
    match kind {
        Value::Array(kinds) => {
            kinds.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(" or ")
        }
        Value::String(kind) => kind.clone(),
        _ => "value".to_string(),
    }
}

fn type_matches(kind: &Value, value: &Value) -> bool {
    match kind {
        Value::Array(kinds) => kinds.iter().any(|k| type_matches(k, value)),
        Value::String(kind) => match kind.as_str() {
            "object" => value.is_object(),
            "array" => value.is_array(),
            "string" => value.is_string(),
            "boolean" => value.is_boolean(),
            "null" => value.is_null(),
            "number" => value.is_number(),
            "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
            _ => false,
        },
        _ => false,
    }
}

fn check_bounds(
    rules: &Map<String, Value>,
    value: &Value,
    path: &[String],
) -> Result<(), SchemaError> {
    let bound = |key: &str| rules.get(key).and_then(Value::as_u64);
    if let Some(array) = value.as_array() {
        if bound("minItems").is_some_and(|min| (array.len() as u64) < min) {
            return Err(fail(
                path,
                &format!("a list of at least {} items", bound("minItems").unwrap_or(0)),
            ));
        }
        if bound("maxItems").is_some_and(|max| (array.len() as u64) > max) {
            return Err(fail(
                path,
                &format!("a list of at most {} items", bound("maxItems").unwrap_or(0)),
            ));
        }
    }
    if let Some(text) = value.as_str() {
        let length = text.chars().count() as u64;
        if bound("minLength").is_some_and(|min| length < min) {
            return Err(fail(
                path,
                &format!("a text of at least {} characters", bound("minLength").unwrap_or(0)),
            ));
        }
        if bound("maxLength").is_some_and(|max| length > max) {
            return Err(fail(
                path,
                &format!("a text of at most {} characters", bound("maxLength").unwrap_or(0)),
            ));
        }
        if let Some(pattern) = rules.get("pattern").and_then(Value::as_str) {
            let matches = regex::Regex::new(pattern).map(|re| re.is_match(text)).unwrap_or(false);
            if !matches {
                return Err(fail(path, &format!("a text that matches {pattern}")));
            }
        }
    }
    if let (Some(minimum), Some(number)) =
        (rules.get("minimum").and_then(Value::as_f64), value.as_f64())
    {
        if number < minimum {
            return Err(fail(path, &format!("a number of at least {minimum}")));
        }
    }
    Ok(())
}
