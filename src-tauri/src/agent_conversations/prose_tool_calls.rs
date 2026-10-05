//! A tool call written as prose (CVL-FR-JPHT).
//!
//! Some models emit the arguments of a call as the text of their reply and ask
//! for no tool. Delivering that text shows the author raw JSON in place of what
//! the tool would have drawn, so such a reply supplies no usable prose and is
//! retried as an empty one (CVL-FR-UJXD).

/// CVL-FR-JPHT: the name of the attached tool whose call `text` is, written as
/// prose, or `None` where the text is anything else.
///
/// The text qualifies when, trimmed and with one enclosing code fence removed,
/// it is exactly one JSON object, and that object either fits one attached
/// tool's argument schema or names an attached tool in `name` beside an
/// `arguments` or `parameters` object.
pub(super) fn tool_written_as_prose<'a>(
    text: &str,
    tools: &'a [rig::completion::ToolDefinition],
) -> Option<&'a str> {
    let body = without_fence(text.trim());
    let mut values = serde_json::Deserializer::from_str(body).into_iter::<serde_json::Value>();
    let value = values.next()?.ok()?;
    if !body[values.byte_offset()..].trim().is_empty() {
        return None;
    }
    let object = value.as_object()?;

    if let Some(name) = object.get("name").and_then(serde_json::Value::as_str) {
        let wraps_arguments = ["arguments", "parameters"]
            .iter()
            .any(|key| object.get(*key).is_some_and(serde_json::Value::is_object));
        if wraps_arguments {
            if let Some(tool) = tools.iter().find(|tool| tool.name == name) {
                return Some(tool.name.as_str());
            }
        }
    }

    tools
        .iter()
        .find(|tool| fits_schema(object, &tool.parameters))
        .map(|tool| tool.name.as_str())
}

/// CVL-FR-JPHT: `text` without one code fence that encloses all of it — an
/// opening line of three backticks, with or without a language tag, and a
/// closing line of three backticks. Text that no fence encloses is returned
/// unchanged.
fn without_fence(text: &str) -> &str {
    let Some(opened) = text.strip_prefix("```") else {
        return text;
    };
    let Some(closed) = opened.strip_suffix("```") else {
        return text;
    };
    match closed.find('\n') {
        Some(line_end) => closed[line_end + 1..].trim(),
        None => text,
    }
}

/// CVL-FR-JPHT: whether `object` holds every key `schema` requires and no key
/// that `schema` does not name. A schema with no `properties` member fits
/// nothing. One whose `properties` is empty and that requires nothing fits the
/// empty object alone, which is that tool's call written out.
fn fits_schema(
    object: &serde_json::Map<String, serde_json::Value>,
    schema: &serde_json::Value,
) -> bool {
    let Some(properties) = schema.get("properties").and_then(serde_json::Value::as_object) else {
        return false;
    };
    let required_present = schema
        .get("required")
        .and_then(serde_json::Value::as_array)
        .map(|required| {
            required
                .iter()
                .filter_map(serde_json::Value::as_str)
                .all(|key| object.contains_key(key))
        })
        .unwrap_or(true);
    required_present && object.keys().all(|key| properties.contains_key(key))
}
