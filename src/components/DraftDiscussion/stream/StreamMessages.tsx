/**
 * The draft discussion column's transcript
 * (`../../../../specifications/ui/DDS-draft-discussion.md` DDS-FR-ZMXQ,
 * DDS-FR-NDSA, DDS-FR-TGWY).
 *
 * One centred content column holding the items `./streamItems.ts` derives. The
 * measure is one rule and there is no second layout: `min(820px, 100% - 28px)`
 * is 820 pixels on a wide panel and the panel less its gutters on a narrow one,
 * centred either way (DDS-FR-ZMXQ). The stylesheet carries it.
 *
 * What this module owns is the order, the dividers, and the transient row. Every
 * form the column draws lives in a file of its own beside this one.
 */
import { Fragment } from "react";

import { MessageBlock } from "./MessageBlock";
import { QuestionCard } from "./QuestionCard";
import { ChangeRow } from "./ChangeRow";
import { questionCardOf } from "./questionParse";
import { coveredBy, streamItems, type StreamItem } from "./streamItems";
import { activityStatus } from "../../../state/agentActivity";
import type { AgentRoster } from "../../agentTags";
import type { AgentTurn, Comment } from "../../../types";
import type { PairedHalf } from "../../CommentRail/questionPairs";

export interface StreamMessagesProps {
  comments: readonly Comment[];
  pairing: ReadonlyMap<string, PairedHalf>;
  isOwn: (comment: Comment) => boolean;
  roster: AgentRoster;
  threadId: string;
  draftId?: string;
  /** DDS-FR-CLBK: the head of a long discussion is folded above this index. */
  visibleFrom: number;
  /** DDS-FR-CLBK / DDS-FR-GKMT: the fold row and the unread divider. */
  beforeComment?: (commentId: string, index: number) => React.ReactNode;
  onQuote?: (commentId: string, host: HTMLElement | null) => void;
  authorOf: (commentId: string) => string;
  /** CMT-FR-15: the thread's menu, carried by the message it opens with. */
  threadActions?: React.ReactNode;
  /** CTA-FR-ZOLW: the turns still outstanding, in the position their answer takes. */
  pendingTurns: readonly AgentTurn[];
  onCancelTurn: (turnId: string) => void;
  /**
   * CTA-FR-MGVJ / CTA-FR-ARBB: what a turn left behind, at the foot of the
   * column rather than beside it, so it takes the content column's own measure
   * (DDS-FR-ZMXQ) like everything else the transcript holds.
   */
  footer?: React.ReactNode;
}

export function StreamMessages(props: StreamMessagesProps) {
  const positions = new Map(props.comments.map((c, at) => [c.id, at]));

  /**
   * DDS-FR-CLBK / DDS-FR-GKMT: what the host draws above a comment — the fold
   * row, and the unread divider.
   *
   * Asked for every comment up front, because grouping has to know. Two messages
   * by one author merge into a block (DDS-FR-QJFE) and a block has one edge, so
   * a run that closed over the reading position would lose it with nothing to
   * show that it had.
   */
  const marks = new Map<string, React.ReactNode>();
  props.comments.forEach((comment, at) => {
    const node = props.beforeComment?.(comment.id, at);
    if (node) marks.set(comment.id, node);
  });

  const items = streamItems({
    comments: props.comments,
    pairing: props.pairing,
    isOwn: props.isOwn,
    visibleFrom: props.visibleFrom,
    breakBefore: new Set(marks.keys()),
  });

  const openingId = props.comments[0]?.id;
  /** Which marks this pass has already placed (see the render below). */
  const drawn = new Set<string>();

  return (
    <div className="dds-stream" data-testid="dds-stream">
      <div className="dds-stream__column">
        {items.map((item) => (
          <Fragment key={item.key}>
            {/* One comment can produce two items — a block for its body and a
                row for the change it announces — and the mark above it belongs
                to the comment rather than to either, so it is drawn once, above
                the first of them. */}
            {coveredBy(item).map((id) => {
              if (!marks.has(id) || drawn.has(id)) return null;
              drawn.add(id);
              return <Fragment key={`mark:${id}`}>{marks.get(id)}</Fragment>;
            })}
            <StreamRow
              item={item}
              props={props}
              openingId={openingId}
              positions={positions}
            />
          </Fragment>
        ))}

        {/* CTA-FR-ZOLW: a turn that has not answered yet, in the position its
            answer will take. DDS-FR-TGWY: a dot and one lowercase word, and the
            message that arrives replaces this row rather than landing below it —
            which it does because the turn is gone the moment the comment lands.
            One row for each agent (CTA-FR-ZOLW), so the row is keyed by the
            agent and stays in place while a newer turn of that agent runs. */}
        {props.pendingTurns.map((turn) => (
          <div
            key={turn.agentId}
            className="dds-message"
            data-testid="dds-transient"
            data-turn-id={turn.id}
            aria-live="polite"
          >
            <div className="dds-message__head">
              <span className="dds-message__author" data-agent="true">
                {turn.nickname}
              </span>
              {/* CTA-FR-VHOY: no timestamp — there is nothing to stamp yet. */}
              <span className="dds-message__actions">
                <button
                  type="button"
                  className="btn btn--ghost btn--icon-xs dds-message__action"
                  aria-label={`Cancel ${turn.nickname}'s reply`}
                  title="Cancel"
                  data-testid="dds-transient-cancel"
                  onClick={() => props.onCancelTurn(turn.id)}
                >
                  ×
                </button>
              </span>
            </div>
            <div className="dds-transient" data-testid="dds-transient-status">
              <span className="dds-transient__dot" aria-hidden="true" />
              <span className="dds-transient__word">
                {activityStatus(turn).toLowerCase().replace(/…$/, "")}
              </span>
            </div>
          </div>
        ))}

        {props.footer}
      </div>
    </div>
  );
}

