import { useEffect, useRef, useState } from "react";

import {
  onSettingsWindowChanged,
  onSettingsWindowFocus,
} from "../events";
import {
  captureActiveInvoker,
  restoreSettingsInvoker,
} from "../state/settingsInvoker";

/** What the main window knows about the Settings child window over it. */
export interface SettingsWindowPresence {
  /** SWN-FR-05: whether the Settings window is open. */
  open: boolean;
  /** Whether that window holds OS focus. */
  focused: boolean;
}

/**
 * What the main window knows about the Settings child window open over it
 * (`../../specifications/ui/SWN-settings-windows.md` SWN-FR-01, SWN-FR-05).
 *
 * The Settings window is a webview of its own, so nothing in this window can see
 * it directly. The backend announces both facts and this is where they land.
 * Three consumers need them and none can read them any other way:
 *
 * - **NTF-FR-08's suppression.** A raise naming the Settings window while it is
 *   open is suppressed, the author having already been told by the thing itself;
 *   and "no window of the application holds OS focus" means this window's focus
 *   *or* that one's.
 * - **the shell's own reflection of what is on screen**, for anything that has
 *   to know the Settings window is up.
 * - **the return of focus** to the control that invoked the window, once it has
 *   closed (SWN-FR-FCRT).
 *
 * Returned as refs alongside the state, because the notification facility reads
 * the window through getters at raise time rather than from a render: a raise
 * that arrives between renders must see the presence as it is then. React does
 * not re-render on a window blur, so the rendered value would be as stale as the
 * last render — and the stale direction is the harmful one, a stale `focused`
 * suppressing a raise entirely and silently (NTF-FR-10).
 */
export function useSettingsWindows(): {
  presence: SettingsWindowPresence;
  presenceRef: React.MutableRefObject<SettingsWindowPresence>;
} {
  const [presence, setPresence] = useState<SettingsWindowPresence>({
    open: false,
    focused: false,
  });
  const presenceRef = useRef(presence);
  presenceRef.current = presence;

  useEffect(() => {
    let cancelled = false;
    const unlisten: Array<() => void> = [];
    const keep = (fn: () => void) => {
      if (cancelled) fn();
      else unlisten.push(fn);
    };
    void onSettingsWindowChanged((open) => {
      // SWN-FR-FCRT: the control in focus as the window opens is the one that
      // invoked it unless the request named another; the focus returns to it as
      // the window closes. Announced on every transition, so a repeated
      // announcement of the same state does nothing twice.
      if (open !== presenceRef.current.open) {
        if (open) captureActiveInvoker();
        else restoreSettingsInvoker();
      }
      // A window that has gone holds no focus either, so the two facts move
      // together rather than leaving a closed window remembered as focused.
      setPresence((prev) =>
        prev.open === open && (open || !prev.focused)
          ? prev
          : { open, focused: open ? prev.focused : false },
      );
    }).then(keep);
    void onSettingsWindowFocus((focused) => {
      setPresence((prev) => (prev.focused === focused ? prev : { ...prev, focused }));
    }).then(keep);
    return () => {
      cancelled = true;
      unlisten.forEach((fn) => fn());
    };
  }, []);

  return { presence, presenceRef };
}
