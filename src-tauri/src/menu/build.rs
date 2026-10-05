//! Building the application menu and installing it on the app (SNV-FR-14 / SNV-FR-23).

use super::*;

/// Build the application menu: an app submenu (Quit), a File submenu
/// (SNV-FR-23), and an Edit submenu with the six standard roles (SNV-FR-14).
pub(super) fn build_app_menu<R: tauri::Runtime, M: tauri::Manager<R>>(
    manager: &M,
) -> tauri::Result<BuiltMenu<R>> {
    use tauri::menu::{MenuBuilder, MenuItem, PredefinedMenuItem, SubmenuBuilder};

    // Build one custom item from its spec. Save and Save All render disabled
    // until the frontend reports something unsaved (SNV-FR-28 / SNV-FR-30).
    let item = |(id, label, accel): MenuItemSpec| {
        MenuItem::with_id(manager, id, label, starts_enabled(id), accel)
    };

    // SWN-FR-14: which submenu carries the two settings entries is the
    // platform's own convention, so both menus are built from the same order
    // tables with this one flag deciding between them.
    let macos = cfg!(target_os = "macos");
    // The Project settings entry is held onto wherever it lands: it is taken
    // out of its menu while no project is open and put back when one opens
    // (SWN-FR-16), and a submenu's items cannot be looked up by id afterwards.
    let mut project_settings = None;

    // Quit is a custom item, not `PredefinedMenuItem::quit`: the predefined role
    // hands the quit to the platform (on macOS, `NSApp terminate:`), which tears
    // the process down without ever reaching this side — so the pending Editor
    // changes EDT-FR-33 requires to be written first would be lost. Routing it
    // through `on_menu_event` is what makes the quit holdable.
    let mut app_builder = SubmenuBuilder::new(manager, "Synthesis");
    for spec in app_menu_order(macos) {
        // SNV-FR-23: a separator precedes About.
        if spec.0 == MENU_ABOUT {
            app_builder = app_builder.item(&PredefinedMenuItem::separator(manager)?);
        }
        let built = item(spec)?;
        if spec.0 == MENU_PROJECT_SETTINGS {
            project_settings = Some(built.clone());
        }
        app_builder = app_builder.item(&built);
    }
    let app_menu = app_builder.build()?;

    // File menu (SNV-FR-23): the custom items in `file_menu_order`, then a
    // separator, About on platforms where the File menu carries it, and Exit.
    // Exit carries the platform-standard quit accelerator (SNV-FR-26). Save and Save All are held onto as well: their enabled state
    // changes for the life of the application (SNV-FR-28 / SNV-FR-30), and a
    // submenu's items cannot be looked up by id afterwards.
    let mut file_builder = SubmenuBuilder::new(manager, "File");
    let mut save = None;
    let mut save_all = None;
    for spec in file_menu_order(macos) {
        let built = item(spec)?;
        match save_slot(spec.0) {
            Some(SaveSlot::Save) => save = Some(built.clone()),
            Some(SaveSlot::SaveAll) => save_all = Some(built.clone()),
            None => {}
        }
        if spec.0 == MENU_PROJECT_SETTINGS {
            project_settings = Some(built.clone());
        }
        file_builder = file_builder.item(&built);
    }
    // SNV-FR-23: a separator, then About (off macOS), then Exit.
    file_builder = file_builder.item(&PredefinedMenuItem::separator(manager)?);
    for spec in file_menu_tail(macos) {
        file_builder = file_builder.item(&item(spec)?);
    }
    let file_menu = file_builder.build()?;

    let settings_owner = Some(if settings_menu_owner(macos) == SettingsMenuOwner::AppMenu {
        app_menu.clone()
    } else {
        file_menu.clone()
    });

    // Edit menu (SNV-FR-14): the six predefined roles, then a separator and the
    // two Editor-scoped find items (SNV-FR-43), whose handles are kept for the
    // same reason the save ones are — their enabled state changes for the life
    // of the application and a submenu's items cannot be looked up by id.
    let mut edit_builder = SubmenuBuilder::new(manager, "Edit")
        .item(&PredefinedMenuItem::undo(manager, Some("Undo"))?)
        .item(&PredefinedMenuItem::redo(manager, Some("Redo"))?)
        .item(&PredefinedMenuItem::separator(manager)?)
        .item(&PredefinedMenuItem::cut(manager, Some("Cut"))?)
        .item(&PredefinedMenuItem::copy(manager, Some("Copy"))?)
        .item(&PredefinedMenuItem::paste(manager, Some("Paste"))?)
        .item(&PredefinedMenuItem::select_all(manager, Some("Select All"))?)
        .item(&PredefinedMenuItem::separator(manager)?);
    let mut find = None;
    let mut find_replace = None;
    for spec in EDIT_MENU_ITEMS {
        let built = item(*spec)?;
        match find_slot(spec.0) {
            Some(FindSlot::Find) => find = Some(built.clone()),
            Some(FindSlot::FindReplace) => find_replace = Some(built.clone()),
            None => {}
        }
        edit_builder = edit_builder.item(&built);
    }
    let edit_menu = edit_builder.build()?;

    // `set_menu` is a full replacement, so we re-provide the standard Window
    // submenu (Minimize/Zoom/Close) the OS default menu would otherwise carry —
    // otherwise installing our Edit menu would silently drop it on macOS.
    let window_menu = SubmenuBuilder::new(manager, "Window")
        .item(&PredefinedMenuItem::minimize(manager, Some("Minimize"))?)
        .item(&PredefinedMenuItem::maximize(manager, Some("Zoom"))?)
        .item(&PredefinedMenuItem::separator(manager)?)
        .item(&PredefinedMenuItem::close_window(manager, Some("Close"))?)
        .build()?;

    let menu = MenuBuilder::new(manager)
        .item(&app_menu)
        .item(&file_menu)
        .item(&edit_menu)
        .item(&window_menu)
        .build()?;

    Ok(BuiltMenu {
        menu,
        save,
        save_all,
        find,
        find_replace,
        project_settings,
        settings_owner,
    })
}

/// Install the native Edit menu on the app (SNV-FR-14). Best-effort: a menu
/// failure is logged and never blocks startup.
pub fn install_app_menu<R: tauri::Runtime>(app: &tauri::App<R>) -> tauri::Result<()> {
    let built = build_app_menu(app)?;
    // Keep the live-enablement handles before the menu is handed over, so
    // `set_save_menu_state` has something to act on (SNV-FR-28 / SNV-FR-30).
    app.state::<SaveMenuItems<R>>()
        .store(built.save, built.save_all);
    app.state::<FindMenuItems<R>>()
        .store(built.find, built.find_replace);
    // SWN-FR-16: the Project settings entry and the submenu it sits in, so it
    // can be taken out while no project is open. It is built present and taken
    // out by the first `set_project_settings_present(false)` the picker's own
    // startup makes, rather than built absent — the two menus are then one
    // order table rather than two.
    app.state::<SettingsMenuItems<R>>()
        .store(built.project_settings, built.settings_owner);
    app.set_menu(built.menu)?;
    Ok(())
}
