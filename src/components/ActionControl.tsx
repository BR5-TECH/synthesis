/**
 * The one floating control every main-viewport tab carries in the bottom-trailing
 * corner of its surface (`ACT-action-control.md`).
 *
 * A tab shows exactly one thing — a draft, an artifact, a plain text file, a
 * Flow, a diff of a file — and everything that thing affords is gathered behind
 * one small mark of chrome rather than spread across a band of buttons, so a tab
 * being read or written in stays about what it shows until something is asked of
 * it (ACT-FR-01, ACT-FR-03).
 *
 * What it offers is decided by the **item** and not by the tab (ACT-FR-04):
 * Discuss on everything, and Graduate, Publish to GitHub, and Archive in
 * addition on a draft — those three contributed by the New Artifact tab, which
 * owns what they do (`NAW-new-artifact.md`).
 *
 * Where the discussions it opens are *read* is the one thing that differs by
 * host. A tab lending the comment rail a margin pins them at its head
 * (ACT-FR-19) and passes `discussionsInRail`; a tab with no such margin — a Flow,
 * a diff, an Editor in raw-text mode — reads them in the floating panel this
 * component renders (ACT-FR-20).
 */
import { useCallback, useEffect, useRef, useState } from "react";
import { Icon } from "./icons";
import { DiscussionComposer, DiscussionSurface } from "./discussion";
import type { UseDiscussionsResult } from "../hooks/useDiscussions";
import { identityBlockFor } from "../hooks/useComments";
import { logDebug } from "../logging";
import { focusDiscussion, requestPendingFocus } from "../state/discussionFocus";
import type { AttachmentInput, NodeType } from "../types";

/** ACT-FR-13: what a composer queues before it is posted. */
export interface ComposerAttachments {
  pending: readonly { input: AttachmentInput; name: string }[];
  inputs: AttachmentInput[];
  addFiles: (files: File[]) => Promise<void>;
  addLink: (url: string, label?: string) => void;
  removeAt: (index: number) => void;
  clear: () => void;
}

/** One entry a host contributes to the control's column (ACT-FR-04). */
export interface HostAction {
  label: string;
  onActivate: () => void;
  disabled?: boolean;
  title?: string;
}

