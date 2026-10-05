//! The managed handles on the menu items whose enabled state, or presence, changes after the menu is installed (SNV-FR-28 / SNV-FR-30 / SWN-FR-16).

use super::*;

/// Handles on the two File-menu items whose enabled state is *live*
/// (SNV-FR-28 / SNV-FR-30). Managed state, populated by `install_app_menu`.
///
/// Kept as handles rather than looked up by id on each call because Tauri's
/// `Menu::get` searches only a menu's direct children, and these live one level
/// down inside the File submenu.
pub struct SaveMenuItems<R: tauri::Runtime> {
    save: Mutex<Option<tauri::menu::MenuItem<R>>>,
    save_all: Mutex<Option<tauri::menu::MenuItem<R>>>,
}

// Hand-written rather than derived: `#[derive(Default)]` would require the
// runtime `R` itself to be `Default`, which no Tauri runtime is.
impl<R: tauri::Runtime> Default for SaveMenuItems<R> {
    fn default() -> Self {
        Self {
            save: Mutex::new(None),
            save_all: Mutex::new(None),
        }
    }
}

impl<R: tauri::Runtime> SaveMenuItems<R> {
    pub(super) fn store(
        &self,
        save: Option<tauri::menu::MenuItem<R>>,
        save_all: Option<tauri::menu::MenuItem<R>>,
    ) {
        *self.save.lock().unwrap_or_else(|p| p.into_inner()) = save;
        *self.save_all.lock().unwrap_or_else(|p| p.into_inner()) = save_all;
    }

    /// Clones out both handles so the caller can touch them off-lock — the
    /// setters hop to the main thread and must not do so holding a mutex.
    pub(super) fn handles(
        &self,
    ) -> (
        Option<tauri::menu::MenuItem<R>>,
        Option<tauri::menu::MenuItem<R>>,
    ) {
        (
            self.save
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone(),
            self.save_all
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone(),
        )
    }
}

/// SNV-FR-28 / SNV-FR-30: set whether Save and Save All are enabled.
///
/// The frontend owns the answer — it holds the buffers and knows which tab is
/// active — and pushes it here whenever it changes. Greying the native item is
/// what disables its accelerator too, so a disabled Save makes ⌘S do nothing
/// (SNV-FR-28) rather than firing an event the frontend would have to discard.
///
/// Menu mutation must happen on the main thread on macOS, so the setters are
/// hopped there. Best-effort throughout: a menu that cannot be updated must not
/// fail the frontend's render path.
#[tauri::command]
pub fn set_save_menu_state<R: tauri::Runtime>(
    save: bool,
    save_all: bool,
    app: tauri::AppHandle<R>,
    items: tauri::State<'_, SaveMenuItems<R>>,
) {
    let (save_item, save_all_item) = items.handles();
    if save_item.is_none() && save_all_item.is_none() {
        return;
    }
    let _ = app.run_on_main_thread(move || {
        if let Some(item) = save_item {
            let _ = item.set_enabled(save);
        }
        if let Some(item) = save_all_item {
            let _ = item.set_enabled(save_all);
        }
    });
}

/// Handles on the two Edit-menu items whose enabled state is *live*
/// (SNV-FR-43). Managed state, populated by `install_app_menu`.
///
/// Separate from `SaveMenuItems` because the two pairs answer to different
/// conditions — Save to whether anything is unsaved, Find to whether an Editor
/// is the active surface — and the frontend recomputes them independently.
pub struct FindMenuItems<R: tauri::Runtime> {
    find: Mutex<Option<tauri::menu::MenuItem<R>>>,
    find_replace: Mutex<Option<tauri::menu::MenuItem<R>>>,
}

// Hand-written for the same reason as `SaveMenuItems`: deriving would require
// the runtime `R` to be `Default`, which no Tauri runtime is.
impl<R: tauri::Runtime> Default for FindMenuItems<R> {
    fn default() -> Self {
        Self {
            find: Mutex::new(None),
            find_replace: Mutex::new(None),
        }
    }
}

impl<R: tauri::Runtime> FindMenuItems<R> {
    pub(super) fn store(
        &self,
        find: Option<tauri::menu::MenuItem<R>>,
        find_replace: Option<tauri::menu::MenuItem<R>>,
    ) {
        *self.find.lock().unwrap_or_else(|p| p.into_inner()) = find;
        *self.find_replace.lock().unwrap_or_else(|p| p.into_inner()) = find_replace;
    }

    /// Clones both handles out so the caller can touch them off-lock — the
    /// setters hop to the main thread and must not do so holding a mutex.
    pub(super) fn handles(
        &self,
    ) -> (
        Option<tauri::menu::MenuItem<R>>,
        Option<tauri::menu::MenuItem<R>>,
    ) {
        (
            self.find.lock().unwrap_or_else(|p| p.into_inner()).clone(),
            self.find_replace
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone(),
        )
    }
}

/// SNV-FR-43: set whether Find and Find & Replace are enabled.
///
/// Both answer to the same condition — an Editor is the active surface — which
/// only the frontend knows, so it pushes the answer here whenever the active tab
/// changes. Greying the native item is what disables its accelerator too, so a
/// disabled Find makes ⌘F do nothing on a Dashboard tab rather than firing an
/// event the frontend would have to discard.
///
/// Menu mutation must happen on the main thread on macOS, so the setters are
/// hopped there. Best-effort throughout: a menu that cannot be updated must not
/// fail the frontend's render path.
#[tauri::command]
pub fn set_find_menu_state<R: tauri::Runtime>(
    enabled: bool,
    app: tauri::AppHandle<R>,
    items: tauri::State<'_, FindMenuItems<R>>,
) {
    let (find, find_replace) = items.handles();
    if find.is_none() && find_replace.is_none() {
        return;
    }
    let _ = app.run_on_main_thread(move || {
        if let Some(item) = find {
            let _ = item.set_enabled(enabled);
        }
        if let Some(item) = find_replace {
            let _ = item.set_enabled(enabled);
        }
    });
}