function StreamRow({
  item,
  props,
  openingId,
  positions,
}: {
  item: StreamItem;
  props: StreamMessagesProps;
  openingId: string | undefined;
  positions: ReadonlyMap<string, number>;
}) {
  // CMT-FR-15 / CMT-FR-HQNV: the thread's actions belong to the comment the
  // thread opens with, whichever form that comment took.
  const opens =
    openingId !== undefined && coveredBy(item).includes(openingId);

  if (item.kind === "date") {
    // DDS-FR-NDSA: a hairline, a lowercase label, a hairline.
    return (
      <div className="dds-day" data-testid="dds-day-divider">
        <span className="dds-day__rule" aria-hidden="true" />
        <span className="dds-day__label">{item.label}</span>
        <span className="dds-day__rule" aria-hidden="true" />
      </div>
    );
  }

  if (item.kind === "change") {
    return (
      <ChangeRow
        reference={item.reference}
        createdAt={item.comment.createdAt}
      />
    );
  }

  if (item.kind === "question") {
    const card = questionCardOf(
      item.question.body,
      item.answer?.body ?? null,
    );
    // DQA-FR-TSJD: a body this reader cannot read loses the grouping and
    // nothing else — the two comments still render, as ordinary messages.
    if (card === null) {
      return (
        <>
          <MessageBlock
            comments={[item.question]}
            own={false}
            roster={props.roster}
            threadId={props.threadId}
            draftId={props.draftId}
            onQuote={props.onQuote}
            firstIndex={positions.get(item.question.id) ?? 0}
            authorOf={props.authorOf}
          />
          {item.answer && (
            <MessageBlock
              comments={[item.answer]}
              own={props.isOwn(item.answer)}
              roster={props.roster}
              threadId={props.threadId}
              draftId={props.draftId}
              onQuote={props.onQuote}
              firstIndex={positions.get(item.answer.id) ?? 0}
              authorOf={props.authorOf}
            />
          )}
        </>
      );
    }
    return (
      <QuestionCard
        card={card}
        question={item.question}
        roster={props.roster}
        onQuote={props.onQuote}
        index={positions.get(item.question.id) ?? 0}
        /* CMT-FR-15: a discussion whose first comment is a submitted question
           carries the thread's own actions on that card, exactly as one opening
           with plain talk carries them on its block. */
        actions={opens ? props.threadActions : undefined}
      />
    );
  }

  return (
    <MessageBlock
      comments={item.comments}
      own={item.own}
      roster={props.roster}
      threadId={props.threadId}
      draftId={props.draftId}
      onQuote={props.onQuote}
      firstIndex={positions.get(item.comments[0].id) ?? 0}
      authorOf={props.authorOf}
      actions={opens ? props.threadActions : undefined}
    />
  );
}

