//! The menu-item ids, the item specs, and the order tables the menus are built from (SNV-FR-23, SNV-FR-14, SWN-FR-14, SWN-FR-16).

/// Menu-item id AND outgoing event name for File -> New File (SNV-FR-24).
pub(super) const MENU_NEW_FILE: &str = "menu:new-file";
/// Menu-item id AND outgoing event name for File -> New Folder (SNV-FR-24).
pub(super) const MENU_NEW_FOLDER: &str = "menu:new-folder";
/// Menu-item id AND outgoing event name for File -> New Artifact (SNV-FR-24).
pub(super) const MENU_NEW_ARTIFACT: &str = "menu:new-artifact";
/// Menu-item id AND outgoing event name for File -> Save (SNV-FR-28/29).
pub(super) const MENU_SAVE: &str = "menu:save";
/// Menu-item id AND outgoing event name for File -> Save All (SNV-FR-30/31).
pub(super) const MENU_SAVE_ALL: &str = "menu:save-all";
/// Menu-item id AND outgoing event name for File -> Close Project (SNV-FR-25).
pub(super) const MENU_CLOSE_PROJECT: &str = "menu:close-project";
/// Outgoing event name announcing a held application quit (SNV-FR-26). Not a
/// menu id: it is emitted for every quit request whatever its source (the Exit
/// item, the app submenu's Quit, or closing the window).
pub(super) const MENU_EXIT_REQUESTED: &str = "menu:exit-requested";
/// Menu-item id for File -> Exit (SNV-FR-26).
pub(super) const MENU_EXIT: &str = "menu:exit";
/// Menu-item id for the app submenu's Quit item. A second entry point to the
/// same action (SNV-FR-26), kept so macOS's application menu carries the quit
/// item users expect to find there.
pub(super) const MENU_QUIT: &str = "menu:quit";
/// Menu-item id AND outgoing event name for Edit -> Find (SNV-FR-43).
pub(super) const MENU_FIND: &str = "menu:find";
/// Menu-item id AND outgoing event name for Edit -> Find & Replace (SNV-FR-43).
pub(super) const MENU_FIND_REPLACE: &str = "menu:find-replace";
/// Menu-item id for the Global settings entry (SWN-FR-14). Not an event name:
/// the entry opens a native child window from this side rather than relaying
/// anything to the frontend (`SWN-settings-windows.md`).
pub(super) const MENU_GLOBAL_SETTINGS: &str = "menu:global-settings";
/// Menu-item id for the Project settings entry (SWN-FR-14, SWN-FR-16).
pub(super) const MENU_PROJECT_SETTINGS: &str = "menu:project-settings";
/// Menu-item id AND outgoing event name for the About entry (SNV-FR-23,
/// ABT-FR-KMVD). The panel it opens is drawn by the frontend, so the entry
/// relays an event rather than opening a window from this side.
pub(super) const MENU_ABOUT: &str = "menu:about";

