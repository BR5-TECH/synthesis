import { useEffect, useRef, useState } from "react";

import {
  onSettingsWindowChanged,
  onSettingsWindowFocus,
} from "../events";
import type { SettingsWindowKind } from "../settingsWindow";

/** What the main window knows about the settings child window over it. */
export interface SettingsWindowPresence {
  /** SWN-FR-05: which settings window is open, or null when none is. */
  open: SettingsWindowKind | null;
  /** Whether that window holds OS focus. */
  focused: boolean;
}

/**
 * What the main window knows about the settings child window open over it
 * (`../../specifications/ui/SWN-settings-windows.md` SWN-FR-01, SWN-FR-05).
 *
 * A settings window is a webview of its own, so nothing in this window can see
 * it directly. The backend announces both facts and this is where they land.
 * Two consumers need them and neither can read them any other way:
 *
 * - **NTF-FR-08's suppression.** A raise naming the settings window that is at
 *   that moment open is suppressed, the author having already been told by the
 *   thing itself; and "no window of the application holds OS focus" means this
 *   window's focus *or* that one's.
 * - **the shell's own reflection of what is on screen**, for anything that has
 *   to know a settings window is up.
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
    open: null,
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
      // A window that has gone holds no focus either, so the two facts move
      // together rather than leaving a closed window remembered as focused.
      setPresence((prev) =>
        prev.open === open && (open !== null || !prev.focused)
          ? prev
          : { open, focused: open === null ? false : prev.focused },
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
