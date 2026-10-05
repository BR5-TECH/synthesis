import { useEffect, useRef, useState } from "react";
import * as api from "../api";

import { Icon } from "./icons";
import { formatRelative } from "./ProjectPicker";
import type { ProjectHandle, RecentProject } from "../types";

interface ProjectSwitcherProps {
  // Name of the currently-open project, shown on the trigger (SNV-FR-17).
  projectName: string;
  // Path of the currently-open project; the matching dropdown row is flagged
  // "current" and is non-actionable (SNV-FR-20).
  projectPath: string;
  // SNV-FR-19 / OVW-FR-11: switch the active project to the freshly-opened
  // handle returned by "open project at path".
  onSwitch: (handle: ProjectHandle) => void;
  // OVW-FR-11 / EDT-FR-33: write the outgoing project's pending Editor changes
  // before the switch touches the backend. Returning false cancels the switch —
  // a write refused because of an unresolved blocker (EDT-FR-32) leaves the
  // current project open. Required, not optional: forgetting to thread it is
  // silent data loss, so the type system is what guarantees the wiring.
  onBeforeSwitch: () => Promise<boolean>;
  // SNV-FR-22 / OVW-FR-11: leave the main window for the Project picker so a
  // project that is not in the recent list can be opened.
  onOpenAnother: () => void;
  // NAW-FR-01 (single-overlay invariant): opening this dropdown closes every
  // other floating overlay of the main window rather than coexisting with it.
  // Optional so the switcher stays usable in isolation.
  onOpen?: () => void;
}

// SNV-FR-21: at most five rows are visible before the recent-entries region
// scrolls vertically (the current-project row counts toward this budget).
export const SWITCHER_MAX_VISIBLE_ROWS = 5;
// Fixed height of one dropdown row, in px; five of these size the scroll
// viewport so a sixth entry is reachable only by vertical scroll. Kept in sync
// with `.proj-switch__row { height }` in kit.css.
export const SWITCHER_ROW_PX = 44;

function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return "could not open project";
}

export function ProjectSwitcher({
  projectName,
  projectPath,
  onSwitch,
  onBeforeSwitch,
  onOpenAnother,
  onOpen,
}: ProjectSwitcherProps) {
  const [open, setOpen] = useState(false);
  const [recents, setRecents] = useState<RecentProject[] | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const rootRef = useRef<HTMLDivElement>(null);

  // SNV-FR-18: (re)load from "list recent projects" each time the dropdown
  // opens so it reflects the latest MRU ordering / missing-pruning. The
  // switcher renders the list as returned; it does not re-sort or re-filter.
  // Reset to null on every open (and on close) so a stale list is never shown
  // before the fresh fetch resolves — the "Loading…" sentinel shows instead.
  useEffect(() => {
    if (!open) {
      setRecents(null);
      return;
    }
    setError("");
    setRecents(null);
    api.listRecentProjects()
      .then(setRecents)
      .catch((e) => {
        setRecents([]);
        setError(errorMessage(e));
      });
  }, [open]);

  // Dismiss on outside click / Escape so the dropdown behaves like a menu.
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const onPick = async (r: RecentProject) => {
    // SNV-FR-20: the current project is non-actionable — no re-open, no
    // teardown. Also ignore re-entrant clicks while a switch is in flight.
    if (r.path === projectPath || busy) return;
    setError("");
    setBusy(true);
    try {
      // OVW-FR-11 / EDT-FR-33: the outgoing project's pending edits are written
      // before the switch reaches the backend — once "open project at path"
      // runs, the old project is no longer saveable.
      if (!(await onBeforeSwitch())) {
        setOpen(false);
        return;
      }
      // SNV-FR-19: switch via "open project at path"; the returned handle
      // drives the in-window teardown + reopen owned by OVW-FR-11.
      const handle = await api.openProjectAtPath(r.path);
      setOpen(false);
      onSwitch(handle);
    } catch (e) {
      // Open failed: keep the dropdown open with an inline error (mirrors the
      // Project picker's per-action error attachment).
      setError(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="proj-switch" ref={rootRef}>
      <button
        type="button"
        className="top-chrome__project"
        data-testid="project-switcher"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls="project-switcher-menu"
        onClick={() =>
          setOpen((o) => {
            // NAW-FR-01: enforced in the opening logic, like every other
            // overlay in the shell.
            if (!o) onOpen?.();
            return !o;
          })
        }
      >
        <Icon.Folder size={12} /> {projectName}
        <Icon.Caret size={10} />
      </button>

      {open && (
        <div
          id="project-switcher-menu"
          className="proj-switch__menu menu"
          role="menu"
          aria-label="Recent projects"
          data-testid="project-switcher-menu"
        >
          {/* SNV-FR-21: the recent-entries region is the only part that
              scrolls; it is capped to five rows and never scrolls
              horizontally (labels truncate instead). */}
          <div
            className="proj-switch__list"
            data-testid="project-switcher-list"
            style={{
              maxHeight: SWITCHER_MAX_VISIBLE_ROWS * SWITCHER_ROW_PX,
              overflowY: "auto",
              overflowX: "hidden",
            }}
          >
            {recents === null && (
              <div className="proj-switch__empty t-muted">Loading…</div>
            )}
            {recents !== null && recents.length === 0 && (
              <div className="proj-switch__empty t-muted">
                No recent projects.
              </div>
            )}
            {recents?.map((r) => {
              const isCurrent = r.path === projectPath;
              return (
                <button
                  type="button"
                  key={`${r.name}:${r.path}`}
                  role="menuitem"
                  className="proj-switch__row"
                  data-current={isCurrent}
                  // SNV-FR-20: the current project is non-actionable.
                  // `aria-current` is not a supported state on `menuitem`, so the
                  // flag is carried by `aria-disabled` plus a "(current)" label.
                  aria-disabled={isCurrent || undefined}
                  aria-label={isCurrent ? `${r.name} (current)` : undefined}
                  title={r.path}
                  onClick={() => onPick(r)}
                >
                  <Icon.Folder size={12} className="icon" />
                  <span className="proj-switch__name">{r.name}</span>
                  {isCurrent && <span className="badge">current</span>}
                  {r.pinned && <span className="badge">pinned</span>}
                  {r.missing && (
                    <span className="badge badge--warn">missing</span>
                  )}
                  <span className="proj-switch__meta">
                    {formatRelative(r.lastOpenedAt)}
                  </span>
                </button>
              );
            })}
          </div>

          {error && <div className="proj-switch__error">✗ {error}</div>}

          {/* SNV-FR-22: escape hatch to the Project picker, presented outside
              the scrolling recent-entries region. */}
          <div className="menu-sep" />
          <button
            type="button"
            role="menuitem"
            className="proj-switch__footer"
            data-testid="project-switcher-open-another"
            onClick={() => {
              setOpen(false);
              onOpenAnother();
            }}
          >
            <Icon.FolderOpen size={12} /> Open Another Project…
          </button>
        </div>
      )}
    </div>
  );
}
