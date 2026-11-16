/**
 * The trailing end of the band of the Delivery lens (`MDL-map-delivery-lens.md`
 * MDL-FR-KKYH): the repository, the time of the last read, and **Refresh** of
 * a GitHub plan, and the band menu (MDL-FR-JRGK, MDL-FR-QIXG, MDL-FR-QAJN).
 */
import { useEffect, useRef, useState } from "react";

import { openTrackerLink } from "../../api/delivery";
import { errorLogCode } from "../../errorLogCode";
import { logInfo, logWarn } from "../../logging";
import { deliveryErrorMessage } from "../../state/deliveryErrors";
import { showToast } from "../../state/toasts";
import { Icon } from "../icons";
import { readTimeText } from "./Notices";

/** MDL-FR-BZHY: the tooltip of the link to the GitHub Project. */
export const OPEN_PROJECT_TOOLTIP = "Open the GitHub Project";

/** MDL-FR-BZHY: open the GitHub Project; a refusal shows its error text in an Error toast. */
function openProject(url: string): void {
  logInfo(["frontend", "remote"], "delivery band opens the github project", {});
  void Promise.resolve()
    .then(() => openTrackerLink(url))
    .catch((e) => {
      logWarn(["frontend", "remote"], "github project link did not open", { code: errorLogCode(e) });
      showToast({
        key: "delivery-project-link",
        level: "Error",
        title: "Link not opened",
        body: deliveryErrorMessage(e),
        address: null,
      });
    });
}

/**
 * MDL-FR-DHHW: the Flow's path in the band of a file source. Activating it
 * reveals the file in the Project panel and opens no tab.
 */
export function FlowPathLink({ path, onReveal }: { path: string; onReveal?: () => void }) {
  return (
    <button
      type="button"
      className="dlens-band__repo dlens-band__link"
      data-testid="delivery-band-path"
      title="Show the Flow file in the Project panel"
      disabled={!onReveal}
      onClick={() => {
        logInfo(["frontend"], "delivery band reveals the flow file", {});
        onReveal?.();
      }}
    >
      {path}
    </button>
  );
}

export interface BandMenuEntry {
  label: string;
  onSelect: () => void;
}

/** The band menu, a transient surface of the tab (ACT-FR-12). */
export function BandMenu({ entries }: { entries: BandMenuEntry[] }) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    root.current?.querySelector<HTMLElement>('[role="menuitem"]')?.focus();
    const onDown = (e: PointerEvent) => {
      if (!root.current?.contains(e.target as Node)) setOpen(false);
    };
    window.addEventListener("pointerdown", onDown);
    return () => window.removeEventListener("pointerdown", onDown);
  }, [open]);
  return (
    <div
      ref={root}
      className="dlens-menu"
      onKeyDown={(e) => {
        if (e.key === "Escape" && open) {
          e.stopPropagation();
          setOpen(false);
        }
      }}
    >
      <button
        type="button"
        className="btn btn--ghost btn--sm btn--icon dlens-menu__toggle"
        aria-label="Plan actions"
        title="Plan actions"
        aria-haspopup="menu"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        <svg className="icon" viewBox="0 0 24 24" width={14} height={14} aria-hidden="true">
          <circle cx="5" cy="12" r="1" />
          <circle cx="12" cy="12" r="1" />
          <circle cx="19" cy="12" r="1" />
        </svg>
      </button>
      {open && (
        <div className="menu dlens-menu__list" role="menu" aria-label="Plan actions">
          {entries.map((entry) => (
            <button
              key={entry.label}
              type="button"
              role="menuitem"
              className="menu-item"
              onClick={() => {
                setOpen(false);
                entry.onSelect();
              }}
            >
              {entry.label}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

/** MDL-FR-KKYH: what the band of a GitHub plan adds before its menu. */
export function GithubBandFacts({
  repository,
  projectUrl,
  readAt,
  refreshing,
  onRefresh,
}: {
  repository: string;
  /** MDL-FR-BZHY: empty until the first GitHub read stores it; the link is disabled then. */
  projectUrl: string;
  readAt: string | null;
  refreshing: boolean;
  onRefresh: () => void;
}) {
  return (
    <>
      <button
        type="button"
        className="dlens-band__repo dlens-band__link"
        data-testid="delivery-band-repo"
        title={OPEN_PROJECT_TOOLTIP}
        aria-label={`${repository}: ${OPEN_PROJECT_TOOLTIP}`}
        disabled={projectUrl === ""}
        onClick={() => openProject(projectUrl)}
      >
        {repository}
        <Icon.ExternalLink />
      </button>
      {readAt && (
        <span
          className="dlens-band__read"
          data-testid="delivery-band-read"
          title={new Date(readAt).toLocaleString()}
        >
          {readTimeText(readAt, Date.now())}
        </span>
      )}
      <button
        type="button"
        className="btn btn--default btn--sm dlens-band__refresh"
        disabled={refreshing}
        onClick={onRefresh}
      >
        <Icon.Refresh size={12} /> {refreshing ? "Refreshing…" : "Refresh"}
      </button>
    </>
  );
}
