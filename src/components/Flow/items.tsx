import { useEffect, useRef } from "react";

import type { ArtifactOption } from "../../hooks/useProjectArtifacts";
import type {
  FlowItem,
  FlowPosition,
  FlowSize,
  SyncLink,
} from "../../state/flowDocument";
import type { ItemDeliveryState } from "../../types";
import { CommentMarkdown } from "../CommentMarkdown";
import {
  BlockedMarker,
  DueDate,
  leafCountText,
  LocalOnlyMark,
  OverlapMarker,
  StatusChip,
  TrackerTag,
} from "./chips";
import { isInteractive, type Reference, type ResizeSides } from "./geometry";
import { KIND_GLYPHS, kindLabel } from "./kindGlyphs";
import { ReferenceRows } from "./references";

/** What an item shows of the delivery state and the overlaps. */
export interface ItemFacts {
  state?: ItemDeliveryState;
  /** FLO-FR-IKMM: the names and ids of the blocking items that are not done. */
  blockers: { id: string; name: string }[];
  /** FLO-FR-KWFH: the other items of each overlap and the shared paths. */
  overlap: { ids: string[]; names: string[]; paths: string[] } | null;
  link?: SyncLink;
  references: Reference[];
}

/** The handlers every item view shares. */
export interface ItemHandlers {
  onDragStart: (e: React.PointerEvent) => void;
  onPointerUp: () => void;
  onConnectStart: (e: React.PointerEvent) => void;
  onRename: (name: string) => void;
  onStartRename: () => void;
  onEndRename: () => void;
  onSelectItems: (ids: string[]) => void;
  onOpenTracker: (url: string) => void;
  onOpenReference: (artifact: ArtifactOption) => void;
}

interface ItemViewBase {
  item: FlowItem;
  at: FlowPosition;
  facts: ItemFacts;
  selected: boolean;
  magnet: boolean;
  editingName: boolean;
  inert: boolean;
  handlers: ItemHandlers;
}

/**
 * FLO-FR-09: the name, renamed in place. It reads as text, and a double click
 * or the panel's Rename turns it into a field. A name is free text.
 */
function ItemName({
  item,
  editing,
  inert,
  handlers,
  className,
}: {
  item: FlowItem;
  editing: boolean;
  inert: boolean;
  handlers: ItemHandlers;
  className: string;
}) {
  if (editing && !inert) {
    return (
      <input
        className={`${className} ${className}--edit`}
        aria-label="Item name"
        autoFocus
        value={item.name}
        onChange={(e) => handlers.onRename(e.target.value)}
        onBlur={handlers.onEndRename}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === "Escape") handlers.onEndRename();
        }}
      />
    );
  }
  return (
    <span
      className={className}
      data-testid={`flow-item-name-${item.id}`}
      onDoubleClick={() => {
        if (!inert) handlers.onStartRename();
      }}
    >
      {item.name || <span className="flow-placeholder">Unnamed</span>}
    </span>
  );
}

/** FLO-FR-IKMM / FLO-FR-KWFH: the markers an item carries. */
function Markers({ facts, handlers }: { facts: ItemFacts; handlers: ItemHandlers }) {
  return (
    <>
      {facts.state?.blocked && facts.blockers.length > 0 && (
        <BlockedMarker
          blockers={facts.blockers.map((b) => b.name)}
          onActivate={() => handlers.onSelectItems(facts.blockers.map((b) => b.id))}
        />
      )}
      {facts.overlap && (
        <OverlapMarker
          others={facts.overlap.names}
          paths={facts.overlap.paths}
          onActivate={() => handlers.onSelectItems(facts.overlap!.ids)}
        />
      )}
    </>
  );
}

export interface CardViewProps extends ItemViewBase {
  creating: boolean;
  draftError?: string;
  onMeasure: (id: string, height: number) => void;
  onCreateDraft: () => void;
  onOpenDraft: (draftId: string) => void;
  onUnlinkDraft: () => void;
}

/**
 * FLO-FR-38: a task or a defect, drawn as a card: its glyph and name, its
 * status chip and markers, its draft row, its references, its description,
 * its tracker tag, and its local-only mark. Selection changes nothing in the
 * card: the editing controls are in the attached panel (FLO-FR-44 layout).
 */
