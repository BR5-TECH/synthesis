/**
 * The choice **Add documents** opens: **Files** and **Folder**
 * (`../../../specifications/ui/DPN-documents-panel.md` DPN-FR-ZMBQ).
 *
 * A floating overlay of the main window, so it follows the rules of SNV-FR-56:
 * it is positioned against the viewport, so the scrolling panel never clips it;
 * a press outside it, Escape, the window losing focus, and focus moving to
 * another control all take it down; and whoever opens it first asks every other
 * overlay to close.
 *
 * The entries are reachable with the Up and Down arrow keys and Enter. Escape
 * closes the choice and changes nothing.
 */
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Icon } from "../icons";
import type { PickDocumentSourcesMode } from "../../types";

interface AddDocumentsMenuProps {
  /** The control that opened the choice, which the choice is placed beside. */
  anchor: HTMLElement;
  onChoose: (mode: PickDocumentSourcesMode) => void;
  /** `restoreFocus` is true when the keyboard closed the choice. */
  onClose: (restoreFocus: boolean) => void;
}

const ENTRIES: { mode: PickDocumentSourcesMode; label: string }[] = [
  { mode: "files", label: "Files" },
  { mode: "folder", label: "Folder" },
];

export function AddDocumentsMenu({
  anchor,
  onChoose,
  onClose,
}: AddDocumentsMenuProps) {
  const frameRef = useRef<HTMLDivElement | null>(null);
  const itemRefs = useRef<(HTMLButtonElement | null)[]>([]);
  const [placement, setPlacement] = useState<{ left: number; top: number } | null>(
    null,
  );
  const closeRef = useRef(onClose);
  closeRef.current = onClose;

  // Measured before it is seen, so the first paint is not at the wrong place.
  useLayoutEffect(() => {
    const frame = frameRef.current;
    if (!frame) return;
    const rect = anchor.getBoundingClientRect();
    const { width, height } = frame.getBoundingClientRect();
    const margin = 4;
    const flipped = rect.bottom + margin + height > window.innerHeight - margin;
    setPlacement({
      left: Math.max(
        margin,
        Math.min(rect.right - width, window.innerWidth - width - margin),
      ),
      top: flipped
        ? Math.max(margin, rect.top - margin - height)
        : rect.bottom + margin,
    });
  }, [anchor]);

  // The choice is hidden until it is placed, and a browser ignores focus() on
  // a hidden element. So the first entry takes the focus after the placement.
  const placed = placement !== null;
  useEffect(() => {
    if (placed) itemRefs.current[0]?.focus();
  }, [placed]);

  useEffect(() => {
    const onPress = (event: MouseEvent) => {
      const target = event.target as Node | null;
      if (!target) return closeRef.current(false);
      // A target the press itself removed from the document is not outside.
      if (!target.isConnected) return;
      if (frameRef.current?.contains(target) || anchor.contains(target)) return;
      closeRef.current(false);
    };
    const onWindowBlur = () => closeRef.current(false);
    document.addEventListener("mousedown", onPress);
    window.addEventListener("blur", onWindowBlur);
    return () => {
      document.removeEventListener("mousedown", onPress);
      window.removeEventListener("blur", onWindowBlur);
    };
  }, [anchor]);

  const move = (from: number, step: number) => {
    const next = (from + step + ENTRIES.length) % ENTRIES.length;
    itemRefs.current[next]?.focus();
  };

  return (
    <div
      ref={frameRef}
      className="menu documents-menu"
      role="menu"
      aria-label="Add documents"
      style={{
        position: "fixed",
        left: placement?.left ?? 0,
        top: placement?.top ?? 0,
        visibility: placement ? "visible" : "hidden",
        zIndex: 200,
      }}
      onKeyDown={(event) => {
        const index = itemRefs.current.findIndex(
          (el) => el === document.activeElement,
        );
        if (event.key === "ArrowDown") {
          event.preventDefault();
          move(index < 0 ? -1 : index, 1);
        } else if (event.key === "ArrowUp") {
          event.preventDefault();
          move(index < 0 ? 0 : index, -1);
        } else if (event.key === "Home") {
          event.preventDefault();
          itemRefs.current[0]?.focus();
        } else if (event.key === "End") {
          event.preventDefault();
          itemRefs.current[ENTRIES.length - 1]?.focus();
        } else if (event.key === "Escape") {
          event.preventDefault();
          event.stopPropagation();
          onClose(true);
        } else if (event.key === "Tab") {
          onClose(false);
        }
      }}
      onBlur={(event) => {
        // Focus moving to another control of the window takes the choice down.
        const next = event.relatedTarget as Node | null;
        if (!next) return;
        if (frameRef.current?.contains(next) || anchor.contains(next)) return;
        onClose(false);
      }}
    >
      {ENTRIES.map((entry, index) => (
        <button
          key={entry.mode}
          type="button"
          role="menuitem"
          className="menu-item documents-menu__item"
          ref={(el) => {
            itemRefs.current[index] = el;
          }}
          onClick={() => onChoose(entry.mode)}
        >
          {entry.mode === "files" ? (
            <Icon.File size={12} aria-hidden="true" />
          ) : (
            <Icon.Folder size={12} aria-hidden="true" />
          )}
          {entry.label}
        </button>
      ))}
    </div>
  );
}
