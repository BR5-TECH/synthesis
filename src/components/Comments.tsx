/**
 * The Comments vertical panel (`specifications/ui/CMP-comments-panel.md`).
 *
 * A project-wide index of every comment thread (CMP-FR-02), grouped by the state
 * that decides whether it still needs someone — Active, Locked, Resolved
 * (CMP-FR-03) — with each thread in exactly one group (CMP-FR-04). A row carries
 * enough to recognise a conversation without reading it: the owner, whether the
 * discussion is about a fragment or the whole owner, the quoted passage, and
 * the comment that opened it (CMP-FR-06). Activating one calls the one route
 * `revealDiscussion` (CMP-FR-10 / CMP-FR-11).
 *
 * The panel is the index and never a conversation surface (CMP-FR-16): every
 * write to a discussion happens in the surface that shows it. Everything this
 * file decides is presentation — grouping, ordering, filtering, and the call of
 * the route.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import * as api from "../api";
import type { CommentsPanel } from "../hooks/useCommentsPanelState";
import { onDiscussionChanged } from "../events";
import {
  type Discussion,
  type DiscussionListItem,
  discussionFragment,
  isFragmentTargeted,
  participantName,
  participantTitle,
} from "../types";
import { useProjectAgents } from "../state/agentRegistry";
import { pendingTurnsOf, useDiscussionSession } from "../state/discussionSession";
import { ownerLabelOf, useOwnerFacts } from "../state/ownerAvailability";
import type { DiscussionReveal } from "../state/revealDiscussion";
import { agentRoster } from "./agentTags";
import { CommentMarkdown } from "./CommentMarkdown";
import {
  attachmentCount,
  firstComment,
  groupThreads,
  replyCount,
} from "./commentGrouping";
import { TIMESTAMP_REFRESH_MS } from "./Notes";
import { Icon } from "./icons";
import {
  EMPTY_BODY_CLASS,
  FILTERED_BODY_CLASS,
  PanelEmptyState,
  PanelFilteredState,
} from "./PanelEmptyState";
import { UnresolvedMarker } from "./UnresolvedMarker";
import { formatRelative } from "./ProjectPicker";

interface CommentsProps {
  /** CMP-FR-15: the session-memory filter text, held above this component. */
  panel: CommentsPanel;
  /** CMP-FR-10: the one route that reveals a discussion. */
  onReveal: (reveal: DiscussionReveal) => void;
}

/**
 * CVP-FR-57: what the conversation is about, which follows its name. The
 * fragment quote where there is one, and the opening comment's first line
 * otherwise.
 */
export function discussionSubject(discussion: Discussion): string {
  const quote = discussionFragment(discussion)?.quote;
  if (quote) return quote;
  const first = (discussion.comments[0]?.body ?? "").trim().split("\n")[0] ?? "";
  if (first === "") return "Discussion";
  return first.length > 60 ? `${first.slice(0, 59)}…` : first;
}

/** What a whole-target row says in the quote's place (CMP-FR-27). */
function wholeNoun(discussion: Discussion): string {
  switch (discussion.target.kind) {
    case "artifact":
      return "file";
    case "draft":
      return "draft";
    case "note":
      return "note";
  }
}

/**
 * CMP-FR-28: the unread, pending-response, and failed-response marks of one
 * row, read from the discussion's session record rather than computed here. A
 * discussion nobody has read carries none of the three.
 */
function RowIndicators({ discussionId }: { discussionId: string }) {
  const session = useDiscussionSession(discussionId);
  const unread = session.unreadCount > 0;
  const pending = pendingTurnsOf(session.turns).length > 0;
  const failed = session.failedResponse;
  if (!unread && !pending && !failed) return null;
  return (
    <div className="comment-row__presentation">
      {unread && (
        <span className="comment-row__badge" data-kind="unread">
          Unread
        </span>
      )}
      {pending && (
        <span className="comment-row__badge" data-kind="pending">
          Waiting
        </span>
      )}
      {/* An agent could not answer here and the offer to ask it again is still
          standing. It says nothing about what failed and offers no retry:
          retrying is done where the conversation is read (CMP-FR-16). */}
      {failed && (
        <span className="comment-row__badge" data-kind="failed">
          No answer
        </span>
      )}
    </div>
  );
}

