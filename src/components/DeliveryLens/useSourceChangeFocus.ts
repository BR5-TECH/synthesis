import { useCallback, useEffect, useRef, type RefObject } from "react";

import { logDebug } from "../../logging";
import type { SpecMapSessionStore } from "../../state/specMap/session";
import { BAND_MENU_TRIGGER } from "./dialogs";

/**
 * A publish (MDL-FR-JRGK) or an export (MDL-FR-QIXG) changes the source of
 * the delivery from a dialog. The dialog closes, and the lens of the old
 * source unmounts with the control that opened the dialog, so focus would
 * stay on the page body. This hook gives the action that reads the source
 * again. After the source changes, focus goes to the band menu trigger of
 * the new lens as soon as that trigger shows, unless focus has a place then.
 *
 * `sourceKey` names the source that the lens shows.
 */
export function useSourceChangeFocus(
  store: SpecMapSessionStore,
  root: RefObject<HTMLElement | null>,
  sourceKey: string,
): () => void {
  const pending = useRef(false);

  const onSourceChanged = useCallback(() => {
    const before = store.snapshot().deliverySource;
    pending.current = true;
    void store.resolveDeliverySource().finally(() => {
      // The source did not change, so no lens unmounts: nothing to do.
      if (store.snapshot().deliverySource === before) pending.current = false;
    });
  }, [store]);

  useEffect(() => {
    if (!pending.current) return;
    pending.current = false;
    const el = root.current;
    if (!el) return;
    // True when nothing more is to be done.
    const settle = (): boolean => {
      const active = document.activeElement;
      if (active && active !== document.body && active.isConnected) return true;
      const trigger = el.querySelector<HTMLElement>(BAND_MENU_TRIGGER);
      if (!trigger) return false;
      trigger.focus();
      logDebug(["frontend"], "delivery lens focus moved to the band menu after a source change", {});
      return true;
    };
    if (settle()) return;
    const observer = new MutationObserver(() => {
      if (settle()) observer.disconnect();
    });
    observer.observe(el, { childList: true, subtree: true });
    return () => observer.disconnect();
  }, [sourceKey, root]);

  return onSourceChanged;
}