export function CardView(props: CardViewProps) {
  const { item, at, facts, selected, magnet, editingName, inert, handlers } = props;
  const rootRef = useRef<HTMLDivElement>(null);
  const { onMeasure } = props;

  // FLO-FR-48: an edge lands on the border the card paints, and only the DOM
  // knows the height of a card that grows with its content.
  useEffect(() => {
    const el = rootRef.current;
    if (!el) return;
    const report = () => onMeasure(item.id, el.offsetHeight);
    report();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(report);
    observer.observe(el);
    return () => observer.disconnect();
  }, [item.id, onMeasure]);

  const state = facts.state;
  const draftName = state?.draft?.name ?? item.draftId;
  return (
    <div
      ref={rootRef}
      className="flow-card"
      role="group"
      aria-label={`${kindLabel(item.kind)} ${item.name}`}
      data-testid={`flow-item-${item.id}`}
      data-item-id={item.id}
      data-kind={item.kind}
      data-selected={selected}
      data-magnet={magnet}
      style={{ left: at.x, top: at.y }}
      onPointerDown={(e) => {
        if (!isInteractive(e.target)) handlers.onDragStart(e);
      }}
      onPointerUp={handlers.onPointerUp}
    >
      <div className="flow-card__head">
        <span className="flow-glyph" aria-hidden="true">
          {KIND_GLYPHS[item.kind]}
        </span>
        <ItemName
          item={item}
          editing={editingName}
          inert={inert}
          handlers={handlers}
          className="flow-card__name"
        />
      </div>
      {(state || facts.overlap) && (
        <div className="flow-card__row">
          <StatusChip state={state?.state} />
          <Markers facts={facts} handlers={handlers} />
        </div>
      )}
      {item.draftId !== undefined &&
        (state?.draftMissing ? (
          <div className="flow-draft flow-draft--missing" data-testid="flow-draft-missing">
            <span aria-hidden="true">⚠</span>
            <span className="flow-draft__name">Draft {item.draftId} is missing</span>
            <button
              type="button"
              className="flow-btn flow-btn--xs"
              disabled={inert}
              onClick={props.onUnlinkDraft}
            >
              Unlink
            </button>
          </div>
        ) : (
          <button
            type="button"
            className="flow-draft"
            aria-label={`Open draft ${draftName}`}
            onClick={() => props.onOpenDraft(item.draftId!)}
          >
            <span aria-hidden="true">✎</span>
            <span className="flow-draft__name">{draftName}</span>
            {state?.run && (
              <span className="flow-draft__run">{state.run.state.replace(/_/g, " ")}</span>
            )}
          </button>
        ))}
      {item.draftId === undefined && (
        <div className="flow-card__row">
          <button
            type="button"
            className="flow-btn flow-btn--xs"
            disabled={inert || props.creating}
            onClick={props.onCreateDraft}
          >
            {props.creating ? "Creating draft…" : "Create draft"}
          </button>
        </div>
      )}
      {props.draftError && (
        <div className="flow-card__error" role="alert">
          The draft was not created. {props.draftError}
        </div>
      )}
      <ReferenceRows references={facts.references} onOpen={handlers.onOpenReference} />
      {item.description && (
        <CommentMarkdown
          body={item.description}
          className="flow-desc"
          testId={`flow-item-description-${item.id}`}
        />
      )}
      {(facts.link || item.localOnly) && (
        <div className="flow-card__row">
          {facts.link && <TrackerTag link={facts.link} onOpen={handlers.onOpenTracker} />}
          {item.localOnly && <LocalOnlyMark />}
        </div>
      )}
      <button
        type="button"
        className="flow-port"
        aria-label={`Connect from ${item.name}`}
        disabled={inert}
        onPointerDown={handlers.onConnectStart}
      />
    </div>
  );
}

