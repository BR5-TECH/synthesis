/**
 * `MIV-map-issue-view.md`: the large view of one item of the board, in the
 * shape of an issue page of a ticket system. A header, a main column with the
 * title and the rich body, and a sidebar with the fields. It covers the canvas,
 * is not modal, and holds every edit of the selected item (MIV-FR-HHQR).
 */
import { useEffect, useRef, type ReactNode } from "react";

import type { ArtifactOption } from "../../../hooks/useProjectArtifacts";
import type { FlowDocument, FlowItem, FlowItemKind } from "../../../state/flowDocument";
import { Icon } from "../../icons";
import { BoardAddress, BoardStatus, KindChip } from "../boardItems";
import type { Reference } from "../geometry";
import type { ItemFacts } from "../items";
import { kindLabel } from "../kindGlyphs";
import type { ItemEditHandlers } from "../panelSections";
import type { IssueViewParts } from "./host";
import { IssueMain } from "./IssueMain";
import { IssueSidebar, type BlockerRow } from "./IssueSidebar";

export interface IssueViewProps {
  doc: FlowDocument;
  item: FlowItem;
  facts: ItemFacts;
  references: Reference[];
  artifacts: ArtifactOption[];
  treeLoaded: boolean;
  inert: boolean;
  handlers: ItemEditHandlers;
  creating: boolean;
  /** MIV-FR-XMMS: rename the item to `name`. */
  onRenameTo: (name: string) => void;
  /** MDL-FR-XJBV: a description is one edit when its field closes. */
  describeOnBlur: boolean;
  kindFilter?: (kind: FlowItemKind) => boolean;
  /** MIV-FR-PGEC: the plan items that block the item. */
  blockers: BlockerRow[];
  /** MIV-FR-HOAE: the view takes the focus when it opens. */
  takeFocus: boolean;
  onOpenTracker: (url: string) => void;
  /** MIV-FR-POMI / MDL-FR-XCJN: **Discuss** in the header. */
  onDiscuss?: () => void;
  onShowItem: (itemId: string) => void;
  onClose: () => void;
  parts: IssueViewParts;
}

/** MIV-FR-HGGS: the accessible name: the address and the name of the item. */
export function issueViewName(item: FlowItem, facts: ItemFacts): string {
  const name = item.name || "Unnamed";
  const ref = facts.tracker?.facts.ref;
  if (ref?.kind === "issue") return `Issue #${ref.number} ${name}`;
  if (item.kind === "milestone") return `Milestone ${name}`;
  return `${kindLabel(item.kind)} ${name}`;
}

export function IssueView(props: IssueViewProps) {
  const { item, facts } = props;
  const sheet = useRef<HTMLDivElement>(null);

  // MIV-FR-HOAE: a view the author opened takes the focus, once.
  useEffect(() => {
    if (props.takeFocus) sheet.current?.focus({ preventScroll: true });
    // The focus is taken once, when the view opens.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // MIV-FR-HGGS: a pinch over the view zooms nothing. The canvas listens for
  // pinches on an ancestor, so the view keeps them to itself.
  useEffect(() => {
    const el = sheet.current;
    if (!el) return;
    const keep = (e: Event) => {
      const wheel = e as WheelEvent;
      if (e.type === "wheel" && !wheel.ctrlKey && !wheel.metaKey) return;
      e.stopPropagation();
      e.preventDefault();
    };
    const kinds = ["wheel", "gesturestart", "gesturechange", "gestureend"];
    for (const kind of kinds) el.addEventListener(kind, keep, { passive: false });
    return () => {
      for (const kind of kinds) el.removeEventListener(kind, keep);
    };
  }, []);

  let address: ReactNode = null;
  if (facts.tracker) {
    address = (
      <BoardAddress
        repository={facts.tracker.repository}
        tracker={facts.tracker.facts}
        onOpen={props.onOpenTracker}
      />
    );
  }

  return (
    <div
      ref={sheet}
      className="miv-sheet"
      role="dialog"
      aria-modal="false"
      aria-label={issueViewName(item, facts)}
      tabIndex={-1}
      data-testid="map-issue-view"
      data-item={item.id}
      onKeyDown={(e) => {
        // MIV-FR-YPWK: Escape closes the view. A field that handles Escape
        // itself stops it first (MIV-FR-SWZY).
        if (e.key === "Escape") {
          e.stopPropagation();
          props.onClose();
        }
      }}
    >
      <div className="miv-head">
        {item.kind === "milestone" ? (
          <Icon.Flag size={14} className="icon dm-lane__flag" aria-label="Milestone" />
        ) : (
          <KindChip kind={item.kind} />
        )}
        {address && <span className="miv-head__ref">{address}</span>}
        <span className="miv-head__status">
          <BoardStatus facts={facts} />
        </span>
        {facts.pending && (
          <span className="badge badge--live" title="This change is on its way to GitHub" data-testid="miv-pending">
            pending
          </span>
        )}
        <span className="miv-head__actions">
          {props.onDiscuss && (
            <button type="button" className="btn btn--default btn--sm" onClick={props.onDiscuss}>
              Discuss
            </button>
          )}
          {props.parts.actions}
          <button
            type="button"
            className="btn btn--ghost btn--sm btn--icon miv-close"
            aria-label="Close"
            title="Close"
            onClick={props.onClose}
          >
            <Icon.X size={14} />
          </button>
        </span>
      </div>
      <div className="miv-body-grid">
        <IssueMain {...props} />
        <IssueSidebar {...props} />
      </div>
    </div>
  );
}
