/**
 * DDS-FR-KTVW / DDS-FR-HGMB: the right column of the New Artifact tab — the
 * conversation between the author and the draft's agents.
 *
 * A **column, not a rail**. The measure is what decides this: an agent's answer
 * is prose, and prose read at 370 pixels is read one clause at a time. That is
 * why the discussion used to be pulled out into a floating window that covered
 * the draft, and why this column exists — so the two are read side by side and
 * neither hides the other.
 *
 * The conversation itself is rendered by the shared `DiscussionSurface`
 * (`owner="column"`) and opened by the shared `DiscussionComposer`
 * (`CVP-conversation-presentation.md` CVP-FR-SDMQ). What this column owns is
 * where the surface sits, the chooser of the draft's discussions, and what is
 * said about how far the author has scrolled. The scroll position, the unread
 * state and the follow rule are the surface's, kept in the session store.
 */
import { useEffect, useMemo, useRef, useState } from "react";

import { Icon } from "../icons";
import {
  DiscussionSurface,
  type OpenDiscussionRequest,
  type OwnerAvailability,
} from "../discussion";
import { OpenDiscussion } from "./OpenDiscussion";
import { COMPOSER_KEY } from "../../hooks/useDiscussions";
import { expandHistory, useDraftDiscussion } from "../../state/draftDiscussion";
import { setDiscussionScroll, useDiscussionSession } from "../../state/discussionSession";
import {
  discussionFragment,
  type AttachmentInput,
  type CommentQuote,
  type Discussion,
  type FragmentTarget,
  type Participant,
  type ProjectAgent,
} from "../../types";

/**
 * DDS-FR-CLBK: how many messages are shown before the head of a long discussion
 * is collapsed into one row.
 *
 * Enough that an ordinary exchange is never collapsed and a month of work is.
 */
export const COLLAPSE_ABOVE = 20;
const KEPT_WHEN_COLLAPSED = 12;

export interface DiscussionColumnProps {
  draftId: string;
  /**
   * CMT-FR-53: the draft's discussions, in the order they were opened. The
   * whole-target ones and the fragment ones are listed together (DDS-FR-VHZN).
   */
  discussions: readonly Discussion[];
  /** Which of them the column is reading, where the draft holds more than one. */
  selectedThreadId: string | null;
  onSelectThread: (threadId: string) => void;
  identity: Participant | null;
  identityBlock: { message: string; route?: string | null } | null;
  agents: readonly ProjectAgent[];
  onReply: (
    threadId: string,
    body: string,
    quotes: CommentQuote[],
    attachments: AttachmentInput[],
  ) => Promise<unknown>;
  onSetLock: (threadId: string, locked: boolean) => Promise<void>;
  onSetResolved: (threadId: string, resolved: boolean) => Promise<void>;
  errors: Record<string, string>;
  blocked: boolean;
  /**
   * DDS-FR-SVBL: the standing proposal, for the anchor bar. Absent while the
   * draft holds none.
   */
  anchor: {
    label: string;
    /** What the return leads to, named for the author. */
    action: string;
    onActivate: () => void;
  } | null;
  /**
   * DDS-FR-XQMF: the column is hidden, so the document column has the tab.
   *
   * Hidden rather than unmounted: an author part way through a reply who hides
   * the column to read the draft finds that reply where they left it.
   */
  hidden?: boolean;
  /**
   * NAW-FR-32 / DDS-FR-VHZN: begin a discussion of the draft from the column's
   * opening composer — a whole-target one, or a fragment one where the request
   * carries the passage.
   */
  onOpenDiscussion: (request: OpenDiscussionRequest) => Promise<Discussion>;
  /** Called with the discussion the backend returned, once the composer cleared. */
  onOpened?: (discussion: Discussion) => void;
  /**
   * DDS-FR-QMBC: the passage of the prompt the author is about to comment on.
   * While it is set the column shows the opening composer for it.
   */
  openingFragment?: FragmentTarget | null;
  /** DDS-FR-QMBC: the author discarded the opening composer of a fragment. */
  onCancelFragment?: () => void;
  /**
   * CMT-FR-19 / DDS-FR-QMBC: whether a fragment discussion's passage can still be
   * found in the prompt. Absent, every discussion is available.
   */
  availabilityOf?: (discussion: Discussion) => OwnerAvailability;
  /** The fragment quote of a discussion was activated. */
  onFocusFragment?: (fragment: FragmentTarget, discussion: Discussion) => void;
  /** The draft's name, for the unavailable state of a fragment. */
  ownerLabel?: string;
  /**
   * ACT-FR-QWNP: raised by the action control's **Discuss**, which opens no
   * composer of its own and moves focus here instead.
   *
   * A counter rather than a boolean, because two presses of Discuss must both
   * put the caret in the composer — and a boolean that is already `true` is a
   * second press nothing happens for.
   */
  focusSignal?: number;
}

