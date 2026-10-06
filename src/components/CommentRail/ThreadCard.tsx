import { Fragment, useEffect, useMemo, useRef, useState } from "react";
import { Icon } from "../icons";
import {
  AttachControl,
  CommentAttachmentList,
  PendingAttachmentStrip,
  useAttachmentDraft,
} from "../CommentAttachments";
import { CommentMarkdown } from "../CommentMarkdown";
import { formatRelative } from "../ProjectPicker";
import { MentionComposer } from "../Agents";
import { activityStatus } from "../../state/agentActivity";
import { agentRoster } from "../agentTags";
import { activeAgents, composerPlaceholder } from "../../state/activeAgents";
import { commentErrorMessage, turnFailureMessage } from "./messages";
import { DiscussionQuestions } from "../DiscussionQuestions";
import { StreamMessages } from "../DraftDiscussion/stream";
import { ThreadActions, TurnOutcomes } from "./ThreadCardParts";
import { pairedHalves } from "./questionPairs";
import { useQuestionSet } from "../../state/questionSets";
import { usePendingContributions } from "../../hooks/usePendingContributions";
import { submitQuestionAnswers } from "../DiscussionQuestions/submit";
import type { ThreadCardProps } from "./cardTypes";
import {
  isLocalParticipant,
  participantTitle,
  discussionDraftId,
  discussionFragment,
  isFragmentTargeted,
  type CommentQuote,
  type Participant,
  type ProjectAgent,
} from "../../types";
import { useParticipantLabel } from "../../state/projectIdentity";

/**
 * CVP-FR-32: whether this message is one the author wrote.
 *
 * Compared on the participant the backend stamped rather than on anything held
 * locally (CMT-FR-10). An agent is never the author's own, whatever it is
 * handled: the split says "mine" and "not mine", and a human logged in as
 * `claude` and an agent handled `claude` must not read identically.
 */
function isOwnComment(
  author: Participant,
  identity: Participant | null | undefined,
): boolean {
  if (!identity || identity.kind !== "human" || author.kind !== "human") {
    return false;
  }
  // CMT-FR-ZCAE: a comment of the local participant is the author's own
  // whichever identity now labels it.
  return isLocalParticipant(author) || author.login === identity.login;
}

