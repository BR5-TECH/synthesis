//! The decoded `AppSecrets` object and the path operations on it
//! (`specifications/core/ASV-application-secret-vault.md` ASV-FR-02 to ASV-FR-07,
//! ASV-FR-17 to ASV-FR-19). Pure functions: no keyring, no lock, no cache.

use serde_json::{Map, Value};

use super::{FIELD_QUARANTINE, FIELD_VERSION, SCHEMA_VERSION};

pub(super) enum Decoded {
    Object(Map<String, Value>),
    Unsupported,
    Malformed,
}

/// An `AppSecrets` object of the current version holding nothing.
pub(super) fn empty_object() -> Map<String, Value> {
    let mut object = Map::new();
    object.insert(FIELD_VERSION.to_string(), Value::from(SCHEMA_VERSION));
    object
}

/// Decide what a stored value is: a decodable object of a version this build
/// knows, an object from a newer build, or something that is neither.
pub(super) fn decode(raw: &str) -> Decoded {
    let value: Value = match serde_json::from_str(raw) {
        Ok(value) => value,
        Err(_) => return Decoded::Malformed,
    };
    let Value::Object(object) = value else {
        return Decoded::Malformed;
    };
    match object.get(FIELD_VERSION).and_then(Value::as_u64) {
        Some(version) if version == SCHEMA_VERSION => Decoded::Object(object),
        Some(version) if version > SCHEMA_VERSION => Decoded::Unsupported,
        // No version at all, a version of zero, or a version that is not a
        // number: not a well-formed `AppSecrets` object (ASV-FR-17).
        _ => Decoded::Malformed,
    }
}

/// ASV-FR-07: the secret at `path`, or `None` where any level of it is absent
/// or is not the shape the path implies.
///
/// ASV-FR-18: no path resolves the reserved `quarantine` field, whatever it is
/// asked for.
pub(super) fn lookup(object: &Map<String, Value>, path: &[String]) -> Option<String> {
    if !addressable(path) {
        return None;
    }
    let (leaf, parents) = path.split_last()?;
    let mut current = object;
    for key in parents {
        current = current.get(key)?.as_object()?;
    }
    current.get(leaf)?.as_str().map(str::to_string)
}

/// Write `secret` at `path`, creating every absent object between the root and
/// the leaf (ASV-FR-05, ASV-FR-07).
///
/// Returns `false` where a level of the path is occupied by a value that is not
/// an object — replacing it would destroy whatever sits there.
pub(super) fn insert_at(object: &mut Map<String, Value>, path: &[String], secret: &str) -> bool {
    if !addressable(path) {
        return false;
    }
    let Some((leaf, parents)) = path.split_last() else {
        return false;
    };
    let mut current = object;
    for key in parents {
        let entry = current
            .entry(key.clone())
            .or_insert_with(|| Value::Object(Map::new()));
        match entry.as_object_mut() {
            Some(next) => current = next,
            None => return false,
        }
    }
    match current.get(leaf) {
        // The leaf exists and is not a secret string; replacing it would drop a
        // whole namespace a later build owns (ASV-FR-06).
        Some(existing) if !existing.is_string() => false,
        _ => {
            current.insert(leaf.clone(), Value::String(secret.to_string()));
            true
        }
    }
}

/// Remove the leaf at `path`. Removing a leaf that is not there succeeds, which
/// is what makes a `Remove` idempotent.
///
/// Intermediate objects left empty are kept rather than pruned: an empty
/// namespace and an absent one read identically (ASV-FR-07), and pruning would
/// risk removing an object a build that does not know the namespace owns.
pub(super) fn remove_at(object: &mut Map<String, Value>, path: &[String]) {
    if !addressable(path) {
        return;
    }
    let Some((leaf, parents)) = path.split_last() else {
        return;
    };
    let mut current = object;
    for key in parents {
        match current.get_mut(key).and_then(Value::as_object_mut) {
            Some(next) => current = next,
            None => return,
        }
    }
    current.remove(leaf);
}

/// Whether a path names something this module will read or write.
///
/// ASV-FR-18: `quarantine` is a reserved root field rather than a namespace, so
/// no `SecretPath` addresses it. The version field is reserved the same way —
/// a path that overwrote it would make the entry undecodable.
pub(super) fn addressable(path: &[String]) -> bool {
    match path.first() {
        Some(root) => root != FIELD_QUARANTINE && root != FIELD_VERSION,
        None => false,
    }
}
