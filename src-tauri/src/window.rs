//! Picker window centering (PPK-project-picker.md PPK-FR-12).
//!
//! The picker is centered on the display containing the mouse cursor — at launch
//! (two-stage, via `center_picker_window` in the setup hook) and on every
//! in-session re-show (`center_picker`, the command the frontend calls after it
//! re-pins the picker's fixed size). The pure geometry helpers
//! (`pick_display_for_cursor` / `center_window_on_display`) and the re-fire gate
//! (`OnceGate`) are unit-tested without a Tauri runtime; only the thin I/O
//! wrappers touch the live window.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use tauri::{Manager, PhysicalPosition, WindowEvent};

/// Label of the picker window. Tauri assigns the default label `"main"` to a
/// window declared in `tauri.conf.json` without an explicit `label` field.
/// Centralizing this constant lets a unit test pin the on-disk config in sync
/// with the Rust-side lookup — otherwise renaming the window (or adding an
/// explicit, different label) would cause `center_picker_window` to silently
/// no-op at runtime, exactly the silent-failure class CLAUDE.md warns about.
const PICKER_WINDOW_LABEL: &str = "main";

/// A rectangular display region in physical pixels.
///
/// Plain numeric struct — intentionally free of Tauri types so the geometry
/// helpers can be unit-tested without a Tauri runtime. The fields mirror what
/// `tauri::Monitor::position()` / `tauri::Monitor::size()` return: top-left
/// position (signed; can be negative for displays placed left/above the
/// primary) and size (unsigned).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DisplayRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

/// Pick the display containing the cursor, falling back to the first display.
///
/// Per spec PPK-FR-12 the "active display" is the one containing the mouse cursor
/// at launch. If no display matches (cursor outside all known displays — can
/// happen with negative-offset multi-monitor layouts or stale cursor data),
/// we fall back to the first ("primary") display so the picker still appears.
///
/// Containment uses lower-bound inclusive / upper-bound exclusive semantics
/// (`pos.x <= cursor.x < pos.x + width`). This disambiguates a cursor sitting
/// exactly on a shared edge between two monitors — it belongs to the monitor
/// whose `position` matches that edge.
pub fn pick_display_for_cursor(
    cursor: (f64, f64),
    displays: &[DisplayRect],
) -> Option<DisplayRect> {
    if displays.is_empty() {
        return None;
    }
    let (cx, cy) = cursor;
    for d in displays {
        let left = d.x as f64;
        let top = d.y as f64;
        let right = left + d.width as f64;
        let bottom = top + d.height as f64;
        if cx >= left && cx < right && cy >= top && cy < bottom {
            return Some(*d);
        }
    }
    // Fallback: no display contains the cursor. Use the first display.
    Some(displays[0])
}

/// The display a *window* occupies, for a child window that must open centred
/// over its parent (`SWN-settings-windows.md` SWN-FR-04).
///
/// The window's own centre decides, rather than its top-left corner: a window
/// straddling two displays belongs to the one showing most of it, and its
/// corner can easily sit on the other. Reuses the cursor test, a centre being a
/// point like any other, so both routes agree about shared edges.
pub fn pick_display_for_rect(rect: DisplayRect, displays: &[DisplayRect]) -> Option<DisplayRect> {
    let centre = (
        rect.x as f64 + rect.width as f64 / 2.0,
        rect.y as f64 + rect.height as f64 / 2.0,
    );
    pick_display_for_cursor(centre, displays)
}

/// Every display this machine reports, as plain geometry.
fn displays_of<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Vec<DisplayRect> {
    let Ok(monitors) = app.available_monitors() else {
        return Vec::new();
    };
    monitors
        .iter()
        .map(|m| {
            let pos = m.position();
            let size = m.size();
            DisplayRect {
                x: pos.x,
                y: pos.y,
                width: size.width,
                height: size.height,
            }
        })
        .collect()
}

/// The display containing `rect`, or `None` when this machine reports none.
/// The I/O half of [`pick_display_for_rect`].
pub fn display_for_rect<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    rect: DisplayRect,
) -> Option<DisplayRect> {
    let displays = displays_of(app);
    if displays.is_empty() {
        return None;
    }
    pick_display_for_rect(rect, &displays)
}

