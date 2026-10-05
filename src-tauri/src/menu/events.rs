//! The id-to-action mapping and the two event handlers the runtime calls (SNV-FR-24, SNV-FR-25, SNV-FR-26).

use super::*;

/// The intent a File-menu item carries once its native menu event fires. Pure
/// data so the id -> action mapping (`menu_event_action`) is unit-testable
/// without a Tauri runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MenuAction {
    /// Open the New File modal (owned by NFI-new-file.md, SNV-FR-24).
    NewFile,
    /// Open the New Folder modal (owned by NFW-new-folder.md, SNV-FR-24).
    NewFolder,
    /// Open the New Artifact modal (owned by NAW-new-artifact.md, SNV-FR-24).
    NewArtifact,
    /// Write the active tab's content (SNV-FR-29).
    Save,
    /// Write everything holding unsaved changes (SNV-FR-31).
    SaveAll,
    /// Close the open project and return to the Project picker (SNV-FR-25).
    CloseProject,
    /// Quit the whole application (SNV-FR-26).
    Exit,
    /// Open, switch to, or toggle the Editor's Find panel (SNV-FR-43).
    Find,
    /// The same for the Find & Replace panel (SNV-FR-43).
    FindReplace,
    /// Open, focus, or switch to the Global settings child window (SWN-FR-14).
    GlobalSettings,
    /// The same for Project settings, offered only while a project is open
    /// (SWN-FR-16).
    ProjectSettings,
    /// Open the About panel in the window that is showing (SNV-FR-KXPE,
    /// ABT-FR-KMVD).
    About,
}

impl MenuAction {
    /// SWN-FR-02: whether this action survives a settings window being open.
    ///
    /// While one is open the parent window "accepts no pointer, keyboard, or
    /// menu interaction", so every item that acts on the parent is inert. The
    /// two settings entries are not among them — they are how the author
    /// switches between the two windows (SWN-FR-07) and how a second activation
    /// focuses the one already open (SWN-FR-06) — and neither is the quit,
    /// which writes the settings window's pending changes first and then goes
    /// (SWN-FR-19).
    pub(super) fn reaches_past_a_settings_window(self) -> bool {
        matches!(
            self,
            Self::GlobalSettings | Self::ProjectSettings | Self::Exit
        )
    }
}

/// Map a native menu-event id to the menu action it represents, or `None` for
/// any id we do not own (the predefined Edit/Window roles, which the OS
/// dispatches itself). Pure — unit-tested in `tests`.
pub(super) fn menu_event_action(id: &str) -> Option<MenuAction> {
    match id {
        MENU_NEW_FILE => Some(MenuAction::NewFile),
        MENU_NEW_FOLDER => Some(MenuAction::NewFolder),
        MENU_NEW_ARTIFACT => Some(MenuAction::NewArtifact),
        MENU_SAVE => Some(MenuAction::Save),
        MENU_SAVE_ALL => Some(MenuAction::SaveAll),
        MENU_CLOSE_PROJECT => Some(MenuAction::CloseProject),
        MENU_FIND => Some(MenuAction::Find),
        MENU_FIND_REPLACE => Some(MenuAction::FindReplace),
        MENU_GLOBAL_SETTINGS => Some(MenuAction::GlobalSettings),
        MENU_PROJECT_SETTINGS => Some(MenuAction::ProjectSettings),
        MENU_ABOUT => Some(MenuAction::About),
        // Both quit entry points — File -> Exit and the app submenu's Quit —
        // are the same action (SNV-FR-26).
        MENU_EXIT | MENU_QUIT => Some(MenuAction::Exit),
        _ => None,
    }
}

