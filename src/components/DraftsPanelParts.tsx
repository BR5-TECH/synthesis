/**
 * The Drafts panel's three floating surfaces: the row menu that escapes the
 * scrolling body, the centred modal that takes focus, and the picker a move
 * chooses its destination in (DRP-FR-16, DRP-FR-22, DRP-FR-28, LCM-FR-07).
 *
 * Presentational components with no knowledge of the tree behind them, held
 * apart from the panel because each is about *positioning and focus* rather
 * than about drafts — and because the panel is easier to read without three
 * unrelated pieces of chrome in the middle of it.
 */
import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";

import { FilterableSelect, type FilterableOption } from "./FilterableSelect";
import { folderName, type MenuAnchor, type TreeItem } from "./draftsTree";
import { ROOT, ROOT_OPTION } from "./draftsTree";
import type { DraftFolder } from "../types";

/**
 * DRP-FR-22 / LCM-FR-07: a row menu, positioned against the viewport rather than
 * against the row.
 *
 * The panel body scrolls (`overflow: auto`), so a menu positioned inside it is
 * clipped at the body's edge — a row near the bottom of the tree loses most of
 * its entries, and what survives paints under the pinned create affordance.
 * The Library's context menu escapes its own panel the same way, which is the
 * behaviour DRP-FR-22 says these must match.
 *
 * It flips above its anchor when it would fall off the bottom, and is clamped
 * to the viewport horizontally so a narrow panel does not push it off the left
 * edge.
 */
export function AnchoredMenu({
  anchor,
  label,
  children,
}: {
  anchor: MenuAnchor | null;
  label: string;
  children: React.ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const [placement, setPlacement] = useState<{ left: number; top: number } | null>(
    null,
  );

  useLayoutEffect(() => {
    const el = ref.current;
    if (!el || !anchor) return;
    const { width, height } = el.getBoundingClientRect();
    const margin = 4;
    const flipped = anchor.y + height > window.innerHeight - margin;
    setPlacement({
      left: Math.max(
        margin,
        Math.min(anchor.x, window.innerWidth - width - margin),
      ),
      top: flipped ? Math.max(margin, anchor.y - height) : anchor.y,
    });
  }, [anchor]);

  return (
    <div
      ref={ref}
      className="menu drafts-overlay drafts-tree__menu"
      role="menu"
      aria-label={label}
      // A draft's menu is rendered inside its row so it can be keyed to it, and
      // the whole row is now clickable (DRP-FR-09) — so without this every menu
      // entry would ALSO open the draft on its way past. The menu is a surface
      // over the row, not a part of it.
      onClick={(e) => e.stopPropagation()}
      onContextMenu={(e) => {
        e.preventDefault();
        e.stopPropagation();
      }}
      style={{
        position: "fixed",
        left: placement?.left ?? anchor?.x ?? 0,
        top: placement?.top ?? anchor?.y ?? 0,
        // Measured before it is seen: the first paint would otherwise flash at
        // the unplaced position.
        visibility: placement ? "visible" : "hidden",
        zIndex: 200,
      }}
    >
      {children}
    </div>
  );
}

/**
 * DRP-FR-16 / DRP-FR-28: a centred confirmation or picker that takes focus when
 * it opens and keeps it while it is open.
 *
 * Without this the dialog opens behind the author's focus: Tab walks the tree
 * *behind the scrim* — visibly focus-ringed under the dim — and the dialog's
 * own first control is a dozen stops away, which is not "operated entirely from
 * the keyboard". Escape and the backdrop dismiss it without invoking anything.
 */
export function Modal({
  label,
  onClose,
  returnFocus,
  children,
}: {
  label: string;
  onClose: () => void;
  /**
   * Where focus goes when the dialog closes. Supplied rather than read from
   * `document.activeElement` on mount, because a dialog opened from a menu
   * entry is opened from an element that unmounts with that menu — leaving
   * nothing to hand focus back to.
   */
  returnFocus?: HTMLElement | null;
  children: React.ReactNode;
}) {
  const ref = useRef<HTMLDivElement>(null);
  // Read through a ref: the effect below is mount-only, and the caller's
  // handler changes identity on every render.
  const onCloseRef = useRef(onClose);
  onCloseRef.current = onClose;

  useEffect(() => {
    const opener = returnFocus ?? (document.activeElement as HTMLElement | null);
    const focusables = () =>
      Array.from(
        ref.current?.querySelectorAll<HTMLElement>(
          'button:not([disabled]), input:not([disabled]), [tabindex]:not([tabindex="-1"])',
        ) ?? [],
      );
    focusables()[0]?.focus();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        onCloseRef.current();
        return;
      }
      if (e.key !== "Tab") return;
      const items = focusables();
      if (items.length === 0) return;
      const first = items[0];
      const last = items[items.length - 1];
      const active = document.activeElement as HTMLElement | null;
      // Wrapped rather than allowed to escape: the tree behind the scrim is not
      // reachable while a modal is over it.
      if (e.shiftKey && (active === first || !ref.current?.contains(active))) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && active === last) {
        e.preventDefault();
        first.focus();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      // Focus goes back where it came from, so dismissing lands the author on
      // the row they opened it from rather than at the top of the document.
      if (opener?.isConnected) opener.focus();
    };
    // Deliberately mount-only: re-running would steal focus back to the first
    // control every time the caller re-renders.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  return (
    <div
      className="scrim"
      onClick={(e) => e.target === e.currentTarget && onClose()}
    >
      <div ref={ref} className="modal drafts-overlay" role="dialog" aria-modal="true" aria-label={label}>
        {children}
      </div>
    </div>
  );
}

