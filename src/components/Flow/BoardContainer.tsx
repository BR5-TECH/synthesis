/**
 * `DBR-delivery-board.md`: a group and a column of the board. A group has the
 * face of a card and its stack below a divider (DBR-FR-SBTR, DBR-FR-SBLK). A
 * column has a header row, the milestone's description, and its stack
 * (DBR-FR-BYQX). The members are drawn by the canvas over the region that
 * holds them; this view draws the frame and the header.
 */
import { useRef } from "react";

import type { FlowItem, FlowPosition, FlowSize } from "../../state/flowDocument";
import { Icon } from "../icons";
import {
  BoardAddress,
  BoardAssignee,
  BoardMarks,
  BoardStatus,
  ErrorStrip,
  KindChip,
  useBoardHeight,
} from "./boardItems";
import { milestoneBadgeClass } from "./boardStatus";
import { DueDate, STATE_LOOK } from "./chips";
import { descriptionExcerpt } from "./excerpt";
import { isInteractive } from "./geometry";
import { ItemName, type ItemFacts, type ItemHandlers } from "./items";
import { kindLabel } from "./kindGlyphs";

export interface BoardContainerProps {
  item: FlowItem;
  at: FlowPosition;
  size: FlowSize;
  facts: ItemFacts;
  selected: boolean;
  magnet: boolean;
  editingName: boolean;
  inert: boolean;
  handlers: ItemHandlers;
  renameOnEnd: boolean;
  collapsed: boolean;
  /** DBR-FR-SBLK: the group holds at least one item. */
  holdsItems: boolean;
  /** DBR-FR-VNSJ: the leaves under the item, and how many of them are done. */
  leaves: number;
  done: number;
  /** FLO-FR-XAPK: the milestone's due date is before today and it is not done. */
  duePast: boolean;
  /** DBR-FR-BYQX: the item is a milestone column. */
  column: boolean;
  /** DBR-FR-CZEU: the container holds the target stack of a drag. */
  dropTarget: boolean;
  /** DBR-FR-QCAJ: the container is dragged. */
  lifted: boolean;
  onToggleCollapsed: () => void;
  /** DBR-FR-SBTR / DBR-FR-BYQX: report the height of the face or the header. */
  onMeasure?: (id: string, height: number) => void;
}

/** FLO-FR-ZASL: the collapse toggle. It never starts a drag (DBR-FR-MYVI). */
function Caret({ item, collapsed, onToggle }: { item: FlowItem; collapsed: boolean; onToggle: () => void }) {
  return (
    <button
      type="button"
      className="dm-caret flow-ctr__toggle"
      aria-label={`${collapsed ? "Expand" : "Collapse"} ${item.name}`}
      aria-expanded={!collapsed}
      onClick={onToggle}
    >
      <Icon.Caret size={14} aria-hidden="true" />
    </button>
  );
}

/** DBR-FR-VNSJ: the count `done/total` of the leaves under an item. */
function Count({ item, done, leaves, className }: { item: FlowItem; done: number; leaves: number; className?: string }) {
  return (
    <span
      className={className}
      data-testid={`flow-leaf-counts-${item.id}`}
      title={`${done} of ${leaves} done`}
    >
      {done}/{leaves}
    </span>
  );
}

/** DBR-FR-IQBN .. DBR-FR-BYQX: a group or a column of the board. */
export function BoardContainer(props: BoardContainerProps) {
  const { item, at, size, facts, selected, magnet, editingName, inert, handlers, collapsed } = props;
  const headRef = useRef<HTMLDivElement>(null);
  // The top border of the frame is above the face or the header.
  useBoardHeight(headRef, item.id, 1, props.onMeasure);
  const common = {
    role: "group",
    "aria-label": `${kindLabel(item.kind)} ${item.name}`,
    "data-testid": `flow-item-${item.id}`,
    "data-container-id": item.id,
    "data-kind": item.kind,
    "data-selected": selected,
    "data-magnet": magnet,
    "data-collapsed": collapsed,
    "data-pending": facts.pending,
    "data-board": true,
    "data-drop-target": props.dropTarget,
    "data-lifted": props.lifted,
    "data-error": facts.error !== undefined,
    "data-state": facts.state?.state,
    style: { left: at.x, top: at.y, width: size.width, height: size.height },
    tabIndex: handlers.menu ? 0 : undefined,
    ...handlers.menu,
    onPointerUp: handlers.onPointerUp,
  };
  const startDrag = (e: React.PointerEvent) => {
    if (!isInteractive(e.target)) handlers.onDragStart(e);
  };
  const port = (
    <button
      type="button"
      className="flow-port flow-port--ctr"
      aria-label={`Connect from ${item.name}`}
      disabled={inert}
      onPointerDown={handlers.onConnectStart}
    />
  );

  if (props.column) {
    const state = facts.state?.state;
    const about = descriptionExcerpt(item.description);
    return (
      <div className="dm-lane flow-ctr" {...common}>
        <div ref={headRef} className="dm-lane__head flow-ctr__head" data-item-id={item.id} onPointerDown={startDrag}>
          <div className="dm-lane__row">
            <Caret item={item} collapsed={collapsed} onToggle={props.onToggleCollapsed} />
            <Icon.Flag size={14} className="icon dm-lane__flag" aria-hidden="true" />
            <ItemName
              item={item}
              editing={editingName}
              inert={inert}
              handlers={handlers}
              className="dm-lane__name flow-ctr__name"
              renameOnEnd={props.renameOnEnd}
            />
            {state && (
              <span className={milestoneBadgeClass(state)} data-testid="flow-status-chip" data-state={state}>
                {STATE_LOOK[state].word.toLowerCase()}
              </span>
            )}
            {item.due && <DueDate due={item.due} past={props.duePast} compact />}
            <BoardMarks item={item} facts={facts} handlers={handlers} />
            <Count item={item} done={props.done} leaves={props.leaves} className="dm-lane__count" />
          </div>
          {about && (
            <div className="dm-lane__desc" title={about} data-testid={`flow-column-description-${item.id}`}>
              {about}
            </div>
          )}
          <ErrorStrip item={item} error={facts.error} />
          {port}
        </div>
      </div>
    );
  }

  const body = !collapsed && (props.holdsItems || props.dropTarget);
  return (
    <div className="dm-group flow-ctr" data-body={body} {...common}>
      <div ref={headRef} className="dm-group__head flow-ctr__head" data-item-id={item.id} onPointerDown={startDrag}>
        <div className="dm-card__id">
          <KindChip kind={item.kind} />
          {facts.tracker && (
            <BoardAddress
              repository={facts.tracker.repository}
              tracker={facts.tracker.facts}
              onOpen={handlers.onOpenTracker}
            />
          )}
          {props.holdsItems && (
            <span className="dm-card__side">
              <Count item={item} done={props.done} leaves={props.leaves} />
              <Caret item={item} collapsed={collapsed} onToggle={props.onToggleCollapsed} />
            </span>
          )}
        </div>
        <ItemName
          item={item}
          editing={editingName}
          inert={inert}
          handlers={handlers}
          className="dm-card__title flow-ctr__name"
          renameOnEnd={props.renameOnEnd}
        />
        <div className="dm-card__meta">
          <BoardStatus facts={facts} />
          <BoardMarks item={item} facts={facts} handlers={handlers} />
          <BoardAssignee facts={facts} />
        </div>
        <ErrorStrip item={item} error={facts.error} />
        {port}
      </div>
      {body && <div className="dm-group__body" aria-hidden="true" />}
    </div>
  );
}