function errorText(e: unknown): string {
  return typeof e === "string" ? e : e instanceof Error ? e.message : String(e);
}

export function Comments({ panel, onReveal }: CommentsProps) {
  // CMP-FR-06: draft names and note labels arrive after the list, so a row
  // redraws when they do.
  useOwnerFacts();
  const [items, setItems] = useState<DiscussionListItem[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  /**
   * CMP-FR-18: every append this application performs updates the row it landed
   * in, from the payload rather than from a re-read.
   *
   * One subscription covers the lot — a comment posted in the Editor's rail, a
   * lock or a resolution set there, and an agent's answer landing in a thread
   * whose artifact is not even open. That last case is why this is subscribed
   * here rather than relayed from the rail: a rail that is not mounted has
   * nobody to tell.
   *
   * A thread the panel has not seen before joins the list from the same payload,
   * which is what makes a newly opened thread appear. Regrouping between Active,
   * Locked, and Resolved is `groupThreads`' job on the next render (CMP-FR-04),
   * so nothing here has to know which group a thread moved to.
   *
   * `ownerUnavailable` is preserved for a thread already in the list, because the
   * event carries the thread and not what the filesystem says about the artifact
   * it belongs to (CMP-FR-12) — only a read can answer that.
   */
  const [recoveries, setRecoveries] = useState(0);
  const failedRef = useRef(false);
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void onDiscussionChanged((thread) => {
      // A read that failed left no list to update in place, and this payload is
      // one thread rather than the project's. Admitting it would render a
      // one-row list and hide everything else, so the recovery from a failure is
      // the one case that reads again — and the event is the proof the backend
      // is answering, which is what makes it the right moment to try.
      if (failedRef.current) {
        failedRef.current = false;
        setRecoveries((n) => n + 1);
        return;
      }
      setItems((prev) => {
        if (prev === null) return prev;
        const index = prev.findIndex((item) => item.discussion.id === thread.id);
        if (index === -1) return [{ discussion: thread, ownerUnavailable: false }, ...prev];
        const next = [...prev];
        next[index] = { discussion: thread, ownerUnavailable: prev[index].ownerUnavailable };
        return next;
      });
    })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch(() => {});
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);
  // CMP non-functional: relative timestamps go stale on their own, so the
  // rendered rows are recomputed as time passes without the panel being
  // reopened — the same treatment the Notes panel gives NTS-FR-14.
  const [, setTick] = useState(0);
  useEffect(() => {
    const timer = setInterval(
      () => setTick((n) => n + 1),
      TIMESTAMP_REFRESH_MS,
    );
    return () => clearInterval(timer);
  }, []);

  const load = useCallback(() => {
    let cancelled = false;
    api
      .listAllDiscussions()
      .then((next) => {
        if (cancelled) return;
        setItems(next);
        setLoadError(null);
        failedRef.current = false;
      })
      .catch((e) => {
        if (cancelled) return;
        setItems([]);
        setLoadError(errorText(e));
        failedRef.current = true;
      });
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => load(), [load, recoveries]);

  /**
   * AGT-FR-29: the enrolment, so a tag that resolves to an enrolled agent reads
   * bold here exactly as it does in the rail — a reader scanning the project's
   * conversations can tell who each was addressed to without opening it.
   *
   * Read from the shared snapshot rather than fetched, because this panel is
   * allowed exactly one list call and invokes nothing else (CMP-FR-18 and the
   * panel's contract boundary). The chrome's agents control already reads the
   * enrolment on mount and on every registry change (AGT-FR-02) and publishes it
   * there, so the roster is current here without the panel asking for it — and
   * enrolling an agent bolds the tags naming it without the panel being reopened.
   */
  const agents = useProjectAgents();
  const roster = useMemo(() => agentRoster(agents), [agents]);
  const groups = groupThreads(items ?? [], panel.text);
  /**
   * CMP-FR-21 / SNV-FR-60: the project holds no thread at all. This is the
   * first-class empty state — the centred block, carrying no action, because
   * the panel is read-only (CMP-FR-16) and a thread is opened in the Editor's
   * rail rather than here.
   */
  const noThreadsAtAll = !loadError && items !== null && items.length === 0;
  /**
   * CMP-FR-24 / SNV-FR-61: the project holds threads and the filter admitted
   * none of them. A different state, rendering in the list's own region with
   * the filter field still present and still holding what was typed — the
   * author's next move is to change that text, not to go and open a thread.
   */
  const filteredToNothing =
    !loadError && items !== null && items.length > 0 && groups.length === 0;

  const renderRow = (item: DiscussionListItem) => {
    const opener = firstComment(item);
    const replies = replyCount(item);
    const attachments = attachmentCount(item);
    // CMP-FR-06: the owner's name — an artifact's path, a draft's name, or a
    // note's label.
    const path = ownerLabelOf(item.discussion.target);
    // CMP-FR-27: a whole-target discussion carries no fragment quote, there
    // being no passage it is pinned to.
    const whole = !isFragmentTargeted(item.discussion);
    const quote = discussionFragment(item.discussion)?.quote ?? "";

    const body = (
      <>
        <div className="comment-row__artifact">
          {/* CMP-FR-06 / CMP-FR-12: the project-relative path. Truncated from
              its head when the panel is too narrow (see `.comment-row__path`),
              so the file name stays visible and the folder is what gives. */}
          <span className="comment-row__path" title={path}>
            {path}
          </span>
          {item.discussion.locked && (
            // The marker is decorative beside the Locked group header that
            // already names the state, so it carries no second announcement.
            <Icon.Lock size={11} aria-hidden="true" />
          )}
          {/* CMP-FR-06: the target kind, as text and not as colour alone. */}
          <span
            className="comment-row__kind"
            data-kind={whole ? "whole" : "fragment"}
          >
            {whole ? "Whole" : "Fragment"}
          </span>
          {item.ownerUnavailable && (
            // CMP-FR-12: the marker names the ARTIFACT, not the conversation —
            // a row can carry it while sitting under Resolved, because a
            // thread's own settled-or-open state is what the groups carry and
            // the two are independent. Same word and same treatment the Notes
            // panel gives a note whose entity has gone (NTS-FR-23).
            <UnresolvedMarker />
          )}
        </div>
        {/* CMP-FR-06: the anchored passage, which is what identifies the thread
            far better than its opening line does. CMP-FR-27: a discussion states
            that it is one in that quote's place, so a row is never read as a
            thread whose anchor failed to load. */}
        {whole ? (
          <div className="comment-row__quote" data-discussion="true">
            <Icon.Comment size={10} aria-hidden="true" />
            <span>Discussion about the whole {wholeNoun(item.discussion)}</span>
          </div>
        ) : (
          <div className="comment-row__quote" title={quote}>
            <Icon.Quote size={10} aria-hidden="true" />
            <span>{quote}</span>
          </div>
        )}
        {opener && (
          <>
            <div className="comment-row__meta">
              {/* CMP-FR-07: rendered from the participant the backend
                  stamped, and an agent is marked as the rail marks one
                  (CMT-FR-10) — a human logged in as `claude` and an agent
                  handled `claude` must not read identically. */}
              <span
                className="comment-row__author"
                data-agent={opener.author.kind === "agent"}
              >
                {/* CMP-FR-07. As in the rail, the title line beneath indents
                    past this marker to align with the name — `kit.css`
                    `.comment-row__title::before` carries a hidden copy. */}
                {opener.author.kind === "agent" && "✦ "}
                {participantName(opener.author)}
              </span>
              {/* CMP-FR-08: the thread's last activity, absolute on hover. */}
              <time
                className="comment-row__time"
                dateTime={item.discussion.updatedAt}
                title={new Date(item.discussion.updatedAt).toLocaleString()}
              >
                {formatRelative(item.discussion.updatedAt)}
              </time>
            </div>
            {/* CMP-FR-30: the title the opening comment's participant carries,
                between the author line and the body, on exactly the terms the
                rail renders one (CTA-FR-YOGW) — so the same comment reads the
                same way in the panel and in the card. The snapshot and never
                the agent's current title: this panel holds no roster and
                resolves nothing. */}
            {participantTitle(opener.author) && (
              <div
                className="comment-row__title"
                data-testid="comment-row-agent-title"
              >
                {participantTitle(opener.author)}
              </div>
            )}
            {/* CMP-FR-06: the opening comment's Markdown, rendered as the rail
                renders it (CMT-FR-09). */}
            <div className="comment-row__body">
              <CommentMarkdown body={opener.body} agentRoster={roster} />
            </div>
          </>
        )}
        {/* CMP-FR-28: carried by words, and changing nothing else about the row. */}
        <RowIndicators discussionId={item.discussion.id} />
        {(replies > 0 || attachments > 0) && (
          <div className="comment-row__replies">
            {replies > 0 && (
              <span>{replies === 1 ? "1 reply" : `${replies} replies`}</span>
            )}
            {/* CMP-FR-25: how many attachments the opening comment carries, so
                a reviewer scanning the list can tell which conversations have a
                picture in them. CMP-FR-26: the count and nothing more — no
                thumbnail is loaded anywhere in this panel. */}
            {attachments > 0 && (
              <span
                className="comment-row__attachments"
                data-testid="comment-row-attachments"
              >
                <Icon.Paperclip size={11} />
                {attachments}
              </span>
            )}
          </div>
        )}
      </>
    );

    // CMP-FR-10 / CMP-FR-29: every row is an activation target. A row whose owner
    // no longer resolves opens the fallback conversation tab, so it is never a
    // plain block.
    return (
      <button
        type="button"
        className={
          item.ownerUnavailable ? "comment-row comment-row--missing" : "comment-row"
        }
        key={item.discussion.id}
        // The whole row is one activation target, so the accessible name has to
        // name the discussion rather than leaving a wall of body text as the label.
        aria-label={`${whole ? "Whole" : "Fragment"} discussion on ${path}: ${
          whole ? `the whole ${wholeNoun(item.discussion)}` : quote
        }`}
        onClick={() =>
          onReveal({
            discussionId: item.discussion.id,
            target: item.discussion.target,
            fragmentTarget: item.discussion.fragmentTarget,
            resolved: item.discussion.resolved,
            ownerLabel: path,
            subject: discussionSubject(item.discussion),
            ownerUnavailable: item.ownerUnavailable,
          })
        }
      >
        {body}
      </button>
    );
  };

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      <div className="panel-header">
        <span className="panel-header__title">Comments</span>
      </div>

      {/* CMP-FR-14: the filter is pinned above the scrollable list and stays
          visible while it scrolls. SNV-FR-60 takes it away with no thread in
          the project at all, because there is then nothing for it to narrow;
          the text itself is held on `panel` rather than in the field, so
          CMP-FR-15 survives the field going away and coming back. */}
      {!noThreadsAtAll && (
        <div className="panel-controls">
          <div className="search-input" style={{ height: 24 }}>
            <Icon.Search size={12} />
            <input
              placeholder="Filter comments…"
              aria-label="Filter comments"
              value={panel.text}
              onChange={(e) => panel.setText(e.target.value)}
            />
          </div>
        </div>
      )}

      <div
        className={
          noThreadsAtAll
            ? EMPTY_BODY_CLASS
            : filteredToNothing
              ? FILTERED_BODY_CLASS
              : "vpanel__body"
        }
      >
        {loadError && <div className="notes__message">{loadError}</div>}

        {/* CMP-FR-21 / SNV-FR-60: no action, rather than a disabled one — this
            panel writes nothing anywhere (CMP-FR-16), so the block names where
            a thread is opened instead of offering to open one. */}
        {noThreadsAtAll && (
          <PanelEmptyState line="No comments yet.">
            A comment thread is opened from the comment rail beside an artifact
            in the Editor, on a passage you select there.
          </PanelEmptyState>
        )}

        {/* CMP-FR-24 / SNV-FR-61: in the list's own region, filter still above. */}
        {filteredToNothing && (
          <PanelFilteredState>No comments match this filter.</PanelFilteredState>
        )}
        {groups.map((group) => (
          <div className="note-group" key={group.key}>
            {/* CMP-FR-05: name plus count, so the header is a flex row. The
                modifier scopes that to this panel — laying the shared class out
                as flex would reach the Notes panel's headers too and break the
                ellipsis truncation they rely on. */}
            <div className="note-group__header note-group__header--counted">
              <span>{group.label}</span>
              <span className="note-group__count">{group.items.length}</span>
            </div>
            {group.items.map(renderRow)}
          </div>
        ))}
      </div>
    </div>
  );
}
