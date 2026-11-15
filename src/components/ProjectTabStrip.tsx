/**
 * The project tab strip of Windows and Linux
 * (`../specifications/ui/PJT-project-tabs.md` PJT-FR-UHOC..PJT-FR-UXCT).
 *
 * It draws the list the backend publishes and decides nothing about it: the
 * order, the needs-attention dots and the tab of this window all come from the
 * props. It renders nothing while one project is open, and nothing on macOS,
 * where the system draws the tab bar (PJT-FR-PBQT).
 */
import { useEffect, useRef } from "react";

import type { ProjectWindow } from "../types";
import { Icon } from "./icons";

interface ProjectTabStripProps {
  windows: ProjectWindow[];
  /** The project id of the window that draws this strip (PJT-FR-XWPP). */
  ownProjectId: string;
  /** PJT-FR-JEGM: bring the window of this project to the front. */
  onActivate: (projectId: string) => void;
  /** PJT-FR-PCCL: run the close flow of this project tab. */
  onClose: (projectId: string) => void;
}

export function ProjectTabStrip({
  windows,
  ownProjectId,
  onActivate,
  onClose,
}: ProjectTabStripProps) {
  const listRef = useRef<HTMLDivElement>(null);

  // PJT-FR-VRRI: the tab of this window stays in view when the tabs do not fit.
  useEffect(() => {
    const own = listRef.current?.querySelector<HTMLElement>('[aria-selected="true"]');
    own?.scrollIntoView?.({ block: "nearest", inline: "nearest" });
  }, [ownProjectId, windows]);

  if (windows.length < 2) return null;

  const tabs = () =>
    Array.from(listRef.current?.querySelectorAll<HTMLElement>('[role="tab"]') ?? []);

  // PJT-FR-LGBV: arrows move focus without wrapping; Home and End jump; Enter
  // and Space activate; Delete closes.
  const onKeyDown = (e: React.KeyboardEvent<HTMLElement>, projectId: string) => {
    const all = tabs();
    const index = all.indexOf(e.currentTarget);
    let target: HTMLElement | undefined;
    switch (e.key) {
      case "ArrowRight":
        target = all[index + 1];
        break;
      case "ArrowLeft":
        target = all[index - 1];
        break;
      case "Home":
        target = all[0];
        break;
      case "End":
        target = all[all.length - 1];
        break;
      case "Enter":
      case " ":
        e.preventDefault();
        onActivate(projectId);
        return;
      case "Delete":
        e.preventDefault();
        onClose(projectId);
        return;
      default:
        return;
    }
    e.preventDefault();
    target?.focus();
  };

  return (
    <div
      ref={listRef}
      className="project-tabs"
      role="tablist"
      aria-label="Open projects"
      data-testid="project-tab-strip"
    >
      {windows.map((w) => {
        const own = w.projectId === ownProjectId;
        return (
          <div
            key={w.projectId}
            className="project-tabs__item"
            data-own={own}
            data-attention={w.needsAttention}
          >
            <button
              type="button"
              role="tab"
              className="project-tabs__tab"
              aria-selected={own}
              aria-label={w.name}
              tabIndex={own ? 0 : -1}
              title={w.projectId}
              onClick={() => onActivate(w.projectId)}
              onKeyDown={(e) => onKeyDown(e, w.projectId)}
            >
              {w.needsAttention && (
                <span
                  className="project-tabs__dot"
                  role="img"
                  aria-label="needs attention"
                  data-testid="project-tab-dot"
                />
              )}
              <span className="project-tabs__name">{w.name}</span>
            </button>
            <button
              type="button"
              className="project-tabs__close"
              aria-label={`Close ${w.name}`}
              tabIndex={-1}
              onClick={() => onClose(w.projectId)}
            >
              <Icon.X size={10} />
            </button>
          </div>
        );
      })}
    </div>
  );
}
