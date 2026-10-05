//! Font enumeration — `specifications/core/FNT-font-enumeration.md`.
//!
//! Reports which font families this machine has available for rendering text,
//! so the Appearance section of `GLS-global-settings.md` can offer the author
//! the fonts they actually have rather than a curated guess (FNT-FR-01).
//!
//! The module is a **read** and nothing else: it installs nothing, downloads
//! nothing, touches no project file, and persists nothing (FNT-FR-05 /
//! FNT-FR-09). It holds no state between calls either, which is what makes a
//! font installed while the app is running visible to the very next call with
//! no relaunch (FNT-FR-06).
//!
//! Tauri-free by design: `enumerate` takes the face list as data, so the
//! deduplication, ordering, and monospace rules are unit-testable without a
//! Tauri runtime and without depending on whatever fonts the test machine
//! happens to carry.

use serde::{Deserialize, Serialize};

/// One font family available to the application (FNT-FR-02).
///
/// A family, never a face: a typographic role selects a family, so the several
/// weights, italics, and widths a family ships collapse into a single entry.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FontFamily {
    /// The family name as the platform registers it, e.g. `"JetBrains Mono"`.
    pub family: String,
    /// FNT-FR-04: whether the family's faces advance every glyph by a fixed
    /// width. A *description*, never a permission — the Appearance section is
    /// free to offer a proportional family for the Source code role
    /// (`GLS-global-settings.md` GLS-FR-18), and this flag only decides how it
    /// is grouped and marked there.
    pub monospace: bool,
}

/// Collapse a face list into the family list `list_system_fonts` returns.
///
/// Pure, so the three rules that shape the result are testable against fixed
/// input rather than against the test machine's font set:
///
/// 1. **Deduplicate (FNT-FR-02):** one entry per family name. A family whose
///    faces disagree about `monospace` is reported monospace when *any* face
///    is — a family shipping a fixed-width face is a fixed-width family for the
///    purpose of choosing it for code.
/// 2. **Order (FNT-FR-03):** case-insensitive by family name, so a consumer
///    renders the list as returned without sorting again.
/// 3. **Drop the unnamed:** a face whose family name is blank names nothing a
///    user could pick.
pub fn enumerate(faces: impl IntoIterator<Item = (String, bool)>) -> Vec<FontFamily> {
    // Keyed by the case-folded name so `Arial` and `arial` are one family, but
    // the value keeps the first spelling seen so the name a user reads is the
    // one the platform registered.
    let mut by_key: std::collections::HashMap<String, FontFamily> = std::collections::HashMap::new();
    for (family, monospace) in faces {
        let family = family.trim().to_string();
        if family.is_empty() {
            continue;
        }
        let key = family.to_lowercase();
        by_key
            .entry(key)
            .and_modify(|entry| entry.monospace |= monospace)
            .or_insert(FontFamily { family, monospace });
    }

    let mut out: Vec<FontFamily> = by_key.into_values().collect();
    // FNT-FR-03. The case-folded name is the primary key; the raw name breaks
    // ties so the order is total and stable rather than dependent on the hash
    // map's iteration order.
    out.sort_by(|a, b| {
        a.family
            .to_lowercase()
            .cmp(&b.family.to_lowercase())
            .then_with(|| a.family.cmp(&b.family))
    });
    out
}

/// Read the machine's installed font families (FNT-FR-02 / FNT-FR-03).
///
/// FNT-FR-07: never fails. A platform whose font set cannot be enumerated
/// yields an empty list, which the Appearance section reads as "no family
/// beyond the built-in faces is offerable" rather than as an error to surface —
/// so this returns `Vec`, not `Result`, and there is no error path for a caller
/// to forget to handle.
pub fn system_font_families() -> Vec<FontFamily> {
    let mut db = fontdb::Database::new();
    // Reads the platform's own font directories. Nothing is written, and the
    // database is dropped when this returns, so no result is cached across
    // calls (FNT-FR-06 / FNT-FR-09).
    db.load_system_fonts();

    enumerate(db.faces().map(|face| {
        let family = face
            .families
            .first()
            .map(|(name, _language)| name.clone())
            .unwrap_or_default();
        (family, face.monospaced)
    }))
}

