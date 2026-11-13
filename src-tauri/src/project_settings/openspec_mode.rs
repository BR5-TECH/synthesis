//! The OpenSpec mode setting (PSS-FR-IMDN, PSS-FR-JXKK, PSS-FR-HVUZ).
//!
//! One project-public value, `auto` or `off`, that `OSM-openspec-mode.md`
//! reads to decide whether OpenSpec mode is active (OSM-FR-UILN). `auto` is
//! the unset state and has no key, so a project that never touched the
//! setting keeps a `project.toml` with no trace of it.

use super::ProjectConfig;

/// The key in `project.toml` (PSS-FR-IMDN).
pub const KEY: &str = "openspecMode";

/// The unset state: OpenSpec mode follows detection.
pub const AUTO: &str = "auto";

/// The one stored value: OpenSpec mode stays inactive.
pub const OFF: &str = "off";

/// PSS-FR-HVUZ: the refusal of a payload that names any other value.
pub const ERR_INVALID: &str = "openspec_mode_invalid";

/// PSS-FR-IMDN, PSS-FR-JXKK: `off` only where the key holds the text `off`.
/// Any other stored value reads as `auto`.
pub fn read(table: &toml::Table) -> &'static str {
    match table.get(KEY).and_then(|v| v.as_str()) {
        Some(OFF) => OFF,
        _ => AUTO,
    }
}

/// PSS-FR-HVUZ: a named value other than `auto` and `off` refuses the whole
/// write before anything is read or written.
pub fn validate(config: &ProjectConfig) -> Result<(), String> {
    match config.openspec_mode.as_deref() {
        None | Some(AUTO) | Some(OFF) => Ok(()),
        Some(_) => Err(ERR_INVALID.to_string()),
    }
}

/// PSS-FR-HVUZ: `off` is stored, `auto` removes the key, and an absent value
/// leaves the key as it stands.
///
/// PSS-FR-JXKK: a stored value that is not `off` is dropped on every write of
/// the store, whether or not the payload names the setting.
pub fn write_into(table: &mut toml::Table, config: &ProjectConfig) {
    match config.openspec_mode.as_deref() {
        Some(OFF) => {
            table.insert(KEY.to_string(), toml::Value::String(OFF.to_string()));
        }
        Some(_) => {
            table.remove(KEY);
        }
        None => {
            if read(table) != OFF {
                table.remove(KEY);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(text: &str) -> toml::Table {
        toml::from_str(text).unwrap()
    }

    fn named(value: &str) -> ProjectConfig {
        ProjectConfig { openspec_mode: Some(value.to_string()), ..Default::default() }
    }

    // PSS-FR-IMDN: an absent key reads as `auto`, and `off` reads as `off`.
    #[test]
    fn an_absent_key_reads_auto_and_off_reads_off() {
        assert_eq!(read(&toml::Table::new()), AUTO);
        assert_eq!(read(&table("openspecMode = \"off\"\n")), OFF);
    }

    // PSS-FR-JXKK: any stored value other than the text `off` reads as `auto`.
    #[test]
    fn a_malformed_stored_value_reads_auto() {
        for text in ["openspecMode = \"auto\"\n", "openspecMode = \"OFF\"\n", "openspecMode = 0\n"]
        {
            assert_eq!(read(&table(text)), AUTO, "{text}");
        }
    }

    // PSS-FR-JXKK: the next write drops a malformed value, also when the
    // payload does not name the setting.
    #[test]
    fn a_write_that_names_nothing_drops_a_malformed_value_and_keeps_off() {
        let mut malformed = table("openspecMode = \"sometimes\"\nother = 1\n");
        write_into(&mut malformed, &ProjectConfig::default());
        assert!(!malformed.contains_key(KEY));
        assert!(malformed.contains_key("other"));

        let mut off = table("openspecMode = \"off\"\n");
        write_into(&mut off, &ProjectConfig::default());
        assert_eq!(read(&off), OFF);
    }

    // PSS-FR-HVUZ: `off` is stored, and `auto` removes the key.
    #[test]
    fn off_is_stored_and_auto_removes_the_key() {
        let mut t = toml::Table::new();
        write_into(&mut t, &named(OFF));
        assert_eq!(t.get(KEY).and_then(|v| v.as_str()), Some(OFF));
        write_into(&mut t, &named(AUTO));
        assert!(!t.contains_key(KEY));
    }

    // PSS-FR-HVUZ: a value other than `auto` and `off` is refused.
    #[test]
    fn another_value_is_refused() {
        assert_eq!(validate(&named("on")), Err(ERR_INVALID.to_string()));
        assert_eq!(validate(&named("")), Err(ERR_INVALID.to_string()));
        assert_eq!(validate(&named(AUTO)), Ok(()));
        assert_eq!(validate(&named(OFF)), Ok(()));
        assert_eq!(validate(&ProjectConfig::default()), Ok(()));
    }
}
