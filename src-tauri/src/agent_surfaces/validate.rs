//! `validate_surface` (`AUI-agent-ui-surfaces.md` AUI-FR-EGAB to AUI-FR-VOYL).
//!
//! A surface is checked in three passes. The first checks each component
//! against its catalog schema. The second checks the tree: the ids, the child
//! references, the reach from `root`, and the depth. The third checks each
//! binding and each function call against the catalog and against where it
//! stands in the tree.

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};

use super::catalog::{catalog, SchemaFailure};

/// AUI-FR-ZEXT: the most components a surface holds.
pub const MAX_COMPONENTS: usize = 100;
/// AUI-FR-ZEXT: the longest path from `root` to a component, in components.
pub const MAX_DEPTH: usize = 8;
/// AUI-FR-IAGR: the most bytes that the components and the data model take.
pub const MAX_BYTES: usize = 32 * 1024;

/// AUI-FR-MTMS: the components that take input.
pub const INPUT_COMPONENTS: [&str; 5] =
    ["TextField", "CheckBox", "ChoicePicker", "Slider", "DateTimeInput"];

/// AUI-FR-PMXJ: the cards, each a whole panel with its own buttons.
pub const CARD_COMPONENTS: [&str; 4] =
    ["ChoiceCard", "ConfirmCard", "ComparisonCard", "ChecklistCard"];

/// AUI-FR-VOYL: the first violation of a surface.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SurfaceViolation {
    /// A stable name of the rule that failed. It is safe to log.
    pub rule: &'static str,
    pub component_id: Option<String>,
    pub property: Option<String>,
    /// What a valid value looks like, in one sentence.
    pub expected: String,
}

impl SurfaceViolation {
    fn new(rule: &'static str, expected: impl Into<String>) -> Self {
        SurfaceViolation { rule, component_id: None, property: None, expected: expected.into() }
    }

    fn at(mut self, component_id: &str) -> Self {
        self.component_id = Some(component_id.to_string());
        self
    }

    fn on(mut self, property: impl Into<String>) -> Self {
        self.property = Some(property.into());
        self
    }
}

/// AUI-FR-EGAB: accept `components` and `data_model` only when they make a
/// valid surface of the discussion catalog.
pub fn validate_surface(components: &[Value], data_model: &Value) -> Result<(), SurfaceViolation> {
    // AUI-FR-IAGR.
    if !data_model.is_object() {
        return Err(SurfaceViolation::new(
            "data_model_not_object",
            "`data_model` is a JSON object",
        ));
    }
    // AUI-FR-ZEXT.
    if components.is_empty() {
        return Err(SurfaceViolation::new("no_components", "a list of at least one component"));
    }
    if components.len() > MAX_COMPONENTS {
        return Err(SurfaceViolation::new(
            "too_many_components",
            format!("at most {MAX_COMPONENTS} components"),
        ));
    }
    // AUI-FR-IAGR.
    let bytes = serde_json::to_vec(components).map(|v| v.len()).unwrap_or(usize::MAX)
        + serde_json::to_vec(data_model).map(|v| v.len()).unwrap_or(usize::MAX);
    if bytes > MAX_BYTES {
        return Err(SurfaceViolation::new(
            "too_large",
            format!(
                "the components and the data model together take at most {MAX_BYTES} bytes of JSON"
            ),
        ));
    }
    let objects = check_components(components)?;
    let tree = Tree::of(&objects)?;
    tree.check_reach()?;
    check_weights(&objects, &tree)?;
    check_table_rows(&objects)?;
    for (id, object) in &objects {
        check_context_keys(id, object)?;
        check_media_url(id, object)?;
        check_unique_values(id, object)?;
        check_comparison_values(id, object)?;
    }
    let in_template = tree.template_members();
    for (id, object) in &objects {
        check_values(id, object, in_template.contains(id.as_str()))?;
    }
    // AUI-FR-MTMS.
    let kinds: Vec<&str> = objects.iter().filter_map(|(_, o)| kind_of(o)).collect();
    let sends = kinds.iter().any(|k| *k == "Button" || CARD_COMPONENTS.contains(k));
    if kinds.iter().any(|k| INPUT_COMPONENTS.contains(k)) && !sends {
        return Err(SurfaceViolation::new(
            "inputs_without_button",
            "a surface with an input also has a Button that sends the values",
        ));
    }
    Ok(())
}

