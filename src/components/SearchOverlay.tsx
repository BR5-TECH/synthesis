import { useEffect } from "react";
import type { RefObject } from "react";
import { SearchResultList } from "./SearchResultList";
import type { SearchHit } from "../types";

interface SearchOverlayProps {
  onClose: () => void;
  onOpenFullResults: () => void;
  // The host's search input, so Escape can return focus to it (SCH-FR-07 /
  // SCH-FR-07) even when focus has moved onto a control inside the overlay.
  inputRef: RefObject<HTMLInputElement | null>;
  /** The streamed result set, already ordered by `ordinal` (SCH-FR-16). */
  hits: SearchHit[];
  running: boolean;
  ended: boolean;
  error: string | null;
  hasQuery: boolean;
  /** SCH-FR-09: follow a result to its surface. Dismisses the overlay. */
  onActivate: (hit: SearchHit) => void;
}

/**
 * SCH-FR-02: the lightweight overlay below the search input — a fast
 * jump-to-result over the *capped* search the input dispatches (SCH-FR-18).
 *
 * It owns no search of its own: the host mounts the hook, so the query, the
 * active mode and the streamed hits are the same ones the input is showing, and
 * dismissal cancels exactly this search and not the full results tab's
 * (SCH-FR-20).
 */
export function SearchOverlay({
  onClose,
  onOpenFullResults,
  inputRef,
  hits,
  running,
  ended,
  error,
  hasQuery,
  onActivate,
}: SearchOverlayProps) {
  // SCH-FR-07: Escape closes the overlay and focus returns to the
  // search input. Outside-pointer dismissal is owned by the host (TopChrome),
  // which knows the bounds of both the input and the overlay.
  useEffect(() => {
    const h = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        inputRef.current?.focus();
        onClose();
      }
    };
    window.addEventListener("keydown", h);
    return () => window.removeEventListener("keydown", h);
  }, [onClose, inputRef]);

  return (
    <div className="search-overlay">
      <div style={{ flex: 1, overflow: "auto" }}>
        <SearchResultList
          hits={hits}
          running={running}
          ended={ended}
          error={error}
          hasQuery={hasQuery}
          // SCH-FR-07: selecting a result dismisses the overlay.
          onActivate={(hit) => {
            onActivate(hit);
            onClose();
          }}
        />
      </div>
      <div className="search-footer">
        <kbd>↑</kbd>
        <kbd>↓</kbd> navigate
        <kbd>↵</kbd> open
        <kbd>Esc</kbd> close
        {/* SCH-FR-06: the "View all results" button at the bottom right, which
            opens the full results page (SCH-FR-08) and dismisses the overlay
            (SCH-FR-07). */}
        <button className="btn btn--default btn--sm" onClick={onOpenFullResults}>
          View all results →
        </button>
      </div>
    </div>
  );
}