/// Dispatch a native menu event (SNV-FR-24 / SNV-FR-25 / SNV-FR-26). Unowned
/// ids (the predefined Edit/Window roles) are ignored — the OS handles them.
/// Every owned item relays its intent to the frontend and changes no backend
/// state here: Close Project's teardown waits for the frontend's `close_project`
/// call, because the pending Editor writes that must precede it (EDT-FR-33) can
/// only happen while the project is still open, and Exit waits for the
/// frontend's `finish_exit` for the same reason. Emits are best-effort.
pub fn handle_menu_event<R: tauri::Runtime>(app: &tauri::AppHandle<R>, id: &str) {
    let Some(action) = menu_event_action(id) else {
        return;
    };
    // SWN-FR-02: a settings window blocks its parent, and the
    // application menu is the parent's. Everything that would act on the parent
    // is swallowed here rather than relayed to a window that cannot show what it
    // did — a disabled webview still runs its JavaScript, so an unfiltered
    // ⌘S would write while the author believes the window is inert.
    if !action.reaches_past_a_settings_window()
        && crate::settings_window::a_settings_window_is_open(app)
    {
        return;
    }
    let event = match action {
        // SWN-FR-14: the two settings entries open, focus, or switch to their
        // window from this side. They relay nothing: a settings window is a
        // window of the application rather than a surface of the main one, so
        // there is no frontend to ask (SWN-FR-01).
        MenuAction::GlobalSettings => {
            crate::settings_window::request_open(
                app,
                crate::settings_window::SettingsWindowKind::Global,
                None,
            );
            return;
        }
        MenuAction::ProjectSettings => {
            crate::settings_window::request_open(
                app,
                crate::settings_window::SettingsWindowKind::Project,
                None,
            );
            return;
        }
        MenuAction::NewFile => MENU_NEW_FILE,
        MenuAction::NewFolder => MENU_NEW_FOLDER,
        MenuAction::NewArtifact => MENU_NEW_ARTIFACT,
        MenuAction::Save => MENU_SAVE,
        MenuAction::SaveAll => MENU_SAVE_ALL,
        MenuAction::CloseProject => MENU_CLOSE_PROJECT,
        // ABT-FR-KMVD: the panel is drawn inside the window, so this side only
        // relays the intent. It is swallowed above while a settings window is
        // open (ABT-FR-NVQT).
        MenuAction::About => MENU_ABOUT,
        // SNV-FR-43: the panels live entirely in the frontend, so this side
        // contributes the items, their accelerators, and their enabled state
        // and relays the intent (EFR-FR-AYNZ / EFR-FR-BJUY).
        MenuAction::Find => MENU_FIND,
        MenuAction::FindReplace => MENU_FIND_REPLACE,
        MenuAction::Exit => {
            request_exit(app);
            return;
        }
    };
    let _ = app.emit(event, ());
}

/// Ask the frontend to write its pending changes and then quit (SNV-FR-26 /
/// EDT-FR-33). When the request cannot be held — there is no webview left to
/// answer it, or an earlier request went unanswered past its grace period — the
/// application quits now, so neither a wedged frontend nor a missing one can
/// leave the user with an application that refuses to close.
pub(super) fn request_exit<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if !begin_exit(app, &app.state::<ExitGate>(), None) {
        app.exit(0);
    }
}

/// Intercept a request to close a window (SNV-FR-26 / EDT-FR-33). Closing the
/// *last* window quits the application, so it is held exactly like any other
/// quit request — and it has to be held here, while the webview still exists to
/// flush and to answer. The frontend replies with `finish_exit`: `true` quits
/// via `app.exit`, `false` cancels and leaves the window open (EDT-FR-32).
///
/// Closing a window that is not the last one is left alone: it closes the window
/// without quitting, so there is nothing to hold. The application declares a
/// single window today, which makes that branch unreachable — it is here so that
/// adding a second one closes it rather than quitting the application out from
/// under the user.
pub fn handle_window_event<R: tauri::Runtime>(
    window: &tauri::Window<R>,
    event: &tauri::WindowEvent,
) {
    // SWN-FR-08: a settings window owns what happens to itself — closing one
    // writes every pending change in every one of its sections first, which is
    // a different teardown from the application's.
    if crate::settings_window::handle_window_event(window, event) {
        return;
    }
    let tauri::WindowEvent::CloseRequested { api, .. } = event else {
        return;
    };
    let app = window.app_handle();
    // The closing window is still counted here; more than one means the
    // application survives this close.
    if app.webview_windows().len() > 1 {
        return;
    }
    if begin_exit(app, &app.state::<ExitGate>(), None) {
        api.prevent_close();
    }
}