export function ThreadCard({
  entry,
  top,
  positioned,
  blocked,
  disabled,
  focused,
  onFocus,
  menuOpen,
  onToggleMenu,
  onCloseMenu,
  onReply,
  onSetLock,
  onSetResolved,
  error,
  onMeasure,
  agents,
  pendingTurns: runningTurns,
  turnFailure,
  failedTurn,
  imageNotice,
  retryingTurnIds,
  onRetryTurn,
  onCancelTurn: cancelTurn,
  variant = "card",
  composer,
  identity,
  messagesRef,
  onMessagesScroll,
  beforeComment,
  visibleFrom = 0,
}: ThreadCardProps) {
  const participantLabel = useParticipantLabel();
  const thread = entry.thread;
  // CTA-FR-ZOLW, CTA-FR-XMCQ: one pending contribution for each agent.
  const { pendingTurns, onCancelTurn } = usePendingContributions(
    runningTurns,
    cancelTurn,
  );
  /**
   * DDS-FR-ZMXQ: the draft's discussion column, which draws the same comments in
   * its own forms. It is not one of CVP-FR-32's modes — a draft conversation
   * holds no presentation instance at all (CVP-FR-DKPW) — so nothing the other
   * three variants render changes for it, and nothing it renders reaches them.
   */
  const stream = variant === "stream";
  const embedded = variant === "embedded";
  // AGT-FR-29 / AGT-FR-38: both lists, so a live `@all` is bold while one in a
  // project where nobody can answer is left as prose.
  const roster = useMemo(() => agentRoster(agents), [agents]);
  /**
   * CTA-FR-IGBO: the conversation's active agents, which is who a reply carrying
   * no tag of its own reaches (CTA-FR-LCFU, CTA-FR-HCMJ).
   *
   * Derived here rather than held anywhere, so the statement above the composer
   * follows both of its inputs without a watcher or a poll of its own: a comment
   * appended anywhere in the conversation and a roster read returning something
   * new each re-render this card. The set a *dispatch* uses is derived again at
   * the moment of that dispatch rather than taken from this.
   */
  const activeRecipients = useMemo(
    () => activeAgents(thread, roster),
    [thread, roster],
  );
  const [localReply, setLocalReply] = useState("");
  const [localQuotes, setLocalQuotes] = useState<CommentQuote[]>([]);
  // CVP-FR-47: the instance's composer where the conversation has one, the
  // card's own otherwise.
  const reply = composer ? composer.body : localReply;
  const setReply = composer ? composer.setBody : setLocalReply;
  const quotes = composer ? composer.quotes : localQuotes;
  const setQuotes: (next: readonly CommentQuote[]) => void = composer
    ? composer.setQuotes
    : (next) => setLocalQuotes([...next]);
  const attachments = useAttachmentDraft(
    composer
      ? { pending: composer.attachments, onChange: composer.setAttachments }
      : undefined,
  );
  const ref = useRef<HTMLDivElement>(null);
  /** CVP-FR-40: guards the accelerator against a second post mid-flight. */
  const postingRef = useRef(false);

  /**
   * CMT-FR-32: a counter that advances when the overflow menu *opens*, which is
   * what the card's other transient surfaces dismiss on.
   *
   * A counter rather than `menuOpen ? 1 : 0`, because that boolean changes on
   * close as well as on open — and closing the overflow menu is exactly what
   * opening the attach menu does, so the two would race and the attach menu
   * would dismiss itself the instant it appeared.
   */
  const [menuOpenings, setMenuOpenings] = useState(0);
  useEffect(() => {
    if (menuOpen) setMenuOpenings((n) => n + 1);
  }, [menuOpen]);

  /**
   * Report the rendered height so the stacker can keep the next card clear of
   * this one (CMT-FR-27) rather than assuming every card is the same size.
   *
   * Measured on every render *and* watched with a `ResizeObserver`, because a
   * card can grow after its last render: an attachment's thumbnail arrives from
   * an async read and is then decoded by the browser, both of which finish long
   * after React is done. A render-time measurement alone leaves the stack
   * computed against the card's pre-image height, and the next card overlaps
   * this one by however tall the picture turned out to be — covering this card's
   * composer entirely, with nothing to trigger a correction.
   */
  /**
   * NAW-FR-32 / CMT-FR-28: bring a focused card into view.
   *
   * The pinned sections scroll within themselves (`max-height` plus
   * `overflow-y: auto`), so a card appended to the Discussion section lands
   * below its fold as soon as the section is full — and on a short window the
   * *first* card past the fold is the one just posted. Without this the composer
   * closes, the strip clears, and the author sees nothing appear at all.
   *
   * `block: "nearest"` scrolls only the ancestor that actually needs to move, so
   * focusing a card that is already visible does nothing, and an aligned card in
   * an Editor tab is unaffected — its own scroller is the body, which CMT-FR-28
   * moves through the anchor rather than through the card.
   */
  useEffect(() => {
    const el = ref.current;
    if (!focused || !el) return;
    if (typeof el.scrollIntoView !== "function") return;
    el.scrollIntoView({ block: "nearest" });
  }, [focused]);

  /** The passage this card was attached to, if it was attached to one at all. */
  const quoted = discussionFragment(thread);

  useEffect(() => {
    const el = ref.current;
    if (!el) return;
    if (!onMeasure) return;
    const report = () => {
      const height = el.getBoundingClientRect().height;
      if (height > 0) onMeasure(thread.id, height);
    };
    report();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(report);
    observer.observe(el);
    return () => observer.disconnect();
  }, [thread.id, onMeasure]);

  /**
   * CMT-FR-12: seed the composer from a comment already in this card — the part
   * of it the reader has selected, or the whole message when they have selected
   * nothing. The excerpt is carried structurally in `quotes` as well as into the
   * body text, so a reply three messages later still names what it answered even
   * if the body is later read on its own (CMT-FR-13).
   *
   * A selection lying outside this comment is not what the reader is quoting —
   * text highlighted in the artifact, or in a different message in the same
   * thread — so it is ignored in favour of the whole message.
   */
  /**
   * DQA-FR-FBWO: which comments are the two halves of one submitted exchange.
   *
   * Presentation and nothing else (DQA-FR-TSJD): no stored relation joins them,
   * neither body is rewritten, and a surface that ignored this map would render
   * two ordinary comments and lose only the grouping. The two halves keep their
   * own heads and their own Quote — they are ordinary comments of the
   * conversation from the moment they land (DQA-FR-NKAX) — and what the map
   * changes is that they are drawn inside one entry with a single rule between
   * them rather than as two stacked cards.
   */
  const pairing = useMemo(() => pairedHalves(thread.comments), [thread.comments]);

  /**
   * DQA-FR-NRZB: the question set this discussion holds, if any.
   *
   * Read here rather than in each of the five presentations because every one
   * of them renders this card — the rail's Discussion section, the action
   * control's floating panel, the detached overlay, the conversation tab, and
   * the draft's discussion column — so the block, the disabled composer, and
   * the withheld Quote all follow from one place (DQA-FR-PXNC, DQA-FR-VJHT).
   */
  const questionSet = useQuestionSet(
    !isFragmentTargeted(thread) ? thread.id : null,
  );

  const quoteFrom = (commentId: string, host: HTMLElement | null) => {
    const selection = typeof window !== "undefined" ? window.getSelection() : null;
    const inside =
      selection !== null &&
      selection.rangeCount > 0 &&
      host !== null &&
      host.contains(selection.getRangeAt(0).commonAncestorContainer);
    // DQA-FR-VJHT: while a set stands, no other human-authored contribution to
    // this discussion is offered, and a seeded quote is the opening of one.
    // Reading, scrolling, locking, resolving and moving the conversation are
    // untouched.
    if (questionSet) return;
    const selected = inside ? selection.toString().trim() : "";
    const excerpt =
      selected !== ""
        ? selected
        : (thread.comments.find((c) => c.id === commentId)?.body ?? "").trim();
    if (excerpt === "") return;
    if (quotes.some((q) => q.commentId === commentId && q.excerpt === excerpt)) {
      return;
    }
    setQuotes([...quotes, { commentId, excerpt }]);
  };

  /**
   * CMT-FR-34: a refused post leaves the composer's content exactly as it was, so
   * the author retries (or copies it out) rather than losing what they wrote. The
   * rejection is swallowed here because the rail has already rendered it on the
   * card — letting it escape would surface as an unhandled rejection and tell the
   * user nothing they are not already being shown.
   */
  const post = async () => {
    if (reply.trim() === "") return;
    if (postingRef.current) return;
    postingRef.current = true;
    try {
      await onReply(thread.id, reply, [...quotes], attachments.inputs);
    } catch {
      // CMT-FR-51: an append refused for its attachments leaves the body *and*
      // every entry of the pending strip intact, so the offending one is
      // removed and the comment posted without any of it being retyped.
      return;
    } finally {
      postingRef.current = false;
    }
    setReply("");
    setQuotes([]);
    attachments.clear();
  };

  /**
   * CVP-FR-40 / CMT-FR-70: **Ctrl+Enter** on Windows and Linux and **Cmd+Enter**
   * on macOS post the composer and prevent the newline the key would otherwise
   * insert.
   *
   * It acts only on a body that is not empty and not whitespace alone, and does
   * nothing at all while a post is already in flight, while the composer is
   * disabled for want of a resolved author identity, while the conversation is
   * locked (a locked card carries no composer at all, so the handler is not even
   * mounted), and while an IME composition is in progress — `isComposing` is what
   * a commit keypress carries, and posting on it would send a half-typed word and
   * eat the commit.
   *
   * The mention picker's own key handling is left untouched: it takes Enter and
   * Tab for completion and never this pair (AGT-FR-27), so Enter completes a
   * nickname where the picker is open and the accelerator posts where it is not.
   */
  const onComposerKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key !== "Enter") return;
    if (!(e.metaKey || e.ctrlKey)) return;
    if (e.nativeEvent.isComposing) return;
    if (disabled || blocked) return;
    if (reply.trim() === "") return;
    if (postingRef.current) return;
    e.preventDefault();
    void post();
  };

  const authorOf = (commentId: string): string => {
    const c = thread.comments.find((x) => x.id === commentId);
    return c ? participantLabel(c.author) : "";
  };

  /**
   * CMT-FR-15 / CMT-FR-16 / CMT-FR-HQNV: the thread's own actions, which the
   * message the thread opens with carries rather than a header row of its own.
   *
   * Built once here because two presentations place them: the card's own
   * author line, and the stream's (DDS-FR-FKZL), which puts every action at
   * the trailing end of that line. The markup is the same either way — where
   * they sit is the surface's decision and what they do is not.
   */
  /**
   * CTA-FR-MGVJ / CTA-FR-ARBB: what a turn left behind — the failed
   * contribution and its Retry, and the unsupported-image notice.
   *
   * Built once because both presentations place them at the foot of the
   * messages, and neither changes what they say.
   */
  // CTA-FR-MGVJ / CTA-FR-ARBB: the failed contribution with its Retry, and the
  // unsupported-image notice, at the foot of the messages.
  const turnOutcomes = (
    <TurnOutcomes
      imageNotice={imageNotice}
      failedTurn={failedTurn}
      thread={thread}
      retryingTurnIds={retryingTurnIds}
      onRetryTurn={onRetryTurn}
    />
  );

  // CMT-FR-15 / CMT-FR-16 / CMT-FR-HQNV: the thread's own actions, which the
  // message the thread opens with carries rather than a header row of its own.
  // Two presentations place them; neither changes what they do.
  const threadActions = (
    <ThreadActions
      thread={thread}
      blocked={blocked}
      menuOpen={menuOpen}
      onToggleMenu={onToggleMenu}
      onCloseMenu={onCloseMenu}
      onSetLock={onSetLock}
      onSetResolved={onSetResolved}
    />
  );

  return (
    <div
      ref={ref}
      className={
        stream
          ? "comment-card comment-card--stream"
          : embedded
            ? "comment-card comment-card--embedded"
            : "comment-card"
      }
      style={positioned ? { top } : undefined}
      data-positioned={positioned}
      data-focused={focused}
      data-locked={thread.locked}
      data-resolved={thread.resolved}
      data-testid={`comment-thread-${thread.id}`}
      /**
       * CMT-FR-28: focusing a card scrolls its anchor into view and the rail
       * re-arranges around the newly focused card — this one moves by tens of
       * pixels as a result.
       *
       * On `mousedown` that movement happened *between* the press and the
       * release, so the control the author pressed travelled out from under the
       * pointer and the click landed on the card instead of on the button: the
       * first press on Detach, Maximize, or the overflow menu of an unfocused
       * card did nothing but scroll, and only a second press — with the card
       * already focused and nothing left to move — worked. On `click` the
       * movement happens after the press and the release have both been
       * delivered, so every control acts on the first press.
       */
      onClick={onFocus}
    >
      {/* CMT-FR-19: an orphaned card shows the quote it was attached to in place
          of an alignment, so it is still identifiable. CMT-FR-55: a discussion
          carries no anchor quote at all — it is about the draft as a whole, and
          a quote of nothing would be a card claiming a passage it has not got.
          CVP-FR-32: embedded in an overlay or a tab the quote is in that
          surface's own header, so repeating it here would be the same line twice. */}
      {quoted !== null && !embedded && (
        <div className="comment-card__quote" title={quoted.quote}>
          ❝ {quoted.quote}
        </div>
      )}

      {thread.locked && (
        <div className="comment-card__badge" data-kind="locked">
          <Icon.Lock size={11} /> Locked
        </div>
      )}

      {/* CVP-FR-TWRL: the scrolling half. Everything the conversation has said
          goes here; the composer below is the half that stays put. In a rail
          card there is nothing to pin against, so this is a plain wrapper and
          the card scrolls as one with the margin around it. */}
      <div
        ref={messagesRef}
        className="comment-card__messages"
        onScroll={onMessagesScroll}
      >
      {/* DDS-FR-ZMXQ: the draft's discussion column draws the same comments in
          its own forms — one centred content column, cards only where an
          exchange carries state (DDS-FR-BRHN), and a submitted question and its
          answer as one card (DQA-FR-KYWR). Every other variant is untouched by
          this branch and keeps the stack below. */}
      {stream && (
        <StreamMessages
          comments={thread.comments}
          pairing={pairing}
          isOwn={(comment) => isOwnComment(comment.author, identity)}
          roster={roster}
          threadId={thread.id}
          draftId={discussionDraftId(thread)}
          visibleFrom={visibleFrom}
          beforeComment={beforeComment}
          /* CMT-FR-12 / DQA-FR-VJHT: Quote goes wherever the composer goes, so
             a locked, resolved, or question-held conversation offers none. */
          onQuote={
            !thread.locked && !thread.resolved && !disabled && !questionSet
              ? quoteFrom
              : undefined
          }
          authorOf={authorOf}
          threadActions={threadActions}
          pendingTurns={pendingTurns}
          onCancelTurn={onCancelTurn}
          footer={turnOutcomes}
        />
      )}
      {!stream && thread.comments.map((comment, index) => (
        <Fragment key={comment.id}>
        {beforeComment?.(comment.id, index)}
        {index >= visibleFrom && (
        <div
          className="comment"
          /* CVP-FR-32: a message the author wrote themselves reads from the
             trailing side and everything said to them from the leading side —
             human and agent alike, because what the split says is "mine" and
             "not mine" rather than anything about who is on the other end. */
          data-own={embedded && isOwnComment(comment.author, identity)}
          /* DQA-FR-FBWO: which half of a submitted exchange this is, or absent
             where it is an ordinary comment. The stylesheet joins the two into
             one entry with a single rule between them. */
          data-pair={pairing.get(comment.id)}
        >
          <div className="comment__head">
            <span
              className="comment__author"
              data-agent={comment.author.kind === "agent"}
            >
              {/* CMT-FR-10. The title line beneath indents past this marker to
                  align with the name — `kit.css` `.comment__title::before`
                  carries a hidden copy of it, so the two must stay in step. */}
              {comment.author.kind === "agent" && "✦ "}
              {participantLabel(comment.author)}
            </span>
            <span className="comment__time" title={comment.createdAt}>
              {formatRelative(comment.createdAt)}
            </span>
            {/* CMT-FR-12: Quote seeds this card's composer, so it goes wherever
                the composer goes — on a locked or a resolved card it would be a
                control that visibly does nothing.
                DQA-FR-VJHT: and while a question set stands, for the same
                reason — no other human-authored contribution to this discussion
                is offered until the questions are answered. */}
            {!thread.locked && !thread.resolved && !disabled && !questionSet && (
              <button
                className="btn btn--ghost btn--icon-xs comment__quote-button"
                // Position in the thread, not just the author: two messages by
                // the same person would otherwise share one name, leaving a
                // keyboard user no way to say which of them they meant
                // (CMT-FR-35).
                aria-label={`Quote comment ${index + 1} by ${participantLabel(comment.author)}`}
                title="Quote"
                // Without this the button's own mousedown collapses the
                // selection before the click lands, and quoting a highlighted
                // excerpt could never work.
                onMouseDown={(e) => e.preventDefault()}
                onClick={(e) =>
                  quoteFrom(comment.id, e.currentTarget.closest(".comment"))
                }
              >
                <Icon.Quote size={12} />
              </button>
            )}
            {/* CMT-FR-15 / CMT-FR-16: the thread's own menu, carried by the
                message the thread opens with rather than by a header row of its
                own. A card used to lead with a row naming the first comment's
                author and the thread's last-touched time, which on an anchored
                card the quote block separated from the message below it — but a
                discussion has no quote (CMT-FR-55), so the row landed directly
                above a second line naming the same author, and read as an empty
                first message that happened to own the menu. The thread's own
                timestamp went with it: CMT-FR-08 asks for one per comment, and
                a card-level one differing from the last comment's by a minute
                is noise. */}
            {index === 0 && threadActions}
          </div>
          {/* CTA-FR-YOGW: the role the agent answered under, on a line of its
              own directly below the name. It is the snapshot this comment's
              own participant carries — never the agent's current title, which
              is why nothing here consults the roster. Absent and empty both
              render nothing at all: no placeholder, no dash, and never
              `Not defined`, which belongs to a prompt (CVL-FR-04). */}
          {participantTitle(comment.author) && (
            <div className="comment__title" data-testid="comment-agent-title">
              {participantTitle(comment.author)}
            </div>
          )}
          {comment.quotes.map((q, i) => (
            <div key={i} className="comment__quoted">
              ❝ {authorOf(q.commentId)}: {q.excerpt}
            </div>
          ))}
          {/* AGT-FR-29: a tag that matches an enrolled agent is rendered
              distinguishably from the surrounding prose, so a reader can tell
              at a glance who a message was addressed to. One that matches
              nobody stays ordinary text (AGT-FR-28). */}
          <CommentMarkdown
            body={comment.body}
            agentRoster={roster}
          />
          {/* CMT-FR-48: below the body, an image as a thumbnail and anything
              else as a named chip. CMT-FR-50: nothing here removes one. */}
          <CommentAttachmentList
            threadId={thread.id}
            draftId={discussionDraftId(thread)}
            attachments={comment.attachments}
          />
        </div>
        )}
        </Fragment>
      ))}

      {/* CTA-FR-ZOLW: each outstanding turn renders as a pending contribution
          attributed to the agent it is waiting on, in the position that agent's
          answer will take, carrying a control that cancels it. It is not part of
          the conversation and not part of the artifact (CTA-FR-OBRO). */}
      {!stream && pendingTurns.map((turn) => (
        <div
          key={turn.agentId}
          className="comment comment--pending"
          data-own={false}
          data-testid="comment-pending"
          data-turn-id={turn.id}
          aria-live="polite"
        >
          <div className="comment__head">
            <span className="comment__author" data-agent="true">
              ✦ {turn.nickname}
            </span>
            {/* CTA-FR-VHOY: no timestamp — there is nothing to stamp yet. The
                cancel control takes the place a delivered comment leaves empty. */}
            <button
              className="btn btn--ghost btn--icon-xs"
              aria-label={`Cancel ${turn.nickname}'s reply`}
              title="Cancel"
              data-testid="comment-pending-cancel"
              onClick={() => onCancelTurn(turn.id)}
            >
              <Icon.X size={11} />
            </button>
          </div>
          {/* CTA-FR-XMCQ / CTA-FR-FBJR: the activity status of what that agent is
              doing at this moment. Worded as a participant who has been asked
              something and has not answered yet, rather than as a spinner or a
              progress bar — what the author is waiting on is a collaborator
              composing a reply, and the card should read like a conversation in
              progress. CTA-FR-KWOF: read from the turn's own activation order, so
              two events arriving out of order settle on the same status either
              way, and it lands on exactly one non-empty line at every
              transition. A turn waiting on a model reads **Thinking…**, and it
              reads that way through every automatic retry and every wait between
              them; it says nothing anywhere about attempts, how many times the
              application had to ask not being something a conversation is
              about. */}
          <p
            className="comment__body comment__body--pending"
            data-testid="comment-pending-status"
          >
            {activityStatus(turn)}
          </p>
        </div>
      ))}

      {!stream && turnOutcomes}
      </div>

      {/* CMT-FR-15: a locked card renders no composer, and its comments stay
          fully readable. CMT-FR-16: nor does a RESOLVED one — a thread the
          author has settled is not a thread to add to, and offering a composer
          inside the disclosure invites a reply nobody is coming back to read.
          Continuing one is the one action it does offer: reopen it, and the
          composer is there again. */}
      {/* DQA-FR-NRZB: the block sits between the message list and the composer,
          in every presentation that renders this card. DQA-FR-TVMH: outside the
          discussion's history entirely — it contributes no comment and occupies
          no position in the list above. */}
      {questionSet && !thread.locked && !thread.resolved && (
        <DiscussionQuestions
          set={questionSet}
          onSubmit={(answers) =>
            submitQuestionAnswers(thread.id, questionSet.setId, answers)
          }
          onSubmitted={() => {
            // DQA-FR-IPFD: the block and its unsent draft go together. Both
            // follow from the backend reporting that the discussion now holds
            // no set, which the submission has already published.
          }}
          describeError={commentErrorMessage}
        />
      )}

      {/* DQA-FR-PXNC: while a set stands the composer is disabled and carries
          one line saying the questions above are to be answered first. It says
          nothing about the agent, about a turn, or about how long an answer has
          been owed. */}
      {questionSet && !thread.locked && !thread.resolved && (
        <p
          className="discussion-questions__composer-note"
          data-testid="discussion-questions-composer-note"
        >
          Answer the questions above to continue the discussion.
        </p>
      )}

      {!questionSet && !thread.locked && !thread.resolved && (
        <div className="comment-card__reply">
          {quotes.map((q, i) => (
            <div key={i} className="comment__quoted comment__quoted--pending">
              ❝ {authorOf(q.commentId)}: {q.excerpt}
              <button
                aria-label="Remove quote"
                title="Remove quote"
                onClick={() => setQuotes(quotes.filter((_, j) => j !== i))}
              >
                <Icon.X size={11} />
              </button>
            </div>
          ))}
          {/* CTA-FR-VQFJ: an unlocked composer offers the project's enrolled
              agents through the mention picker. A locked thread carries no
              composer at all (CMT-FR-15) and therefore offers no way to address
              anyone. */}
          <div
            className="comment-composer"
            // CMT-FR-45: a file dropped on the composer attaches without the
            // Attach control being opened.
            onDragOver={(e) => e.preventDefault()}
            onDrop={(e) => {
              const files = Array.from(e.dataTransfer?.files ?? []);
              if (files.length === 0) return;
              e.preventDefault();
              void attachments.addFiles(files);
            }}
          >
            {/* The three sit on one line as one field — Attach leading, the
                message between them, Post trailing — so writing a reply and
                sending it are one gesture in one place, in the rail, the
                detached overlay, and the conversation tab alike. */}
            <div className="comment-composer__row">
              {/* CMT-FR-32: at most one of the card's transient surfaces is
                  open at a time — opening this dismisses the overflow menu, and
                  opening that dismisses this. */}
              <AttachControl
                disabled={disabled || blocked}
                onFiles={(files) => void attachments.addFiles(files)}
                onLink={attachments.addLink}
                onOpen={onCloseMenu}
                dismissSignal={menuOpenings}
                align="leading"
              />
              <MentionComposer
                className="comment-card__composer"
                // CTA-FR-EMQS: the accessible name names the composer exactly as
                // it does in a conversation with no active agent at all — the
                // recipients are presentation, and an untruncated list is
                // disclosed nowhere, in no tooltip, no hover title, and no
                // accessible name.
                ariaLabel={`Reply to thread ${thread.id}`}
                /* CTA-FR-YGMB: who a reply carrying no tag of its own will
                   reach, carried here and nowhere else — no line above the
                   field, beside it, or below it states a recipient, the field
                   itself being where the author is about to write to them.

                   A reading rather than anything the conversation saved: it is
                   recomputed from this card's two inputs, the comments and the
                   roster, so an agent enrolled or withdrawn is named or dropped
                   as soon as the surface knows of it, and an `@all` behind the
                   set is answered for by the room as it now is (AGT-FR-37). A
                   conversation with no active agent reads `Reply…` and changes
                   nothing else: the composer stays enabled and a post is still
                   accepted (CTA-FR-FKLG). Nothing here names the agents the typed
                   body itself tags — those are the author's own words, already
                   legible where they wrote them (AGT-FR-29). */
                placeholder={composerPlaceholder(activeRecipients)}
                value={reply}
                disabled={disabled || blocked}
                agents={agents as ProjectAgent[]}
                onChange={setReply}
                onKeyDown={onComposerKeyDown}
                // One line to start, five at most, wherever a conversation is
                // read — the rail, the overlay, and the tab alike.
                autoGrow
                onPaste={(e) => {
                  // CMT-FR-45: pasted image data attaches; a paste carrying no
                  // file is ordinary text and reaches the composer untouched.
                  const files = Array.from(e.clipboardData?.files ?? []);
                  if (files.length === 0) return;
                  e.preventDefault();
                  void attachments.addFiles(files);
                }}
                // CMT-FR-32 / AGT-FR-30: opening this card's overflow menu dismisses
                // the picker as it dismisses the card's other transient surfaces.
                dismissSignal={menuOpen ? 1 : 0}
              />
              {/* DDS-FR-HQTX: the hint for the post accelerator, in the field it
                  applies to (CMT-FR-70). The draft's discussion column alone —
                  the rail's card is too narrow to spend the width on, and the
                  overlay and the tab state it nowhere else either. */}
              {stream && (
                <span className="kbd dds-reply__kbd" aria-hidden="true">
                  ⌘↵
                </span>
              )}
              {/* CMT-FR-11: rendered only while there is something to discard. */}
              {(reply !== "" ||
                quotes.length > 0 ||
                attachments.pending.length > 0) && (
                <button
                  className="btn btn--ghost btn--icon-sm comment-composer__control"
                  aria-label="Cancel"
                  title="Cancel"
                  onClick={() => {
                    setReply("");
                    setQuotes([]);
                    // CMT-FR-46: the strip goes with the typed body.
                    attachments.clear();
                  }}
                >
                  <Icon.X size={13} />
                </button>
              )}
              <button
                className="btn btn--ghost btn--icon-sm comment-composer__control comment-card__post"
                aria-label="Post"
                title="Post"
                disabled={disabled || blocked || reply.trim() === ""}
                onClick={() => void post()}
              >
                <Icon.Send size={13} />
              </button>
            </div>
            {/* CMT-FR-46: the pending strip sits inside the field, below the
                line the message is written on. */}
            <PendingAttachmentStrip
              pending={attachments.pending}
              onRemove={attachments.removeAt}
            />
          </div>
        </div>
      )}

      {/* CMT-FR-34: a failed operation attaches to the card that triggered it and
          leaves the composer's content intact. */}
      {error && (
        <div className="comment-card__error" role="alert">
          {commentErrorMessage(error)}
        </div>
      )}

      {/* CTA-FR-IGNT: a turn that failed renders its typed failure inline at the
          foot of the card, leaving every comment in the thread untouched — a
          conversation gains a line only when someone actually said something
          (CTA-FR-ZJZD). */}
      {turnFailure && (
        <div
          className="comment-card__error"
          role="alert"
          data-testid="comment-turn-error"
        >
          {turnFailureMessage(turnFailure)}
        </div>
      )}
    </div>
  );
}
