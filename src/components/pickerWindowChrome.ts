import * as api from "../api";
import { logDebug, logWarn } from "../logging";
import { getCurrentWindow, LogicalSize } from "@tauri-apps/api/window";

// Fixed outer size of the picker window (spec PPK-FR-09 / PPK-FR-10 / PPK-FR-11).
// These must match `src-tauri/tauri.conf.json` — Tauri opens the window at
// the configured size, and re-applying it here pins the dimensions in case
// the OS window manager attempted to grow or maximize the window before
// our `setMaximizable(false)`/`setResizable(false)` calls landed.
// The right column is rendered at natural size; the left column absorbs the
// remaining width. The picker CSS (kit.css) is tuned so both columns fit
// inside these dimensions with no scrolling other than the recent list.
export const PICKER_WINDOW_WIDTH = 800;
export const PICKER_WINDOW_HEIGHT = 600;

/**
 * PPK-FR-TBNQ: the number of corrections in a row after which the picker stops
 * applying its fixed size again. A platform that refuses the size (for example
 * a tiling window manager) would otherwise cause a loop without end.
 */
export const MAX_SIZE_CORRECTIONS = 3;

// A logical size that differs by no more than this is the fixed size. The
// conversion from physical pixels can round by one pixel.
const SIZE_TOLERANCE = 1;

export type PickerWindow = ReturnType<typeof getCurrentWindow>;

// Upper bound (ms) on how long we wait for the OS to finish leaving full-screen
// before sizing/centering the picker. On macOS `setFullscreen(false)` resolves
// when the call is dispatched, not when the Space-exit animation completes, and
// a `setSize`/centering applied mid-transition is silently discarded by the
// window manager. We resolve as soon as the post-exit resize fires; this
// ceiling only bounds the wait if no resize is observed. A frame that macOS
// restores later is corrected by `keepPickerWindowSize` (PPK-FR-TBNQ).
const FULLSCREEN_EXIT_SETTLE_MS = 800;

// Leave OS full-screen and wait for the window to actually settle out of it.
// Leaving full-screen changes the outer size, so the platform emits a resize;
// we await that (with the ceiling above as a fallback) so the fixed size and
// centering that follow land on a normal window. Best-effort and self-cleaning:
// the resize listener and timer are always torn down, even if setFullscreen
// rejects on a platform that does not support it.
async function exitFullscreen(w: PickerWindow): Promise<void> {
  let resolveSettled!: () => void;
  const settled = new Promise<void>((resolve) => {
    resolveSettled = resolve;
  });
  let unlisten: (() => void) | undefined;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let done = false;
  const finish = () => {
    if (done) return;
    done = true;
    if (timer) clearTimeout(timer);
    if (unlisten) {
      try {
        unlisten();
      } catch {
        // ignore teardown failures
      }
    }
    resolveSettled();
  };
  try {
    if (typeof w.onResized === "function") {
      try {
        // Subscribe before toggling so the post-exit resize is never missed.
        unlisten = await w.onResized(() => finish());
      } catch {
        // resize subscription unavailable — rely on the timeout fallback
      }
    }
    timer = setTimeout(finish, FULLSCREEN_EXIT_SETTLE_MS);
    await w.setFullscreen(false);
    await settled;
  } finally {
    finish();
  }
}

// PPK-FR-09 / PPK-FR-12: pin the fixed size, then centre on the active display.
// The backend computes the active display from the cursor and never reads a
// persisted position (PPK-FR-12). The centring is best-effort.
// When `stillPicker` returns false after the size is set, the window no longer
// shows the picker, and the centring does not move it.
async function pinSizeAndCenter(
  w: PickerWindow,
  stillPicker: () => boolean = () => true,
): Promise<void> {
  await w.setSize(new LogicalSize(PICKER_WINDOW_WIDTH, PICKER_WINDOW_HEIGHT));
  if (!stillPicker()) return;
  await api.centerPicker().catch(() => {});
}

