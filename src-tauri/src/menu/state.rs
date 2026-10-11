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
}
