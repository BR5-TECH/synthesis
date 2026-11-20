//! The Zoom submenu of the Window menu (SNV-FR-LVDP, SNV-FR-QHMA, SNV-FR-IPCN).
//!
//! The editor zoom level lives in the frontend (`EZM-editor-zoom.md`
//! EZM-FR-SOZM). This side builds the items, relays a click to the focused
//! window, and shows the check marks and the enabled state that the frontend
//! pushes with `set_zoom_menu_state`.

use super::*;
use std::sync::Mutex;

/// Zoom In: the next step above the level (EZM-FR-DMLT).
pub(super) const MENU_ZOOM_IN: &str = "menu:zoom-in";
/// Zoom Out: the next step below the level (EZM-FR-DMLT).
pub(super) const MENU_ZOOM_OUT: &str = "menu:zoom-out";
/// Actual Size: the level goes to 100% (EZM-FR-DMLT).
pub(super) const MENU_ZOOM_RESET: &str = "menu:zoom-reset";
/// The event that a step item emits, with `{ "percent": <step> }`. The step
/// items have one id each, but they share this one event.
pub(super) const MENU_ZOOM_SET: &str = "menu:zoom-set";
/// The id of a step item is this prefix and the step, for example
/// `menu:zoom-step:125`.
const ZOOM_STEP_PREFIX: &str = "menu:zoom-step:";

/// EZM-FR-COYK: the zoom steps, in percent.
pub(super) const EDITOR_ZOOM_STEPS: [u32; 13] =
    [50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200, 250, 300];
/// EZM-FR-COYK: the smallest level.
const ZOOM_MIN: u32 = 50;
/// EZM-FR-COYK: the largest level.
const ZOOM_MAX: u32 = 300;

/// SNV-FR-XRTA: the label of the platform's window zoom item. The editor zoom
/// has the name "Zoom", so the window item must not have it too.
pub(super) const WINDOW_ZOOM_LABEL: &str = "Zoom Window";
/// SNV-FR-LVDP: the label of the submenu.
pub(super) const ZOOM_SUBMENU_LABEL: &str = "Zoom";

/// SNV-FR-NZJR: one entry of the Window menu, in the order `build_app_menu`
/// builds them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WindowEntry {
    Minimize,
    /// SNV-FR-XRTA: the platform's window zoom.
    ZoomWindow,
    /// SNV-FR-LVDP: the editor zoom submenu.
    ZoomSubmenu,
    Separator,
    /// Next Tab and Previous Tab (`WINDOW_MENU_ITEMS`).
    TabItems,
}

/// SNV-FR-NZJR: the order of the Window menu. On macOS the system adds its tab
/// commands after these (PJT-FR-WZLQ).
pub(super) const WINDOW_MENU_ORDER: &[WindowEntry] = &[
    WindowEntry::Minimize,
    WindowEntry::ZoomWindow,
    WindowEntry::ZoomSubmenu,
    WindowEntry::Separator,
    WindowEntry::TabItems,
];

/// SNV-FR-LVDP: the three command items, before the separator and the steps.
pub(super) const ZOOM_MENU_ITEMS: &[MenuItemSpec] = &[
    (MENU_ZOOM_IN, "Zoom In", Some("CmdOrCtrl+=")),
    (MENU_ZOOM_OUT, "Zoom Out", Some("CmdOrCtrl+-")),
    (MENU_ZOOM_RESET, "Actual Size", Some("CmdOrCtrl+0")),
];

/// The id of the item of one step.
pub(super) fn zoom_step_id(step: u32) -> String {
    format!("{ZOOM_STEP_PREFIX}{step}")
}

/// The step of a step item id, or `None` when the id is not the id of a step.
pub(super) fn zoom_step_of(id: &str) -> Option<u32> {
    let step: u32 = id.strip_prefix(ZOOM_STEP_PREFIX)?.parse().ok()?;
    EDITOR_ZOOM_STEPS.contains(&step).then_some(step)
}

/// SNV-FR-LVDP: the label of the item of one step.
pub(super) fn zoom_step_label(step: u32) -> String {
    format!("{step}%")
}

/// What each item of the submenu shows for one level and one enabled state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct ZoomMenuPlan {
    pub(super) zoom_in: bool,
    pub(super) zoom_out: bool,
    pub(super) reset: bool,
    pub(super) steps: bool,
    /// The step that has the check mark, or `None` between two steps.
    pub(super) checked: Option<u32>,
}

/// SNV-FR-QHMA / SNV-FR-IPCN: the enabled state and the check mark of each item.
/// Pure, so each case is tested without a GUI runtime.
pub(super) fn zoom_menu_plan(level: u32, enabled: bool) -> ZoomMenuPlan {
    ZoomMenuPlan {
        zoom_in: enabled && level < ZOOM_MAX,
        zoom_out: enabled && level > ZOOM_MIN,
        reset: enabled,
        steps: enabled,
        checked: EDITOR_ZOOM_STEPS.contains(&level).then_some(level),
    }
}

/// The handles of the submenu items. A submenu's items cannot be found by id
/// later, so the builder keeps them.
pub(super) struct ZoomHandles<R: tauri::Runtime> {
    pub(super) zoom_in: tauri::menu::MenuItem<R>,
    pub(super) zoom_out: tauri::menu::MenuItem<R>,
    pub(super) reset: tauri::menu::MenuItem<R>,
    pub(super) steps: Vec<(u32, tauri::menu::CheckMenuItem<R>)>,
}