export async function applyPickerWindowChrome(): Promise<void> {
  try {
    const w = getCurrentWindow();
    // SNV-FR-27: the picker is never presented inside an OS full-screen space.
    // When File → Close Project (SNV-FR-25) is activated while the shared main
    // window is in native full-screen, the window is still full-screen as the
    // picker reclaims it. Leave full-screen first so the fixed size (PPK-FR-09)
    // and centering (PPK-FR-12) applied below land on a normal window rather
    // than being swallowed by the full-screen space. This is distinct from the
    // maximized handling below — full-screen is a separate OS window state, and
    // a window cannot be unmaximized while still full-screen, so it goes first.
    if (typeof w.isFullscreen === "function") {
      try {
        if (await w.isFullscreen()) await exitFullscreen(w);
      } catch {
        // some platforms reject before the window is fully created; fall
        // through so the fixed size and centering below still apply
      }
    }
    // Order: ensure not maximized, drop resize/maximize affordances, then
    // pin the outer size. setResizable(false) on its own does not always
    // disable the OS maximize control, so we also clear maximizable.
    if (typeof w.isMaximized === "function") {
      try {
        if (await w.isMaximized()) await w.unmaximize();
      } catch {
        // ignore — some platforms reject before window is fully created
      }
    }
    if (typeof w.setMaximizable === "function") {
      try {
        await w.setMaximizable(false);
      } catch {
        // some platforms (e.g. Linux/wayland) may not support this
      }
    }
    await w.setResizable(false);
    // PPK-FR-12: when the picker is re-shown within a session — after closing
    // (SNV-FR-25) or switching (OVW-FR-11) a project — the shared window still
    // sits at the main shell's last position, so it is centred again here.
    await pinSizeAndCenter(w);
  } catch {
    // Outside the Tauri runtime (tests, plain Vite dev), these APIs reject.
    // The picker still renders; window chrome is a no-op in that environment.
  }
}

// The logical inner size of the window, or null when the platform cannot
// report it. `setSize(LogicalSize)` sets the inner size, so that is the size
// to compare.
async function logicalInnerSize(
  w: PickerWindow,
): Promise<{ width: number; height: number } | null> {
  if (typeof w.innerSize !== "function") return null;
  const size = await w.innerSize();
  let scale = 1;
  if (typeof w.scaleFactor === "function") {
    const s = await w.scaleFactor();
    if (typeof s === "number" && s > 0) scale = s;
  }
  return { width: size.width / scale, height: size.height / scale };
}

function isFixedSize(size: { width: number; height: number }): boolean {
  return (
    Math.abs(size.width - PICKER_WINDOW_WIDTH) <= SIZE_TOLERANCE &&
    Math.abs(size.height - PICKER_WINDOW_HEIGHT) <= SIZE_TOLERANCE
  );
}

/**
 * PPK-FR-TBNQ: keep the fixed size while the picker shows in its own window.
 *
 * At the end of a full-screen exit, macOS can restore the frame the window had
 * before it went full-screen, after `applyPickerWindowChrome` set the size.
 * Each resize to another size, while the window is not full-screen, applies
 * the fixed size and the centring again. When the size still does not hold
 * after `MAX_SIZE_CORRECTIONS` in a row, the guard stops and logs a warning.
 * A resize to the fixed size resets the count.
 *
 * The guard checks the size once when it starts, and then on each resize.
 * Returns the function that removes the guard. Resize events that arrive
 * while a check runs are merged into one check that follows it.
 */
export async function keepPickerWindowSize(): Promise<() => void> {
  let w: PickerWindow;
  try {
    w = getCurrentWindow();
  } catch {
    // Outside the Tauri runtime there is no window to keep.
    return () => {};
  }
  if (typeof w.onResized !== "function") return () => {};
  let stopped = false;
  let corrections = 0;
  let warned = false;
  let running = false;
  let pending = false;

  const checkOnce = async () => {
    try {
      if (typeof w.isFullscreen === "function" && (await w.isFullscreen())) {
        return;
      }
      const size = await logicalInnerSize(w);
      if (stopped || size === null) return;
      if (isFixedSize(size)) {
        corrections = 0;
        warned = false;
        return;
      }
      if (corrections >= MAX_SIZE_CORRECTIONS) {
        if (!warned) {
          warned = true;
          logWarn(["frontend"], "picker window size does not hold; stop correcting", {
            width: Math.round(size.width),
            height: Math.round(size.height),
            corrections,
          });
        }
        return;
      }
      corrections += 1;
      logDebug(["frontend"], "picker window size changed; apply fixed size again", {
        width: Math.round(size.width),
        height: Math.round(size.height),
        corrections,
      });
      await pinSizeAndCenter(w, () => !stopped);
    } catch (err) {
      logDebug(["frontend"], "picker window size check failed", {
        error: err instanceof Error ? err.message : String(err),
      });
    }
  };

  const onResize = async () => {
    if (stopped) return;
    if (running) {
      pending = true;
      return;
    }
    running = true;
    try {
      do {
        pending = false;
        await checkOnce();
      } while (pending && !stopped);
    } finally {
      running = false;
    }
  };

  let unlisten: () => void;
  try {
    unlisten = await w.onResized(() => {
      void onResize();
    });
  } catch (err) {
    logDebug(["frontend"], "picker window resize subscription failed", {
      error: err instanceof Error ? err.message : String(err),
    });
    return () => {};
  }
  // A frame that the platform restored before the subscription has no event
  // here, so check the size once now.
  void onResize();
  return () => {
    stopped = true;
    try {
      unlisten();
    } catch {
      // ignore teardown failures
    }
  };
}