export interface ActionControlProps {
  /**
   * ACT-FR-02: the discussions of the item this tab is bound to, and everything
   * needed to continue them. The host owns the hook, because a host with a rail
   * feeds the same result to it (ACT-FR-19).
   */
  discussions: UseDiscussionsResult;
  /**
   * Ignored. The shared composer keeps the body and the pending strip in the
   * discussion session store, keyed by the item's target (ACT-FR-14). A host
   * that still passes them can stop.
   */
  composer?: string;
  onComposerChange?: (text: string) => void;
  attachments?: ComposerAttachments;
  /**
   * ACT-FR-04: the item's resolved artifact type, which decides the noun the
   * control names the item by.
   */
  artifactType?: NodeType;
  /**
   * ACT-FR-04: a read-only tab — a Diff tab, a History detail tab — affords
   * Discuss alone whatever its item's type, because an action that changes an
   * item is taken where that item is being worked on and not from a reading of a
   * past state of it.
   */
  readOnly?: boolean;
  /**
   * ACT-FR-23: whether the item has material to discuss. Only a draft holding no
   * file has none — every other item always does, an empty file being a thing an
   * author may well want to ask about.
   */
  canDiscuss?: boolean;
  /**
   * ACT-FR-07: the item no longer resolves in the project. Every entry renders
   * disabled, there being nothing left to discuss or to hand over.
   */
  missing?: boolean;
  /** ACT-FR-04/05: the draft's own three entries, in their fixed slots. */
  graduate?: HostAction;
  /**
   * ACT-FR-05: **Publish to GitHub**, between Graduate and Archive.
   *
   * Its enablement and every disabled reason are the host's (NAW-FR-ZQMX,
   * NAW-FR-VBHT); this control renders what it is handed.
   */
  publish?: HostAction;
  archive?: HostAction;
  /**
   * ACT-FR-19: the host pins the discussions at its comment rail's head, so this
   * control renders neither the discussions control nor the floating panel.
   */
  discussionsInRail?: boolean;
  /**
   * ACT-FR-12: one of the host's own transient surfaces has opened — a comment
   * card's overflow menu — so whatever this control had open is dismissed.
   */
  dismissSignal?: number;
  /** ACT-FR-12: this control has opened one of its surfaces. */
  onSurfaceOpened?: () => void;
  /** What the composer's header names, e.g. "draft" or "specification". */
  itemNoun?: string;
  /**
   * The control's accessible name, which says what it acts on rather than what
   * it is: "Draft actions" in a New Artifact tab, "Actions" over a file. A
   * keyboard author reaching it hears the item, not the widget.
   */
  controlLabel?: string;
  /** CMT-FR-33: every operation is inert while a modal blocks the tab. */
  blocked?: boolean;
  /**
   * ACT-FR-09: one of this control's surfaces has opened or closed.
   *
   * What the host does with it is the host's business — the New Artifact tab
   * gives the page more to scroll while something lies over its trailing edge, so
   * whatever a surface covers comes out from under it.
   */
  onOverlayChange?: (open: boolean) => void;
  /**
   * ACT-FR-16: a discussion was just opened, so its card is focused where this
   * tab reads discussions.
   *
   * Reported rather than done here, because where that is differs by host: a tab
   * with a rail focuses the card at the rail's head (ACT-FR-19), and one without
   * focuses it in the panel this control opens (ACT-FR-20).
   */
  onDiscussionOpened?: (threadId: string) => void;
  /**
   * ACT-FR-QWNP: what **Discuss** does on a tab that already renders the
   * conversation.
   *
   * A New Artifact tab holds the draft's discussion in a column of its own, so
   * the control opens no composer: it moves focus to the column, where the
   * conversation the author is about to add to is already on screen. A tab with
   * no such column passes nothing and gets the control's own composer.
   */
  onDiscuss?: () => void;
  /**
   * ACT-FR-QWNP / ACT-FR-23: why **Discuss** cannot be taken at this moment,
   * though the item affords it.
   *
   * The one tab this applies to is a New Artifact tab whose discussion column
   * already stands: the conversation is on screen, so the action has nothing
   * left to do. The reason is the host's to give, because only the host knows
   * the state that produced it — the control renders it disabled and says it
   * on hover (ACT-FR-06).
   */
  discussUnavailable?: string;
  /**
   * CVP-FR-08: what the owner of the discussions in this panel is called and
   * which kind of surface renders them attached, for the chrome of a
   * presentation the author has moved out of this tab (ACT-FR-RWHV).
   */
  ownerLabel?: string;
  /** Ignored. A discussion has one surface and is never detached (CVP-FR-02). */
  ownerSurface?: string;
}

/** ACT-FR-13: what the composer's header calls the thing being discussed. */
function nounFor(artifactType: NodeType | undefined): string {
  if (artifactType === "spec") return "specification";
  if (artifactType === "flow") return "flow";
  return "file";
}

