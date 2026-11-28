//! CDX-FR-XIGW / CDX-FR-BDZW / CDX-FR-TPFM: the strict schema `--output-schema`
//! carries, and the reading of an answer written to it.
//!
//! The CLI sends the schema as strict structured output, which refuses an
//! optional property, an open object, `oneOf`, and a few keywords outright. The
//! schema below is therefore the executor's envelope schema with the contract
//! at `result`, rewritten by the strict-dialect rules of the contract surface.
//! Both halves are fixed text compiled into the binary, so nothing here is
//! derived from the task (EAC-FR-VTLG).

use std::borrow::Cow;

use serde_json::{json, Map, Value};

use crate::tools::agent_exec::protocol::{ResultContract, ENVELOPE_SCHEMA, LIMIT_ENVELOPE};

/// CDX-FR-XIGW: the keywords strict structured output does not accept.
const DROPPED_KEYWORDS: &[&str] = &["if", "then", "else", "maxProperties", "$schema"];

/// CDX-FR-BDZW: what `escalation.kind` means, said where the agent reads it.
pub const ESCALATION_KIND_DESCRIPTION: &str = "null for an escalation that asks the user a \
question. plan_update only for a plan update.";

/// The envelope schema with `contract` at `result`, before the strict rules: the
/// document whose optional positions CDX-FR-TPFM reads a `null` against.
fn original_schema(contract: ResultContract) -> Value {
    let mut envelope: Value =
        serde_json::from_str(ENVELOPE_SCHEMA).expect("the envelope schema is JSON");
    envelope["properties"]["result"] =
        json!({ "anyOf": [{ "type": "null" }, contract.schema_value().clone()] });
    envelope
}

/// CDX-FR-XIGW: the strict schema for a turn that names `contract`, as the text
/// of `output-schema-<contract>.json`.
pub fn output_schema(contract: ResultContract) -> String {
    // `result` is `anyOf` of null and the contract already, and it admits null,
    // so the rules leave it one level deep.
    let mut strict = to_strict(&original_schema(contract));
    // The contract surface's rule that `metadata` holds nothing.
    strict["properties"]["metadata"] = json!({ "type": "null" });
    // CDX-FR-BDZW.
    if let Some(Value::Object(kind)) = strict.pointer_mut("/properties/escalation/properties/kind")
    {
        kind.insert("description".into(), Value::from(ESCALATION_KIND_DESCRIPTION));
    }
    strict.to_string()
}

fn admits(schema: &Map<String, Value>, kind: &str) -> bool {
    match schema.get("type") {
        Some(Value::String(t)) => t == kind,
        Some(Value::Array(types)) => types.iter().any(|t| t.as_str() == Some(kind)),
        _ => false,
    }
}

fn is_object_schema(schema: &Map<String, Value>) -> bool {
    admits(schema, "object") || schema.contains_key("properties")
}

/// CDX-FR-XIGW: an optional property made nullable. A schema that already
/// admits `null` stays as it is.
fn nullable(schema: Value) -> Value {
    let Value::Object(mut map) = schema else {
        return schema;
    };
    if let Some(Value::Array(values)) = map.get_mut("enum") {
        if !values.iter().any(Value::is_null) {
            values.push(Value::Null);
        }
    }
    match map.get_mut("type") {
        Some(Value::Array(types)) => {
            if !types.iter().any(|t| t.as_str() == Some("null")) {
                types.push(Value::from("null"));
            }
            Value::Object(map)
        }
        Some(Value::String(t)) if t == "null" => Value::Object(map),
        Some(Value::String(t)) => {
            let t = t.clone();
            map.insert("type".into(), json!([t, "null"]));
            Value::Object(map)
        }
        _ => {
            let admits_null = map.get("anyOf").and_then(Value::as_array).is_some_and(|branches| {
                branches.iter().any(|b| b.get("type").and_then(Value::as_str) == Some("null"))
            });
            if admits_null {
                Value::Object(map)
            } else {
                json!({ "anyOf": [Value::Object(map), { "type": "null" }] })
            }
        }
    }
}