impl<R: tauri::Runtime> Clone for ZoomHandles<R> {
    fn clone(&self) -> Self {
        Self {
            zoom_in: self.zoom_in.clone(),
            zoom_out: self.zoom_out.clone(),
            reset: self.reset.clone(),
            steps: self.steps.clone(),
        }
    }
}

/// Managed state: the handles, and the last state that the frontend pushed.
pub struct ZoomMenuItems<R: tauri::Runtime> {
    handles: Mutex<Option<ZoomHandles<R>>>,
    last: Mutex<(u32, bool)>,
}

impl<R: tauri::Runtime> Default for ZoomMenuItems<R> {
    fn default() -> Self {
        // The items start disabled, at 100% (SNV-FR-IPCN, EZM-FR-SOZM).
        Self { handles: Mutex::new(None), last: Mutex::new((100, false)) }
    }
}

impl<R: tauri::Runtime> ZoomMenuItems<R> {
    pub(super) fn store(&self, handles: Option<ZoomHandles<R>>) {
        *self.handles.lock().unwrap_or_else(|p| p.into_inner()) = handles;
    }

    fn remember(&self, level: u32, enabled: bool) {
        *self.last.lock().unwrap_or_else(|p| p.into_inner()) = (level, enabled);
    }

    pub(super) fn last(&self) -> (u32, bool) {
        *self.last.lock().unwrap_or_else(|p| p.into_inner())
    }

    fn handles(&self) -> Option<ZoomHandles<R>> {
        self.handles.lock().unwrap_or_else(|p| p.into_inner()).clone()
    }
}

/// Build the Zoom submenu and keep its handles. Every item starts disabled,
/// and the 100% item has the check mark (SNV-FR-IPCN, SNV-FR-QHMA).
pub(super) fn build_zoom_submenu<R: tauri::Runtime, M: tauri::Manager<R>>(
    manager: &M,
) -> tauri::Result<(tauri::menu::Submenu<R>, ZoomHandles<R>)> {
    use tauri::menu::{CheckMenuItem, MenuItem, PredefinedMenuItem, SubmenuBuilder};

    let item =
        |(id, label, accel): MenuItemSpec| MenuItem::with_id(manager, id, label, false, accel);
    let zoom_in = item(ZOOM_MENU_ITEMS[0])?;
    let zoom_out = item(ZOOM_MENU_ITEMS[1])?;
    let reset = item(ZOOM_MENU_ITEMS[2])?;
    let mut builder = SubmenuBuilder::new(manager, ZOOM_SUBMENU_LABEL)
        .item(&zoom_in)
        .item(&zoom_out)
        .item(&reset)
        .item(&PredefinedMenuItem::separator(manager)?);
    let mut steps = Vec::with_capacity(EDITOR_ZOOM_STEPS.len());
    for step in EDITOR_ZOOM_STEPS {
        let check = CheckMenuItem::with_id(
            manager,
            zoom_step_id(step),
            zoom_step_label(step),
            false,
            step == 100,
            None::<&str>,
        )?;
        builder = builder.item(&check);
        steps.push((step, check));
    }
    Ok((builder.build()?, ZoomHandles { zoom_in, zoom_out, reset, steps }))
}

/// Show one plan on the items. Menu changes must run on the main thread on
/// macOS. Best-effort: a menu that cannot change must not fail the caller, but
/// each failure is logged, so a stale Zoom menu can be explained.
fn apply<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    handles: ZoomHandles<R>,
    plan: ZoomMenuPlan,
) {
    let sink = app.clone();
    let hopped = app.run_on_main_thread(move || {
        let mut failed = 0usize;
        let mut note = |result: tauri::Result<()>| {
            if result.is_err() {
                failed += 1;
            }
        };
        note(handles.zoom_in.set_enabled(plan.zoom_in));
        note(handles.zoom_out.set_enabled(plan.zoom_out));
        note(handles.reset.set_enabled(plan.reset));
        for (step, check) in &handles.steps {
            note(check.set_enabled(plan.steps));
            note(check.set_checked(plan.checked == Some(*step)));
        }
        if failed > 0 {
            crate::logging::log_warn(
                &sink,
                &crate::logging::BUFFER,
                &[crate::logging::Domain::Backend],
                "zoom menu items could not be updated",
                crate::log_fields! { "failed" => failed },
            );
        }
    });
    if let Err(error) = hopped {
        crate::logging::log_warn(
            app,
            &crate::logging::BUFFER,
            &[crate::logging::Domain::Backend],
            "zoom menu update could not reach the main thread",
            crate::log_fields! { "error" => error.to_string() },
        );
    }
}

/// SNV-FR-QHMA: show the last pushed state again. The menu layer toggles the
/// check mark of a check item on each click, so after a click the marks can be
/// wrong until this runs.
pub(super) fn reapply<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Some(items) = app.try_state::<ZoomMenuItems<R>>() else {
        return;
    };
    let Some(handles) = items.handles() else {
        return;
    };
    let (level, enabled) = items.last();
    apply(app, handles, zoom_menu_plan(level, enabled));
}

/// SNV-FR-QHMA / SNV-FR-IPCN: show the editor zoom level and whether the active
/// tab of the focused window is zoomable. Only the frontend knows both, so it
/// pushes them here when either changes.
#[project_scope_macros::project_scoped]
#[tauri::command]
pub fn set_zoom_menu_state<R: tauri::Runtime>(
    level: u32,
    enabled: bool,
    app: tauri::AppHandle<R>,
    items: tauri::State<'_, ZoomMenuItems<R>>,
) {
    items.remember(level, enabled);
    let Some(handles) = items.handles() else {
        return;
    };
    apply(&app, handles, zoom_menu_plan(level, enabled));
}
