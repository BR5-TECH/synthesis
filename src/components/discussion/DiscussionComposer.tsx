/**
 * The one composer of every discussion
 * (`CVP-conversation-presentation.md` CVP-FR-SDMQ, CVP-FR-40, CVP-FR-41,
 * CVP-FR-55, CVP-FR-56).
 *
 * It renders the Markdown input, the agent mention picker, the attachments, the
 * validation, and the typed-error and identity-error display. It has two modes,
 * and the field the author writes in is identical in both:
 *
 * - **reply** continues a discussion that exists. The discussion id is the key
 *   of what the author has typed.
 * - **opening** creates a discussion. It takes the target and the optional
 *   fragment target, and the discussion target key is the key of what the author
 *   has typed. The registry holds nothing for it while it is unposted
 *   (CVP-FR-64).
 *
 * What the author has typed, quoted, and attached is held in the session store
 * (`../../state/discussionSession.ts`), not in this component. Unmounting the
 * composer, changing its owner surface, and closing and reopening a tab all
 * find the same text (CVP-FR-47).
 *
 * Moved from the composer block of `../CommentRail/ThreadCard.tsx` and from
 * `../DraftDiscussion/OpenDiscussion.tsx`, which post, attach, and clear on the
 * same terms.
 */
import { useEffect, useMemo, useRef, useState } from "react";

import { Icon } from "../icons";
import {
  AttachControl,
  PendingAttachmentStrip,
  useAttachmentDraft,
} from "../CommentAttachments";
import { MentionComposer } from "../Agents";
import { agentRoster } from "../agentTags";
import { commentErrorMessage } from "../CommentRail/messages";
import { logDebug, logWarn } from "../../logging";
import { activeAgents, composerPlaceholder } from "../../state/activeAgents";
import {
  clearComposer,
  setComposerAttachments,
  setComposerBody,
  setComposerQuotes,
  setDiscussionError,
  setFocusWasComposer,
  useDiscussionSession,
} from "../../state/discussionSession";
import {
  discussionTargetKey,
  type AttachmentInput,
  type CommentQuote,
  type Discussion,
  type DiscussionTarget,
  type FragmentTarget,
  type ProjectAgent,
} from "../../types";
import { isPostAccelerator, postAcceleratorHint } from "./composerKeys";
import { useParticipantLabel } from "../../state/projectIdentity";

/** What an opening composer asks for when the author posts. */
export interface OpenDiscussionRequest {
  target: DiscussionTarget;
  /** `null` is a discussion about the whole target. */
  fragmentTarget: FragmentTarget | null;
  body: string;
  attachments: AttachmentInput[];
}

interface ComposerCommon {
  agents: readonly ProjectAgent[];
  /** Off for want of a resolved author identity (CMT-FR-24). */
  disabled?: boolean;
  /** Off while an operation of the surface is in flight. */
  blocked?: boolean;
  /**
   * A typed error from the owner. The composer also records the typed error of
   * its own refused post, so an owner may pass nothing (CMT-FR-34).
   */
  error?: string;
  /** Overrides the accessible name of the text field. */
  ariaLabel?: string;
  /** Overrides the placeholder. A reply names who an untagged message reaches. */
  placeholder?: string;
  /** Show the hint of the post accelerator beside the field (DDS-FR-HQTX). */
  showAcceleratorHint?: boolean;
  /**
   * Raised by an owner to put the caret in the field. A counter, because two
   * requests must both move focus (ACT-FR-QWNP).
   */
  focusSignal?: number;
  /** CMT-FR-32: dismisses the mention picker when another surface opens. */
  dismissSignal?: number;
  /** CMT-FR-32: called when the Attach menu opens, to close the overflow menu. */
  onAttachOpen?: () => void;
  /** CMT-FR-32: advances when the overflow menu opens, to close the Attach menu. */
  attachDismissSignal?: number;
  /**
   * CVP-FR-55: render the discard control even when the field is empty. For the
   * draft card that opens a new fragment-targeted discussion, where it abandons
   * the draft itself (CMT-FR-07).
   */
  alwaysShowCancel?: boolean;
  /** Called when the author discards. The text and the attachments are cleared first. */
  onCancel?: () => void;
  /** The discussion is locked. A locked discussion carries no composer. */
  locked?: boolean;
}

