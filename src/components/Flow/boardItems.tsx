/**
 * `DBR-delivery-board.md`: the card of the board and the parts that a card
 * and a group header share: the address, the state glyph, the status, and the
 * marker glyphs. A card has three lines and one height (DBR-FR-LNZI).
 */
import type { FlowItem, FlowPosition, FlowSize } from "../../state/flowDocument";
import type { TrackerFacts } from "../../types";
import { PendingMark, STATE_LOOK } from "./chips";
import { isInteractive } from "./geometry";
import { externalBlockers, ItemName, type ItemFacts, type ItemHandlers } from "./items";
import { KIND_GLYPHS, kindLabel } from "./kindGlyphs";

/**
 * DBR-FR-XOOU: the address of an item of a GitHub plan: `owner/name#number`
 * for an issue. It truncates at its start, so the number always shows, and
 * activating it opens the tracker item (FLO-FR-ULEM).
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
  const text =
    tracker.ref.kind === "milestone"
      ? `${repository} milestone ${tracker.ref.number}`
      : `${repository}#${tracker.ref.number}`;
  return (
    <button
      type="button"
      className="flow-baddr"
      data-testid="flow-tracker-tag"
      title={`Open ${text} on GitHub`}
      aria-label={`Open ${text} on GitHub`}
      onClick={() => onOpen(tracker.ref.url)}
    >
      <bdi className="flow-baddr__text">{text}</bdi>
    </button>
  );
}

/**
 * DBR-FR-QZPA: the state glyph: a shape and a colour for each delivery state,
 * named in its tooltip. No glyph until the delivery state answers.
 */
export function StateGlyph({ facts }: { facts: ItemFacts }) {
  const state = facts.state?.state;
  if (!state) return null;
  const look = STATE_LOOK[state];
  return (
    <span
      className="flow-sglyph"
      data-state={state}
      role="img"
      aria-label={look.word}
      title={look.word}
      data-testid="flow-state-glyph"
    >
      {look.shape}
    </span>
  );
}

/**
 * DBR-FR-HWHO: the status an item shows: the `Status` of its issue in a
 * GitHub plan, or the name of its delivery state in a Flow file.
 */
export function boardStatusText(facts: ItemFacts): string | null {
  const tracker = facts.tracker?.facts;
  if (tracker) return tracker.ref.kind === "issue" ? tracker.status || "No status" : null;
  const state = facts.state?.state;
  return state ? STATE_LOOK[state].word : null;
}

/**
 * DBR-FR-GFGG / DBR-FR-CJSG / MDL-FR-PUCM: the marker glyphs: pending, error,
 * Blocked, Overlap, and the draft. Each keeps its tooltip, and Blocked and
 * Overlap keep their activation.
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
  return (
    <span className="flow-bmarks">
      {facts.pending && <PendingMark />}
      {facts.error && (
        <span
          className="flow-bmark flow-bmark--error"
          role="img"
          aria-label={facts.error}
          title={facts.error}
          data-testid={`flow-item-error-${item.id}`}
        >
          !
        </span>
      )}
      {facts.state?.blocked && blockers.length > 0 && (
        <button
          type="button"
          className="flow-bmark flow-bmark--blocked"
          title={`Blocked by ${blockers.join(", ")}`}
          aria-label={`Blocked by ${blockers.join(", ")}. Select the blocking items.`}
          data-testid="flow-marker-blocked"
          onClick={() => handlers.onSelectItems(facts.blockers.map((b) => b.id))}
        >
          ⊘
        </button>
      )}
      {facts.overlap && (
        <button
          type="button"
          className="flow-bmark flow-bmark--overlap"
          title={`Overlaps with ${facts.overlap.names.join(", ")} on ${facts.overlap.paths.join(", ")}`}
          aria-label={`Overlaps with ${facts.overlap.names.join(", ")}. Select the overlapping items.`}
          data-testid="flow-marker-overlap"
          onClick={() => handlers.onSelectItems(facts.overlap!.ids)}
        >
          ⧉
        </button>
      )}
      {item.draftId !== undefined &&
        (facts.state?.draftMissing ? (
          <span
            className="flow-bmark flow-bmark--warn"
            role="img"
            aria-label={`Draft ${item.draftId} is missing`}
            title={`Draft ${item.draftId} is missing`}
            data-testid="flow-draft-missing"
          >
            ⚠
          </span>
        ) : (
          <span
            className="flow-bmark flow-bmark--draft"
            role="img"
            aria-label={`Draft ${draftName}`}
            title={`Draft ${draftName}`}
            data-testid="flow-draft-glyph"
          >
            ✎
          </span>
        ))}
    </span>
  );
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
}

/**
 * DBR-FR-LNZI .. DBR-FR-ZRBD: a task or a defect on the board: its kind and
 * its address on the top line, its name on the middle line, and its status
 * with the state glyph and the marker glyphs on the bottom line. It shows no
 * chip, reference, excerpt, or draft row; the panel shows them.
 */
export function BoardCard(props: BoardCardProps) {
  const { item, at, size, facts, selected, magnet, lifted, editingName, inert, handlers } = props;
  const status = boardStatusText(facts);
  return (
    <div
      className="flow-card flow-bcard"
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
      <div className="flow-bcard__line flow-bcard__top">
        <span className="flow-glyph" aria-hidden="true">
          {KIND_GLYPHS[item.kind]}
        </span>
        <span className="flow-bcard__kind">{kindLabel(item.kind)}</span>
        {facts.tracker && (
          <BoardAddress
            repository={facts.tracker.repository}
            tracker={facts.tracker.facts}
            onOpen={handlers.onOpenTracker}
          />
        )}
      </div>
      <div className="flow-bcard__line flow-bcard__title">
        <ItemName
          item={item}
          editing={editingName}
          inert={inert}
          handlers={handlers}
          className="flow-card__name flow-bcard__name"
          renameOnEnd={props.renameOnEnd}
        />
      </div>
      <div className="flow-bcard__line flow-bcard__status">
        <StateGlyph facts={facts} />
        {status !== null && (
          <span className="flow-bcard__statusname" data-testid="flow-card-status">
            {status}
          </span>
        )}
        <BoardMarks item={item} facts={facts} handlers={handlers} />
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