fn kind_of(object: &Map<String, Value>) -> Option<&str> {
    object.get("component").and_then(Value::as_str)
}

/// The components of a surface by id, each as its JSON object.
type Objects<'a> = Vec<(String, &'a Map<String, Value>)>;

/// AUI-FR-EGAB / AUI-FR-GMJK: each component against its catalog schema.
fn check_components(components: &[Value]) -> Result<Objects<'_>, SurfaceViolation> {
    let catalog = catalog();
    let mut seen = HashSet::new();
    let mut objects = Vec::with_capacity(components.len());
    for (index, component) in components.iter().enumerate() {
        let place = format!("#{index}");
        let Some(object) = component.as_object() else {
            return Err(SurfaceViolation::new("not_an_object", "each component is a JSON object")
                .at(&place));
        };
        let id = match object.get("id").and_then(Value::as_str) {
            Some(id) if is_valid_id(id) => id,
            _ => {
                return Err(SurfaceViolation::new(
                    "bad_id",
                    "an id of 1 to 64 letters, digits, `_`, and `-`",
                )
                .at(&place)
                .on("id"));
            }
        };
        if !seen.insert(id) {
            return Err(SurfaceViolation::new("duplicate_id", "an id that no other component has")
                .at(id)
                .on("id"));
        }
        let names = catalog.component_names();
        let kind = match kind_of(object).filter(|kind| catalog.component(kind).is_some()) {
            Some(kind) => kind,
            None => {
                return Err(SurfaceViolation::new(
                    "unknown_component",
                    format!("one of the catalog components: {}", names.join(", ")),
                )
                .at(id)
                .on("component"));
            }
        };
        if let Err(error) = catalog.check_component(kind, component) {
            // A precise reason of this module, such as an unknown function,
            // says more than the schema's own; where none applies, the schema
            // error stands. Template rules are checked later with the tree.
            check_values(id, object, true)?;
            check_context_keys(id, object)?;
            let rule = match error.failure {
                SchemaFailure::Missing => "missing_property",
                SchemaFailure::Unknown => "unknown_property",
                SchemaFailure::Invalid => "invalid_value",
            };
            return Err(SurfaceViolation::new(rule, error.expected)
                .at(id)
                .on(error.path.join(".")));
        }
        objects.push((id.to_string(), object));
    }
    Ok(objects)
}