/// CDX-FR-XIGW: `schema` rewritten by the strict-dialect rules of the contract
/// surface, at every depth.
pub fn to_strict(schema: &Value) -> Value {
    match schema {
        Value::Array(items) => Value::Array(items.iter().map(to_strict).collect()),
        Value::Object(map) => {
            let mut out: Map<String, Value> = map
                .iter()
                .filter(|(key, _)| !DROPPED_KEYWORDS.contains(&key.as_str()))
                .map(|(key, value)| {
                    // `properties` is a map of names, not a schema, so its keys
                    // are never read as keywords.
                    let value = if key == "properties" {
                        match value {
                            Value::Object(props) => Value::Object(
                                props.iter().map(|(k, v)| (k.clone(), to_strict(v))).collect(),
                            ),
                            other => other.clone(),
                        }
                    } else {
                        to_strict(value)
                    };
                    (key.clone(), value)
                })
                .collect();
            if let Some(branches) = out.remove("oneOf") {
                out.insert("anyOf".into(), branches);
            }
            if admits(&out, "array") && !out.contains_key("items") {
                out.insert("items".into(), json!({ "type": "null" }));
            }
            if is_object_schema(&out) {
                let required: Vec<String> = out
                    .get("required")
                    .and_then(Value::as_array)
                    .map(|r| r.iter().filter_map(Value::as_str).map(str::to_string).collect())
                    .unwrap_or_default();
                let props = match out.remove("properties") {
                    Some(Value::Object(props)) => props,
                    _ => Map::new(),
                };
                let names: Vec<Value> = props.keys().cloned().map(Value::from).collect();
                let props: Map<String, Value> = props
                    .into_iter()
                    .map(|(name, prop)| {
                        let prop = if required.contains(&name) { prop } else { nullable(prop) };
                        (name, prop)
                    })
                    .collect();
                out.insert("properties".into(), Value::Object(props));
                out.insert("required".into(), Value::Array(names));
                out.insert("additionalProperties".into(), Value::Bool(false));
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

/// The branches a value at `schema` may match: the schema itself, or each
/// branch of its `oneOf` or `anyOf`, flattened.
fn branches(schema: &Value) -> Vec<&Map<String, Value>> {
    let Value::Object(map) = schema else {
        return Vec::new();
    };
    let nested = map.get("oneOf").or_else(|| map.get("anyOf")).and_then(Value::as_array);
    match nested {
        Some(list) => list.iter().flat_map(branches).collect(),
        None => vec![map],
    }
}

/// One schema that admits what any of `schemas` admits.
fn union(schemas: Vec<&Value>) -> Value {
    match schemas.as_slice() {
        [] => Value::Null,
        [one] => (*one).clone(),
        _ => json!({ "anyOf": schemas }),
    }
}

/// CDX-FR-TPFM / EAC-FR-RBLI: remove each key whose value is `null` where every
/// branch of `schema` that names the key holds it optional. A `null` at a
/// required position stays, so the rules after this still refuse it.
fn strip(value: &mut Value, schema: &Value) {
    let candidates = branches(schema);
    match value {
        Value::Object(map) => {
            let keys: Vec<String> = map.keys().cloned().collect();
            for key in keys {
                let naming: Vec<&Map<String, Value>> = candidates
                    .iter()
                    .copied()
                    .filter(|b| b.get("properties").and_then(|p| p.get(&key)).is_some())
                    .collect();
                if naming.is_empty() {
                    continue;
                }
                let optional = naming.iter().all(|b| {
                    !b.get("required")
                        .and_then(Value::as_array)
                        .is_some_and(|r| r.iter().any(|n| n.as_str() == Some(key.as_str())))
                });
                if optional && map.get(&key).is_some_and(Value::is_null) {
                    map.remove(&key);
                    continue;
                }
                let sub = union(naming.iter().filter_map(|b| b["properties"].get(&key)).collect());
                if let Some(child) = map.get_mut(&key) {
                    strip(child, &sub);
                }
            }
        }
        Value::Array(items) => {
            let sub = union(candidates.iter().filter_map(|b| b.get("items")).collect());
            for item in items {
                strip(item, &sub);
            }
        }
        _ => {}
    }
}

/// CDX-FR-TPFM: the answer text of a turn that named `contract`, with each
/// `null` at an optional position read as the field absent.
///
/// Text that is not one JSON document, or that is over the envelope limit, is
/// returned as it came, so the decoder refuses it for what it is.
pub fn strip_optional_nulls(text: &str, contract: ResultContract) -> Cow<'_, str> {
    if text.len() > LIMIT_ENVELOPE {
        return Cow::Borrowed(text);
    }
    let Ok(mut value) = serde_json::from_str::<Value>(text) else {
        return Cow::Borrowed(text);
    };
    strip(&mut value, &original_schema(contract));
    Cow::Owned(value.to_string())
}
