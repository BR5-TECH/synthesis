//! What the command registry does and does not expose for a draft
//! (DRS-FR-12, DRS-FR-13).

use super::*;


#[test]
fn draft_command_functions_are_in_scope() {
    // Compile-time references: renaming or removing a command without
    // updating `generate_handler!` / `COMMAND_NAMES` fails to compile here.
    // `graduate_draft` is deliberately absent — DRS-FR-18 leaves this module
    // with no command that publishes a prompt at all.
    let _ = list_drafts;
    let _ = create_draft::<tauri::Wry>;
    let _ = search_drafts;
    let _ = open_draft;
    let _ = rename_draft;
    let _ = set_draft_status;
    let _ = delete_draft;
    let _ = load_draft_file_contents;
    let _ = save_draft_file_contents;
}

/// DRS-FR-12 / DRS-FR-13: there is no command that lists a draft's files,
/// creates a file or a folder inside one, renames a path within one, or
/// deletes one.
///
/// Asserted against the registry the frontend can actually reach rather than
/// against this module's own surface: a helper left behind here is dead
/// code, but a *command* left behind is an operation a caller could still
/// use to put a draft into the state DRS-FR-11 forbids.
#[test]
fn no_registered_command_reaches_inside_a_draft() {
    for gone in [
        "list_draft_files",
        "create_draft_file",
        "create_draft_folder",
        "rename_draft_path",
        "delete_draft_path",
    ] {
        assert!(
            !crate::COMMAND_NAMES.contains(&gone),
            "{gone} is still registered",
        );
    }
}
