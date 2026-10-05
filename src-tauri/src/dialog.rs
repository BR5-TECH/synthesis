//! OS-native folder picker (FSA-FR-01).
//!
//! Thin Tauri command wrapping `tauri-plugin-dialog`. The cancel/select
//! mapping lives in `fs::map_dialog_result` so it stays free of Tauri types and
//! is unit-testable without a real dialog.

use crate::fs;
use tauri_plugin_dialog::DialogExt;

/// FSA-FR-01: open the OS-native folder picker via the Tauri `dialog` plugin and
/// return the selected absolute path or a `cancelled` sentinel. Cancellation
/// is NEVER surfaced as an error — it is a normal outcome carried in the
/// return type (see `fs::BrowseResult`).
///
/// We run the picker via `blocking_pick_folder` on a `tauri::async_runtime`
/// blocking task so the Tauri command future remains non-blocking from the
/// frontend's perspective while still using the synchronous dialog API
/// (which is what `tauri-plugin-dialog` exposes for desktop).
///
/// The mapping from `Option<FilePath>` → `BrowseResult` lives in the `fs`
/// module (`fs::map_dialog_result`) so the cancel/select semantics can be
/// unit-tested without a real dialog. See `src/fs/mod.rs` tests `fr1_*`.
#[tauri::command]
pub async fn browse_for_folder(app: tauri::AppHandle) -> Result<fs::BrowseResult, String> {
    let picked = tauri::async_runtime::spawn_blocking(move || {
        let dialog = app.dialog().file();
        // The dialog plugin returns `Option<FilePath>`. We coerce to
        // `Option<PathBuf>` here to keep `fs::map_dialog_result` free of
        // Tauri-specific types (which matters for the synthesis-core split).
        dialog
            .blocking_pick_folder()
            .and_then(|fp| fp.into_path().ok())
    })
    .await
    .map_err(|e| format!("dialog task failed: {e}"))?;
    Ok(fs::map_dialog_result(picked))
}

/// The file counterpart of `browse_for_folder`, on the same contract: the
/// selected absolute path or the `cancelled` sentinel, never an error for a
/// dismissal.
///
/// Its one caller today is the AI integrations section's binary field
/// (`../ui/AII-ai-integrations.md` AII-FR-05), which lets the author point at a
/// CLI the application could not detect. No filter is applied — an executable
/// has no extension on Unix, and a filter that hid extensionless files would
/// hide exactly what this picker exists to select.
#[tauri::command]
pub async fn browse_for_file(app: tauri::AppHandle) -> Result<fs::BrowseResult, String> {
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .blocking_pick_file()
            .and_then(|fp| fp.into_path().ok())
    })
    .await
    .map_err(|e| format!("dialog task failed: {e}"))?;
    Ok(fs::map_dialog_result(picked))
}

/// FSA-FR-16: open the OS-native save-file dialog with `default_name` pre-filled
/// and return the chosen absolute path or the `cancelled` sentinel. On the same
/// contract as `browse_for_folder`: a dismissal is never an error.
///
/// It resolves a destination and writes nothing itself, so a caller that never
/// follows up leaves the filesystem untouched. Its one caller today is the Logs
/// panel's export control (`../ui/LOG-logs.md` LOG-FR-17), which hands the path
/// to `logging::export_logs`.
#[tauri::command]
pub async fn browse_for_save_path(
    app: tauri::AppHandle,
    default_name: String,
) -> Result<fs::BrowseResult, String> {
    let picked = tauri::async_runtime::spawn_blocking(move || {
        app.dialog()
            .file()
            .set_file_name(default_name)
            .blocking_save_file()
            .and_then(|fp| fp.into_path().ok())
    })
    .await
    .map_err(|e| format!("dialog task failed: {e}"))?;
    Ok(fs::map_dialog_result(picked))
}

#[cfg(test)]
mod tests {
    use crate::fs;

    // MA-4 / FSA-FR-01: pin the wire shape of `browse_for_folder`'s cancellation
    // outcome end-to-end through the same `map_dialog_result` pipeline the
    // Tauri command body uses. We don't mock `tauri-plugin-dialog` (it does
    // not ship a mockable surface for `blocking_pick_folder`); instead we
    // exercise the function body's payload path directly: feed `None`
    // (= user dismissed) through `fs::map_dialog_result` and assert the JSON
    // shape the frontend's `invoke()` consumer will see.
    #[test]
    fn browse_for_folder_cancellation_serializes_as_cancelled_string() {
        // The command's `Ok` branch produces `fs::map_dialog_result(picked)`,
        // where `picked: Option<PathBuf>` is `None` on user dismissal.
        let result: fs::BrowseResult = fs::map_dialog_result(None);
        assert!(matches!(result, fs::BrowseResult::Cancelled));

        // The frontend speaks JSON via `invoke()`. The cancellation outcome
        // must serialize as the bare string `"cancelled"` — externally
        // tagged, no payload — so any client-side `if (result === "cancelled")`
        // (or discriminator switch) keeps working.
        let v = serde_json::to_value(&result).unwrap();
        assert_eq!(v, serde_json::Value::String("cancelled".into()));
    }

    // FSA-FR-16: the save dialog shares `browse_for_folder`'s cancellation
    // contract exactly, so a caller can branch on one shape for both. Asserted
    // through the same `map_dialog_result` pipeline the command body uses,
    // because `tauri-plugin-dialog` ships no mockable `blocking_save_file`.
    #[test]
    fn browse_for_save_path_shares_the_browse_result_contract() {
        assert!(matches!(
            fs::map_dialog_result(None),
            fs::BrowseResult::Cancelled
        ));

        // FSA-FR-16: resolving a destination writes NOTHING at it,
        // so a caller that never follows up leaves the filesystem untouched.
        // Asserted against a real temp path rather than the fixed literal
        // below, which no test may create.
        let tmp = tempfile::TempDir::new().unwrap();
        let destination = tmp.path().join("synthesis-logs.jsonl");
        let resolved = fs::map_dialog_result(Some(destination.clone()));
        assert!(matches!(resolved, fs::BrowseResult::Selected { .. }));
        assert!(
            !destination.exists(),
            "the save dialog resolves a path and creates nothing at it"
        );
        // And the cancellation branch likewise creates nothing anywhere.
        let _ = fs::map_dialog_result(None);
        assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 0);

        let chosen = fs::map_dialog_result(Some(std::path::PathBuf::from(
            "/Users/x/Desktop/synthesis-logs.jsonl",
        )));
        let v = serde_json::to_value(&chosen).unwrap();
        assert_eq!(
            v.get("selected")
                .and_then(|s| s.get("path"))
                .and_then(|p| p.as_str()),
            Some("/Users/x/Desktop/synthesis-logs.jsonl"),
        );
    }

    #[test]
    fn command_functions_are_in_scope() {
        // Compile-time guard: renaming or removing a command without updating
        // `generate_handler!` / `COMMAND_NAMES` fails to compile here.
        let _ = super::browse_for_folder;
        let _ = super::browse_for_file;
        let _ = super::browse_for_save_path;
    }

    // Companion to the test above: pin the selected-path wire shape. A
    // refactor that breaks either branch must fail here, not in the UI.
    #[test]
    fn browse_for_folder_selected_serializes_with_path_field() {
        let picked = Some(std::path::PathBuf::from("/Users/x/dev/proj"));
        let result = fs::map_dialog_result(picked);
        let v = serde_json::to_value(&result).unwrap();
        let inner = v.get("selected").expect("expected `selected` tag");
        assert_eq!(
            inner.get("path").and_then(|p| p.as_str()),
            Some("/Users/x/dev/proj")
        );
    }
}