/// One menu item as `build_app_menu` constructs it: `(id, label, accelerator)`.
/// Every custom item is declared as one of these and built by iterating, so the
/// const tables below are a faithful stand-in for the rendered menu — which
/// cannot be built at all without a GUI runtime.
pub(super) type MenuItemSpec = (&'static str, &'static str, Option<&'static str>);

/// The File menu's first six items, in render order. `build_app_menu` iterates
/// this to construct them, so the order the user sees is exactly what the unit
/// test pins; Exit is appended after these, behind a separator. SNV-FR-23.
///
/// The three creation items lead, in the same order the Library's folder context
/// menu presents them (LCM-FR-01), so the two entry points into creation read
/// identically.
pub(super) const FILE_MENU_ITEMS_BEFORE_SETTINGS: &[MenuItemSpec] = &[
    (MENU_NEW_FILE, "New File", None),
    (MENU_NEW_ARTIFACT, "New Artifact", None),
    (MENU_NEW_FOLDER, "New Folder", None),
    (MENU_SAVE, "Save", Some("CmdOrCtrl+S")),
    (MENU_SAVE_ALL, "Save All", Some("CmdOrCtrl+Shift+S")),
];

/// The File menu's items *after* the pair of settings entries (SNV-FR-23).
pub(super) const FILE_MENU_ITEMS_AFTER_SETTINGS: &[MenuItemSpec] =
    &[(MENU_CLOSE_PROJECT, "Close Project", None)];

/// The two settings entries, in render order (SWN-FR-14): Global settings
/// first, because everything it holds belongs to the user and the machine and
/// is offered whether or not a project is open (SWN-FR-15), where Project
/// settings names something that may not exist (SWN-FR-16).
///
/// They sit in the **app-name menu** on macOS and in the **File menu**, between
/// Save All and Close Project, on every other platform — which is what
/// `settings_menu_owner` decides and `file_menu_order` reflects.
pub(super) const SETTINGS_MENU_ITEMS: &[MenuItemSpec] = &[
    (MENU_GLOBAL_SETTINGS, "Global settings", None),
    (MENU_PROJECT_SETTINGS, "Project settings", None),
];

/// The About entry (SNV-FR-23, ABT-FR-KMVD). It sits in the same submenu as the
/// settings entries: the app-name menu on macOS, the File menu elsewhere. A
/// separator precedes it and it sits immediately before the quit item.
pub(super) const ABOUT_ITEM: MenuItemSpec = (MENU_ABOUT, "About", None);

/// Which submenu carries the two settings entries on this platform (SWN-FR-14).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SettingsMenuOwner {
    /// macOS: the app-name menu, where a platform user looks for them.
    AppMenu,
    /// Every other platform: the File menu, between Save All and Close Project.
    FileMenu,
}

/// True on macOS. A parameter of the pure helpers below rather than a `cfg!`
/// inside them, so a unit test can pin both platforms' menus from either.
pub(super) const fn settings_menu_owner(macos: bool) -> SettingsMenuOwner {
    if macos {
        SettingsMenuOwner::AppMenu
    } else {
        SettingsMenuOwner::FileMenu
    }
}

/// The File menu's custom items in render order, for `macos` or not
/// (SNV-FR-23). Exit is appended after these behind a separator.
pub(super) fn file_menu_order(macos: bool) -> Vec<MenuItemSpec> {
    let mut items: Vec<MenuItemSpec> = FILE_MENU_ITEMS_BEFORE_SETTINGS.to_vec();
    if settings_menu_owner(macos) == SettingsMenuOwner::FileMenu {
        items.extend_from_slice(SETTINGS_MENU_ITEMS);
    }
    items.extend_from_slice(FILE_MENU_ITEMS_AFTER_SETTINGS);
    items
}

/// The File menu's items after the separator that follows `file_menu_order`
/// (SNV-FR-23): About on every platform but macOS, then Exit. The separator
/// stands between Close Project and About, and nothing stands between About
/// and Exit.
pub(super) fn file_menu_tail(macos: bool) -> Vec<MenuItemSpec> {
    let mut items: Vec<MenuItemSpec> = Vec::new();
    if settings_menu_owner(macos) == SettingsMenuOwner::FileMenu {
        items.push(ABOUT_ITEM);
    }
    items.push(FILE_EXIT_ITEM);
    items
}

/// The app-name menu's custom items in render order. On macOS it leads with the
/// two settings entries (SWN-FR-14), then About behind a separator (SNV-FR-23);
/// everywhere else it carries Quit alone.
pub(super) fn app_menu_order(macos: bool) -> Vec<MenuItemSpec> {
    let mut items: Vec<MenuItemSpec> = Vec::new();
    if settings_menu_owner(macos) == SettingsMenuOwner::AppMenu {
        items.extend_from_slice(SETTINGS_MENU_ITEMS);
        items.push(ABOUT_ITEM);
    }
    items.push(APP_QUIT_ITEM);
    items
}

