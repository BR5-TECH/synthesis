import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { Icon } from "./icons";
import type { ToastActionMenu as ToastActionMenuModel } from "../state/toasts";

/**
 * The action menu of a toast (`NTF-notifications.md` NTF-FR-JXQL,
 * NTF-FR-PHXY, NTF-FR-LHVU).
 *
 * A control of its own after the body, apart from the click target of the
 * toast, so a click on it or on an item never routes to the address. Opening
 * the menu focuses the menu itself and no item, because no duration is the
 * default. `onDone` runs when a pick succeeded; a pick that failed leaves the
 * toast and closes the menu.
 */
export function ToastActionMenu({
  menu,
  onDone,
}: {
  menu: ToastActionMenuModel;
  onDone: () => void;
}) {
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  // NTF-FR-PHXY: focus goes back to the control after a failed pick. The
  // control is disabled while the pick runs, so focus waits for the render
  // that enables it again.
  const [refocus, setRefocus] = useState(false);
  const controlRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const rootRef = useRef<HTMLDivElement>(null);

  // NTF-FR-PHXY: opening moves focus to the menu, on no item.
  useEffect(() => {
    if (open) menuRef.current?.focus();
  }, [open]);

  useEffect(() => {
    if (!refocus || busy) return;
    setRefocus(false);
    controlRef.current?.focus();
  }, [refocus, busy]);

  // NTF-FR-PHXY: a click outside the menu closes it.
  useEffect(() => {
    if (!open) return;
    const onPointerDown = (e: PointerEvent) => {
      if (!rootRef.current?.contains(e.target as Node | null)) setOpen(false);
    };
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [open]);

  const items = () =>
    Array.from(
      menuRef.current?.querySelectorAll<HTMLElement>('[role="menuitem"]') ?? [],
    );

  const close = () => {
    setOpen(false);
    controlRef.current?.focus();
  };

  const pick = (id: string) => {
    if (busy) return;
    setBusy(true);
    menu.onPick(id).then(
      () => {
        setBusy(false);
        setOpen(false);
        onDone();
      },
      () => {
        // NTF-FR-LHVU: the toast stays. The surface already showed the Error
        // toast of the failure.
        setBusy(false);
        setOpen(false);
        setRefocus(true);
      },
    );
  };

  const onMenuKeyDown = (e: KeyboardEvent<HTMLDivElement>) => {
    const all = items();
    const current = all.indexOf(document.activeElement as HTMLElement);
    switch (e.key) {
      case "ArrowDown": {
        e.preventDefault();
        all[current === -1 ? 0 : (current + 1) % all.length]?.focus();
        break;
      }
      case "ArrowUp": {
        e.preventDefault();
        const last = all.length - 1;
        all[current <= 0 ? last : current - 1]?.focus();
        break;
      }
      case "Enter": {
        e.preventDefault();
        const id = all[current]?.dataset.itemId;
        if (id !== undefined) pick(id);
        break;
      }
      case "Escape": {
        e.preventDefault();
        e.stopPropagation();
        close();
        break;
      }
      case "Tab":
        setOpen(false);
        break;
      default:
        break;
    }
  };

  return (
    <div ref={rootRef} style={{ position: "relative", alignSelf: "flex-start" }}>
      <button
        ref={controlRef}
        type="button"
        className="btn btn--ghost btn--sm"
        data-testid="notification-toast-menu-control"
        aria-haspopup="menu"
        aria-expanded={open}
        disabled={busy}
        onClick={() => setOpen((was) => !was)}
        style={{ display: "inline-flex", alignItems: "center", gap: 4 }}
      >
        {menu.label}
        <Icon.Caret size={10} className="icon" />
      </button>
      {open && (
        <div
          ref={menuRef}
          role="menu"
          aria-label={menu.label}
          tabIndex={-1}
          data-testid="notification-toast-menu"
          onKeyDown={onMenuKeyDown}
          className="card"
          style={{
            position: "absolute",
            top: "100%",
            left: 0,
            marginTop: 4,
            zIndex: 1,
            minWidth: 120,
            padding: 4,
            display: "flex",
            flexDirection: "column",
            boxShadow: "var(--shadow-3)",
            background: "var(--bg-elevated, var(--bg-panel))",
            outline: "none",
          }}
        >
          {menu.items.map((item) => (
            <button
              key={item.id}
              type="button"
              role="menuitem"
              tabIndex={-1}
              data-item-id={item.id}
              className="btn btn--ghost btn--sm"
              style={{ justifyContent: "flex-start", textAlign: "start" }}
              onClick={() => pick(item.id)}
            >
              {item.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