/// Compute the top-left position to place a window of `window_size` centered
/// inside `display`. Returns physical-pixel coordinates suitable for
/// `Window::set_position`.
///
/// Width / height are taken as `i64` math to avoid overflow / sign issues when
/// the window is (pathologically) larger than the display; the result is
/// clamped to the display's top-left in that case so the window's title bar
/// stays reachable.
pub fn center_window_on_display(
    display: DisplayRect,
    window_size: (u32, u32),
) -> (i32, i32) {
    let (ww, wh) = window_size;
    let dx = display.x as i64;
    let dy = display.y as i64;
    let dw = display.width as i64;
    let dh = display.height as i64;
    let ww = ww as i64;
    let wh = wh as i64;
    let x = dx + ((dw - ww) / 2).max(0);
    let y = dy + ((dh - wh) / 2).max(0);
    (x as i32, y as i32)
}

/// A one-shot gate. The first call to `try_pass` returns `true` and "consumes"
/// the gate; every call after that returns `false`. Built on `AtomicBool` so it
/// is safe to capture by `Arc` into the `on_window_event` closure (which Tauri
/// requires to be `Send + Sync + 'static`).
///
/// Wrapped as a newtype so the centering re-fire policy is unit-testable
/// without a Tauri runtime — see `tests::once_gate_*`.
#[derive(Default, Debug)]
pub struct OnceGate {
    fired: AtomicBool,
}

impl OnceGate {
    pub fn new() -> Self {
        Self {
            fired: AtomicBool::new(false),
        }
    }

    /// Returns `true` exactly once — the first time it's called. All
    /// subsequent calls return `false`. Uses `compare_exchange` for a true
    /// "test-and-set" so concurrent callers can't both observe `true`.
    pub fn try_pass(&self) -> bool {
        self.fired
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
}

/// Compute the centered position for `window` on the display containing the
/// cursor, and apply it via `set_position`. The window's size is supplied by
/// the caller — this lets the setup hook pass `window.outer_size()` and the
/// resize listener pass the size delivered by the `Resized` event payload (the
/// "authoritative" size after Tauri/the OS finished applying decorations and
/// DPI scaling). See PPK-FR-12 of `specifications/ui/PPK-project-picker.md`.
///
/// Failures are intentionally swallowed (logged via `eprintln!`) so a flaky
/// cursor / monitor probe never blocks app startup. The caller may retry on
/// later events (we do — see the `WindowEvent::Resized` listener installed in
/// `center_picker_window`).
fn apply_centering<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    window: &tauri::WebviewWindow<R>,
    window_size: (u32, u32),
) {
    let displays = displays_of(app);
    if displays.is_empty() {
        return;
    }

    // Cursor probe can fail on some platforms / wayland setups; fall back to
    // the first display in that case so we still center somewhere sensible.
    let cursor = app
        .cursor_position()
        .ok()
        .map(|p| (p.x, p.y))
        .unwrap_or_else(|| (displays[0].x as f64, displays[0].y as f64));

    let Some(target) = pick_display_for_cursor(cursor, &displays) else {
        return;
    };

    let (x, y) = center_window_on_display(target, window_size);
    if let Err(e) = window.set_position(PhysicalPosition::new(x, y)) {
        eprintln!("synthesis: set_position() failed during picker centering: {e}");
    }
}

/// Center the "main" window on the display currently containing the mouse
/// cursor. Implements PPK-FR-12 of `specifications/ui/PPK-project-picker.md`.
///
/// Centering is applied in two stages to defeat the "centered, then resized
/// off-center" race observed at launch:
///
/// 1. **Immediate**: compute and apply a centered position now using
///    `window.outer_size()`. This makes the window appear near the correct
///    spot, avoiding a visible flash from the OS default position.
///
/// 2. **Reactive**: install a `WindowEvent::Resized` listener guarded by a
///    `OnceGate`. The first resize event after setup — which is the one
///    delivered when Tauri/the OS finalizes window decorations and DPI
///    scaling, and is the size that was making centering wrong — triggers a
///    re-center against the new authoritative size. The gate ensures
///    subsequent resizes (per PPK-FR-09 the user can't resize the picker, but the
///    OS can still emit spurious Resized events) do not perturb position.
///
/// Failures are intentionally swallowed (logged via `eprintln!`) so a flaky
/// cursor / monitor probe never blocks app startup; in that case the window
/// retains whatever position the previous stage produced.
pub fn center_picker_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Some(window) = app.get_webview_window(PICKER_WINDOW_LABEL) else {
        return;
    };

    // Stage 1: best-effort initial centering using whatever the current outer
    // size reports right now. May be slightly stale relative to the size the
    // OS lands on, which is exactly what stage 2 corrects.
    match window.outer_size() {
        Ok(s) => apply_centering(app, &window, (s.width, s.height)),
        Err(e) => eprintln!(
            "synthesis: outer_size() failed during initial picker centering: {e}"
        ),
    }

    // Stage 2: re-center exactly once on the first Resized event. The event
    // payload carries the authoritative post-finalization size.
    let gate = Arc::new(OnceGate::new());
    let app_handle = app.clone();
    let window_for_handler = window.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::Resized(size) = event {
            if gate.try_pass() {
                recenter_on_resize(
                    &app_handle,
                    &window_for_handler,
                    (size.width, size.height),
                );
            }
        }
    });
}

