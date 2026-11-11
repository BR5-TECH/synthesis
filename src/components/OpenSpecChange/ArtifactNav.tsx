/**
 * The artifact list of a change, and its narrow form
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-QIZW,
 * OCW-FR-RRXG, OCW-FR-DRQD, OCW-FR-GCME).
 *
 * The list stands on the leading side of the reading pane. Where the reading
 * pane would be narrower than 360 px beside it, the list collapses to a menu
 * above the reading pane, so the reading pane takes the whole width.
 */
import { useEffect, useState, type ReactNode } from "react";

/** OCW-FR-GCME: the width of the artifact list, as `openspec-change.css` sets it. */
export const ARTIFACT_LIST_WIDTH = 200;
/** OCW-FR-GCME: the narrowest reading pane that stands beside the list. */
export const READING_MIN_WIDTH = 360;

/**
 * OCW-FR-GCME: whether the list collapses, from the measured width of the
 * body that holds the list and the reading pane. The decision reads the body
 * alone, so the collapse does not change what it reads.
 */
export function collapsesAt(bodyWidth: number): boolean {
  return bodyWidth > 0 && bodyWidth - ARTIFACT_LIST_WIDTH < READING_MIN_WIDTH;
}

/** OCW-FR-GCME: the width of the History rail, as `the-drafts-tree.css` sets it. */
export const HISTORY_RAIL_WIDTH = 220;

/**
 * OCW-FR-GCME: whether the History rail moves above the document, from the
 * measured width of the stage that holds the rail and the document. Beside
 * the rail, the document keeps a reading pane of 360 px at least. Where the
 * stage is narrower than the rail and that reading pane, the rail moves above
 * the document, and the document takes the whole width. The decision reads
 * the stage alone, so the move does not change what it reads.
 */
export function railStacksAt(stageWidth: number): boolean {
  return stageWidth > 0 && stageWidth - HISTORY_RAIL_WIDTH < READING_MIN_WIDTH;
}

/**
 * OCW-FR-GCME: follow the width of an element, where the platform measures
 * one, and apply a rule to it. The ref is a callback, so an element that
 * mounts after the first render is measured too.
 */
export function useWidthRule(
  rule: (width: number) => boolean,
): { applies: boolean; ref: (element: HTMLElement | null) => void } {
  const [element, setElement] = useState<HTMLElement | null>(null);
  const [applies, setApplies] = useState(false);
  useEffect(() => {
    if (!element || typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver((entries) => {
      const width = entries[entries.length - 1]?.contentRect.width ?? 0;
      setApplies(rule(width));
    });
    observer.observe(element);
    return () => observer.disconnect();
  }, [element, rule]);
  return { applies, ref: setElement };
}

/** OCW-FR-GCME: follow the width of the body that holds the list. */
export function useCollapsedList(): { collapsed: boolean; ref: (element: HTMLElement | null) => void } {
  const { applies, ref } = useWidthRule(collapsesAt);
  return { collapsed: applies, ref };
}

/** One artifact of the list. */
export interface NavItem {
  id: string;
  /** What follows the id: a state, a count of files. */
  detail: string;
  /** OCW-FR-RRXG: the count of undecided hunks, where there are any. */
  undecided?: number;
  /** The `data-status` of the detail, for its tone. */
  status?: string;
}

export function ArtifactNav({
  items,
  shown,
  onChoose,
  collapsed,
  label = "Artifacts",
}: {
  items: readonly NavItem[];
  shown: string | null;
  onChoose: (id: string) => void;
  collapsed: boolean;
  label?: string;
}) {
  if (collapsed) {
    return (
      <div className="openspec-change__menu" data-testid="openspec-artifact-menu">
        <label className="openspec-change__menu-label t-ui-xs">
          Artifact
          <select
            className="openspec-change__menu-select"
            aria-label={label}
            value={shown ?? ""}
            onChange={(e) => onChoose(e.target.value)}
          >
            {items.map((item) => (
              <option key={item.id} value={item.id}>
                {menuText(item)}
              </option>
            ))}
          </select>
        </label>
      </div>
    );
  }
  return (
    <nav className="openspec-change__list" aria-label={label}>
      {items.map((item) => (
        <button
          key={item.id}
          type="button"
          className="openspec-change__item"
          aria-current={item.id === shown ? "true" : undefined}
          data-testid="openspec-artifact"
          onClick={() => onChoose(item.id)}
        >
          <span className="openspec-change__item-id">{item.id}</span>
          {item.undecided !== undefined && item.undecided > 0 && (
            <span
              className="openspec-change__item-hunks"
              data-testid="openspec-artifact-hunks"
              aria-label={`${item.undecided} undecided ${item.undecided === 1 ? "change" : "changes"}`}
            >
              {item.undecided}
            </span>
          )}
          <span className="openspec-change__item-state" data-status={item.status}>
            {item.detail}
          </span>
        </button>
      ))}
    </nav>
  );
}

function menuText(item: NavItem): string {
  const hunks =
    item.undecided !== undefined && item.undecided > 0
      ? ` · ${item.undecided} undecided ${item.undecided === 1 ? "change" : "changes"}`
      : "";
  return `${item.id} · ${item.detail}${hunks}`;
}

/** OCW-FR-GCME: the list, or the menu above the reading pane, and the pane. */
export function NavLayout({
  nav,
  collapsed,
  children,
}: {
  nav: ReactNode;
  collapsed: boolean;
  children: ReactNode;
}) {
  return collapsed ? (
    <div className="openspec-change__stacked">
      {nav}
      {children}
    </div>
  ) : (
    <>
      {nav}
      {children}
    </>
  );
}
