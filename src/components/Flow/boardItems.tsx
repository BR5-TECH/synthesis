/**
 * `DBR-delivery-board.md`: the card of the board and the parts that a card
 * and a group face share: the kind chip, the address, the status dot and
 * word, the markers, and the error strip. A card is as high as its lines
 * (DBR-FR-LNZI), so it reports its height to the board layout.
 */
import { useContext, useEffect, useRef, type RefObject } from "react";

import type { FlowItem, FlowPosition, FlowSize } from "../../state/flowDocument";
import type { TrackerFacts } from "../../types";
import { boardDot, boardStatusWord, BoardStatusNamesContext, KIND_CHIPS } from "./boardStatus";
import { Icon } from "../icons";
import { isInteractive } from "./geometry";
import { externalBlockers, ItemName, type ItemFacts, type ItemHandlers } from "./items";
import { kindLabel } from "./kindGlyphs";

/** DBR-FR-DZSU: the kind chip: three letters in the colours of the kind. */
export function KindChip({ kind }: { kind: FlowItem["kind"] }) {
  return (
    <span className="dm-kind" data-kind={kind} role="img" aria-label={kindLabel(kind)} data-testid="flow-kind-chip">
      {KIND_CHIPS[kind]}
    </span>
  );
}

/** DBR-FR-XOOU: the full address of a tracker item: `owner/name#number`. */
export function fullAddress(repository: string, tracker: TrackerFacts): string {
  return tracker.ref.kind === "milestone"
    ? `${repository} milestone ${tracker.ref.number}`
    : `${repository}#${tracker.ref.number}`;
}

/**
 * DBR-FR-XOOU: the address of an item of a GitHub plan: `#number`, with the
 * full address in its tooltip and accessible name. Activating it opens the
 * tracker item (FLO-FR-ULEM).
 */
export function BoardAddress({
  repository,
  tracker,
  onOpen,
}: {
  repository: string;
  tracker: TrackerFacts;
  onOpen: (url: string) => void;
}) {
  const text = fullAddress(repository, tracker);
  return (
    <button
      type="button"
      className="dm-card__ref"
      data-testid="flow-tracker-tag"
      title={`Open ${text} on GitHub`}
      aria-label={`Open ${text} on GitHub`}
      onClick={() => onOpen(tracker.ref.url)}
    >
      #{tracker.ref.number}
    </button>
  );
}

/**
 * DBR-FR-HWHO / DBR-FR-QZPA: the status dot and the status word in lowercase.
 * Nothing where the item shows no status.
 */
export function BoardStatus({ facts }: { facts: ItemFacts }) {
  const names = useContext(BoardStatusNamesContext);
  const source = { tracker: facts.tracker?.facts, state: facts.state?.state };
  const word = boardStatusWord(source);
  const dot = boardDot(source, names);
  if (word === null) return null;
  return (
    <span className="dm-card__status" data-testid="flow-card-status" data-dot={dot ?? undefined}>
      {dot && <i className="dm-dot" data-s={dot} aria-hidden="true" data-testid="flow-status-dot" />}
      <span className="dm-card__status-word">{word}</span>
    </span>
  );
}

/**
 * DBR-FR-GFGG / DBR-FR-ZBVZ / MDL-FR-PUCM: the markers: pending, Blocked,
 * Overlap, and the draft, each a badge with a lowercase word. Each keeps its
 * tooltip, and Blocked and Overlap keep their activation.
 */