/// AUI-FR-GMJK.
pub fn is_valid_id(id: &str) -> bool {
    !id.is_empty()
        && id.chars().count() <= 64
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// The child references of one component, and whether each is a template.
fn children_of(object: &Map<String, Value>) -> Vec<(String, &'static str, bool)> {
    let mut out = Vec::new();
    match object.get("children") {
        Some(Value::Array(ids)) => {
            out.extend(
                ids.iter().filter_map(Value::as_str).map(|id| (id.to_string(), "children", false)),
            );
        }
        Some(Value::Object(template)) => {
            if let Some(id) = template.get("componentId").and_then(Value::as_str) {
                out.push((id.to_string(), "children.componentId", true));
            }
        }
        _ => {}
    }
    if let Some(id) = object.get("child").and_then(Value::as_str) {
        out.push((id.to_string(), "child", false));
    }
    // The trigger and the content of a Modal.
    for key in ["trigger", "content"] {
        if let Some(id) = object.get(key).and_then(Value::as_str) {
            out.push((id.to_string(), key, false));
        }
    }
    if let Some(tabs) = object.get("tabs").and_then(Value::as_array) {
        out.extend(
            tabs.iter()
                .filter_map(|tab| tab.get("child").and_then(Value::as_str))
                .map(|id| (id.to_string(), "tabs.child", false)),
        );
    }
    out
}

/// The tree of a surface, by id.
struct Tree {
    children: HashMap<String, Vec<(String, bool)>>,
    order: Vec<String>,
}

impl Tree {
    /// AUI-FR-GMJK / AUI-FR-AQBE: one `root`, and every reference resolves.
    fn of(objects: &[(String, &Map<String, Value>)]) -> Result<Tree, SurfaceViolation> {
        let ids: HashSet<&str> = objects.iter().map(|(id, _)| id.as_str()).collect();
        if !ids.contains("root") {
            return Err(SurfaceViolation::new(
                "no_root",
                "exactly one component with the id `root`",
            ));
        }
        let mut children = HashMap::new();
        for (id, object) in objects {
            let mut list = Vec::new();
            for (child, property, template) in children_of(object) {
                if !ids.contains(child.as_str()) {
                    return Err(SurfaceViolation::new(
                        "missing_child",
                        format!("the id of a component of this surface, and `{child}` is none"),
                    )
                    .at(id)
                    .on(property));
                }
                list.push((child, template));
            }
            children.insert(id.clone(), list);
        }
        Ok(Tree { children, order: objects.iter().map(|(id, _)| id.clone()).collect() })
    }

    /// AUI-FR-AQBE / AUI-FR-ZEXT: every component reachable from `root`, no
    /// component reachable from itself, and no path longer than the bound.
    fn check_reach(&self) -> Result<(), SurfaceViolation> {
        let mut deepest: HashMap<&str, usize> = HashMap::new();
        let mut on_path: Vec<&str> = Vec::new();
        self.walk("root", 1, &mut on_path, &mut deepest)?;
        if let Some(id) = self.order.iter().find(|id| !deepest.contains_key(id.as_str())) {
            return Err(SurfaceViolation::new(
                "unreachable",
                "a component that `root` reaches through child references",
            )
            .at(id));
        }
        Ok(())
    }

    fn walk<'a>(
        &'a self,
        id: &'a str,
        depth: usize,
        on_path: &mut Vec<&'a str>,
        deepest: &mut HashMap<&'a str, usize>,
    ) -> Result<(), SurfaceViolation> {
        if on_path.contains(&id) {
            return Err(SurfaceViolation::new(
                "cycle",
                "a tree in which no component contains itself",
            )
            .at(id));
        }
        if depth > MAX_DEPTH {
            return Err(SurfaceViolation::new(
                "too_deep",
                format!("a path from `root` of at most {MAX_DEPTH} components"),
            )
            .at(id));
        }
        // A component already reached at this depth or deeper adds nothing.
        if deepest.get(id).is_some_and(|d| *d >= depth) {
            return Ok(());
        }
        deepest.insert(id, depth);
        on_path.push(id);
        for (child, _) in self.children.get(id).map(Vec::as_slice).unwrap_or_default() {
            self.walk(child, depth + 1, on_path, deepest)?;
        }
        on_path.pop();
        Ok(())
    }

    /// AUI-FR-LXOY: the template of each template child list, and everything
    /// inside it.
    /// The components that name `id` as a child.
    fn parents_of(&self, id: &str) -> Vec<&str> {
        self.children
            .iter()
            .filter(|(_, children)| children.iter().any(|(child, _)| child == id))
            .map(|(parent, _)| parent.as_str())
            .collect()
    }

    fn template_members(&self) -> HashSet<&str> {
        let mut members = HashSet::new();
        let mut pending: Vec<&str> = self
            .children
            .values()
            .flatten()
            .filter(|(_, template)| *template)
            .map(|(id, _)| id.as_str())
            .collect();
        while let Some(id) = pending.pop() {
            if members.insert(id) {
                if let Some(children) = self.children.get(id) {
                    pending.extend(children.iter().map(|(child, _)| child.as_str()));
                }
            }
        }
        members
    }
}

/// AUI contract surface: `weight` stands on a direct child of a `Row` or a
/// `Column` alone.
fn check_weights(
    objects: &[(String, &Map<String, Value>)],
    tree: &Tree,
) -> Result<(), SurfaceViolation> {
    let kinds: HashMap<&str, &str> =
        objects.iter().filter_map(|(id, o)| kind_of(o).map(|k| (id.as_str(), k))).collect();
    for (id, object) in objects {
        if !object.contains_key("weight") {
            continue;
        }
        let parents = tree.parents_of(id);
        let in_row = !parents.is_empty()
            && parents.iter().all(|p| matches!(kinds.get(p), Some(&"Row") | Some(&"Column")));
        if !in_row {
            return Err(SurfaceViolation::new(
                "weight_outside_row_or_column",
                "`weight` only on a direct child of a Row or a Column",
            )
            .at(id)
            .on("weight"));
        }
    }
    Ok(())
}

