//! The delivery settings (PSS-FR-KZGG, PSS-FR-KZYD).
//!
//! The settings are project-public: the delivery source choice and the
//! mapping between issue Types and plan kinds are the same for every checkout
//! of the project. They live in a table of their own, so a write of this table
//! carries every other section through unchanged, and a write of another
//! section carries this one through.
//!
//! The model is `crate::delivery::settings`. `DSR-delivery-source.md` is the
//! only caller, and no function here is a Tauri command.

use crate::delivery::settings::{
    CreateTypes, DeliverySettings, DeliverySourceChoice, MappedKind, ReadEntry,
};

use super::{load_public, save_public};

/// PSS-FR-KZGG: the project-public table the delivery settings live in.
const DELIVERY_KEY: &str = "delivery";
const SOURCE_KEY: &str = "source";
const SOURCE_KIND_KEY: &str = "kind";
const SOURCE_FLOW_ID_KEY: &str = "flowId";
const CREATE_TYPES_KEY: &str = "createTypes";
const READ_ENTRIES_KEY: &str = "readEntries";
const ENTRY_TYPE_KEY: &str = "type";
const ENTRY_KIND_KEY: &str = "kind";
const UNTYPED_KEY: &str = "untyped";

/// The four create Type keys and the field of each, in the order the table
/// writes them.
fn create_type_fields(types: &mut CreateTypes) -> [(&'static str, &mut String); 4] {
    [
        ("epic", &mut types.epic),
        ("feature", &mut types.feature),
        ("task", &mut types.task),
        ("defect", &mut types.defect),
    ]
}

/// A string value that is not blank.
fn text(value: Option<&toml::Value>) -> Option<&str> {
    value.and_then(toml::Value::as_str).map(str::trim).filter(|s| !s.is_empty())
}

/// PSS-FR-KZGG: the source choice. An unset or unreadable choice, and a
/// `file` choice that names no artifact, read as `automatic`.
fn read_source(value: Option<&toml::Value>) -> DeliverySourceChoice {
    let Some(table) = value.and_then(toml::Value::as_table) else {
        return DeliverySourceChoice::Automatic;
    };
    match text(table.get(SOURCE_KIND_KEY)) {
        Some("github") => DeliverySourceChoice::Github,
        Some("file") => match text(table.get(SOURCE_FLOW_ID_KEY)) {
            Some(flow_id) => DeliverySourceChoice::File { flow_id: flow_id.to_string() },
            None => DeliverySourceChoice::Automatic,
        },
        _ => DeliverySourceChoice::Automatic,
    }
}

/// PSS-FR-KZGG: the create Types. A value that is unset, is not a string, or
/// is blank reads as its default, one field at a time.
fn read_create_types(value: Option<&toml::Value>) -> CreateTypes {
    let mut types = CreateTypes::default();
    let Some(table) = value.and_then(toml::Value::as_table) else { return types };
    for (key, field) in create_type_fields(&mut types) {
        if let Some(name) = text(table.get(key)) {
            *field = name.to_string();
        }
    }
    types
}

/// PSS-FR-KZGG: the read entries, in the stored order. An entry without a Type
/// name or with a kind outside the enumeration is not read.
fn read_entries(value: Option<&toml::Value>) -> Vec<ReadEntry> {
    let Some(entries) = value.and_then(toml::Value::as_array) else { return Vec::new() };
    entries
        .iter()
        .filter_map(toml::Value::as_table)
        .filter_map(|entry| {
            let type_name = text(entry.get(ENTRY_TYPE_KEY))?;
            let kind = MappedKind::parse(text(entry.get(ENTRY_KIND_KEY))?)?;
            Some(ReadEntry { type_name: type_name.to_string(), kind })
        })
        .collect()
}

/// PSS-FR-KZGG: the stored settings, with defaults applied. A malformed
/// `project.toml` is the typed error of PSS-FR-10.
pub fn load_delivery_settings_from(root: &crate::fs::RootFs) -> Result<DeliverySettings, String> {
    let table = load_public(root)?;
    let Some(section) = table.get(DELIVERY_KEY).and_then(toml::Value::as_table) else {
        return Ok(DeliverySettings::default());
    };
    Ok(DeliverySettings {
        source: read_source(section.get(SOURCE_KEY)),
        create_types: read_create_types(section.get(CREATE_TYPES_KEY)),
        read_entries: read_entries(section.get(READ_ENTRIES_KEY)),
        untyped: text(section.get(UNTYPED_KEY)).and_then(MappedKind::parse),
    })
}

/// PSS-FR-KZGG / PSS-FR-KZYD: persist the settings in their own table, in
/// place of the table stored before. Every other project-public section is
/// carried through unchanged (PSS-FR-17). The caller validates the settings
/// (DSR-FR-NFFP, DSR-FR-CPYM); this write stores what it is given.
pub fn save_delivery_settings_to(
    root: &crate::fs::RootFs,
    settings: &DeliverySettings,
) -> Result<(), String> {
    let mut table = load_public(root)?;
    let mut section = toml::Table::new();

    let mut source = toml::Table::new();
    source.insert(SOURCE_KIND_KEY.into(), toml::Value::String(settings.source.as_str().into()));
    if let DeliverySourceChoice::File { flow_id } = &settings.source {
        source.insert(SOURCE_FLOW_ID_KEY.into(), toml::Value::String(flow_id.clone()));
    }
    section.insert(SOURCE_KEY.into(), toml::Value::Table(source));

    let mut types = toml::Table::new();
    let mut copy = settings.create_types.clone();
    for (key, field) in create_type_fields(&mut copy) {
        types.insert(key.into(), toml::Value::String(field.clone()));
    }
    section.insert(CREATE_TYPES_KEY.into(), toml::Value::Table(types));

    if !settings.read_entries.is_empty() {
        let entries = settings
            .read_entries
            .iter()
            .map(|entry| {
                let mut row = toml::Table::new();
                row.insert(ENTRY_TYPE_KEY.into(), toml::Value::String(entry.type_name.clone()));
                row.insert(ENTRY_KIND_KEY.into(), toml::Value::String(entry.kind.as_str().into()));
                toml::Value::Table(row)
            })
            .collect();
        section.insert(READ_ENTRIES_KEY.into(), toml::Value::Array(entries));
    }
    // An unset untyped choice is an absent key: TOML has no null.
    if let Some(untyped) = settings.untyped {
        section.insert(UNTYPED_KEY.into(), toml::Value::String(untyped.as_str().into()));
    }

    table.insert(DELIVERY_KEY.to_string(), toml::Value::Table(section));
    save_public(root, &table)
}