export function BoardMarks({
  item,
  facts,
  handlers,
}: {
  item: FlowItem;
  facts: ItemFacts;
  handlers: ItemHandlers;
}) {
  const external = externalBlockers(facts);
  const blockers = [...facts.blockers.map((b) => b.name), ...external];
  const draftName = facts.state?.draft?.name ?? item.draftId;
  const blocked = facts.state?.blocked === true && blockers.length > 0;
  if (!facts.pending && !facts.proposal && !facts.ghost && !blocked && !facts.overlap && item.draftId === undefined) {
    return null;
  }
  return (
    <span className="dm-card__markers">
      {facts.pending && (
        <span
          className="badge badge--live"
          title="This change is on its way to GitHub"
          data-testid="flow-pending"
        >
          pending
        </span>
      )}
      {facts.proposal && (
        // DBR-FR-AGIN: a plan proposal changes or withdraws the item.
        <span
          className={facts.proposal === "withdrawn" ? "badge badge--danger" : "badge badge--accent"}
          title={facts.proposal === "withdrawn" ? "A plan proposal closes this issue" : "A plan proposal changes this issue"}
          data-testid="flow-marker-proposal"
        >
          {facts.proposal}
        </span>
      )}
      {facts.ghost && (
        // DBR-FR-WKNI: the issue a plan proposal adds.
        <span className="badge badge--ok" title="A plan proposal adds this issue" data-testid="flow-marker-added">
          added
        </span>
      )}
      {blocked && (
        <button
          type="button"
          className="badge badge--danger dm-mark"
          title={`Blocked by ${blockers.join(", ")}`}
          aria-label={`Blocked by ${blockers.join(", ")}. Select the blocking items.`}
          data-testid="flow-marker-blocked"
          onClick={() => handlers.onSelectItems(facts.blockers.map((b) => b.id))}
        >
          blocked
        </button>
      )}
      {facts.overlap && (
        <button
          type="button"
          className="badge badge--warn dm-mark"
          title={`Overlaps with ${facts.overlap.names.join(", ")} on ${facts.overlap.paths.join(", ")}`}
          aria-label={`Overlaps with ${facts.overlap.names.join(", ")}. Select the overlapping items.`}
          data-testid="flow-marker-overlap"
          onClick={() => handlers.onSelectItems(facts.overlap!.ids)}
        >
          overlap
        </button>
      )}
      {item.draftId !== undefined &&
        (facts.state?.draftMissing ? (
          <span
            className="badge badge--warn"
            role="img"
            aria-label={`Change ${item.draftId} is missing`}
            title={`Change ${item.draftId} is missing`}
            data-testid="flow-draft-missing"
          >
            change missing
          </span>
        ) : (
          <span
            className="badge"
            role="img"
            aria-label={`Change ${draftName}`}
            title={`Change ${draftName}`}
            data-testid="flow-draft-glyph"
          >
            change
          </span>
        ))}
    </span>
  );
}

/**
 * DBR-FR-WQAN: the assignee of an issue of a GitHub plan: `@login` of the
 * first assignee and `+N` for the others, with every login in its tooltip and
 * accessible name. Nothing for an issue with no assignee, a milestone, or an
 * item of a Flow file.
 */
export function BoardAssignee({ facts }: { facts: ItemFacts }) {
  const tracker = facts.tracker?.facts;
  const logins = tracker?.ref.kind === "issue" ? (tracker.assignees ?? []) : [];
  if (logins.length === 0) return null;
  const label = `Assigned to ${logins.map((l) => `@${l}`).join(", ")}`;
  return (
    <span className="dm-card__assignee" role="img" title={label} aria-label={label} data-testid="flow-card-assignee">
      <span className="dm-card__login">@{logins[0]}</span>
      {logins.length > 1 && <span>+{logins.length - 1}</span>}
    </span>
  );
}

/** DBR-FR-CJSG: the error strip: the error text on one line, in full in its tooltip. */
export function ErrorStrip({ item, error }: { item: FlowItem; error: string | undefined }) {
  if (error === undefined) return null;
  return (
    <div className="dm-card__error" title={error} data-testid={`flow-item-error-${item.id}`}>
      <Icon.Info size={12} aria-hidden="true" />
      <span className="dm-card__error-text">{error}</span>
    </div>
  );
}

/**
 * DBR-FR-LNZI / DBR-FR-SBTR / DBR-FR-BYQX: report the height of `ref` to the
 * board layout, plus `extra` for the borders around it. jsdom lays nothing
 * out, so there the height is 0 and the layout keeps its default.
 */
export function useBoardHeight(
  ref: RefObject<HTMLElement | null>,
  id: string,
  extra: number,
  onMeasure: ((id: string, height: number) => void) | undefined,
) {
  useEffect(() => {
    const el = ref.current;
    if (!el || !onMeasure) return;
    const report = () => {
      if (el.offsetHeight > 0) onMeasure(id, el.offsetHeight + extra);
    };
    report();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(report);
    observer.observe(el);
    return () => observer.disconnect();
  }, [ref, id, extra, onMeasure]);
}