/// AUI contract surface: the literal `rows` of a `Table` are a list of
/// objects. Bound rows are read where they are drawn.
fn check_table_rows(objects: &[(String, &Map<String, Value>)]) -> Result<(), SurfaceViolation> {
    for (id, object) in objects {
        if kind_of(object) != Some("Table") {
            continue;
        }
        let Some(rows) = object.get("rows") else {
            continue;
        };
        let bound = rows.get("@path").is_some() || rows.get("@call").is_some();
        let objects_only = rows.as_array().is_some_and(|items| items.iter().all(Value::is_object));
        if !bound && !objects_only {
            return Err(SurfaceViolation::new(
                "invalid_value",
                "a list of objects, or a binding {\"@path\": \"/rows\"}",
            )
            .at(id)
            .on("rows"));
        }
    }
    Ok(())
}

/// AUI-FR-PMXJ: the components that load media from their `url`.
pub const MEDIA_COMPONENTS: [&str; 3] = ["Image", "Video", "AudioPlayer"];

/// AUI-FR-GUUJ: a media source: an `https` address, `project:<path>` with a
/// project-relative path, or `attachment:<digest>`.
pub fn is_media_url(url: &str) -> bool {
    if let Some(rest) = url.strip_prefix("https://") {
        return !rest.is_empty() && !rest.starts_with('/');
    }
    if let Some(path) = url.strip_prefix("project:") {
        return !path.is_empty()
            && !path.starts_with('/')
            && !path.contains('\\')
            && path
                .split('/')
                .all(|segment| !segment.is_empty() && segment != "." && segment != "..");
    }
    if let Some(digest) = url.strip_prefix("attachment:") {
        return digest.len() == 64 && digest.chars().all(|c| c.is_ascii_hexdigit());
    }
    false
}

/// AUI-FR-GUUJ: an address that `openUrl` opens.
pub fn is_link_url(url: &str) -> bool {
    ["https://", "http://", "mailto:"]
        .iter()
        .any(|scheme| url.strip_prefix(scheme).is_some_and(|rest| !rest.is_empty()))
}

/// AUI-FR-GUUJ: the literal `url` of a media component.
fn check_media_url(id: &str, object: &Map<String, Value>) -> Result<(), SurfaceViolation> {
    if !kind_of(object).is_some_and(|k| MEDIA_COMPONENTS.contains(&k)) {
        return Ok(());
    }
    match object.get("url").and_then(Value::as_str) {
        Some(url) if !is_media_url(url) => Err(SurfaceViolation::new(
            "bad_url",
            "an `https` address, `project:<project-relative path>`, or `attachment:<digest>`",
        )
        .at(id)
        .on("url")),
        _ => Ok(()),
    }
}