export function DiscussionColumn(props: DiscussionColumnProps) {
  const { draftId, discussions } = props;
  const view = useDraftDiscussion(draftId);

  /**
   * NAW-FR-32: the author is writing the first message of a **new** discussion
   * rather than replying in the one on screen.
   *
   * Held here rather than in the store because it is about this sitting and not
   * about the draft: a column returned to shows the conversation, which is what
   * the author came back to read.
   */
  const [composingOther, setComposingNew] = useState(false);
  /** CMT-FR-17: whether the resolved discussions are being shown with the rest. */
  const [showResolved, setShowResolved] = useState(false);

  // DDS-FR-QMBC: a passage chosen in the prompt puts the column on the opening
  // composer that will carry it.
  const fragment = props.openingFragment ?? null;
  const composingNew = composingOther || fragment !== null;

  // DDS-FR-QMBC: a discussion focused from outside — the mark of a fragment in
  // the prompt, or a new discussion just opened — is the one the column reads,
  // and a resolved one is reached through its disclosure.
  const { selectedThreadId } = props;
  useEffect(() => {
    if (selectedThreadId === null) return;
    setComposingNew(false);
    if (discussions.some((d) => d.id === selectedThreadId && d.resolved)) {
      setShowResolved(true);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedThreadId]);

  /**
   * CMT-FR-56: a resolved discussion is not one of the draft's open
   * conversations. It leaves the chooser and is reached through the disclosure,
   * so what the column offers is what is still being talked about.
   */
  const open = useMemo(() => discussions.filter((d) => !d.resolved), [discussions]);
  const resolved = useMemo(() => discussions.filter((d) => d.resolved), [discussions]);
  const offered = showResolved ? [...open, ...resolved] : open;

  const entry =
    offered.find((d) => d.id === props.selectedThreadId) ?? offered[0];
  const thread = composingNew ? undefined : entry;

  /**
   * DDS-FR-DPRJ / DDS-FR-GKMT: whether a transcript is on screen at all.
   *
   * A column showing none — a draft nobody has spoken about yet, a draft whose
   * discussions are all resolved, or an author writing the first message of
   * another — follows nothing. Saying it is paused would describe a transcript
   * the author cannot see.
   */
  const transcript = thread !== undefined;
  // The surface keeps the reading position in the session store; the column
  // only says what it holds.
  const session = useDiscussionSession(thread?.id ?? "");
  const atTail = session.atTail;

  // ACT-FR-QWNP: put the caret where the author is about to write. The composer
  // is the column's last text field whichever of the two the column is showing —
  // the opener before a discussion exists and the reply after.
  const columnRef = useRef<HTMLElement>(null);
  const signal = props.focusSignal ?? 0;
  const focusComposer = () => {
    const fields = columnRef.current?.querySelectorAll("textarea");
    fields?.[fields.length - 1]?.focus();
  };
  useEffect(() => {
    if (signal !== 0) focusComposer();
  }, [signal]);
  // DDS-FR-QMBC: Comment on a passage puts the caret in the composer that
  // carries it.
  useEffect(() => {
    if (fragment !== null) focusComposer();
  }, [fragment]);

  // DDS-FR-CLBK: the head of a long discussion, as one row that expands in
  // place. Counted rather than measured, because a count is stable.
  const count = thread?.comments.length ?? 0;
  const hidden =
    view.historyExpanded || count <= COLLAPSE_ABOVE ? 0 : count - KEPT_WHEN_COLLAPSED;
  const firstShownId = hidden > 0 ? (thread?.comments[hidden]?.id ?? null) : null;

  return (
    <section
      ref={columnRef}
      className="dds-discussion"
      aria-label="Discussion"
      hidden={props.hidden}
      data-testid="draft-discussion-column"
    >
      {/* DDS-FR-DPRJ / DDS-FR-GKMT: the column's own state, in words. The badge
          says which of the two things the column is doing, so an author who has
          scrolled up is never left wondering why nothing is moving. It stands
          only while there is a transcript for it to be about. */}
      <header className="dds-discussion__head">
        <span className="dds-discussion__title t-eyebrow">Discussion</span>
        {transcript && (
          <span
            className={
              atTail
                ? "dds-discussion__state dds-discussion__state--following"
                : "badge badge--warn dds-discussion__state"
            }
            role="status"
            data-testid="discussion-follow-state"
          >
            {atTail ? "following" : "paused · reading history"}
          </span>
        )}
      </header>

      {/* DDS-FR-SVBL: while a proposal stands and the author is reading back,
          the way to the change under review — the one thing they came here from
          and the one thing scrolling has taken off screen. */}
      {props.anchor !== null && transcript && !atTail && (
        <div className="dds-anchor" data-testid="discussion-anchor-bar">
          <span className="dds-anchor__label t-meta">anchor</span>
          <span className="dds-anchor__name t-ui-xs" title={props.anchor.label}>
            {props.anchor.label}
          </span>
          <button
            type="button"
            className="dds-anchor__action t-ui-xs"
            onClick={() => {
              props.anchor?.onActivate();
              // The way back is to the tail of the reading, which is where the
              // arrivals the author scrolled away from are.
              const list = columnRef.current?.querySelector<HTMLElement>(
                ".comment-card__messages",
              );
              if (list && thread) {
                list.scrollTop = list.scrollHeight;
                setDiscussionScroll(thread.id, list.scrollTop, true);
              }
            }}
          >
            {props.anchor.action}
          </button>
        </div>
      )}

      {/* NAW-FR-32 / CMT-FR-53: which of the draft's discussions is being read,
          and the way to begin another. A draft with none renders no row at all —
          the column below is the composer that opens the first. */}
      {discussions.length > 0 && (
        <div className="dds-discussion__threads" role="tablist" aria-label="Discussions">
          {offered.map((d, at) => {
            const quoted = discussionFragment(d);
            return (
              <button
                key={d.id}
                type="button"
                role="tab"
                className="dds-discussion__thread t-ui-xs"
                aria-selected={!composingNew && d.id === thread?.id}
                data-active={!composingNew && d.id === thread?.id}
                data-target={quoted ? "fragment" : "whole"}
                title={
                  quoted ? `Discussion ${at + 1}: ${quoted.quote}` : `Discussion ${at + 1}`
                }
                onClick={() => {
                  setComposingNew(false);
                  props.onCancelFragment?.();
                  props.onSelectThread(d.id);
                }}
              >
                <Icon.Comment size={11} /> {at + 1}
              </button>
            );
          })}
          <button
            type="button"
            className="dds-discussion__thread t-ui-xs"
            data-active={composingNew}
            data-testid="discussion-new"
            aria-label="Begin another discussion about this draft"
            title="Begin another discussion about this draft"
            onClick={() => setComposingNew(true)}
          >
            <Icon.Plus size={11} />
          </button>
          {/* CMT-FR-17 / CMT-FR-56: the resolved discussions, behind a
              disclosure rather than gone — a conversation that was settled is
              still the record of how it was settled. */}
          {resolved.length > 0 && (
            <button
              type="button"
              className="dds-discussion__thread dds-discussion__resolved t-ui-xs"
              aria-expanded={showResolved}
              data-active={showResolved}
              onClick={() => setShowResolved((shown) => !shown)}
            >
              {resolved.length} resolved
            </button>
          )}
        </div>
      )}

      <div className="dds-discussion__body">
        {!thread && props.identityBlock && (
          <div className="comment-rail__blocked" role="status">
            {props.identityBlock.message}
            {props.identityBlock.route && (
              <div className="comment-rail__route">{props.identityBlock.route}</div>
            )}
          </div>
        )}

        {thread ? (
          /* DDS-FR-ZMXQ: this column draws the conversation in its own forms —
             one centred content column, cards only where an exchange carries
             state (DDS-FR-BRHN), and a rail rather than a fill on the author's
             own message (DDS-FR-VTKD). The markup is the shared surface's; the
             `stream` variant sets only its measure. */
          <DiscussionSurface
            discussion={thread}
            owner="column"
            variant="stream"
            agents={props.agents}
            identity={props.identity}
            identityBlock={props.identityBlock}
            disabled={props.identityBlock !== null}
            blocked={props.blocked}
            focused
            availability={props.availabilityOf?.(thread)}
            ownerLabel={props.ownerLabel}
            onFocusFragment={props.onFocusFragment}
            visibleFrom={hidden}
            beforeComment={(commentId) => {
              // DDS-FR-CLBK: everything above the fold, as one row, drawn at the
              // first message that IS shown so it reads as the head of what is
              // on screen rather than as a gap in it.
              if (commentId !== firstShownId) return null;
              return (
                <button
                  type="button"
                  className="dds-earlier t-ui-xs"
                  data-testid="discussion-earlier"
                  onClick={() => expandHistory(draftId)}
                >
                  <span className="dds-earlier__rule" aria-hidden="true" />
                  {hidden} earlier {hidden === 1 ? "message" : "messages"}
                  <span className="dds-earlier__rule" aria-hidden="true" />
                </button>
              );
            }}
            error={props.errors[thread.id]}
            onReply={props.onReply}
            onSetLock={props.onSetLock}
            onSetResolved={props.onSetResolved}
          />
        ) : (
          /* A draft nobody has spoken about yet. The column carries the
             composer that begins the conversation rather than an empty
             transcript, which would read as one that failed to load. */
          <OpenDiscussion
            state={
              fragment !== null
                ? "fragment"
                : discussions.length === 0
                  ? "unspoken"
                  : open.length === 0
                    ? "resolved"
                    : "another"
            }
            fragment={fragment}
            resolved={resolved.length}
            draftId={draftId}
            agents={props.agents}
            disabled={props.blocked || props.identityBlock !== null}
            error={props.errors[COMPOSER_KEY]}
            onOpen={props.onOpenDiscussion}
            onOpened={(created) => {
              setComposingNew(false);
              props.onOpened?.(created);
            }}
            onCancel={() => {
              setComposingNew(false);
              props.onCancelFragment?.();
            }}
          />
        )}
      </div>
    </section>
  );
}