export interface BoardCardProps {
  item: FlowItem;
  at: FlowPosition;
  size: FlowSize;
  facts: ItemFacts;
  selected: boolean;
  magnet: boolean;
  lifted: boolean;
  editingName: boolean;
  inert: boolean;
  handlers: ItemHandlers;
  renameOnEnd: boolean;
  /** DBR-FR-LNZI: report the height the lines of the card take. */
  onMeasure?: (id: string, height: number) => void;
}

/**
 * DBR-FR-LNZI .. DBR-FR-CJSG: a task or a defect on the board: its kind chip
 * and its address on the top line, its name on at most two lines, and its
 * status dot, its status, and its markers on the bottom line. It shows no
 * reference, excerpt, or draft row; the panel shows them.
 */
export function BoardCard(props: BoardCardProps) {
  if (props.facts.ghost) return <GhostCard {...props} ghost={props.facts.ghost} />;
  return <LiveCard {...props} />;
}

/**
 * DBR-FR-WKNI: the ghost card of an issue a plan proposal adds. It has the
 * lines of a card and a dashed border, and no drag, rename, menu, or port.
 */
function GhostCard(props: BoardCardProps & { ghost: { kindKnown: boolean } }) {
  const { item, at, size, facts, selected, handlers } = props;
  const faceRef = useRef<HTMLDivElement>(null);
  useBoardHeight(faceRef, item.id, 2, props.onMeasure);
  return (
    <div
      className="dm-card dm-card--ghost"
      role="group"
      aria-label={`New issue ${item.name}, proposed`}
      title={item.name}
      data-testid={`flow-item-${item.id}`}
      data-item-id={item.id}
      data-kind={item.kind}
      data-ghost="true"
      data-selected={selected}
      style={{ left: at.x, top: at.y, width: size.width, height: size.height }}
    >
      <div className="dm-card__face" ref={faceRef}>
        <div className="dm-card__id">
          {props.ghost.kindKnown && <KindChip kind={item.kind} />}
          <span className="dm-card__ref">new</span>
        </div>
        <div className="dm-card__title">{item.name}</div>
        <div className="dm-card__meta">
          <BoardMarks item={item} facts={facts} handlers={handlers} />
        </div>
      </div>
    </div>
  );
}

function LiveCard(props: BoardCardProps) {
  const { item, at, size, facts, selected, magnet, lifted, editingName, inert, handlers } = props;
  const faceRef = useRef<HTMLDivElement>(null);
  useBoardHeight(faceRef, item.id, 2, props.onMeasure);
  return (
    <div
      className="dm-card"
      role="group"
      aria-label={`${kindLabel(item.kind)} ${item.name}`}
      title={item.name}
      data-testid={`flow-item-${item.id}`}
      data-item-id={item.id}
      data-kind={item.kind}
      data-state={facts.state?.state}
      data-selected={selected}
      data-magnet={magnet}
      data-pending={facts.pending}
      data-error={facts.error !== undefined}
      data-lifted={lifted}
      style={{ left: at.x, top: at.y, width: size.width, height: size.height }}
      tabIndex={handlers.menu ? 0 : undefined}
      {...handlers.menu}
      onPointerDown={(e) => {
        if (!isInteractive(e.target)) handlers.onDragStart(e);
      }}
      onPointerUp={handlers.onPointerUp}
    >
      <div className="dm-card__face" ref={faceRef}>
        <div className="dm-card__id">
          <KindChip kind={item.kind} />
          {facts.tracker && (
            <BoardAddress
              repository={facts.tracker.repository}
              tracker={facts.tracker.facts}
              onOpen={handlers.onOpenTracker}
            />
          )}
        </div>
        <ItemName
          item={item}
          editing={editingName}
          inert={inert}
          handlers={handlers}
          className="dm-card__title"
          renameOnEnd={props.renameOnEnd}
        />
        <div className="dm-card__meta">
          <BoardStatus facts={facts} />
          <BoardMarks item={item} facts={facts} handlers={handlers} />
          <BoardAssignee facts={facts} />
        </div>
        <ErrorStrip item={item} error={facts.error} />
      </div>
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