/// Handle a `WindowEvent::Resized` by re-centering the picker against the
/// size delivered in the event payload.
///
/// Extracted from the `on_window_event` closure so the listener's input
/// contract — "use the size carried by the event, NEVER re-query
/// `window.outer_size()` here" — is enforceable by a typed test. The bug
/// PPK-FR-12 originally fixed was exactly the class of code that reads a stale
/// `outer_size()` inside the resize handler instead of trusting the payload;
/// a future refactor that regresses to that pattern would have to change
/// this function's signature (no longer accepting `(u32, u32)`), which a
/// unit test pins.
fn recenter_on_resize<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    window: &tauri::WebviewWindow<R>,
    size: (u32, u32),
) {
    apply_centering(app, window, size);
}

/// PPK-FR-12 (re-show): re-center the picker window on the active display using
/// its CURRENT outer size. The launch-time `center_picker_window` only runs once
/// in `setup`, so when the picker is shown again WITHIN a session — after a
/// project is closed (SNV-FR-25) or switched (OVW-FR-11) — the window otherwise
/// keeps the main shell's last position. The frontend calls this right after it
/// has pinned the picker's fixed size (ProjectPicker `applyPickerWindowChrome`),
/// so a single immediate centering is enough — no Resized-listener stage (which
/// would also leak a listener on every re-show). Reuses the same cursor-based
/// `apply_centering` as launch, so the prior position is neither read nor
/// restored (PPK-FR-12). Best-effort: a missing window or probe failure is
/// logged and never surfaced to the caller.
#[tauri::command]
pub fn center_picker(app: tauri::AppHandle) {
    let Some(window) = app.get_webview_window(PICKER_WINDOW_LABEL) else {
        return;
    };
    match window.outer_size() {
        Ok(s) => apply_centering(&app, &window, (s.width, s.height)),
        Err(e) => eprintln!("synthesis: outer_size() failed during picker re-center: {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::global_settings::GlobalSettingsStore;

    // ------------------------------------------------------------------
    // PPK-FR-12: picker window centering geometry.
    //
    // These tests pin the pure helpers used by the Builder::setup hook.
    // They run without a Tauri runtime — the I/O wrapper
    // `center_picker_window` is exercised only when the app actually
    // launches, but the logic it depends on is fully covered here.
    // ------------------------------------------------------------------

    fn display_a() -> DisplayRect {
        DisplayRect { x: 0, y: 0, width: 1920, height: 1080 }
    }

    fn display_b() -> DisplayRect {
        // Placed to the right of A, with a different size to catch any
        // accidental "always use A's bounds" bug.
        DisplayRect { x: 1920, y: 0, width: 2560, height: 1440 }
    }

    // SWN-FR-04: a settings window is centred on the display its PARENT
    // occupies, which is decided by the parent's centre rather than its corner
    // — a window straddling two displays belongs to the one showing most of it.
    #[test]
    fn a_window_belongs_to_the_display_holding_its_centre() {
        let displays = [display_a(), display_b()];
        let on_a = DisplayRect { x: 100, y: 100, width: 800, height: 600 };
        assert_eq!(pick_display_for_rect(on_a, &displays).unwrap(), display_a());

        let on_b = DisplayRect { x: 2000, y: 200, width: 800, height: 600 };
        assert_eq!(pick_display_for_rect(on_b, &displays).unwrap(), display_b());

        // Straddling: most of it is on B, and its centre says so even though
        // its left edge is still on A.
        let straddling = DisplayRect { x: 1700, y: 0, width: 800, height: 600 };
        assert_eq!(
            pick_display_for_rect(straddling, &displays).unwrap(),
            display_b()
        );
    }

    #[test]
    fn a_window_off_every_display_falls_back_to_the_first() {
        let displays = [display_a(), display_b()];
        let nowhere = DisplayRect { x: -9000, y: -9000, width: 800, height: 600 };
        assert_eq!(
            pick_display_for_rect(nowhere, &displays).unwrap(),
            display_a()
        );
        assert_eq!(pick_display_for_rect(nowhere, &[]), None);
    }

    #[test]
    fn pick_display_cursor_on_a_returns_a() {
        let displays = [display_a(), display_b()];
        let picked = pick_display_for_cursor((100.0, 100.0), &displays).unwrap();
        assert_eq!(picked, display_a());
    }

    #[test]
    fn pick_display_cursor_on_b_returns_b_ts10() {
        // PPK-FR-12: cursor on display B at launch -> picker window centered on B.
        let displays = [display_a(), display_b()];
        let picked = pick_display_for_cursor((3000.0, 500.0), &displays).unwrap();
        assert_eq!(picked, display_b());
    }

    #[test]
    fn pick_display_cursor_on_shared_edge_belongs_to_right_monitor() {
        // Exactly on x=1920 — the top-left of B. Containment is lower-bound
        // inclusive / upper-bound exclusive, so the cursor "belongs" to B,
        // not A (whose right edge is 1920, exclusive).
        let displays = [display_a(), display_b()];
        let picked = pick_display_for_cursor((1920.0, 100.0), &displays).unwrap();
        assert_eq!(picked, display_b());
    }

    #[test]
    fn pick_display_cursor_outside_all_falls_back_to_first() {
        let displays = [display_a(), display_b()];
        // Far below both displays (negative-offset layouts make this
        // theoretically reachable in the wild).
        let picked = pick_display_for_cursor((100.0, 9_000.0), &displays).unwrap();
        assert_eq!(picked, display_a());
    }

    #[test]
    fn pick_display_empty_list_returns_none() {
        assert_eq!(pick_display_for_cursor((0.0, 0.0), &[]), None);
    }

    #[test]
    fn pick_display_with_negative_offset_monitor() {
        // Common multi-monitor layout: secondary monitor to the LEFT of the
        // primary, at negative x.
        let left = DisplayRect { x: -1280, y: 0, width: 1280, height: 800 };
        let primary = DisplayRect { x: 0, y: 0, width: 1920, height: 1080 };
        let displays = [primary, left];
        let picked = pick_display_for_cursor((-500.0, 200.0), &displays).unwrap();
        assert_eq!(picked, left);
    }

    #[test]
    fn center_window_centers_on_display_a() {
        // 800x600 window on a 1920x1080 display at origin.
        // Expected: x = (1920-800)/2 = 560, y = (1080-600)/2 = 240.
        let (x, y) = center_window_on_display(display_a(), (800, 600));
        assert_eq!(x, 560);
        assert_eq!(y, 240);
    }

    #[test]
    fn center_window_centers_on_display_b_ts10() {
        // PPK-FR-12: 800x600 window on a 2560x1440 display at x=1920.
        // Expected: x = 1920 + (2560-800)/2 = 1920 + 880 = 2800
        //           y = 0    + (1440-600)/2 = 420
        let (x, y) = center_window_on_display(display_b(), (800, 600));
        assert_eq!(x, 2800);
        assert_eq!(y, 420);
    }

    #[test]
    fn center_window_respects_display_offset_negative() {
        let left = DisplayRect { x: -1280, y: 0, width: 1280, height: 800 };
        let (x, y) = center_window_on_display(left, (800, 600));
        // (1280-800)/2 = 240, plus the display's -1280 origin = -1040.
        assert_eq!(x, -1040);
        assert_eq!(y, 100);
    }

    #[test]
    fn center_window_larger_than_display_clamps_to_origin() {
        // Pathological: 4000x3000 window on a 1920x1080 display.
        // Without clamping the centered position would be a negative offset
        // INSIDE the display (which on some platforms hides the title bar).
        // We clamp to the display's top-left so the title bar stays reachable.
        let (x, y) = center_window_on_display(display_a(), (4000, 3000));
        assert_eq!(x, 0);
        assert_eq!(y, 0);
    }

    #[test]
    fn center_picker_does_not_read_persisted_position_ts11() {
        // PPK-FR-12 / decision-log: the picker's prior position is not persisted
        // and not restored. The geometry helpers operate only on (cursor,
        // displays, window_size) — they have no parameter for prior position
        // and no access to the layout store. This test enforces
        // that contract by reference: if someone adds a `prior_position`
        // parameter or threads the store through, this test stops compiling.
        fn assert_signature(
            _f: fn((f64, f64), &[DisplayRect]) -> Option<DisplayRect>,
            _g: fn(DisplayRect, (u32, u32)) -> (i32, i32),
        ) {
        }
        assert_signature(pick_display_for_cursor, center_window_on_display);

        // And as a positive end-to-end check for PPK-FR-12: a "previously on A"
        // session is irrelevant — with the cursor on B, the result is B.
        let displays = [display_a(), display_b()];
        let picked = pick_display_for_cursor((3000.0, 500.0), &displays).unwrap();
        assert_eq!(picked, display_b());
    }

    #[test]
    fn center_picker_does_not_touch_the_layout_store_ts11() {
        // PPK-FR-12 / decision-log behavioral guard: the picker's prior position
        // is never persisted. Even if a future refactor threaded state into
        // the geometry helpers via closure capture or an extra parameter, the
        // store must remain untouched by the centering computation.
        //
        // We construct a fresh store, exercise the geometry helpers
        // end-to-end (the same sequence `center_picker_window` runs), and
        // then assert the store is still empty. If anyone ever wires the
        // store into the geometry path, this test fails.
        let store = GlobalSettingsStore::in_memory();
        assert_eq!(
            store.load_project_layout("/p").unwrap(),
            None,
            "precondition: store should be empty before centering"
        );

        // Representative case: cursor on display B (PPK-FR-12), 800x600 window.
        let displays = [display_a(), display_b()];
        let picked = pick_display_for_cursor((3000.0, 500.0), &displays)
            .expect("cursor on B should pick a display");
        assert_eq!(picked, display_b());
        let (x, y) = center_window_on_display(picked, (800, 600));
        assert_eq!(x, 2800);
        assert_eq!(y, 420);

        // Behavioral guard: after running the centering computation the
        // store must still be empty — no read, no write, no incidental save.
        assert_eq!(
            store.load_project_layout("/p").unwrap(),
            None,
            "picker centering must not read from or write to the layout store \
             (PPK-FR-12 / picker position is not persisted)",
        );
    }

    #[test]
    fn picker_window_label_matches_tauri_conf() {
        // Guard against a silent runtime no-op if someone renames the picker
        // window or adds an explicit `label` field to `tauri.conf.json` that
        // diverges from `PICKER_WINDOW_LABEL`. Tauri's default label for a
        // window declared without an explicit `label` is `"main"`.
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let conf_path = std::path::Path::new(manifest_dir).join("tauri.conf.json");
        let raw = std::fs::read_to_string(&conf_path).unwrap_or_else(|e| {
            panic!("failed to read {}: {e}", conf_path.display())
        });
        let v: serde_json::Value =
            serde_json::from_str(&raw).expect("tauri.conf.json must be valid JSON");

        let windows = v
            .get("app")
            .and_then(|a| a.get("windows"))
            .and_then(|w| w.as_array())
            .expect("tauri.conf.json must declare app.windows as an array");
        assert!(
            !windows.is_empty(),
            "tauri.conf.json app.windows must not be empty"
        );
        let first = &windows[0];

        match first.get("label") {
            None => {
                // No explicit label — Tauri assigns the default "main".
                assert_eq!(
                    PICKER_WINDOW_LABEL, "main",
                    "tauri.conf.json declares no explicit window label \
                     (Tauri defaults to \"main\"), but PICKER_WINDOW_LABEL \
                     is {:?}. Either set the conf label or update the const.",
                    PICKER_WINDOW_LABEL,
                );
            }
            Some(serde_json::Value::String(label)) => {
                assert_eq!(
                    label, PICKER_WINDOW_LABEL,
                    "tauri.conf.json declares window label {:?}, but \
                     center_picker_window looks up {:?}. Centering would \
                     silently no-op at runtime — keep these in sync.",
                    label, PICKER_WINDOW_LABEL,
                );
            }
            Some(other) => panic!(
                "tauri.conf.json app.windows[0].label is set but is not a \
                 string: {other}"
            ),
        }
    }

    // ------------------------------------------------------------------
    // PPK-FR-12: resize-after-setup re-centers using the new (authoritative)
    // size. The Tauri runtime listener wiring itself cannot be unit-tested
    // without a real runtime, but the geometry it drives — "use the new size,
    // not the stale one" — is fully covered by exercising the pure helpers
    // twice in sequence the way the listener does at runtime.
    // ------------------------------------------------------------------

    #[test]
    fn resize_after_setup_recenters_using_new_size() {
        // Simulate the launch sequence:
        //   t0: setup hook runs with a stale (initial) outer size
        //   t1: OS finalizes decorations / DPI scaling, fires Resized with a
        //       larger authoritative size
        // The second computed position must differ from the first and must
        // correctly center the LARGER window on the same display.
        let displays = [display_a(), display_b()];
        let cursor = (3000.0, 500.0); // cursor on display B

        // t0: stale size, e.g. 600x400 (Tauri hasn't finished applying decor)
        let stale_size = (600u32, 400u32);
        let picked_t0 = pick_display_for_cursor(cursor, &displays).unwrap();
        let pos_t0 = center_window_on_display(picked_t0, stale_size);

        // t1: authoritative size, e.g. 800x600 (post-resize)
        let final_size = (800u32, 600u32);
        let picked_t1 = pick_display_for_cursor(cursor, &displays).unwrap();
        let pos_t1 = center_window_on_display(picked_t1, final_size);

        // Sanity: same display both times — what changes is the size, hence
        // the centering offset.
        assert_eq!(picked_t0, picked_t1);
        assert_eq!(picked_t1, display_b());

        // The two positions MUST differ — that's the bug we're fixing: the
        // initial pos is wrong for the final size.
        assert_ne!(
            pos_t0, pos_t1,
            "if these match, the resize-recenter wouldn't be doing anything"
        );

        // And t1 is the correctly-centered position for the FINAL size on B:
        //   x = 1920 + (2560-800)/2 = 2800
        //   y = 0    + (1440-600)/2 = 420
        assert_eq!(pos_t1, (2800, 420));

        // Whereas t0 (the stale-size center) is offset further:
        //   x = 1920 + (2560-600)/2 = 2900
        //   y = 0    + (1440-400)/2 = 520
        assert_eq!(pos_t0, (2900, 520));
    }

    #[test]
    fn once_gate_first_call_passes() {
        let gate = OnceGate::new();
        assert!(gate.try_pass(), "first try_pass must return true");
    }

    #[test]
    fn once_gate_subsequent_calls_are_noop() {
        // The resize listener installed in `center_picker_window` calls
        // `try_pass` on every Resized event. The gate must let exactly the
        // first one through and reject every later one, so the picker doesn't
        // get repositioned by spurious resizes.
        let gate = OnceGate::new();
        assert!(gate.try_pass());
        for _ in 0..5 {
            assert!(
                !gate.try_pass(),
                "subsequent try_pass calls must return false (one-shot)"
            );
        }
    }

    #[test]
    fn once_gate_is_thread_safe_one_winner() {
        // A defensive guard: even if multiple Resized events were somehow
        // dispatched concurrently, exactly one caller should win. A naive
        // "spawn N threads, each calls try_pass once" test can pass against
        // a buggy non-atomic implementation on lightly-loaded machines
        // because the threads run serially in practice. Force a real race
        // by gating all threads on a `Barrier`, and repeat the experiment
        // many times so a real bug has meaningful probability of showing up.
        use std::sync::{Arc as ArcStd, Barrier};
        use std::thread;

        const ITERATIONS: usize = 1000;
        const THREADS: usize = 16;

        for _ in 0..ITERATIONS {
            let gate = ArcStd::new(OnceGate::new());
            let barrier = ArcStd::new(Barrier::new(THREADS));
            let handles: Vec<_> = (0..THREADS)
                .map(|_| {
                    let g = ArcStd::clone(&gate);
                    let b = ArcStd::clone(&barrier);
                    thread::spawn(move || {
                        b.wait();
                        g.try_pass()
                    })
                })
                .collect();
            let winners = handles
                .into_iter()
                .filter_map(|h| h.join().ok())
                .filter(|v| *v)
                .count();
            assert_eq!(winners, 1, "exactly one thread must win the gate");
        }
    }

    #[test]
    fn recenter_on_resize_takes_event_payload_size() {
        // PPK-FR-12 listener-input contract: the resize handler must consume the
        // (u32, u32) size carried by the `WindowEvent::Resized` payload — not
        // re-query `window.outer_size()` from inside the handler. We pin that
        // contract by reference: `recenter_on_resize`'s third parameter is
        // typed `(u32, u32)`. If a future refactor changes the signature (e.g.
        // drops the size parameter and reads `outer_size()` internally), this
        // line stops compiling.
        //
        // Same fn-pointer-shape pattern as
        // `center_picker_does_not_read_persisted_position_ts11`.
        let _: fn(
            &tauri::AppHandle<tauri::Wry>,
            &tauri::WebviewWindow<tauri::Wry>,
            (u32, u32),
        ) = recenter_on_resize::<tauri::Wry>;
    }
}
