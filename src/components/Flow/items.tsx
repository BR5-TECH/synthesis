import { useContext, useEffect, useRef, useState } from "react";

import type { ArtifactOption } from "../../hooks/useProjectArtifacts";
import type { FlowItem, FlowPosition, FlowSize } from "../../state/flowDocument";
import type { ItemDeliveryState, TrackerFacts } from "../../types";
import {
  BlockedMarker,
  DueDate,
  leafCountText,
  OverlapMarker,
  PendingMark,
  StatusChip,
  TrackerStatusChip,
  TrackerTag,
} from "./chips";
import { descriptionExcerpt } from "./excerpt";
import { FlowExcerptsContext } from "./excerptContext";
import { isInteractive, type Reference, type ResizeSides } from "./geometry";
import { KIND_GLYPHS, kindLabel } from "./kindGlyphs";
import { measureContainer } from "./measure";
import type { ContainerMeasure } from "./useFlowLayout";
import { ReferenceRows } from "./references";
import type { MenuBindings } from "../MapDiscussion/NodeMenu";

/** What an item shows of the delivery state and the overlaps. */
export interface ItemFacts {
  state?: ItemDeliveryState;
  /** FLO-FR-IKMM: the names and ids of the blocking items that are not done. */
  blockers: { id: string; name: string }[];
  /** FLO-FR-KWFH: the other items of each overlap and the shared paths. */
  overlap: { ids: string[]; names: string[]; paths: string[] } | null;
  /** FLO-FR-ULEM: the tracker item of an item of a GitHub plan. */
  tracker?: { repository: string; facts: TrackerFacts };
  /** MDL-FR-PUCM: an edit of the item is pending. */
  pending: boolean;
  /** DBR-FR-AGIN: the proposal mark of the item. */
  proposal?: "proposed" | "withdrawn";
  /** DBR-FR-WKNI: the item is the ghost card of an added row. */
  ghost?: { kindKnown: boolean };
  /** MDL-FR-PUCM / MDL-FR-HNLP: the error the host shows on the item. */
  error?: string;
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
  /**
   * MDL-FR-HULA: the handlers that open the context menu of the item. Present
   * where the host offers **Discuss**, and then the item takes the focus.
   */
  menu?: MenuBindings;
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
  /**
   * MDL-FR-XJBV: a rename is one edit when the field closes, not one per key,
   * because each edit of a GitHub plan is a request.
   */
  renameOnEnd: boolean;
}

/** The field of a rename in progress. */
function NameField({
  item,
  renameOnEnd,
  handlers,
  className,
}: {
  item: FlowItem;
  renameOnEnd: boolean;
  handlers: ItemHandlers;
  className: string;
}) {
  const [text, setText] = useState(item.name);
  const end = (keep: boolean) => {
    if (renameOnEnd && keep && text !== item.name) handlers.onRename(text);
    handlers.onEndRename();
  };
  return (
    <input
      className={`${className} ${className}--edit`}
      aria-label="Item name"
      autoFocus
      value={renameOnEnd ? text : item.name}
      onChange={(e) => {
        if (renameOnEnd) setText(e.target.value);
        else handlers.onRename(e.target.value);
      }}
      onBlur={() => end(true)}
      onKeyDown={(e) => {
        if (e.key === "Enter") end(true);
        if (e.key === "Escape") end(false);
      }}
    />
  );
}

/**
 * FLO-FR-09: the name, renamed in place. It reads as text, and a double click
 * or the panel's Rename turns it into a field. A name is free text.
 */
