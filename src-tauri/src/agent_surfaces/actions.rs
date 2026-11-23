//! The author's one action on a surface: which component sends it, whether
//! its context is the one that component declares, and the body of the
//! comment it becomes (`AUI-agent-ui-surfaces.md` AUI-FR-KBVY, AUI-FR-AFKN,
//! AUI-FR-XCXV, AUI-FR-KJPZ, AUI-FR-XMUS, AUI-FR-CGDQ, AUI-FR-WCRS).

use std::collections::HashSet;

use serde_json::{Map, Value};

use super::*;

/// AUI-FR-KJPZ: the most bytes that a context and a data model take.
const MAX_ACTION_BYTES: usize = 32 * 1024;
/// AUI-FR-CGDQ: the longest label and value of the comment body.
const LABEL_MAX_CHARS: usize = 80;
const VALUE_MAX_CHARS: usize = 500;
/// AUI-FR-XCXV: the longest own answer of a choice.
const OTHER_MAX_CHARS: usize = 2_000;

/// The component that sends an action.
enum Sender<'a> {
    Button(&'a Value),
    Card(&'a Value),
}

fn kind(component: &Value) -> &str {
    component.get("component").and_then(Value::as_str).unwrap_or("")
}

fn text_at<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    let mut current = value;
    for key in path {
        current = current.get(key)?;
    }
    current.as_str()
}

fn event_of(button: &Value) -> Option<&Value> {
    button.get("action").and_then(|a| a.get("event"))
}

/// AUI-FR-KBVY / AUI-FR-WCRS: the action names a card declares, each with the
/// label of the button that sends it.
fn card_actions(card: &Value) -> Vec<(&str, String)> {
    let named = |path: &[&str], label: &str| text_at(card, path).map(|n| (n, label.to_string()));
    let send = text_at(card, &["sendLabel"]).unwrap_or("Send").to_string();
    match kind(card) {
        "ChoiceCard" | "ChecklistCard" => named(&["action", "name"], &send).into_iter().collect(),
        "ConfirmCard" => ["confirm", "decline"]
            .iter()
            .filter_map(|side| {
                let label = text_at(card, &[side, "label"]).unwrap_or(side);
                named(&[side, "name"], label)
            })
            .collect(),
        "ComparisonCard" => {
            named(&["pick", "name"], text_at(card, &["pick", "label"]).unwrap_or("Pick"))
                .into_iter()
                .collect()
        }
        _ => Vec::new(),
    }
}

/// AUI-FR-KBVY: the Button or the card that `action` names.
fn sender_of<'a>(surface: &'a AgentSurface, action: &SurfaceAction) -> Option<Sender<'a>> {
    let component = surface.components.iter().find(|c| {
        c.get("id").and_then(Value::as_str) == Some(action.source_component_id.as_str())
    })?;
    if kind(component) == "Button" {
        let name = event_of(component).and_then(|e| e.get("name")).and_then(Value::as_str);
        return (name == Some(action.name.as_str())).then_some(Sender::Button(component));
    }
    card_actions(component)
        .iter()
        .any(|(name, _)| *name == action.name)
        .then_some(Sender::Card(component))
}

/// AUI-FR-AFKN: the declared keys, each declared literal as it is, and each
/// absolute binding equal to the data model at its pointer.
fn button_context_is_valid(button: &Value, action: &SurfaceAction) -> bool {
    let Some(context) = action.context.as_object() else {
        return false;
    };
    let empty = Map::new();
    let declared = event_of(button)
        .and_then(|e| e.get("context"))
        .and_then(Value::as_object)
        .unwrap_or(&empty);
    if context.len() != declared.len() {
        return false;
    }
    declared.iter().all(|(key, declared)| {
        let Some(sent) = context.get(key) else {
            return false;
        };
        match declared.get("@path").and_then(Value::as_str) {
            // A relative pointer reads a template item, which the action does
            // not name.
            Some(pointer) if !(pointer.is_empty() || pointer.starts_with('/')) => true,
            Some(pointer) => sent == action.data_model.pointer(pointer).unwrap_or(&Value::Null),
            None => sent == declared,
        }
    })
}

/// The option values of a card list, by the field that holds them.
fn values_of<'a>(card: &'a Value, list: &str) -> Vec<&'a str> {
    card.get(list)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|item| item.get("value").and_then(Value::as_str))
        .collect()
}

/// A list of strings, each one of `allowed` and none twice.
fn chosen<'a>(value: Option<&'a Value>, allowed: &[&str]) -> Option<Vec<&'a str>> {
    let items = value?.as_array()?;
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in items {
        let item = item.as_str()?;
        if !allowed.contains(&item) || !seen.insert(item) {
            return None;
        }
        out.push(item);
    }
    Some(out)
}

/// AUI-FR-XCXV: the context of each card kind.
fn card_context_is_valid(card: &Value, action: &SurfaceAction) -> bool {
    let Some(context) = action.context.as_object() else {
        return false;
    };
    let keys: HashSet<&str> = context.keys().map(String::as_str).collect();
    match kind(card) {
        "ChoiceCard" => {
            let other_allowed = card.get("allowOther").and_then(Value::as_bool) == Some(true);
            let allowed: HashSet<&str> =
                if other_allowed { ["selected", "other"].into() } else { ["selected"].into() };
            let other = context.get("other");
            let other_text = match other {
                None => "",
                Some(Value::String(text)) if text.chars().count() <= OTHER_MAX_CHARS => text,
                Some(_) => return false,
            };
            let Some(selected) = chosen(context.get("selected"), &values_of(card, "options"))
            else {
                return false;
            };
            let multiple = card.get("multiple").and_then(Value::as_bool) == Some(true);
            keys.is_subset(&allowed)
                && keys.contains("selected")
                && (multiple || selected.len() <= 1)
                && (!selected.is_empty() || (other_allowed && !other_text.trim().is_empty()))
        }
        "ChecklistCard" => {
            keys == ["checked"].into()
                && chosen(context.get("checked"), &values_of(card, "items")).is_some()
        }
        "ComparisonCard" => {
            let picked = context.get("picked").and_then(Value::as_str);
            keys == ["picked"].into()
                && picked.is_some_and(|p| values_of(card, "options").contains(&p))
        }
        "ConfirmCard" => context.is_empty(),
        _ => false,
    }
}

