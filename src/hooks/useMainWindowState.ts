import { useEffect, useRef } from "react";
import {
  availableMonitors,
  getCurrentWindow,
  LogicalSize,
} from "@tauri-apps/api/window";
import type { Monitor } from "@tauri-apps/api/window";
import {
  loadAppPreferences,
  patchAppPreferences,
} from "../state/appPreferences";
import {
  loadLayoutPreferences,
  patchLayoutPreferences,
} from "../state/layoutPreferences";
import type { LayoutPreferences } from "../types";

/**
 * True when a logical width/height fits inside the given monitor's logical
 * work area. `Monitor.workArea` is in physical pixels in Tauri 2, so we
 * convert by dividing by `scaleFactor` before comparing. Falls back to the
 * raw physical size if scaleFactor is missing or non-positive.
 */
function fitsMonitor(width: number, height: number, m: Monitor): boolean {
  const scale = m.scaleFactor && m.scaleFactor > 0 ? m.scaleFactor : 1;
  const logicalW = m.workArea.size.width / scale;
  const logicalH = m.workArea.size.height / scale;
  return width <= logicalW && height <= logicalH;
}

async function fitsAnyMonitor(
  width: number,
  height: number,
): Promise<boolean> {
  try {
    const monitors = await availableMonitors();
    if (!monitors || monitors.length === 0) return false;
    return monitors.some((m) => fitsMonitor(width, height, m));
  } catch {
    return false;
  }
}

/** Read the window's current full-screen state, treating any failure as "no". */
async function isFullscreenSafe(w: {
  isFullscreen?: () => Promise<boolean>;
}): Promise<boolean> {
  if (typeof w.isFullscreen !== "function") return false;
  try {
    return await w.isFullscreen();
  } catch {
    return false;
  }
}

/**
 * Manages the main window's outer dimensions, maximized state, and OS
 * full-screen state per SNV-shell-navigation.md SNV-FR-08 / SNV-FR-11 /
 * SNV-FR-12 / SNV-FR-13 / SNV-FR-38 / SNV-FR-39.
 *
 * On mount:
 *   - Re-enables resize / maximize affordances (the picker locks these down
 *     for its fixed-size window and nothing else reverses them).
 *   - Loads layout preferences via `api.loadLayoutPreferences()`.
 *   - If no persisted main-window state: maximize (SNV-FR-11).
 *   - If persisted but does not fit any current display: fall back to
 *     maximize (SNV-FR-13).
 *   - Otherwise: apply persisted outer dimensions and ensure unmaximized
 *     (SNV-FR-12).
 *   - Then, if the user-global full-screen preference is set, enters
 *     full-screen *over* that geometry (SNV-FR-39) — so the size the window
 *     returns to on leaving full-screen is still the project's own.
 *
 * Two persisted facts with two different scopes come together here:
 *
 *   - **geometry + maximized** — per project, in the layout payload
 *     (`api.saveLayoutPreferences`).
 *   - **full-screen** — user-global, on the app-preferences record
 *     (`patchAppPreferences`, SNV-FR-38 / GSS-FR-19).
 *
 * Both are written from the same resize listener, because Tauri emits
 * `Resized` for an OS full-screen transition and offers no separate
 * full-screen event. Geometry is deliberately **not** persisted while the
 * window is full-screen: the full-screen size is the display's, not a size the
 * user chose, and writing it would overwrite the geometry SNV-FR-39 requires
 * the window to return to.
 */