/// The application menu plus the handles whose enabled state changes later.
/// The two are `Option` so a File menu that somehow declared neither still
/// installs — the menu works, only its live enablement goes inert — rather than
/// unwrapping in a startup path. A unit test pins that both are in fact declared.
pub(super) struct BuiltMenu<R: tauri::Runtime> {
    pub(super) menu: tauri::menu::Menu<R>,
    pub(super) save: Option<tauri::menu::MenuItem<R>>,
    pub(super) save_all: Option<tauri::menu::MenuItem<R>>,
    pub(super) find: Option<tauri::menu::MenuItem<R>>,
    pub(super) find_replace: Option<tauri::menu::MenuItem<R>>,
    /// SWN-FR-16: the Project settings entry and the submenu it lives in, so it
    /// can be taken out while no project is open and put back in the same place.
    pub(super) project_settings: Option<tauri::menu::MenuItem<R>>,
    pub(super) settings_owner: Option<tauri::menu::Submenu<R>>,
}

/// SWN-FR-16: the Project settings entry, the submenu that owns it, and whether
/// it is currently in that submenu. Managed state, populated by
/// `install_app_menu`.
///
/// The entry is **absent** rather than greyed-out while no project is open, so
/// the menu offers nothing that names a project that does not exist. Held as
/// handles for the same reason the Save pair is: a submenu's items cannot be
/// looked up by id once built.
pub struct SettingsMenuItems<R: tauri::Runtime> {
    item: Mutex<Option<tauri::menu::MenuItem<R>>>,
    owner: Mutex<Option<tauri::menu::Submenu<R>>>,
    present: Mutex<bool>,
}

// Hand-written for the same reason as `SaveMenuItems`: deriving would require
// `R: Default`.
impl<R: tauri::Runtime> Default for SettingsMenuItems<R> {
    fn default() -> Self {
        Self {
            item: Mutex::new(None),
            owner: Mutex::new(None),
            // The menu is built with the entry in place; the frontend has not
            // opened a project yet, so the very first `set_project_settings_present`
            // takes it out.
            present: Mutex::new(true),
        }
    }
}

impl<R: tauri::Runtime> SettingsMenuItems<R> {
    pub(super) fn store(
        &self,
        item: Option<tauri::menu::MenuItem<R>>,
        owner: Option<tauri::menu::Submenu<R>>,
    ) {
        *self.item.lock().unwrap_or_else(|p| p.into_inner()) = item;
        *self.owner.lock().unwrap_or_else(|p| p.into_inner()) = owner;
        *self.present.lock().unwrap_or_else(|p| p.into_inner()) = true;
    }

    /// Record the requested presence and report whether it is a change, so a
    /// repeated call does not insert the same item twice.
    pub(super) fn claim(&self, present: bool) -> bool {
        let mut slot = self.present.lock().unwrap_or_else(|p| p.into_inner());
        if *slot == present {
            return false;
        }
        *slot = present;
        true
    }

    pub(super) fn handles(
        &self,
    ) -> (
        Option<tauri::menu::MenuItem<R>>,
        Option<tauri::menu::Submenu<R>>,
    ) {
        (
            self.item
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone(),
            self.owner
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .clone(),
        )
    }
}

/// SWN-FR-16: offer the Project settings entry only while a project is open.
///
/// Driven from the project's own open and close paths rather than from the
/// frontend, because "a project is open" is this side's fact — and because the
/// entry must be gone from the picker's menu before the author can reach for it
/// (`PPK-project-picker.md` PPK-FR-15).
pub fn set_project_settings_present<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    present: bool,
) {
    // `try_state` rather than `state`: this is called from the project's own
    // open and close paths, which run on handles that may not carry the menu at
    // all (a test harness, a headless run). A menu that was never installed has
    // nothing to update, which is not a reason to abort the open.
    let Some(state) = app.try_state::<SettingsMenuItems<R>>() else {
        return;
    };
    if !state.claim(present) {
        return;
    }
    let (item, owner) = state.handles();
    let (Some(item), Some(owner)) = (item, owner) else {
        return;
    };
    let index = project_settings_index(cfg!(target_os = "macos"));
    // Cloned before the closure takes ownership of the rest: the menu is
    // mutated on the main thread, and a failure there has to reach the session
    // log rather than a stream nobody reads once the application is packaged.
    let sink = app.clone();
    let _ = app.run_on_main_thread(move || {
        let result = if present {
            owner.insert(&item, index)
        } else {
            owner.remove(&item)
        };
        if let Err(e) = result {
            // SWN-FR-16: the menu now offers an entry that names a project that
            // may not exist, or withholds one that does. Nothing else reports
            // it, and the author sees only a menu that is subtly wrong.
            crate::logging::log_warn(
                &sink,
                &crate::logging::BUFFER,
                &[crate::logging::Domain::Backend],
                "the Project settings menu entry could not be updated",
                crate::log_fields! { "present" => present, "reason" => e.to_string() },
            );
        }
    });
}