export function MovePicker({
  item,
  folders,
  refusalFor,
  returnFocus,
  onCancel,
  onMove,
}: {
  item: TreeItem;
  folders: DraftFolder[];
  refusalFor: (destination: string) => string | null;
  returnFocus?: HTMLElement | null;
  onCancel: () => void;
  onMove: (destination: string) => void;
}) {
  const label = item.kind === "draft" ? item.name : folderName(item.path);
  /**
   * DRP-FR-28: the implicit root and every folder, each carrying the reason a
   * move onto it would be refused rather than being left out of the list.
   *
   * The control is the application's own filterable select rather than a list
   * of radios: a worktree with twenty folders is the ordinary case once an
   * author has been using this for a month, and a list that has to be scrolled
   * past to reach the buttons is not a chooser. Typing narrows it, which is
   * also the only way to reach a folder whose name you remember but whose
   * place in the tree you do not.
   */
  const options = useMemo<FilterableOption[]>(
    () =>
      [
        ROOT,
        ...[...folders]
          .map((f) => f.path)
          .sort((a, b) => a.toLowerCase().localeCompare(b.toLowerCase())),
      ].map((path) => {
        const refusal = refusalFor(path);
        return {
          // The root's id cannot be the empty string: that is what the select
          // reads as "nothing selected".
          id: path === ROOT ? ROOT_OPTION : path,
          label: path === ROOT ? "Drafts (root)" : path,
          disabled: refusal !== null,
          hint: refusal ?? undefined,
        };
      }),
    [folders, refusalFor],
  );
  const [choice, setChoice] = useState<string>(
    () => options.find((o) => !o.disabled)?.id ?? "",
  );
  const chosen = options.find((o) => o.id === choice);

  return (
    <Modal label={`Move ${label}`} returnFocus={returnFocus} onClose={onCancel}>
      <>
        <div className="modal__head">
          <div className="modal__title">Move “{label}”</div>
        </div>
        <div className="modal__body drafts-move__body">
          <FilterableSelect
            label="Destination folder"
            testId="drafts-move-destination"
            options={options}
            value={choice}
            onChange={setChoice}
          />
        </div>
        <div className="modal__actions">
          <button className="btn btn--ghost" onClick={onCancel}>
            Cancel
          </button>
          <button
            className="btn btn--primary"
            disabled={!chosen || chosen.disabled}
            onClick={() =>
              chosen &&
              !chosen.disabled &&
              onMove(chosen.id === ROOT_OPTION ? ROOT : chosen.id)
            }
          >
            Move
          </button>
        </div>
      </>
    </Modal>
  );
}