/// AUI-FR-JLHW: a context key is an identifier.
pub fn is_context_key(key: &str) -> bool {
    let mut chars = key.chars();
    key.len() <= 64
        && chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// AUI-FR-JLHW: an event name is lowercase letters, digits, and `_`.
pub fn is_event_name(name: &str) -> bool {
    let mut chars = name.chars();
    name.len() <= 64
        && chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

/// AUI-FR-JLHW: the name and the context keys of a button's event.
fn check_context_keys(id: &str, object: &Map<String, Value>) -> Result<(), SurfaceViolation> {
    let event = object.get("action").and_then(|a| a.get("event"));
    if let Some(name) = event.and_then(|e| e.get("name")) {
        if !name.as_str().is_some_and(is_event_name) {
            return Err(SurfaceViolation::new(
                "bad_event_name",
                "an event name of lowercase letters, digits, and `_` that starts with a letter, at most 64 characters",
            )
            .at(id)
            .on("action.event.name"));
        }
    }
    let context = event.and_then(|e| e.get("context")).and_then(Value::as_object);
    if let Some(key) = context.into_iter().flat_map(|c| c.keys()).find(|k| !is_context_key(k)) {
        return Err(SurfaceViolation::new(
            "bad_context_key",
            "a context key of letters, digits, and `_` that starts with a letter, at most 64 characters",
        )
        .at(id)
        .on(format!("action.event.context.{}", key.chars().take(64).collect::<String>())));
    }
    Ok(())
}

/// AUI-FR-DYVP: the option values, and the criterion keys, of one component
/// are unique within it.
fn check_unique_values(id: &str, object: &Map<String, Value>) -> Result<(), SurfaceViolation> {
    let lists: &[(&str, &str)] = match kind_of(object) {
        Some("ChoicePicker") | Some("ChoiceCard") => &[("options", "value")],
        Some("ComparisonCard") => &[("options", "value"), ("criteria", "key")],
        Some("ChecklistCard") => &[("items", "value")],
        _ => &[],
    };
    for (list, field) in lists {
        let mut seen = HashSet::new();
        for (index, item) in
            object.get(*list).and_then(Value::as_array).into_iter().flatten().enumerate()
        {
            if let Some(value) = item.get(*field).and_then(Value::as_str) {
                if !seen.insert(value) {
                    return Err(SurfaceViolation::new(
                        "duplicate_value",
                        format!("a `{field}` that no other entry of `{list}` has"),
                    )
                    .at(id)
                    .on(format!("{list}.{index}.{field}")));
                }
            }
        }
    }
    Ok(())
}

/// AUI-FR-GITI: each key of the values of a comparison option is a criterion
/// key of its card.
fn check_comparison_values(id: &str, object: &Map<String, Value>) -> Result<(), SurfaceViolation> {
    if kind_of(object) != Some("ComparisonCard") {
        return Ok(());
    }
    let keys: HashSet<&str> = object
        .get("criteria")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|c| c.get("key").and_then(Value::as_str))
        .collect();
    for (index, option) in
        object.get("options").and_then(Value::as_array).into_iter().flatten().enumerate()
    {
        let values = option.get("values").and_then(Value::as_object);
        if values.into_iter().flat_map(|v| v.keys()).any(|k| !keys.contains(k.as_str())) {
            return Err(SurfaceViolation::new(
                "unknown_criterion",
                "values whose keys are the criterion keys of this card",
            )
            .at(id)
            .on(format!("options.{index}.values")));
        }
    }
    Ok(())
}

/// AUI-FR-HCCU / AUI-FR-LXOY: each binding and each call of one component.
fn check_values(
    id: &str,
    object: &Map<String, Value>,
    in_template: bool,
) -> Result<(), SurfaceViolation> {
    let mut path = Vec::new();
    for (key, value) in object {
        // A template child list names a data-model list with a plain path.
        if key == "children" {
            if let Some(list_path) = value.get("path").and_then(Value::as_str) {
                check_pointer(id, "children.path", list_path, in_template)?;
            }
            continue;
        }
        path.push(key.clone());
        walk_value(id, value, &mut path, in_template)?;
        path.pop();
    }
    Ok(())
}

fn walk_value(
    id: &str,
    value: &Value,
    path: &mut Vec<String>,
    in_template: bool,
) -> Result<(), SurfaceViolation> {
    match value {
        Value::Object(object) => {
            if let Some(pointer) = object.get("@path") {
                let Some(pointer) = pointer.as_str().filter(|_| object.len() == 1) else {
                    return Err(SurfaceViolation::new(
                        "bad_binding",
                        "a binding {\"@path\": \"/pointer\"} with no other key",
                    )
                    .at(id)
                    .on(path.join(".")));
                };
                return check_pointer(id, &path.join("."), pointer, in_template);
            }
            if let Some(name) = object.get("@call") {
                let is_open_url = name.as_str() == Some("openUrl");
                // AUI-FR-ZJRQ: a Button's action is an event, or a
                // functionCall of `openUrl` alone.
                if path.len() == 2 && path[0] == "action" && path[1] == "functionCall" {
                    if !is_open_url {
                        return Err(SurfaceViolation::new(
                            "bad_action",
                            "an action that is an `event`, or a `functionCall` of `openUrl`",
                        )
                        .at(id)
                        .on(path.join(".")));
                    }
                    return check_call(id, object, name, path, in_template);
                }
                // AUI-FR-ZJRQ: `openUrl` runs on a press of the author alone.
                if is_open_url {
                    return Err(SurfaceViolation::new(
                        "open_url_outside_action",
                        "`openUrl` only as the `functionCall` action of a Button",
                    )
                    .at(id)
                    .on(path.join(".")));
                }
                // AUI-FR-JLHW: an action's context holds literals and bindings.
                if path.first().map(String::as_str) == Some("action") {
                    return Err(SurfaceViolation::new(
                        "call_in_context",
                        "a context value that is a literal or a binding {\"@path\": \"/pointer\"}",
                    )
                    .at(id)
                    .on(path.join(".")));
                }
                return check_call(id, object, name, path, in_template);
            }
            for (key, item) in object {
                path.push(key.clone());
                walk_value(id, item, path, in_template)?;
                path.pop();
            }
            Ok(())
        }
        Value::Array(items) => {
            for (index, item) in items.iter().enumerate() {
                path.push(index.to_string());
                walk_value(id, item, path, in_template)?;
                path.pop();
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// AUI-FR-HCCU / AUI-FR-PXGX: a call names a catalog function with its
/// arguments, and `@index` stands inside a template alone.
fn check_call(
    id: &str,
    object: &Map<String, Value>,
    name: &Value,
    path: &mut Vec<String>,
    in_template: bool,
) -> Result<(), SurfaceViolation> {
    let catalog = catalog();
    let name_text = name.as_str().unwrap_or("");
    let is_index = name_text == "@index";
    if !is_index && catalog.function(name_text).is_none() {
        let mut names: Vec<&str> = catalog
            .document()
            .get("functions")
            .and_then(Value::as_object)
            .map(|f| f.keys().map(String::as_str).collect())
            .unwrap_or_default();
        names.push("@index");
        return Err(SurfaceViolation::new(
            "unknown_function",
            format!("a function of the catalog: {}", names.join(", ")),
        )
        .at(id)
        .on(path.join(".")));
    }
    if is_index && !in_template {
        return Err(SurfaceViolation::new(
            "index_outside_template",
            "`@index` only inside a template",
        )
        .at(id)
        .on(path.join(".")));
    }
    // The arguments first, so a call nested in a call is named itself.
    if let Some(args) = object.get("args") {
        path.push("args".to_string());
        walk_value(id, args, path, in_template)?;
        path.pop();
    }
    if !is_index {
        let as_value = Value::Object(object.clone());
        if let Err(error) = catalog.check_function(name_text, &as_value) {
            let mut at = path.clone();
            at.extend(error.path);
            return Err(SurfaceViolation::new("bad_call", error.expected).at(id).on(at.join(".")));
        }
    }
    // AUI-FR-GUUJ: a literal address of `openUrl`.
    if name_text == "openUrl" {
        if let Some(url) = object.get("args").and_then(|a| a.get("url")).and_then(Value::as_str) {
            if !is_link_url(url) {
                return Err(SurfaceViolation::new(
                    "bad_url",
                    "an `https`, `http`, or `mailto` address",
                )
                .at(id)
                .on(format!("{}.args.url", path.join("."))));
            }
        }
    }
    // AUI contract surface: `length` and `numeric` take `min`, `max`, or both.
    let bounded = matches!(name.as_str(), Some("length") | Some("numeric"));
    let args = object.get("args");
    if bounded && !args.is_some_and(|a| a.get("min").is_some() || a.get("max").is_some()) {
        let mut at = path.clone();
        at.push("args".to_string());
        return Err(SurfaceViolation::new("bad_call", "a call with `min`, `max`, or both")
            .at(id)
            .on(at.join(".")));
    }
    Ok(())
}

/// AUI-FR-LXOY: a JSON Pointer (RFC 6901). A relative one, with no leading
/// `/`, stands inside a template alone.
fn check_pointer(
    id: &str,
    property: &str,
    pointer: &str,
    in_template: bool,
) -> Result<(), SurfaceViolation> {
    let escapes_valid =
        pointer.split('~').skip(1).all(|rest| rest.starts_with('0') || rest.starts_with('1'));
    if !escapes_valid {
        return Err(SurfaceViolation::new(
            "bad_pointer",
            "a JSON Pointer such as `/items/0/name`; `~` is written `~0` and `/` in a key `~1`",
        )
        .at(id)
        .on(property));
    }
    let absolute = pointer.is_empty() || pointer.starts_with('/');
    if !absolute && !in_template {
        return Err(SurfaceViolation::new(
            "relative_path_outside_template",
            "an absolute pointer that starts with `/`; a relative one only inside a template",
        )
        .at(id)
        .on(property));
    }
    Ok(())
}