const RESIZE_HANDLES: ReadonlyArray<[string, ResizeSides]> = [
  ["n", { left: false, right: false, top: true, bottom: false }],
  ["s", { left: false, right: false, top: false, bottom: true }],
  ["w", { left: true, right: false, top: false, bottom: false }],
  ["e", { left: false, right: true, top: false, bottom: false }],
  ["nw", { left: true, right: false, top: true, bottom: false }],
  ["ne", { left: false, right: true, top: true, bottom: false }],
  ["sw", { left: true, right: false, top: false, bottom: true }],
  ["se", { left: false, right: true, top: false, bottom: true }],
];

export interface ContainerViewProps extends ItemViewBase {
  size: FlowSize;
  collapsed: boolean;
  /** FLO-FR-ZASL: the number of leaves under the container. */
  leaves: number;
  /** FLO-FR-XAPK: the milestone's due date is before today and it is not done. */
  duePast: boolean;
  onToggleCollapsed: () => void;
  onResizeStart: (sides: ResizeSides, e: React.PointerEvent) => void;
}

/**
 * FLO-FR-38: a deliverable, a milestone, an epic, or a feature, drawn around
 * what it holds: a header with its glyph, name, due date, markers, status chip,
 * and tracker tag, and a region for its members. Only the header drags it; a
 * press on its region is a press on the canvas.
 */
export function ContainerView(props: ContainerViewProps) {
  const { item, at, size, facts, selected, magnet, editingName, inert, handlers } = props;
  const label = `${kindLabel(item.kind)} ${item.name}`;
  return (
    <div
      className="flow-ctr"
      role="group"
      aria-label={label}
      data-testid={`flow-item-${item.id}`}
      data-container-id={item.id}
      data-kind={item.kind}
      data-selected={selected}
      data-magnet={magnet}
      data-collapsed={props.collapsed}
      style={{ left: at.x, top: at.y, width: size.width, height: size.height }}
      onPointerUp={handlers.onPointerUp}
    >
      <div
        className="flow-ctr__head"
        data-item-id={item.id}
        onPointerDown={(e) => {
          if (!isInteractive(e.target)) handlers.onDragStart(e);
        }}
      >
        <button
          type="button"
          className="flow-ctr__toggle"
          aria-label={`${props.collapsed ? "Expand" : "Collapse"} ${item.name}`}
          aria-expanded={!props.collapsed}
          onClick={props.onToggleCollapsed}
        >
          {props.collapsed ? "▸" : "▾"}
        </button>
        <span className="flow-glyph" aria-hidden="true">
          {KIND_GLYPHS[item.kind]}
        </span>
        <ItemName
          item={item}
          editing={editingName}
          inert={inert}
          handlers={handlers}
          className="flow-ctr__name"
        />
        {item.due && <DueDate due={item.due} past={props.duePast} />}
        <span className="flow-ctr__spacer" />
        <Markers facts={facts} handlers={handlers} />
        <StatusChip state={facts.state?.state} />
        {facts.link && <TrackerTag link={facts.link} onOpen={handlers.onOpenTracker} />}
        {item.localOnly && <LocalOnlyMark />}
        <button
          type="button"
          className="flow-port flow-port--ctr"
          aria-label={`Connect from ${item.name}`}
          disabled={inert}
          onPointerDown={handlers.onConnectStart}
        />
      </div>
      {props.collapsed ? (
        <div className="flow-ctr__counts" data-testid={`flow-leaf-counts-${item.id}`}>
          {leafCountText(facts.state?.leafCounts, props.leaves)}
        </div>
      ) : (
        (facts.references.length > 0 || item.description) && (
          <div className="flow-ctr__about">
            <ReferenceRows references={facts.references} onOpen={handlers.onOpenReference} />
            {item.description && (
              <CommentMarkdown
                body={item.description}
                className="flow-desc-ctr"
                testId={`flow-item-description-${item.id}`}
              />
            )}
          </div>
        )
      )}
      {!props.collapsed &&
        !inert &&
        RESIZE_HANDLES.map(([where, sides]) => (
          <span
            key={where}
            className={`flow-resize flow-resize--${where}`}
            data-testid={`flow-resize-${item.id}-${where}`}
            aria-hidden="true"
            onPointerDown={(e) => props.onResizeStart(sides, e)}
          />
        ))}
    </div>
  );
}