export function ItemName({
  item,
  editing,
  inert,
  handlers,
  className,
  renameOnEnd,
}: {
  item: FlowItem;
  editing: boolean;
  inert: boolean;
  handlers: ItemHandlers;
  className: string;
  renameOnEnd: boolean;
}) {
  if (editing && !inert) {
    return (
      <NameField
        item={item}
        renameOnEnd={renameOnEnd}
        handlers={handlers}
        className={className}
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

/**
 * MDL-FR-JPYU: the open blockers of a GitHub issue that are not in the plan,
 * each named by its number and title.
 */
export function externalBlockers(facts: ItemFacts): string[] {
  return (facts.tracker?.facts.externalBlockers ?? []).map((b) => `#${b.number} ${b.title}`);
}

/** FLO-FR-IKMM / FLO-FR-KWFH / MDL-FR-PUCM: the markers an item carries. */
function Markers({ facts, handlers }: { facts: ItemFacts; handlers: ItemHandlers }) {
  const external = externalBlockers(facts);
  return (
    <>
      {facts.pending && <PendingMark />}
      {facts.state?.blocked && facts.blockers.length + external.length > 0 && (
        <BlockedMarker
          blockers={[...facts.blockers.map((b) => b.name), ...external]}
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

/**
 * FLO-FR-XCRP: the excerpt of a description in at most two lines. The full
 * text is in the panel of the selected item (FLO-FR-13). MDL-FR-RILW: a canvas
 * that shows no excerpts gets nothing.
 */
function useExcerpt(item: FlowItem): string | null {
  const shown = useContext(FlowExcerptsContext);
  return shown ? descriptionExcerpt(item.description) : null;
}

function Excerpt({ item }: { item: FlowItem }) {
  const text = useExcerpt(item);
  if (!text) return null;
  return (
    <p className="flow-excerpt" title={text} data-testid={`flow-item-excerpt-${item.id}`}>
      {text}
    </p>
  );
}

export interface CardViewProps extends ItemViewBase {
  draftError?: string;
  onMeasure: (id: string, height: number) => void;
  onOpenDraft: (draftId: string) => void;
  onUnlinkDraft: () => void;
}

/**
 * FLO-FR-38 / FLO-FR-CMPK: a task or a defect, drawn as a compact card: its
 * glyph and name, its status chip and markers, its draft row, its references,
 * its excerpt, and its tracker tag. Selection changes nothing in the card:
 * the full description and the draft actions are in the attached panel
 * (FLO-FR-13, FLO-FR-NRBV).
 */
export function CardView(props: CardViewProps) {
  const { item, at, facts, selected, magnet, editingName, inert, handlers, renameOnEnd } = props;
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
      data-pending={facts.pending}
      style={{ left: at.x, top: at.y }}
      tabIndex={handlers.menu ? 0 : undefined}
      {...handlers.menu}
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
          renameOnEnd={renameOnEnd}
        />
      </div>
      {(state || facts.overlap || facts.pending || facts.tracker?.facts.status) && (
        <div className="flow-card__row">
          <StatusChip state={state?.state} />
          <TrackerStatusChip tracker={facts.tracker?.facts} />
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
      {props.draftError && (
        <div className="flow-card__error" role="alert">
          {props.draftError}
        </div>
      )}
      {facts.error && (
        <div className="flow-card__error" role="alert" data-testid={`flow-item-error-${item.id}`}>
          {facts.error}
        </div>
      )}
      <ReferenceRows references={facts.references} onOpen={handlers.onOpenReference} />
      <Excerpt item={item} />
      {facts.tracker && (
        <div className="flow-card__row">
          <TrackerTag
            repository={facts.tracker.repository}
            tracker={facts.tracker.facts}
            onOpen={handlers.onOpenTracker}
          />
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
  /** MDL-FR-ECPZ: report the room the header and the content need. */
  onMeasure?: (id: string, measure: ContainerMeasure) => void;
  /**
   * FLO-FR-SQUS / FLO-FR-OGPF: the place of a milestone column in the
   * milestone order, from 1. A column is not resized.
   */
  column?: number;
}

/**
 * FLO-FR-38: a milestone, an epic, or a feature, drawn around
 * what it holds: a header with its glyph, name, due date, markers, status chip,
 * and tracker tag, and a region for its members. Only the header drags it; a
 * press on its region is a press on the canvas.
 */
export function ContainerView(props: ContainerViewProps) {
  const { item, at, size, facts, selected, magnet, editingName, inert, handlers, renameOnEnd } =
    props;
  const label = `${kindLabel(item.kind)} ${item.name}`;
  const rootRef = useRef<HTMLDivElement>(null);
  const headRef = useRef<HTMLDivElement>(null);
  const excerpt = useExcerpt(item);
  const { onMeasure, collapsed } = props;
  // MDL-FR-ECPZ: measure again when what the header or the content shows
  // changes. A collapsed container shows no content, so it keeps its last.
  const shows = [
    item.name,
    item.due ?? "",
    item.description ?? "",
    facts.references.length,
    facts.state?.state ?? "",
    facts.blockers.length,
    facts.overlap?.ids.length ?? 0,
    facts.tracker?.facts?.status ?? "",
    facts.tracker?.facts?.ref.number ?? "",
    facts.error ?? "",
    editingName,
    size.width,
  ].join("\u0001");
  useEffect(() => {
    const root = rootRef.current;
    const head = headRef.current;
    if (!onMeasure || collapsed || !root || !head) return;
    onMeasure(item.id, measureContainer(root, head));
  }, [onMeasure, collapsed, item.id, shows]);
  return (
    <div
      ref={rootRef}
      className="flow-ctr"
      role="group"
      aria-label={label}
      data-testid={`flow-item-${item.id}`}
      data-container-id={item.id}
      data-kind={item.kind}
      data-selected={selected}
      data-magnet={magnet}
      data-collapsed={props.collapsed}
      data-pending={facts.pending}
      data-state={facts.state?.state}
      style={{ left: at.x, top: at.y, width: size.width, height: size.height }}
      tabIndex={handlers.menu ? 0 : undefined}
      {...handlers.menu}
      onPointerUp={handlers.onPointerUp}
    >
      <div
        ref={headRef}
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
        {props.column !== undefined && (
          <span className="flow-ctr__order" data-testid={`flow-column-order-${item.id}`}>
            {props.column}
          </span>
        )}
        <span className="flow-glyph" aria-hidden="true">
          {KIND_GLYPHS[item.kind]}
        </span>
        <ItemName
          item={item}
          editing={editingName}
          inert={inert}
          handlers={handlers}
          className="flow-ctr__name"
          renameOnEnd={renameOnEnd}
        />
        {item.due && <DueDate due={item.due} past={props.duePast} />}
        <span className="flow-ctr__spacer" />
        <Markers facts={facts} handlers={handlers} />
        <StatusChip state={facts.state?.state} />
        <TrackerStatusChip tracker={facts.tracker?.facts} />
        {facts.tracker && (
          <TrackerTag
            repository={facts.tracker.repository}
            tracker={facts.tracker.facts}
            onOpen={handlers.onOpenTracker}
          />
        )}
        <button
          type="button"
          className="flow-port flow-port--ctr"
          aria-label={`Connect from ${item.name}`}
          disabled={inert}
          onPointerDown={handlers.onConnectStart}
        />
      </div>
      {facts.error && (
        <div className="flow-card__error" role="alert" data-testid={`flow-item-error-${item.id}`}>
          {facts.error}
        </div>
      )}
      {props.collapsed ? (
        <div className="flow-ctr__counts" data-testid={`flow-leaf-counts-${item.id}`}>
          {leafCountText(facts.state?.leafCounts, props.leaves)}
        </div>
      ) : (
        (facts.references.length > 0 || excerpt !== null) && (
          <div className="flow-ctr__about">
            <ReferenceRows references={facts.references} onOpen={handlers.onOpenReference} />
            <Excerpt item={item} />
          </div>
        )
      )}
      {!props.collapsed &&
        !inert &&
        props.column === undefined &&
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