export function useMainWindowState(
  enabled: boolean = true,
  /**
   * Identity of the open project. Switching projects (OVW-FR-11) opens a
   * different layout slot, so the geometry is re-read and re-applied —
   * SNV-FR-35, SNV-FR-40, OVW-FR-11 requires B to open at B's own size, not A's.
   */
  projectKey: string = "",
): void {
  const readyRef = useRef(false);
  // Last full-screen state we persisted, so the resize listener only writes on
  // an actual transition rather than on every resize event.
  const fullscreenRef = useRef(false);
  // A resize arriving in the same tick as a full-screen exit can still report
  // the display's dimensions — macOS resolves `setFullscreen(false)` when the
  // transition is *dispatched*, not when the Space-exit animation completes.
  // Persisting that would destroy the geometry SNV-FR-39 restores. One
  // post-exit resize is skipped; the settled one that follows is stored.
  const skipNextGeometryRef = useRef(false);
  const projectRef = useRef(projectKey);
  projectRef.current = projectKey;

  useEffect(() => {
    if (!enabled) return;
    let cancelled = false;
    let unlistenResized: (() => void) | undefined;

    const run = async () => {
      const w = getCurrentWindow();

      // SNV-FR-12 / SNV-FR-13: undo any lockdown the picker applied so the user can
      // actually resize and maximize the IDE shell. We do this BEFORE any
      // maximize/setSize decision so the platform accepts those calls.
      try {
        await w.setResizable(true);
      } catch (err) {
        console.warn("setResizable(true) failed", err);
      }
      try {
        // `setMaximizable` was added to @tauri-apps/api/window in v2; guard
        // for older type bindings just in case. The corresponding capability
        // `core:window:allow-set-maximizable` is granted in
        // src-tauri/capabilities/default.json.
        const maybeSetMaximizable = (
          w as unknown as {
            setMaximizable?: (v: boolean) => Promise<void>;
          }
        ).setMaximizable;
        if (typeof maybeSetMaximizable === "function") {
          await maybeSetMaximizable.call(w, true);
        }
      } catch (err) {
        console.warn("setMaximizable(true) failed", err);
      }

      let prefs: LayoutPreferences = {};
      try {
        prefs = await loadLayoutPreferences(projectKey);
      } catch {
        prefs = {};
      }
      if (cancelled) return;

      // Positive, finite dimensions only. A record whose geometry fields are
      // absent deserialises on the backend to `0.0` rather than staying absent,
      // so `typeof === "number"` would accept a zero here and the restore below
      // would call `setSize(0, 0)` — a window the user cannot see, let alone
      // recover. The same guard rejects the 0×0 an iconify can report.
      const hasPersistedGeometry =
        typeof prefs.mainWindowOuterWidth === "number" &&
        typeof prefs.mainWindowOuterHeight === "number" &&
        prefs.mainWindowOuterWidth > 0 &&
        prefs.mainWindowOuterHeight > 0;

      try {
        if (!hasPersistedGeometry) {
          // SNV-FR-11: no persisted state → maximize on the available work area.
          await w.maximize();
        } else {
          const width = prefs.mainWindowOuterWidth as number;
          const height = prefs.mainWindowOuterHeight as number;
          const fits = await fitsAnyMonitor(width, height);
          if (!fits) {
            // SNV-FR-13: persisted geometry does not fit any display → maximize.
            await w.maximize();
          } else if (prefs.mainWindowMaximized) {
            await w.maximize();
          } else {
            // SNV-FR-12: restore persisted (logical) outer dimensions; ensure
            // unmaximized.
            try {
              if (await w.isMaximized()) await w.unmaximize();
            } catch {
              // ignore
            }
            await w.setSize(new LogicalSize(width, height));
          }
        }
      } catch {
        // Outside the Tauri runtime, window calls reject silently.
      }

      // SNV-FR-39: apply the user-global full-screen preference last, so it
      // goes on top of the per-project geometry restored above rather than
      // being undone by it. Leaving full-screen then reveals the window at that
      // geometry, which is exactly what SNV-FR-39 asks for.
      let wantFullscreen = false;
      try {
        wantFullscreen = !!(await loadAppPreferences()).mainWindowFullscreen;
      } catch {
        wantFullscreen = false;
      }
      if (cancelled) return;
      if (wantFullscreen) {
        try {
          await w.setFullscreen(true);
        } catch (err) {
          // A platform that refuses full-screen leaves the window at its
          // restored geometry — degraded, but usable.
          console.warn("setFullscreen(true) failed", err);
        }
      }
      // Seed from what the window ACTUALLY is, not from what we asked for. The
      // two can differ: a platform that refused the request, or (on macOS) a
      // window the OS restored into a full-screen space of its own while the
      // stored preference says otherwise. Seeding from the request would make
      // the first resize look like a transition and write the wrong answer.
      fullscreenRef.current = await isFullscreenSafe(w);
      if (cancelled) return;

      readyRef.current = true;

      // Persistence on resize/maximize transitions. We listen for resize
      // events; on each, we capture the current outer size and maximized
      // state and merge into the existing preferences payload.
      try {
        unlistenResized = await w.onResized(async () => {
          if (!readyRef.current || cancelled) return;
          try {
            // SNV-FR-38: entering or leaving OS full-screen arrives here as a
            // resize — Tauri emits no dedicated full-screen event. Persist the
            // transition (and only the transition) to the user-global record.
            const fullscreen = await isFullscreenSafe(w);
            if (fullscreen !== fullscreenRef.current) {
              const leftFullscreen = fullscreenRef.current && !fullscreen;
              fullscreenRef.current = fullscreen;
              // Patched, not replaced: the record also carries the theme, which
              // this hook must not clear (GSS-FR-20).
              void patchAppPreferences({
                mainWindowFullscreen: fullscreen,
              }).catch(() => {
                // The window is already full-screen either way; the next
                // transition writes again.
              });
              // This very event announced the exit; `outerSize()` may still
              // report the display's dimensions until the transition settles.
              if (leftFullscreen) skipNextGeometryRef.current = true;
            }
            // The full-screen size is the display's, not a size the user chose
            // for this project. Persisting it would overwrite the geometry the
            // window is supposed to return to on leaving full-screen
            // (SNV-FR-39), so geometry writes pause for the duration.
            if (fullscreen) return;
            if (skipNextGeometryRef.current) {
              skipNextGeometryRef.current = false;
              return;
            }

            const size = await w.outerSize();
            const maximized = await w.isMaximized();
            // `outerSize()` returns physical pixels. Convert to logical
            // before persisting so the next launch's `LogicalSize(...)`
            // restore lands on the same on-screen size.
            let scale = 1;
            try {
              const maybeScaleFactor = (
                w as unknown as {
                  scaleFactor?: () => Promise<number>;
                }
              ).scaleFactor;
              if (typeof maybeScaleFactor === "function") {
                const s = await maybeScaleFactor.call(w);
                if (typeof s === "number" && s > 0) scale = s;
              }
            } catch {
              // fall back to scale 1
            }
            const width = size.width / scale;
            const height = size.height / scale;
            // An iconify/minimize reports 0×0 on some platforms. Persisting it
            // would restore the next launch to an invisible window.
            if (!(width > 0 && height > 0)) return;
            // Patched, not replaced: the same record carries the vertical
            // panel's width fraction, which `useVerticalPanel` owns. Writing a
            // snapshot taken at mount would revert the user's last drag.
            await patchLayoutPreferences(projectRef.current, {
              mainWindowOuterWidth: width,
              mainWindowOuterHeight: height,
              mainWindowMaximized: maximized,
            });
          } catch {
            // ignore transient errors during resize bursts
          }
        });
        // The effect may have been torn down while `run()` was still awaiting,
        // in which case its cleanup already ran and never saw this listener.
        // Detach it here or one leaks per open/close cycle.
        if (cancelled) {
          unlistenResized();
          unlistenResized = undefined;
        }
      } catch {
        // not in Tauri runtime
      }
    };

    void run();

    return () => {
      cancelled = true;
      readyRef.current = false;
      if (unlistenResized) {
        try {
          unlistenResized();
        } catch {
          // ignore
        }
      }
    };
  }, [enabled, projectKey]);
}