export type DiscussionComposerProps = ComposerCommon &
  (
    | {
        mode: "reply";
        discussion: Discussion;
        onReply: (
          discussionId: string,
          body: string,
          quotes: CommentQuote[],
          attachments: AttachmentInput[],
        ) => Promise<unknown>;
      }
    | {
        mode: "opening";
        target: DiscussionTarget;
        fragmentTarget?: FragmentTarget | null;
        onOpen: (request: OpenDiscussionRequest) => Promise<Discussion>;
        /** Called with the discussion the backend returned, after the field is cleared. */
        onOpened?: (discussion: Discussion) => void;
      }
  );

function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}

export function DiscussionComposer(props: DiscussionComposerProps) {
  const participantLabel = useParticipantLabel();
  const { agents } = props;
  const sessionKey =
    props.mode === "reply" ? props.discussion.id : discussionTargetKey(props.target);
  const session = useDiscussionSession(sessionKey);
  const body = session.body;
  const quotes = session.quotes;
  const disabled = props.disabled ?? false;
  const blocked = props.blocked ?? false;
  const [posting, setPosting] = useState(false);
  /** CVP-FR-40: guards the accelerator against a second post mid-flight. */
  const postingRef = useRef(false);
  const rootRef = useRef<HTMLDivElement>(null);

  const attachments = useAttachmentDraft({
    pending: session.attachments,
    onChange: (next) => setComposerAttachments(sessionKey, next),
  });

  // AGT-FR-29 / AGT-FR-38: both lists, so a live `@all` is bold while one in a
  // project where nobody can answer is left as prose.
  const roster = useMemo(() => agentRoster([...agents]), [agents]);
  const thread = props.mode === "reply" ? props.discussion : null;
  /**
   * CTA-FR-IGBO: who a reply carrying no tag of its own reaches. Derived from the
   * comments and the roster rather than held anywhere (CVP-FR-33).
   */
  const recipients = useMemo(
    () => (thread ? activeAgents(thread, roster) : []),
    [thread, roster],
  );
  const placeholder =
    props.placeholder ??
    (props.mode === "reply" ? composerPlaceholder(recipients) : "Ask an agent…");

  // ACT-FR-QWNP: put the caret where the author is about to write.
  const signal = props.focusSignal ?? 0;
  useEffect(() => {
    if (signal === 0) return;
    rootRef.current?.querySelector("textarea")?.focus();
  }, [signal]);

  const error = props.error ?? session.error;
  const canPost = !(disabled || blocked || props.locked) && body.trim() !== "";

  /**
   * CMT-FR-34 / CVP-FR-41: a refused post leaves the text, the quotes, and every
   * pending attachment exactly as they were, and the typed error renders inline.
   * The rejection is swallowed here because it is rendered here.
   */
  const post = async () => {
    if (!canPost || postingRef.current) return;
    postingRef.current = true;
    setPosting(true);
    let created: Discussion | undefined;
    try {
      logDebug(["frontend"], "a discussion composer posts", {
        mode: props.mode,
        key: sessionKey,
      });
      if (props.mode === "reply") {
        await props.onReply(props.discussion.id, body, [...quotes], attachments.inputs);
      } else {
        created = await props.onOpen({
          target: props.target,
          fragmentTarget: props.fragmentTarget ?? null,
          body,
          attachments: attachments.inputs,
        });
      }
    } catch (e) {
      // CMT-FR-51: the pending strip is intact, so the offending entry is
      // removed and the comment posted without retyping.
      setDiscussionError(sessionKey, errorText(e));
      logWarn(["frontend"], "a discussion composer post was refused", {
        mode: props.mode,
        key: sessionKey,
      });
      return;
    } finally {
      postingRef.current = false;
      setPosting(false);
    }
    // Only a post that landed clears the composer.
    clearComposer(sessionKey);
    setDiscussionError(sessionKey, undefined);
    if (props.mode === "opening" && created) props.onOpened?.(created);
  };

  /**
   * CVP-FR-40 / CMT-FR-70: Ctrl+Enter or Cmd+Enter posts and prevents the newline.
   * It acts only on a body that is not empty, never while a post is in flight,
   * while the composer is disabled or the discussion is locked, and never during
   * an IME composition. The mention picker keeps its own keys: the text area does
   * not offer this event to us while the picker is open.
   */
  const onKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (!isPostAccelerator(e)) return;
    if (!canPost || postingRef.current) return;
    e.preventDefault();
    void post();
  };

  const discard = () => {
    clearComposer(sessionKey);
    // CMT-FR-46: the strip goes with the typed body.
    props.onCancel?.();
  };

  const showDiscard =
    props.alwaysShowCancel === true ||
    body !== "" ||
    quotes.length > 0 ||
    attachments.pending.length > 0;

  const authorOf = (commentId: string): string => {
    const c = thread?.comments.find((x) => x.id === commentId);
    return c ? participantLabel(c.author) : "";
  };

  if (props.locked) return null;

  return (
    <div
      ref={rootRef}
      className="comment-card__reply"
      data-testid="discussion-composer"
      data-mode={props.mode}
      data-composer-key={sessionKey}
      onFocusCapture={(e) =>
        setFocusWasComposer(sessionKey, (e.target as Element).tagName === "TEXTAREA")
      }
    >
      {quotes.map((q, i) => (
        <div key={i} className="comment__quoted comment__quoted--pending">
          ❝ {authorOf(q.commentId)}: {q.excerpt}
          <button
            aria-label="Remove quote"
            title="Remove quote"
            onClick={() =>
              setComposerQuotes(
                sessionKey,
                quotes.filter((_, j) => j !== i),
              )
            }
          >
            <Icon.X size={11} />
          </button>
        </div>
      ))}
      <div
        className="comment-composer"
        // CMT-FR-45: a file dropped on the composer attaches without the Attach
        // control being opened.
        onDragOver={(e) => e.preventDefault()}
        onDrop={(e) => {
          const files = Array.from(e.dataTransfer?.files ?? []);
          if (files.length === 0) return;
          e.preventDefault();
          void attachments.addFiles(files);
        }}
      >
        {/* CVP-FR-55: one field. Attach leads, the message is between, Post
            trails, all on one line. */}
        <div className="comment-composer__row">
          <AttachControl
            disabled={disabled || blocked}
            onFiles={(files) => void attachments.addFiles([...files])}
            onLink={attachments.addLink}
            onOpen={props.onAttachOpen}
            dismissSignal={props.attachDismissSignal}
            align="leading"
          />
          <MentionComposer
            className="comment-card__composer"
            // CTA-FR-EMQS: the accessible name does not carry the recipients.
            ariaLabel={
              props.ariaLabel ??
              (props.mode === "reply"
                ? `Reply to thread ${props.discussion.id}`
                : "Start a discussion")
            }
            placeholder={placeholder}
            value={body}
            disabled={disabled || blocked}
            agents={agents as ProjectAgent[]}
            onChange={(next) => setComposerBody(sessionKey, next)}
            onKeyDown={onKeyDown}
            // CVP-FR-56: one line to start, five at most.
            autoGrow
            onPaste={(e) => {
              // CMT-FR-45: pasted image data attaches; a paste carrying no file
              // is ordinary text and reaches the field untouched.
              const files = Array.from(e.clipboardData?.files ?? []);
              if (files.length === 0) return;
              e.preventDefault();
              void attachments.addFiles(files);
            }}
            dismissSignal={props.dismissSignal}
          />
          {props.showAcceleratorHint && (
            <span className="kbd dds-reply__kbd" aria-hidden="true">
              {postAcceleratorHint()}
            </span>
          )}
          {/* CMT-FR-11: rendered only while there is something to discard. */}
          {showDiscard && (
            <button
              className="btn btn--ghost btn--icon-sm comment-composer__control"
              aria-label="Cancel"
              title="Cancel"
              onClick={discard}
            >
              <Icon.X size={13} />
            </button>
          )}
          <button
            className="btn btn--ghost btn--icon-sm comment-composer__control comment-card__post"
            aria-label="Post"
            title="Post"
            disabled={!canPost || posting}
            onClick={() => void post()}
          >
            <Icon.Send size={13} />
          </button>
        </div>
        {/* CMT-FR-46: the pending strip sits inside the field. */}
        <PendingAttachmentStrip
          pending={attachments.pending}
          onRemove={attachments.removeAt}
        />
      </div>
      {/* CMT-FR-34: a failed operation attaches to the composer that triggered
          it and leaves the content intact. */}
      {error && (
        <div
          className="comment-card__error"
          role="alert"
          data-testid="discussion-composer-error"
        >
          {commentErrorMessage(error)}
        </div>
      )}
    </div>
  );
}
