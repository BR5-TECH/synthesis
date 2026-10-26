import { useSyncExternalStore } from "react";
import { Icon } from "../icons";
import type { OutlineEntry } from "./outline";
import type { DocumentOutline } from "./useDocumentOutline";
import { useDropdown } from "./useDropdown";

const NONE: readonly OutlineEntry[] = [];
const noSubscription = () => () => {};
const EMPTY_HEADING = "Empty heading";

/**
 * EDT-FR-LGQA / EDT-FR-PNGU / EDT-FR-EBNU / EDT-FR-AQCR / EDT-FR-KOUB: the
 * table of contents control and the panel it opens, in the action cluster of
 * an Editor tab and in the chrome row of a New Artifact tab (NAW-FR-TFFX).
 *
 * The panel reads the outline only while it is open, so a closed panel costs
 * nothing on a long document, and it follows each edit while it stays open
 * (EDT-FR-KJLQ).
 */
export function TocControl({
  outline,
  className = "btn btn--ghost btn--icon",
  iconSize = 15,
}: {
  outline: DocumentOutline;
  /** The button classes of the row the control stands in. */
  className?: string;
  iconSize?: number;
}) {
  const panel = useDropdown();
  const { open } = panel;
  const entries = useSyncExternalStore(
    open ? outline.subscribe : noSubscription,
    open ? outline.getSnapshot : () => NONE,
  );
  panel.itemRefs.current.length = entries.length;

  const select = (entry: OutlineEntry) => {
    panel.close(false);
    outline.reveal(entry);
  };

  return (
    <div ref={panel.rootRef} className="toc" onKeyDown={panel.onRootKeyDown}>
      <button
        ref={panel.triggerRef}
        type="button"
        className={`${className} toc__trigger`}
        aria-label="Table of contents"
        title="Table of contents"
        aria-haspopup="tree"
        aria-expanded={open}
        data-active={open}
        onClick={panel.toggle}
        onKeyDown={panel.onTriggerKeyDown}
      >
        <Icon.Toc size={iconSize} />
      </button>
      {open && (
        <div className="menu toc__panel" data-testid="toc-panel">
          {entries.length === 0 ? (
            <p className="toc__empty t-ui-xs">No headings</p>
          ) : (
            <div role="tree" aria-label="Table of contents" className="toc__list">
              {entries.map((entry, index) => (
                <button
                  key={`${index}:${entry.level}`}
                  ref={(el) => {
                    panel.itemRefs.current[index] = el;
                  }}
                  type="button"
                  role="treeitem"
                  aria-level={entry.level}
                  aria-selected={false}
                  tabIndex={index === 0 ? 0 : -1}
                  className="menu-item toc__entry"
                  data-level={entry.level}
                  title={entry.text === "" ? EMPTY_HEADING : entry.text}
                  onClick={() => select(entry)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      select(entry);
                      return;
                    }
                    panel.onItemKeyDown(e, index);
                  }}
                >
                  {/* A heading with no text still has a place in the list,
                      so it shows a stand-in rather than a blank row. */}
                  <span
                    className={
                      entry.text === "" ? "toc__text toc__text--empty" : "toc__text"
                    }
                  >
                    {entry.text === "" ? EMPTY_HEADING : entry.text}
                  </span>
                </button>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