export function ActionControl({
  discussions,
  artifactType,
  readOnly = false,
  canDiscuss = true,
  missing = false,
  graduate,
  publish,
  archive,
  discussionsInRail = false,
  dismissSignal,
  onSurfaceOpened,
  itemNoun,
  controlLabel = "Actions",
  blocked = false,
  onOverlayChange,
  onDiscussionOpened,
  onDiscuss,
  discussUnavailable,
  ownerLabel,
}: ActionControlProps) {
  /**
   * ACT-FR-12: at most one of the control's expansion, the composer, and the
   * discussion panel is open at any moment. One piece of state rather than three
   * booleans is what makes that true by construction rather than by three
   * effects that have to agree.
   */
  const [floating, setFloating] = useState<
    "none" | "actions" | "composer" | "panel"
  >("none");
  const actionsOpen = floating === "actions";
  const composerOpen = floating === "composer";
  const panelOpen = floating === "panel";

  const [posting, setPosting] = useState(false);
  const [showResolved, setShowResolved] = useState(false);
  const [composerFocus, setComposerFocus] = useState(0);

  const toggleRef = useRef<HTMLButtonElement | null>(null);
  /** ACT-FR-13: where focus lands while the composer is disabled. */
  const blockedRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    onOverlayChange?.(floating !== "none");
  }, [floating, onOverlayChange]);

  /**
   * ACT-FR-13: where focus goes when the composer is dismissed. The composer
   * autofocuses its text area, so dismissing it without putting focus back would
   * drop it on the document body.
   */
  const restoreFocus = useCallback(() => {
    toggleRef.current?.focus();
  }, []);

  const openSurface = useCallback(
    (next: "none" | "actions" | "composer" | "panel") => {
      setFloating(next);
      if (next !== "none") {
        onSurfaceOpened?.();
      }
    },
    [onSurfaceOpened],
  );

  // ACT-FR-12: a host surface opened, so whatever this control had open goes.
  const firstDismiss = useRef(true);
  useEffect(() => {
    if (firstDismiss.current) {
      firstDismiss.current = false;
      return;
    }
    setFloating("none");
  }, [dismissSignal]);

  /**
   * ACT-FR-03 / ACT-FR-13: the control collapses, and the composer goes, on
   * Escape and on a pointer-down anywhere else in the tab alike — one surface
   * lying over the tab, dismissed the one way, rather than two that answer a
   * press differently.
   */
  useEffect(() => {
    if (floating === "none") return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      // AGT-FR-30 / ACT-FR-12: the mention picker is a transient of the composer
      // that owns it, so one Escape closes the picker and leaves the composer —
      // a nickname being completed must not cost the sentence it is in.
      //
      // Read in the CAPTURE phase, which is the whole point of registering
      // below: the picker closes from its own `document` listener in the BUBBLE
      // phase, so a bubble-phase check here runs after it and sees an empty DOM,
      // closing the composer along with it. Capture on `window` runs first.
      if (document.querySelector('[data-testid="mention-picker"]')) return;
      // Consumed here rather than left to bubble or to whatever else Escape
      // might mean at the window level (the native shell's own fullscreen
      // exit among them): this keystroke closed a surface, and nothing past
      // this point should also act on it.
      e.preventDefault();
      e.stopPropagation();
      if (floating === "composer" || floating === "panel") restoreFocus();
      setFloating("none");
    };
    // `mousedown` rather than `click`, so a press that begins outside dismisses
    // before it can activate anything — and so the click itself still reaches
    // the surface underneath. Bound on `window` rather than `document`, which is
    // where the shell's other dismissals are bound and what catches an event
    // dispatched at the window itself.
    const onDown = (e: MouseEvent) => {
      // The discussion panel stays: it is a conversation being read rather than
      // a menu being chosen from, and a press in the tab while reading one is
      // as likely to be scrolling what it is about as it is to be leaving it.
      if (floating !== "actions" && floating !== "composer") return;
      // ACT-FR-17: a post already in flight holds the composer until it lands,
      // because a refusal renders in the composer it was refused in and keeps
      // the body and the strip so the message is not retyped. Dismissing here
      // would unmount the one surface that failure has to render in, and the
      // message would go with it — a stray click must not cost a sent message
      // its error.
      if (floating === "composer" && posting) return;
      const el = e.target as Element | null;
      // The composer, its mention picker, and its attach menu all render inside
      // this control's own element, so one test covers every part of it.
      if (el?.closest?.("[data-action-control]")) return;
      // No `restoreFocus()` here, unlike Escape: the press is itself putting
      // focus somewhere the author chose, and pulling it back to the control
      // would take it off whatever they just reached for.
      setFloating("none");
    };
    window.addEventListener("keydown", onKey, true);
    window.addEventListener("mousedown", onDown);
    return () => {
      window.removeEventListener("keydown", onKey, true);
      window.removeEventListener("mousedown", onDown);
    };
  }, [floating, posting, restoreFocus]);

  // ACT-FR-17: the same reason the rail's composers state, and the same route.
  const identityBlock = identityBlockFor(discussions.identityError);

  useEffect(() => {
    if (composerOpen && identityBlock !== null) blockedRef.current?.focus();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [composerOpen]);

  /**
   * ACT-FR-16: the opening composer posts through the discussions hook, which
   * opens the discussion about the whole item and dispatches the turns. The
   * item is the whole target and never a fragment (CMS-FR-36).
   */
  const open = useCallback(
    async (request: { body: string; attachments: AttachmentInput[] }) => {
      setPosting(true);
      try {
        return await discussions.open(request.body, request.attachments);
      } finally {
        setPosting(false);
      }
    },
    [discussions],
  );

  /**
   * ACT-FR-16: the composer is gone once the discussion exists. The tab's owning
   * surface shows it and focus lands in its reply composer.
   */
  const opened = (created: { id: string }) => {
    logDebug(["frontend"], "an action control opened a discussion", {
      discussionId: created.id,
    });
    setFloating("none");
    requestPendingFocus(created.id, "composer");
    onDiscussionOpened?.(created.id);
    if (discussionsInRail) return;
    openSurface("panel");
    focusDiscussion(created.id, "composer");
  };

  /**
   * ACT-FR-04: the action set, decided by the item and by whether the tab can act
   * on it. ACT-FR-05: the fixed order, with whichever entry the item does not
   * afford simply absent rather than rendered inactive.
   */
  // ACT-FR-06/07: an action the item affords but cannot take renders disabled and
  // says why, so the shape of what the item affords stays legible.
  const discussEnabled =
    canDiscuss && !missing && !blocked && discussUnavailable === undefined;
  // ACT-FR-06: every state that disables the action states why, and the reasons
  // are ordered by which one the author has to act on. A file that is gone
  // outranks a modal that is merely in the way, and both outrank a host saying
  // the action has nothing left to do.
  const discussTitle = missing
    ? "This file no longer exists in the project"
    : blocked
      ? "Finish what is open over this tab first"
      : discussUnavailable !== undefined
        ? discussUnavailable
        : canDiscuss
          ? "Discuss this with the agents you address"
          : "There is nothing here to discuss yet";

  const noun = itemNoun ?? nounFor(artifactType);

  /** ACT-FR-20: the count the discussions control carries. */
  const unresolved = discussions.unresolved;
  const hasDiscussions = discussions.discussions.length > 0;
  const unresolvedList = discussions.discussions
    .map((entry) => entry.thread)
    .filter((d) => !d.resolved);
  const resolvedList = discussions.discussions
    .map((entry) => entry.thread)
    .filter((d) => d.resolved);
  const surfaceOf = (d: (typeof unresolvedList)[number]) => (
    <DiscussionSurface
      key={d.id}
      discussion={d}
      owner="panel"
      variant="card"
      agents={discussions.agents}
      identity={discussions.identity}
      identityBlock={identityBlock}
      disabled={identityBlock !== null}
      blocked={blocked}
      ownerLabel={ownerLabel}
      availability={missing ? "unavailable" : "available"}
      error={discussions.errors[d.id]}
      onReply={discussions.reply}
      onSetLock={discussions.setLock}
      onSetResolved={discussions.setResolved}
    />
  );

  return (
    <div className="action-control" data-action-control>
      {/* ACT-FR-20: the floating discussion panel, on a tab that lends the rail
          no margin. It is otherwise closed, because a conversation read alongside
          a graph or a diff is a thing to open rather than a column those surfaces
          give up width for permanently. */}
      {panelOpen && !discussionsInRail && (
        <div
          className="action-control__panel"
          role="dialog"
          aria-label="Discussion"
        >
          <div className="action-control__panel-head t-ui-xs">
            <span>Discussion</span>
            <button
              className="btn btn--ghost btn--icon btn--sm"
              aria-label="Close discussion"
              onClick={() => {
                restoreFocus();
                setFloating("none");
              }}
            >
              <Icon.X size={12} />
            </button>
          </div>
          {/* ACT-FR-21: each discussion is the shared surface, the one every
              owner renders. The resolved ones sit behind the disclosure that
              closes the panel. */}
          <div className="action-control__panel-body">
            {unresolvedList.map((d) => surfaceOf(d))}
            {resolvedList.length > 0 && (
              <div className="action-control__resolved">
                <button
                  className="comment-rail__disclosure"
                  aria-expanded={showResolved}
                  onClick={() => setShowResolved((open) => !open)}
                >
                  {showResolved ? "\u25be" : "\u25b8"} {resolvedList.length}{" "}
                  resolved {resolvedList.length === 1 ? "thread" : "threads"}
                </button>
                {showResolved && resolvedList.map((d) => surfaceOf(d))}
              </div>
            )}
          </div>
        </div>
      )}

      {/* ACT-FR-13: the floating composer is the shared DiscussionComposer in
          opening mode. It is closed rather than always present, and it carries
          no header: the surface's accessible name and the placeholder say what
          is being discussed. The item is the whole target, never a fragment. */}
      {composerOpen && discussions.target && (
        <div
          className="draft-composer"
          role="dialog"
          aria-label={`Discuss this ${noun}`}
        >
          {identityBlock && (
            <div
              ref={blockedRef}
              className="draft-composer__blocked t-ui-xs"
              role="status"
              tabIndex={-1}
            >
              {identityBlock.message}
              {identityBlock.route && <div>{identityBlock.route}</div>}
            </div>
          )}
          <DiscussionComposer
            mode="opening"
            target={discussions.target}
            fragmentTarget={null}
            agents={discussions.agents}
            disabled={identityBlock !== null}
            blocked={blocked}
            ariaLabel={`Discuss this ${noun}`}
            placeholder={`What should be done with this ${noun}?`}
            focusSignal={composerFocus}
            onOpen={open}
            onOpened={opened}
          />
        </div>
      )}

      {/* ACT-FR-03 / ACT-FR-05: expanded, the actions render as a column of
          labelled entries directly above the control — named rather than bare
          glyphs, because they are not a set the author can be expected to read
          from icons alone. */}
      {actionsOpen && (
        <div className="draft-actions__menu" role="menu">
          <button
            role="menuitem"
            className="draft-actions__item"
            disabled={!discussEnabled}
            title={discussTitle}
            onClick={() => {
              // ACT-FR-QWNP: a tab whose conversation is already rendered takes
              // focus there rather than opening a second place to write into it.
              if (onDiscuss) {
                setFloating("none");
                onDiscuss();
                return;
              }
              setComposerFocus((n) => n + 1);
              openSurface("composer");
            }}
          >
            <Icon.Comment size={13} /> Discuss
          </button>
          {/* ACT-FR-04: a read-only tab affords Discuss alone whatever its
              item's type, so the entries that change the item are dropped here
              rather than trusted to every host to withhold. */}
          {graduate && !readOnly && (
            <button
              role="menuitem"
              className="draft-actions__item"
              disabled={graduate.disabled || missing}
              title={graduate.title}
              onClick={() => {
                setFloating("none");
                graduate.onActivate();
              }}
            >
              <Icon.Graduate size={13} /> {graduate.label}
            </button>
          )}
          {publish && !readOnly && (
            <button
              role="menuitem"
              className="draft-actions__item"
              disabled={publish.disabled || missing}
              title={publish.title}
              onClick={() => {
                setFloating("none");
                publish.onActivate();
              }}
            >
              <Icon.Link size={13} /> {publish.label}
            </button>
          )}
          {archive && !readOnly && (
            <button
              role="menuitem"
              className="draft-actions__item"
              disabled={archive.disabled || missing}
              title={archive.title}
              onClick={() => {
                setFloating("none");
                archive.onActivate();
              }}
            >
              <Icon.Archive size={13} /> {archive.label}
            </button>
          )}
        </div>
      )}

      <div className="action-control__dock">
        {/* ACT-FR-20: the discussions control, rendered only while the item
            carries at least one discussion — so an item nobody has discussed
            carries one mark of chrome rather than two. */}
        {!discussionsInRail && hasDiscussions && (
          <button
            className="action-control__discussions"
            aria-label={
              panelOpen ? "Hide discussion" : `Show discussion (${unresolved})`
            }
            aria-expanded={panelOpen}
            data-open={panelOpen}
            onClick={() => {
              if (panelOpen) {
                setFloating("none");
                return;
              }
              openSurface("panel");
            }}
          >
            <Icon.Comment size={13} />
            <span className="t-ui-xs">{unresolved}</span>
          </button>
        )}

        {/* ACT-FR-03: collapsed it is a single graphical control and nothing
            else; expanded it turns itself into a dismissal. */}
        <button
          ref={toggleRef}
          className="draft-actions__toggle"
          aria-label={
            actionsOpen ? `Close ${controlLabel.toLowerCase()}` : controlLabel
          }
          aria-expanded={actionsOpen}
          aria-haspopup="menu"
          data-open={actionsOpen}
          onClick={() => {
            if (actionsOpen) {
              setFloating("none");
              return;
            }
            openSurface("actions");
          }}
        >
          {actionsOpen ? <Icon.X size={15} /> : <Icon.Diamond size={15} />}
        </button>
      </div>
    </div>
  );
}