/// AUI-FR-KJPZ.
fn within_bounds(action: &SurfaceAction) -> bool {
    let (Some(context), Some(model)) = (action.context.as_object(), action.data_model.as_object())
    else {
        return false;
    };
    let bytes = serde_json::to_vec(context).map(|v| v.len()).unwrap_or(usize::MAX)
        + serde_json::to_vec(model).map(|v| v.len()).unwrap_or(usize::MAX);
    bytes <= MAX_ACTION_BYTES
}

/// AUI-FR-CGDQ: one line of text, at most `max` characters.
pub fn one_line(text: &str, max: usize) -> String {
    let line = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if line.chars().count() <= max {
        return line;
    }
    let mut cut: String = line.chars().take(max.saturating_sub(1)).collect();
    cut.push('…');
    cut
}

fn value_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// AUI-FR-XMUS / AUI-FR-CGDQ: the body of the author's comment: the pressed
/// button, then each context key in `lines` order.
pub fn action_body(label: &str, lines: &[(String, String)]) -> String {
    let mut body = format!("Sent “{}”.", one_line(label, LABEL_MAX_CHARS));
    for (key, value) in lines {
        body.push_str(&format!("\n- {key}: {}", one_line(value, VALUE_MAX_CHARS)));
    }
    body
}

/// AUI-FR-XMUS: the literal text of the button's `Text` child, or else the
/// event name.
fn button_label(surface: &AgentSurface, button: &Value, name: &str) -> String {
    let child = button.get("child").and_then(Value::as_str);
    surface
        .components
        .iter()
        .find(|c| child.is_some() && c.get("id").and_then(Value::as_str) == child)
        .filter(|c| kind(c) == "Text")
        .and_then(|c| c.get("text"))
        .and_then(Value::as_str)
        .filter(|t| !t.trim().is_empty())
        .unwrap_or(name)
        .to_string()
}

/// AUI-FR-WCRS: the label of the option whose value is `value`.
fn option_label<'a>(card: &'a Value, list: &str, value: &'a str) -> &'a str {
    card.get(list)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .find(|item| item.get("value").and_then(Value::as_str) == Some(value))
        .and_then(|item| item.get("label").and_then(Value::as_str))
        .unwrap_or(value)
}

fn labels(card: &Value, list: &str, values: Option<&Value>) -> String {
    let names: Vec<&str> = values
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|v| option_label(card, list, v))
        .collect();
    if names.is_empty() {
        "none".to_string()
    } else {
        names.join(", ")
    }
}

/// AUI-FR-WCRS: the label and the lines of a card action.
fn card_body(card: &Value, action: &SurfaceAction) -> String {
    let label = card_actions(card)
        .into_iter()
        .find(|(name, _)| *name == action.name)
        .map(|(_, label)| label)
        .unwrap_or_else(|| action.name.clone());
    let context = &action.context;
    let mut lines = Vec::new();
    match kind(card) {
        "ChoiceCard" => {
            lines.push(("selected".to_string(), labels(card, "options", context.get("selected"))));
            if let Some(other) =
                context.get("other").and_then(Value::as_str).filter(|o| !o.trim().is_empty())
            {
                lines.push(("other".to_string(), other.to_string()));
            }
        }
        "ChecklistCard" => {
            lines.push(("checked".to_string(), labels(card, "items", context.get("checked"))));
        }
        "ComparisonCard" => {
            let picked = context.get("picked").and_then(Value::as_str).unwrap_or("");
            lines.push(("picked".to_string(), option_label(card, "options", picked).to_string()));
        }
        _ => {}
    }
    action_body(&label, &lines)
}

/// AUI-FR-KBVY to AUI-FR-WCRS: the comment body of an accepted action, or the
/// typed error that refuses it.
pub fn accept(surface: &AgentSurface, action: &SurfaceAction) -> Result<String, String> {
    let sender = sender_of(surface, action).ok_or_else(|| ERR_UNKNOWN_ACTION.to_string())?;
    if !within_bounds(action) {
        return Err(ERR_INVALID_CONTEXT.to_string());
    }
    match sender {
        Sender::Button(button) => {
            if !button_context_is_valid(button, action) {
                return Err(ERR_INVALID_CONTEXT.to_string());
            }
            let order: Vec<String> = event_of(button)
                .and_then(|e| e.get("context"))
                .and_then(Value::as_object)
                .map(|c| c.keys().cloned().collect())
                .unwrap_or_default();
            let lines: Vec<(String, String)> = order
                .into_iter()
                .filter_map(|key| action.context.get(&key).map(|v| (key, value_text(v))))
                .collect();
            Ok(action_body(&button_label(surface, button, &action.name), &lines))
        }
        Sender::Card(card) => {
            if !card_context_is_valid(card, action) {
                return Err(ERR_INVALID_CONTEXT.to_string());
            }
            Ok(card_body(card, action))
        }
    }
}