/// Where the Project settings entry sits in the submenu that owns it, so
/// SWN-FR-16 can put it back at the same place it was removed from.
///
/// Derived from the same order tables the menu is built out of, so an item
/// added before it cannot silently move the insertion point.
pub(super) fn project_settings_index(macos: bool) -> usize {
    let order = if settings_menu_owner(macos) == SettingsMenuOwner::AppMenu {
        app_menu_order(macos)
    } else {
        file_menu_order(macos)
    };
    order
        .iter()
        .position(|(id, _, _)| *id == MENU_PROJECT_SETTINGS)
        .unwrap_or(0)
}

/// The Edit menu's custom items, appended after the predefined roles behind a
/// separator, in render order (SNV-FR-43). Both carry their platform-standard
/// accelerator; greying them is what makes ⌘F and ⌘R inert outside an Editor.
///
/// A lone `&` in a label is a mnemonic marker the menu layer strips from the
/// rendered text, so a literal ampersand is escaped as `&&` — "Find && Replace"
/// is what puts "Find & Replace" in front of the user.
pub(super) const EDIT_MENU_ITEMS: &[MenuItemSpec] = &[
    (MENU_FIND, "Find", Some("CmdOrCtrl+F")),
    (MENU_FIND_REPLACE, "Find && Replace", Some("CmdOrCtrl+R")),
];

/// Items that render *disabled* (SNV-FR-28 / SNV-FR-30 / SNV-FR-43). Save and
/// Save All are enabled only while something is unsaved, and Find / Find &
/// Replace only while an Editor is the active surface — facts only the frontend
/// knows. At startup none holds: no project is mounted and no tab is open, so
/// all four start greyed out and their `set_*_menu_state` commands drive them
/// from there. Building them enabled would leave live accelerators bound to menu
/// items nothing has computed a state for.
pub(super) const INITIALLY_DISABLED: &[&str] =
    &[MENU_SAVE, MENU_SAVE_ALL, MENU_FIND, MENU_FIND_REPLACE];

/// Whether a menu item renders enabled. Pure, so the initial state is pinned
/// without a GUI runtime.
pub(super) fn starts_enabled(id: &str) -> bool {
    !INITIALLY_DISABLED.contains(&id)
}

/// Which live-enablement slot a menu item occupies, if any.
///
/// Split out of `build_app_menu` so the mapping is unit-testable: capturing the
/// handles themselves needs a GUI runtime, and a swap between the two slots
/// would grey out Save when Save All should be greyed — with nothing to catch
/// it, because the visible symptom only exists in a rendered menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SaveSlot {
    Save,
    SaveAll,
}

pub(super) fn save_slot(id: &str) -> Option<SaveSlot> {
    match id {
        MENU_SAVE => Some(SaveSlot::Save),
        MENU_SAVE_ALL => Some(SaveSlot::SaveAll),
        _ => None,
    }
}

/// The same, for the two Edit-menu find items (SNV-FR-43).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FindSlot {
    Find,
    FindReplace,
}

pub(super) fn find_slot(id: &str) -> Option<FindSlot> {
    match id {
        MENU_FIND => Some(FindSlot::Find),
        MENU_FIND_REPLACE => Some(FindSlot::FindReplace),
        _ => None,
    }
}

/// File -> Exit (SNV-FR-26), carrying the platform-standard quit chord. The
/// accelerator lives here rather than on the app submenu's Quit because one
/// chord cannot be bound to two items.
pub(super) const FILE_EXIT_ITEM: MenuItemSpec = (MENU_EXIT, "Exit", Some("CmdOrCtrl+Q"));

/// The app submenu's Quit item: the same action reached a second way, so it
/// carries no accelerator of its own.
pub(super) const APP_QUIT_ITEM: MenuItemSpec = (MENU_QUIT, "Quit", None);
