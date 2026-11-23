//! The message log of a surface (`AUI-agent-ui-surfaces.md` AUI-FR-IAAP,
//! AUI-FR-TYBX, AUI-FR-JYKT, AUI-FR-HOJP).
//!
//! A surface is the ordered log of its A2UI v1.0 messages. Its current state
//! is the fold of the log: the components of `createSurface`, replaced or
//! added by id by each `updateComponents`, and the data model of
//! `createSurface`, changed at its path by each `updateDataModel`.

use serde::Deserialize;
use serde_json::{json, Map, Value};

use super::catalog::{CATALOG_ID, FIRST_CATALOG_ID};
use super::*;

/// The A2UI protocol version of every message.
pub const PROTOCOL_VERSION: &str = "1.0";

/// AUI-FR-TYBX: the first message of a log.
pub fn create_surface(surface_id: &str, components: &[Value], data_model: &Value) -> Value {
    json!({
        "version": PROTOCOL_VERSION,
        "createSurface": {
            "surfaceId": surface_id,
            "catalogId": CATALOG_ID,
            "components": components,
            "dataModel": data_model,
        }
    })
}

/// AUI-FR-TYBX: the current components and data model of a surface.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CurrentState {
    pub components: Vec<Value>,
    pub data_model: Value,
}

impl AgentSurface {
    /// AUI-FR-TYBX: the fold of the log, in order.
    pub fn current(&self) -> CurrentState {
        fold(&self.messages)
    }
}

/// AUI-FR-TYBX: fold `messages` in order.
pub fn fold(messages: &[Value]) -> CurrentState {
    let mut state = CurrentState { components: Vec::new(), data_model: json!({}) };
    for message in messages {
        if let Some(create) = message.get("createSurface") {
            state.components =
                create.get("components").and_then(Value::as_array).cloned().unwrap_or_default();
            state.data_model = create.get("dataModel").cloned().unwrap_or_else(|| json!({}));
        } else if let Some(update) = message.get("updateComponents") {
            for component in
                update.get("components").and_then(Value::as_array).into_iter().flatten()
            {
                let id = component.get("id");
                match state.components.iter_mut().find(|c| c.get("id") == id) {
                    Some(slot) => *slot = component.clone(),
                    None => state.components.push(component.clone()),
                }
            }
        } else if let Some(update) = message.get("updateDataModel") {
            let path = update.get("path").and_then(Value::as_str).unwrap_or("");
            set_at(&mut state.data_model, path, update.get("value").cloned());
        }
    }
    state
}

/// Set (or, with no value, remove) the value at an RFC 6901 pointer.
fn set_at(model: &mut Value, pointer: &str, value: Option<Value>) {
    if pointer.is_empty() || pointer == "/" {
        *model = value.filter(Value::is_object).unwrap_or_else(|| json!({}));
        return;
    }
    let tokens: Vec<String> = pointer
        .trim_start_matches('/')
        .split('/')
        .map(|t| t.replace("~1", "/").replace("~0", "~"))
        .collect();
    let mut node = model;
    for token in &tokens[..tokens.len() - 1] {
        if !node.is_object() {
            *node = json!({});
        }
        let Some(object) = node.as_object_mut() else { return };
        node = object.entry(token.clone()).or_insert_with(|| json!({}));
    }
    let last = &tokens[tokens.len() - 1];
    if !node.is_object() {
        *node = json!({});
    }
    if let Some(object) = node.as_object_mut() {
        match value {
            Some(value) => {
                object.insert(last.clone(), value);
            }
            None => {
                object.remove(last);
            }
        }
    }
}

/// AUI-FR-JYKT / AUI-FR-HOJP: a stored record, in either shape.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StoredSurface {
    id: String,
    discussion_id: String,
    target: crate::comments::DiscussionTarget,
    comment_id: String,
    catalog_id: String,
    #[serde(default)]
    messages: Option<Vec<Value>>,
    #[serde(default)]
    components: Option<Vec<Value>>,
    #[serde(default)]
    data_model: Option<Value>,
    authored_by: crate::comments::Participant,
    created_at: String,
    state: SurfaceState,
    #[serde(default)]
    submission: Option<SurfaceSubmission>,
}

/// AUI-FR-HOJP: the first catalog wrote two common properties with an `@`.
fn renamed(component: &Value) -> Value {
    let Some(object) = component.as_object() else {
        return component.clone();
    };
    let mut out = Map::new();
    for (key, value) in object {
        let key = match key.as_str() {
            "@checks" => "checks",
            "@accessibility" => "accessibility",
            other => other,
        };
        out.insert(key.to_string(), value.clone());
    }
    Value::Object(out)
}

impl From<StoredSurface> for AgentSurface {
    fn from(stored: StoredSurface) -> Self {
        let first = stored.catalog_id == FIRST_CATALOG_ID;
        let messages = match stored.messages {
            Some(messages) => messages,
            None => {
                let components: Vec<Value> = stored
                    .components
                    .unwrap_or_default()
                    .iter()
                    .map(|c| if first { renamed(c) } else { c.clone() })
                    .collect();
                let data_model = stored.data_model.unwrap_or_else(|| json!({}));
                vec![create_surface(&stored.id, &components, &data_model)]
            }
        };
        AgentSurface {
            id: stored.id,
            discussion_id: stored.discussion_id,
            target: stored.target,
            comment_id: stored.comment_id,
            catalog_id: if first { CATALOG_ID.to_string() } else { stored.catalog_id },
            messages,
            authored_by: stored.authored_by,
            created_at: stored.created_at,
            state: stored.state,
            submission: stored.submission,
        }
    }
}