/// `"list system fonts"` (FNT-FR-01).
///
/// FNT-FR-10: the answer is a property of the machine, so it is identical
/// whether or not a project is open and whichever worktree is active — nothing
/// here takes a project, a path, or any managed state.
#[tauri::command]
pub fn list_system_fonts() -> Vec<FontFamily> {
    system_font_families()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Pins the command fn in scope so a rename fails to compile here rather
    /// than silently at runtime (the `generate_handler!` registration is
    /// asserted separately in `lib.rs`).
    #[test]
    fn command_is_in_scope() {
        let _ = list_system_fonts;
    }

    #[test]
    fn fnt_ts01_the_command_returns_the_documented_payload_shape() {
        // FNT-FR-01: the Appearance section renders its family controls from
        // this without error, so the shape is what matters — the content is
        // whatever the machine running the test happens to carry, including
        // nothing at all.
        let families = list_system_fonts();
        for entry in &families {
            assert!(
                !entry.family.trim().is_empty(),
                "an entry a user could not pick is not an entry (FNT-FR-02)"
            );
        }
        // The wire shape is what the Appearance section consumes.
        let json = serde_json::to_string(&FontFamily {
            family: "JetBrains Mono".into(),
            monospace: true,
        })
        .unwrap();
        assert_eq!(json, r#"{"family":"JetBrains Mono","monospace":true}"#);
    }

    #[test]
    fn fnt_ts02_a_family_carrying_several_faces_appears_exactly_once() {
        // FNT-FR-02: regular, bold, and italic are three faces of one family,
        // and a typographic role selects a family.
        let families = enumerate([
            ("Inter".to_string(), false),
            ("Inter".to_string(), false),
            ("Inter".to_string(), false),
        ]);
        assert_eq!(
            families,
            vec![FontFamily {
                family: "Inter".into(),
                monospace: false
            }]
        );
    }

    #[test]
    fn fnt_ts03_entries_are_ordered_case_insensitively_by_family_name() {
        // FNT-FR-03: a consumer renders them as returned, without sorting again.
        let families = enumerate([
            ("Zapfino".to_string(), false),
            ("arial".to_string(), false),
            ("Menlo".to_string(), true),
        ]);
        let names: Vec<&str> = families.iter().map(|f| f.family.as_str()).collect();
        assert_eq!(
            names,
            vec!["arial", "Menlo", "Zapfino"],
            "lowercase `arial` sorts before `Menlo`, which an ASCII sort would not do"
        );
    }

    #[test]
    fn fnt_ts04_fixed_width_and_proportional_families_share_one_list() {
        // FNT-FR-02, FNT-FR-04: `monospace` describes the family; both kinds are present
        // in the single returned list, because the flag never restricts which
        // role a family may be chosen for (FNT-FR-04).
        let families = enumerate([
            ("JetBrains Mono".to_string(), true),
            ("Inter".to_string(), false),
        ]);
        assert_eq!(
            families,
            vec![
                FontFamily {
                    family: "Inter".into(),
                    monospace: false
                },
                FontFamily {
                    family: "JetBrains Mono".into(),
                    monospace: true
                },
            ]
        );
    }

    #[test]
    fn a_family_with_one_fixed_width_face_is_reported_fixed_width() {
        // FNT-FR-02 + FNT-FR-04 together: the faces collapse into one entry,
        // and the entry has to say something about width. A family shipping a
        // fixed-width face is offerable for code, so `monospace` is the OR
        // across its faces rather than whichever face happened to be read
        // first — an order-dependent answer would make the Source code role's
        // grouping (GLS-FR-18) flicker between runs.
        let families = enumerate([
            ("Iosevka".to_string(), false),
            ("Iosevka".to_string(), true),
        ]);
        assert_eq!(
            families,
            vec![FontFamily {
                family: "Iosevka".into(),
                monospace: true
            }]
        );
    }

    #[test]
    fn fnt_ts07_a_machine_whose_font_set_cannot_be_read_yields_an_empty_list() {
        // FNT-FR-08 / FNT-FR-07: the command never fails. `enumerate` is the
        // whole of the failure path — a platform that reports no faces reports
        // an empty list, and the type has no error arm for a caller to mishandle.
        assert!(enumerate([]).is_empty());
        assert!(
            enumerate([("".to_string(), false), ("   ".to_string(), true)]).is_empty(),
            "a face naming no family is not an offerable entry"
        );
    }

    #[test]
    fn fnt_ts09_the_answer_does_not_depend_on_a_project() {
        // FNT-FR-10: the same list whether or not a project is open and
        // whichever worktree is active — the command takes no project, no path,
        // and no managed state, so two calls made under any circumstances
        // agree.
        let first = list_system_fonts();
        let second = list_system_fonts();
        assert_eq!(
            first, second,
            "the fonts a machine has installed are a property of the machine"
        );
    }

    #[test]
    fn fnt_ts05_and_ts06_the_module_writes_nothing_and_caches_nothing() {
        // Two claims a behavioural test cannot make, asserted against the
        // source instead:
        //
        // FNT-FR-05 (nothing is written anywhere): calling the command and
        // observing no change proves nothing — a write to a path the test does
        // not know about is invisible. What *is* checkable is that the module
        // contains no write at all.
        //
        // FNT-FR-06 (a font installed while the app runs shows up in the next
        // call): two calls returning equal lists is exactly what a *cached*
        // implementation would also produce, so equality cannot distinguish
        // them. What distinguishes them is process-lifetime state — so assert
        // there is none.
        let source = include_str!("fonts.rs");
        // Only the half above the test module: the tests themselves legitimately
        // mention these names.
        let module = source.split("#[cfg(test)]").next().unwrap();

        for write in ["fs::write", "File::create", "OpenOptions", "create_dir"] {
            assert!(
                !module.contains(write),
                "FNT-FR-05: this module reads and never writes, but it contains `{write}`"
            );
        }
        for cache in ["OnceLock", "LazyLock", "OnceCell", "lazy_static", "thread_local", "static "]
        {
            assert!(
                !module.contains(cache),
                "FNT-FR-06 / FNT-FR-09: the result must reflect the machine at the \
                 moment of the call, so the module may hold no state across calls, \
                 but it contains `{cache}`"
            );
        }
    }

    #[test]
    fn fnt_ts08_an_uninstalled_family_is_simply_absent() {
        // FNT-FR-09: a family the machine no longer has is absent from the
        // result and nothing else happens — this module never validates a
        // stored family and never writes one, so resolving that absence belongs
        // to `GLS-global-settings.md` GLS-FR-22.
        //
        // Asserted as an equality rather than as "does not contain Fira Code":
        // the latter passes against an `enumerate` returning almost anything,
        // which would say nothing about what the Appearance section is handed.
        assert_eq!(
            enumerate([("Inter".to_string(), false)]),
            vec![FontFamily {
                family: "Inter".into(),
                monospace: false
            }],
            "the result is exactly the installed families — a family that was \
             uninstalled is absent because it is not in the input, not because \
             anything here reasoned about it"
        );
    }
}
