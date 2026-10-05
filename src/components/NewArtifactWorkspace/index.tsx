import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";

import * as api from "../../api";
import { isGithubShadow, participantName, undecidedCount } from "../../types";
import type {
  DraftGraduation,
  DraftHistoryChanged,
  DraftHistoryEntry,
  DraftHistoryList,
  DraftRecord,
  DraftStatus,
  DiscussionTarget,
  AttachmentInput,
} from "../../types";
import { useDraftDiscussionWiring } from "./useDraftDiscussionWiring";
import { DRAFT_HISTORY_CHANGED, onGraduationRunChanged } from "../../events";
import { listen } from "@tauri-apps/api/event";
import { Icon } from "../icons";
import { GraduationStart } from "../GraduationStart";
import {
  isTerminalRun,
  stateLabel,
} from "../../state/graduation";
import { Editor } from "../Editor";
import { DraftVersionReading } from "../DraftVersionReading";
import type { EditMode } from "../../state/editHistory";
import { AUTOSAVE_DELAY_MS } from "../../state/editSessions";
import { ActionControl } from "../ActionControl";
import {
  DiscussionColumn,
  RatioControl,
  Splitter,
  cycleRatio,
  gridTemplate,
  type SplitRatio,
} from "../DraftDiscussion";
import { DraftHistoryRail } from "./DraftHistoryRail";
import {
  GithubShadowBanner,
  GithubShadowTag,
  SHADOW_ARCHIVE_REASON,
  SHADOW_DISCUSS_REASON,
  SHADOW_PUBLISH_REASON,
} from "./GithubShadow";
import { useDraftReview } from "./useDraftReview";
import { useDraftPublication } from "../../hooks/useDraftPublication";
import {
  DraftPublicationBand,
  DraftPublicationTag,
  PublicationOverlays,
  publicationActionTitle,
} from "../DraftPublication";
import {
  imageRefusal,
  isRefusal,
  sourceLabelOf,
  standingOf,
  versionCountOf,
} from "./labels";

// Re-exported so the wording stays testable from where it always was, and so a
// reader who follows a citation to this tab still finds them.
export {
  imageRefusal,
  liveMarkerFor,
  sourceLabelOf,
  standingOf,
  versionCountOf,
} from "./labels";
import {
  draftDiscussionState,
  setDraftRatio,
  setDraftDiscussionHidden,
  setFocusedHunk,
  useDraftDiscussion,
} from "../../state/draftDiscussion";
import { useDiscussionControl } from "../../hooks/useDiscussionControl";
import { useCommentArrangement } from "../../hooks/useCommentArrangement";
import { identityBlockFor } from "../../hooks/useComments";
import {
  hunkPosition,
  openReview,
  undecidedHunks,
  useDraftProposals,
  useReviewing,
} from "../../state/draftProposals";
import { hunkCandidateBuffers } from "../../state/candidateBuffers";
import type { WorkspaceProps } from "./workspaceProps";
import { InconsistentDraft } from "./InconsistentDraft";

/**
 * New Artifact — the tab a draft is developed in
 * (`../../specifications/ui/NAW-new-artifact.md`).
 *
 * The tab is one **editing surface** beneath one **action row**, with the
 * draft's **History rail** shown or hidden as a leading column beside it and a
 * single **action control** floating in the bottom-trailing corner of the field
 * around the surface (NAW-FR-05, NAW-FR-27). The rail is hidden by default and
 * the editing surface has the whole width, because a draft is one prompt being
 * written and the width belongs to the writing (NAW-FR-08).
 *
 * The rail records **settled versions of the prompt and nothing else**
 * (NAW-FR-36): an accepted proposal puts one version there, the first of them
 * putting the prompt it superseded there as `Original` beside it, and the
 * author's own typing between them is simply the live prompt. A draft nobody has
 * proposed a change to therefore holds no version at all — the live prompt IS its
 * `Original`, which is what the rail's head row says (NAW-FR-07). It is the
 * draft's own history and is not the application-wide History of artifacts and
 * Git revisions (NAW-FR-43), which is `HVW-history-viewer.md`'s bottom panel and
 * shares neither storage nor surface with this.
 *
 * Every action a draft affords — Discuss, Graduate, Archive — is behind that
 * one control (NAW-FR-28), so a tab being written in carries one
 * small mark of chrome over its field rather than a row of buttons across its
 * head.
 *
 * The editing surface is not a lookalike: it is the Editor, mounted over a store
 * whose transport reads and writes the draft's prompt instead of an artifact
 * (NAW-FR-11), so the prompt gets the same two modes, the same toolbar, the same
 * frontmatter region, the same find panels and the same undo history an artifact
 * does — and none of it is implemented twice.
 *
 * Nothing here reaches the project until the draft is graduated (NAW-FR-20),
 * which is the only operation in this component that writes outside
 * `.synthesis/drafts/`.
 */

/**
 * NAW-FR-13: how long a draft's prompt rests after the last keystroke before it
 * is written — the same rest an artifact takes under EDT-FR-70, because the two
 * are the same promise made about two kinds of file.
 */
export { AUTOSAVE_DELAY_MS };

/**
 * NAW-FR-41 / DRS-FR-15: the typed refusal a draft whose storage is not the
 * single prompt DRS-FR-11 requires answers every operation with.
 *
 * Matched as a substring because a Tauri command's rejection arrives as the
 * error's own text rather than as a typed value.
 */
const ERR_NOT_SINGLE_FILE = "draft_not_single_file";
/** NAW-FR-41 / DHS-FR-21: a history that could not be reconciled. */
const ERR_HISTORY_RECOVERY_FAILED = "history_recovery_failed";

export function NewArtifactWorkspace({
  draftId,
  drafts,
  name,
  onDraftChanged,
  draftsRevision,
  onGraduationStarted,
  onOpenRun,
  onArchived,
  onNameChanged,
}: WorkspaceProps) {
  useSyncExternalStore(drafts.subscribe, drafts.getVersion);
  const session = drafts.ensure(draftId);
  const selected = session.selected;
  const railShown = session.railShown;

  const [record, setRecord] = useState<DraftRecord | null>(null);
  const [error, setError] = useState<string | null>(null);
  /**
   * NAW-FR-41 / DRS-FR-15: the draft's storage is not the single prompt the
   * invariant requires, so it does not open as a draft at all. Held apart from
   * `error` because it replaces the whole tab rather than annotating it.
   */
  const [inconsistent, setInconsistent] = useState(false);
  /**
   * NAW-FR-17: the start dialog is `GRV-graduation-review.md`'s, opened from
   * this tab's **Graduate** action and from nowhere else (GRU-FR-GLSO). This tab
   * decides only whether it is open.
   */
  const [starting, setStarting] = useState(false);
  /**
   * NAW-FR-44 / NAW-FR-BJQX: the graduation run this draft is bound to, read from
   * the queue rather than remembered here (DRS-FR-18). It is what makes the tab
   * read-only while a run holds the prompt, and read-only for good once the
   * draft is `graduated`.
   */
  const [graduation, setGraduation] = useState<DraftGraduation | null>(null);
  const [renaming, setRenaming] = useState(false);
  const [nameText, setNameText] = useState("");
  /**
   * NAW-FR-04: why the name the author typed could not be taken. Rendered
   * against the field it was typed into rather than in the tab's error banner,
   * and the field stays open with the text in it, because the correction is one
   * character away and a banner would send the author back to reopen the field.
   */
  const [nameError, setNameError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  /**
   * Bumped on every edit the Editor reports, so the autosave timer below is
   * restarted by the *last* keystroke of a burst rather than the first.
   */
  const [editTick, setEditTick] = useState(0);

  // -- the History rail (NAW-FR-07 … NAW-FR-10, NAW-FR-36 … NAW-FR-42) -----

  /**
   * NAW-FR-40: the rail's list, loaded without loading anything it holds.
   *
   * Fetched when the tab mounts rather than when the rail is shown, because the
   * toggle carries the version count whether or not the rail is open
   * (NAW-FR-08) — and `"list draft history"` reads no snapshot, so an unopened
   * rail still costs one directory walk and nothing more.
   */
  const [history, setHistory] = useState<DraftHistoryList | null>(null);
  /** NAW-FR-41: a history that could not be reconciled, with its retry. */
  const [historyError, setHistoryError] = useState<string | null>(null);
  /** NAW-FR-09: the version being read, or null for the live prompt. */
  const [viewing, setViewing] = useState<DraftHistoryEntry | null>(null);
  const [viewText, setViewText] = useState<string | null>(null);
  /** NAW-FR-41: a version whose text could not be loaded. */
  const [viewError, setViewError] = useState<string | null>(null);
  /**
   * NAW-FR-09: which of the two modes the reading is in, once the author has
   * chosen one.
   *
   * Null until they do, which is what lets a version open in whichever mode the
   * live prompt is in — a version read the way the prompt is being read. Held
   * apart from the live prompt's own mode rather than shared with it, because
   * returning restores the live prompt's editor state exactly as it was left
   * (NAW-FR-10) and a toggle made over a reading is not an edit to that state.
   */
  const [viewMode, setViewMode] = useState<EditMode | null>(null);

  /**
   * CMT-FR-52 … CMT-FR-62: the draft's discussions, which the comment margin
   * beside the page renders and continues. This tab only ever *begins* one
   * (NAW-FR-32, per `ACT-action-control.md` ACT-FR-16); every later message in it
   * is posted in its card.
   */
  const target = useMemo<DiscussionTarget>(
    () => ({ kind: "draft", draftId }),
    [draftId],
  );
  // NAW-FR-15: the draft's composer lives in the draft's session, which is
  // cleared with the project and the worktree exactly as the rest of its session
  // state is — so the strip and the half-written message survive the tab and go
  // when the draft's other state does (ACT-FR-14).
  const composerStore = useMemo(
    () => ({
      body: session.composer,
      attachments: session.composerAttachments,
      setBody: (text: string) => drafts.setComposer(draftId, text),
      setAttachments: (pending: { input: AttachmentInput; name: string }[]) =>
        drafts.setComposerAttachments(draftId, pending),
      current: () => drafts.ensure(draftId).composerAttachments,
    }),
    [session.composer, session.composerAttachments, drafts, draftId],
  );
  const control = useDiscussionControl(target, true, composerStore);
  const discussions = control.discussions;
  /** CMT-FR-28: which discussion card is focused (NAW-FR-32). */
  /**
   * NAW-FR-30: the control's own surfaces are dismissed by whatever else this
   * tab opens over them.
   *
   * One counter rather than the pair this held while the conversation was in a
   * margin: with the discussion a column of its own there is no second family
   * of floating card menus for the control to be mutually exclusive with.
   */
  const [dismissControl] = useState(0);
  /**
   * NAW-FR-27: whether one of the control's surfaces is open, which is what gives
   * the page more to scroll while something lies over its trailing edge. Reported
   * by the control rather than held with it, the tab being what owns the field.
   */
  const [overlayOpen, setOverlayOpen] = useState(false);
  const identityBlock = identityBlockFor(discussions.identityError);

  /**
   * CMT-FR-64: the field the page and the margin share, measured so the cards
   * know which arrangement they are in. The History rail is inside the tab and
   * outside this element, so showing it narrows what is measured here — which is
   * right, because it narrows what the cards actually have.
   */
  const draftEditorRef = useRef<HTMLDivElement | null>(null);
  const arrangement = useCommentArrangement(draftEditorRef);

  /**
   * DDS-FR-PNXR: how this draft's tab is split, and the element a drag is
   * measured against.
   *
   * The ratio is held per draft in a module store rather than here, because it
   * must survive this tab being backgrounded and returned to — which is exactly
   * what component state does not do.
   */
  /**
   * NAW-FR-14 / DDS-FR-KPSZ: the threads anchored to a passage of the prompt.
   *
   * They render below the page on CMT-FR-64's arrangement rather than in a
   * margin beside it. This tab does not serve them yet — `showComments` is off
   * on the embedded Editor for the reason NAW-FR-14 records — so the list is
   * empty and the field opens no margin.
   */
  const anchoredThreads: readonly unknown[] = [];

  const splitRef = useRef<HTMLDivElement | null>(null);
  const view = useDraftDiscussion(draftId);
  const setRatio = useCallback(
    (ratio: SplitRatio) => setDraftRatio(draftId, ratio),
    [draftId],
  );
  // DDS-FR-XQMF: hiding the discussion column is not narrowing it, so it is its
  // own control rather than a fourth preset. The splitter and the presets act
  // on two columns that both stand.
  const toggleDiscussion = useCallback(
    () => setDraftDiscussionHidden(draftId, !draftDiscussionState(draftId).hidden),
    [draftId],
  );

  // DDS-FR-PNXR: one accelerator cycles the three presets. Unclaimed elsewhere
  // in the keymap, and deliberately not bound to a modifier the editing surface
  // uses — the caret is in the prose most of the time this tab is open.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key !== "\\" || !(e.metaKey || e.ctrlKey) || e.altKey) return;
      // DDS-FR-XQMF: there is no split to cycle while one column is hidden, and
      // a keypress that silently rearranged what the author cannot see would
      // land them on another split the next time they show the column.
      if (draftDiscussionState(draftId).hidden) return;
      e.preventDefault();
      setDraftRatio(draftId, cycleRatio(draftDiscussionState(draftId).ratio));
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [draftId]);


  /**
   * NAW-FR-35 / DCP-FR-04: the draft's undecided proposal, if it has one.
   *
   * Read from the store rather than fetched here, so the marker follows a
   * decision made in another window — or a proposal recorded while this tab was
   * closed — without this tab polling for either (DCR-FR-03).
   */
  const proposals = useDraftProposals(draftId);
  const pendingProposal = proposals.find((p) => p.state === "pending");

  /**
   * DDS-FR-SVBL: what the anchor bar names while the author is reading back.
   *
   * The proposal and the change under review — the one thing the author left
   * the tail to look at, and the one thing scrolling has taken off screen.
   */
  const anchorBar = useMemo(() => {
    if (pendingProposal === undefined) return null;
    const undecided = undecidedHunks(pendingProposal);
    const at = view.focusedHunkId
      ? hunkPosition(pendingProposal, view.focusedHunkId)
      : hunkPosition(pendingProposal, undecided[0]?.id ?? "");
    const changes = pendingProposal.hunkCount;
    return {
      label: `${participantName(pendingProposal.agent)} · proposal of ${changes} ${
        changes === 1 ? "change" : "changes"
      }`,
      action: at > 0 ? `Back to change ${at}` : "Back to the proposal",
      onActivate: () => {
        const hunkId = view.focusedHunkId ?? undecided[0]?.id ?? null;
        if (hunkId !== null) setFocusedHunk(draftId, hunkId);
      },
    };
  }, [pendingProposal, view.focusedHunkId, draftId]);
  /**
   * NAW-FR-10 / NAW-FR-30: whether the review modal is standing over the tab.
   *
   * The reading is the OUTERMOST of this tab's states, so its Escape has to
   * yield to every surface above it — the review modal and the destination
   * chooser (both modals over the tab) and whatever the action control has
   * open. Each of those binds its own dismissal on the window, so without this
   * one keypress would back out of the modal AND cost the author the reading
   * underneath it, which is two dismissals for one Escape.
   */
  const reviewing = useReviewing() !== null;

  const docKey = selected === null ? null : drafts.key(draftId, selected);
  const dirty = docKey !== null && (drafts.docs.get(docKey)?.dirty ?? false);
  const {
    focusedThreadId,
    setFocusedThreadId,
    discussFocus,
    setDiscussFocus,
    openDiscussion,
    draftDiscussions,
    fragmentsGroup,
  } = useDraftDiscussionWiring({
    draftId,
    discussions,
    path: selected,
    source: docKey !== null ? (drafts.docs.get(docKey)?.buffer ?? "") : "",
  });
  /**
   * NAW-FR-09 / EDT-FR-17: the mode the reading is in — the author's own choice
   * where they have made one, and otherwise the mode the live prompt is in, so a
   * version opens as the prompt is currently being read rather than in a mode the
   * author put aside.
   */
  const readingMode: EditMode =
    viewMode ??
    (docKey !== null ? drafts.docs.get(docKey)?.mode : undefined) ??
    "wysiwyg";

  /**
   * Which record read is the current one: two reads can be in the air at once,
   * and the later-issued one is the answer even when it lands first.
   */
  const recordGeneration = useRef(0);

  /**
   * Take a record as the one in force: hold it for rendering, and tell the
   * shell's store where the prompt now sits, so a reconciliation made while this
   * tab is unmounted has something current to compare against (NAW-FR-25).
   */
  const appliedPrompt = useRef<string | null>(null);
  const applyRecord = useCallback(
    (next: DraftRecord) => {
      recordGeneration.current += 1;
      const prompt = next.promptPath ?? null;
      appliedPrompt.current = prompt;
      setRecord(next);
      setInconsistent(false);
      drafts.setPromptPath(draftId, prompt);
      // NAW-FR-06: a draft is one prompt, so there is nothing to choose — the
      // selection *is* the record's `promptPath`, and a tab reopened after a
      // rename opens on the file the rename produced.
      drafts.select(draftId, prompt);
    },
    [drafts, draftId],
  );

  useEffect(() => {
    let cancelled = false;
    void api
      .openDraft(draftId)
      .then((r) => {
        if (cancelled) return;
        // The record carries the status and the destination root; its `name` is
        // deliberately not read back out of it — `name` is the prop above.
        applyRecord(r);
      })
      .catch((e) => {
        if (cancelled) return;
        // NAW-FR-41 / DRS-FR-15: an inconsistent draft does not open as a draft
        // at all. It is a state of the tab rather than an error banner over an
        // editing surface that must not be rendered.
        if (isRefusal(e, ERR_NOT_SINGLE_FILE)) setInconsistent(true);
        else setError(String(e));
      });
    return () => {
      cancelled = true;
    };
  }, [draftId, applyRecord]);

  // The rename field opens seeded with the name currently in force, whichever
  // surface last set it.
  useEffect(() => setNameText(name), [name]);

  /**
   * NAW-FR-10: the way back to the live prompt is always at hand while a version
   * is showing — the rail's live-prompt row, the strip's own control, Escape
   * from anywhere in the reading, and hiding the rail.
   */
  /**
   * NAW-FR-10 / non-functional: the rail's live-prompt row, so a return made
   * from inside the reading puts the author back on it rather than dropping
   * focus to the document — where the next Tab restarts at the top of the tab
   * and a screen-reader user loses their place.
   */
  const liveRowRef = useRef<HTMLButtonElement | null>(null);

  const backToLive = useCallback(() => {
    setViewing(null);
    setViewText(null);
    setViewError(null);
    // NAW-FR-09: a reading is seeded from the live prompt's own mode, so the
    // author's choice holds for as long as they stay in the reading and ends
    // with it. Kept across versions selected one after another — that is one
    // reading — and dropped here, so the next one opens as the prompt is being
    // read rather than as some earlier reading was left.
    setViewMode(null);
  }, []);

  /**
   * The return as the author makes it — from the strip, from Escape, or from a
   * failed reading — which lands focus on the row that says what the prompt now
   * is. Distinct from [`backToLive`] itself, which is also what an acceptance
   * landing elsewhere calls (NAW-FR-42): that one must not move focus, the
   * author not having asked for anything.
   */
  const returnToLive = useCallback(() => {
    backToLive();
    // After the reading has gone, so the row is the thing being focused rather
    // than a control inside a surface that is about to unmount.
    requestAnimationFrame(() => liveRowRef.current?.focus());

  }, [backToLive]);

  /**
   * NAW-FR-40: one `"list draft history"` per tab, and one more for each
   * `"draft history changed"` that could not be applied from its payload.
   *
   * A failure to reconcile is rendered rather than swallowed (NAW-FR-41): the
   * prompt is presented as neither current nor stale until it resolves, which is
   * why `history` is left null rather than being given an empty list.
   */
  const loadHistory = useCallback(async () => {
    try {
      const list = await api.listDraftHistory(draftId);
      setHistory(list);
      setHistoryError(null);
    } catch (e) {
      setHistory(null);
      setHistoryError(
        isRefusal(e, ERR_HISTORY_RECOVERY_FAILED)
          ? "This draft's history could not be recovered, so the prompt cannot be shown as current or stale yet."
          : String(e),
      );
    }
  }, [draftId]);

  useEffect(() => {
    // Only once the record has landed: a draft that does not open as a draft at
    // all (NAW-FR-41) has no history to list, and asking for one would put a
    // second refusal in front of the author for the same fact.
    if (record === null) return;
    void loadHistory();
  }, [loadHistory, record]);

  /**
   * NAW-FR-40: the rail follows `"draft history changed"` while the tab is open,
   * so a version an acceptance produced appears without the rail re-listing.
   *
   * The event carries the entry itself, but the *live prompt's* digest moves
   * with it — and that is what the modified marker is computed from (NAW-FR-37)
   * — so the list is re-read rather than patched. It is one directory walk and
   * no snapshot (DHS-FR-10), which is what makes re-reading the cheaper answer
   * than holding a second copy that could disagree.
   */
  useEffect(() => {
    const unlisten = listen<DraftHistoryChanged>(DRAFT_HISTORY_CHANGED, (event) => {
      if (event.payload?.draftId !== draftId) return;
      // NAW-FR-42: a version an acceptance produced puts the tab back on the
      // live prompt if one was being read, so the author sees what landed
      // rather than the reading they had open. A reconciliation that *withdrew*
      // an entry (a null payload) does the same, that entry no longer being a
      // version of the prompt at all (DHS-FR-22).
      backToLive();
      void loadHistory();
    });
    return () => {
      void unlisten.then((off) => off());
    };
  }, [draftId, loadHistory, backToLive]);

  /**
   * NAW-FR-25: a rename made in the Drafts panel moves this draft's prompt, and
   * this tab has to follow it.
   *
   * Without this the action row goes on naming the old file and — far worse —
   * the pending auto-save goes on writing through the old path, which
   * `write_text_atomic` recreates: the draft would end up holding two files
   * (DRS-FR-11's own refusal), with the author's typing in the one nothing
   * points at.
   *
   * The shell performs the same reconciliation for a draft whose tab is *not*
   * mounted (`useShellSession`); this one is what redraws the tab in front of
   * the author.
   */
  const seenRevision = useRef<number | null>(null);
  useEffect(() => {
    // The mount effect above already read the record at whatever revision the
    // tab opened on; this exists for the ones after it.
    if (seenRevision.current === null) {
      seenRevision.current = draftsRevision;
      return;
    }
    seenRevision.current = draftsRevision;
    const generation = ++recordGeneration.current;
    void api
      .openDraft(draftId)
      .then((fresh) => {
        // A record applied while this read was in flight is the newer answer,
        // and this one is a snapshot from before it. Without this guard a read
        // issued just ahead of a rename can land after it and rekey the session
        // *backwards*, onto the file the rename moved away from.
        if (generation !== recordGeneration.current) return;
        // `followPrompt` carries the buffer, the undo history and the selection
        // in one move. It is idempotent, and the shell may already have run it
        // for this rename.
        drafts.followPrompt(draftId, fresh.promptPath ?? null);
        applyRecord(fresh);
      })
      .catch(() => {
        // A draft that has just been deleted or graduated elsewhere reads as
        // "not found"; the tab is closing behind this, so it is not an error to
        // put in front of the author.
      });
    // Deliberately not keyed on `record`: this reconciles *to* the record, and
    // depending on it would re-read on its own result, without end.
  }, [draftsRevision, draftId, drafts, applyRecord]);

  /**
   * NAW-FR-13: write the pending buffer through the draft's session store, which
   * always writes the file that buffer belongs to. Reports whether the caller may
   * proceed — a failed write leaves the buffer dirty and in memory, so the
   * content is still in front of the author to retry.
   */
  // -- images in the prompt (NAW-FR-50 … NAW-FR-56) -------------------------

  /**
   * NAW-FR-54: what the tab last announced about an image, read by a live
   * region so a screen-reader user learns that the image went in, that it was
   * refused and why, or that the reference is gone.
   *
   * One region rather than one per outcome: the three are the same kind of
   * thing to a reader, and a region per outcome would announce an empty string
   * on every re-render of the two that did not change.
   */
  const [imageStatus, setImageStatus] = useState("");
  /**
   * NAW-FR-51: the typed refusal of the last insertion, rendered inline where
   * the author is looking. Cleared by the next insertion, so a refusal that has
   * been answered does not stand over a picture that went in.
   */
  const [imageError, setImageError] = useState<string | null>(null);
  /**
   * NAW-FR-51: the asset an insertion has stored whose reference has not
   * reached the saved prompt yet.
   *
   * Held so the write that carries it can be made **immediately** rather than
   * after the ordinary rest, and so a write that fails can take the asset back
   * with `"discard draft image"` — a failed insertion leaves neither a
   * reference to a missing image nor an image nothing references.
   */
  const pendingInsertRef = useRef<string | null>(null);

  const announceImage = useCallback((message: string) => {
    setImageStatus(message);
  }, []);

  const saveCurrent = useCallback(async () => {
    const res = await drafts.flush(draftId);
    if (!res.ok) {
      setError("Could not write this draft's prompt. Your changes are still here.");
      return false;
    }
    setSaved(true);
    onDraftChanged();
    // NAW-FR-37: the marker is computed from the bytes the save path persisted
    // rather than from the buffer, so it moves when the write lands and not
    // before — and the dirty indicator and this marker never contradict.
    void loadHistory();
    return true;
  }, [drafts, draftId, onDraftChanged, loadHistory]);

  /**
   * NAW-FR-50 / NAW-FR-51: the tab's side of the editing surface's image offer.
   *
   * The tab takes the image data the surface offers it, stores it under the
   * draft's own `assets/` folder, and answers with the Markdown destination to
   * insert. What the surface then does with that answer is its own — one edit,
   * one undo step, one predictable selection (EDT-FR-86).
   *
   * A store that is **refused** answers with nothing at all, so no reference is
   * inserted, the prompt is byte-for-byte what it was, and no undo step is
   * taken. The refusals divide in what they tell the author: the three image
   * refusals say the picture was not acceptable, the lock says the draft takes
   * no write while its run holds it, and `asset_store_failed` says the store did
   * not happen and may simply be tried again.
   */
  const acceptImage = useCallback(
    async (image: { mediaType: string; filename: string | null; data: string }) => {
      setImageError(null);
      let asset;
      try {
        asset = await api.storeDraftImage(
          draftId,
          image.mediaType,
          image.filename,
          image.data,
        );
      } catch (e) {
        const message = imageRefusal(String(e));
        setImageError(message);
        announceImage(message);
        return null;
      }
      // NAW-FR-51: the prompt is written **immediately**, without waiting the
      // ordinary rest out, because the saved prompt is what protects the asset
      // from housekeeping (per `../../specifications/core/DAS-draft-assets.md`
      // DAS-FR-13). The write is scheduled for after the surface has applied
      // the insertion, which is why it is queued rather than awaited here.
      pendingInsertRef.current = asset.path;
      announceImage("Image inserted.");
      return `![](${asset.reference})`;
    },
    [draftId, announceImage],
  );

  /**
   * NAW-FR-52: resolve one of the prompt's image destinations to drawable
   * content, fetched when the image is actually shown rather than when the
   * prompt loads.
   *
   * A destination that resolves to no draft-owned asset — a missing file, an
   * unresolved reference-style label, an external address, a path leaving the
   * draft — answers `null`, and the surface renders its non-blocking
   * placeholder. Nothing here fetches an address.
   */
  const resolveImage = useCallback(
    async (destination: string) => {
      try {
        const content = await api.readDraftImage(draftId, destination);
        return `data:${content.mediaType};base64,${content.data}`;
      } catch {
        return null;
      }
    },
    [draftId],
  );

  const imageHost = useMemo(
    () => ({ resolve: resolveImage, accept: acceptImage }),
    [resolveImage, acceptImage],
  );

  /**
   * DCR-FR-13: write the pending buffer before an acceptance lands on the prompt.
   *
   * The autosave timer (below) can still be counting down when the author
   * accepts. Without this the accepted text is written first and the debounce
   * then fires and puts the author's stale buffer straight back over it — the
   * change silently undone, with no dialog and no error. Flushing here closes
   * that window: the buffer goes first, the acceptance overwrites it, and the
   * timer has nothing left to write.
   */
  const flushBeforeApply = useCallback(
    async (path: string) => {
      if (!dirty || selected !== path) return;
      await saveCurrent();
    },
    [dirty, selected, saveCurrent],
  );

  /**
   * NAW-FR-42 / DCR-FR-13: read the prompt again after an acceptance rewrote
   * it, so the document column is showing the accepted text the moment the
   * decision lands — and return to the live prompt if a version was being read,
   * so the author sees what landed rather than the reading they had open.
   *
   * Nothing watches a draft's prompt for external change — a draft is written
   * only by this application — so the retained session would keep serving the
   * buffer it loaded before the acceptance, and the tab would go on showing the
   * old text through closing and reopening, the retained edit state outliving
   * both (NAW-FR-12).
   */
  const reloadAfterApply = useCallback(
    (path: string) => {
      setViewing(null);
      setViewText(null);
      setViewError(null);
      void drafts.docs.reload(drafts.key(draftId, path));
    },
    [drafts, draftId],
  );

  /**
   * NAW-FR-13: the prompt writes itself. There is no Save control — a draft is
   * working material, and asking the author to remember to keep it is the one
   * way it gets lost. The timer restarts on every keystroke, so a burst of
   * typing is one write at the end of it, and the write that matters most —
   * the one before the tab closes — still happens through the flush that path
   * already performs.
   */
  useEffect(() => {
    if (!dirty) return;
    const timer = setTimeout(() => void saveCurrent(), AUTOSAVE_DELAY_MS);
    return () => clearTimeout(timer);
  }, [dirty, editTick, saveCurrent]);

  /**
   * NAW-FR-51: an insertion writes the prompt **immediately**, without waiting
   * the ordinary rest of NAW-FR-13 out, because the saved prompt is what
   * protects the asset from housekeeping.
   *
   * Where that write fails the asset is discarded, so the failed insertion
   * leaves neither a reference to a missing image nor an image nothing
   * references. A discard that is itself refused changes none of that: the
   * saved prompt still carries no reference, the author still reads the write's
   * own failure and nothing about the cleanup, and the orphaned asset is left
   * for the next housekeeping pass (NAW-FR-55).
   */
  useEffect(() => {
    const path = pendingInsertRef.current;
    if (!path || !dirty) return;
    pendingInsertRef.current = null;
    void (async () => {
      if (await saveCurrent()) return;
      announceImage("The prompt could not be written. The image was not inserted.");
      try {
        await api.discardDraftImage(draftId, path);
      } catch {
        // DAS-FR-28: left for the next pass, and never reported to the author —
        // the failure they are owed is the write's, which they already have.
      }
    })();
  }, [editTick, dirty, saveCurrent, draftId, announceImage]);

  /**
   * NAW-FR-55: the draft's asset housekeeping, triggered when the draft opens
   * and when it closes — the close-triggered call made before the tab releases
   * the draft.
   *
   * A **trigger and not a query**: the answer is not read, not waited on, and
   * nothing is rendered from it. Neither call blocks anything, and a pass that
   * fails changes nothing about the open or the close — it raises no dialog,
   * marks nothing, and is reported through the application's own diagnostics
   * rather than to the author.
   */
  useEffect(() => {
    void api.sweepDraftAssets(draftId).catch(() => {});
    return () => {
      void api.sweepDraftAssets(draftId).catch(() => {});
    };
  }, [draftId]);

  // The write report belongs to the document it was made about.
  useEffect(() => setSaved(false), [docKey]);

  useEffect(() => {
    if (!viewing) return;
    if (starting || overlayOpen || reviewing) return;
    // Nothing above the reading is open, so Escape is the reading's to take.
    // Bound only in that case rather than guarded inside the handler, so a
    // surface that opens while a version is showing takes the key back the
    // moment it opens and hands it over again the moment it closes.
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") returnToLive();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [viewing, returnToLive, starting, overlayOpen, reviewing]);

  /**
   * NAW-FR-09 / NAW-FR-40: read one version. `"load draft history entry"` is
   * invoked when a version is actually selected and at no other moment.
   *
   * It replaces no buffer: the live prompt's text, its dirty state, its editing
   * mode, its undo history and its find-panel state are all exactly as they were
   * left, and are restored the moment the author returns to it (NAW-FR-10).
   */
  const readVersion = useCallback(
    async (entry: DraftHistoryEntry) => {
      setViewing(entry);
      setViewText(null);
      setViewError(null);
      try {
        const loaded = await api.loadDraftHistoryEntry(entry.id);
        setViewText(loaded.content);
      } catch (e) {
        // NAW-FR-41: rendered where the author is looking, leaving the live
        // prompt and the editor untouched, with a retry and the return beside
        // it. The rail keeps its other rows and every one stays selectable.
        setViewError(
          isRefusal(e, "snapshot_corrupt")
            ? "Its stored snapshot did not match its checksum."
            : String(e),
        );
      }
    },
    [],
  );

  const toggleRail = useCallback(() => {
    // NAW-FR-10: hiding the rail returns to the live prompt, so the author
    // cannot be left reading a version with no rail to leave it by.
    if (railShown) backToLive();
    drafts.setRailShown(draftId, !railShown);
  }, [railShown, backToLive, drafts, draftId]);

  /** Back out of the chrome's rename, restoring the name in force. */
  const cancelName = () => {
    setRenaming(false);
    setNameError(null);
    setNameText(name);
  };

  const commitName = async () => {
    const next = nameText.trim();
    if (!next || next === name) {
      setRenaming(false);
      setNameError(null);
      setNameText(name);
      return;
    }
    // NAW-FR-25: renaming the draft renames the prompt, so a dirty buffer is
    // written under its current name first. A timer firing after the rename
    // would write through the old path and recreate the file just renamed away.
    const oldPrompt = record?.promptPath ?? null;
    if (oldPrompt !== null && dirty && !(await saveCurrent())) return;
    try {
      const updated = await api.renameDraft(draftId, next);
      setRenaming(false);
      setNameError(null);
      const newPrompt = updated.promptPath ?? null;
      // NAW-FR-25: the edit session under the old path is carried across so the
      // rename costs no buffer and no undo history. Applied before the record,
      // so the selection is never briefly a path nothing holds.
      if (oldPrompt !== null && newPrompt !== null && newPrompt !== oldPrompt) {
        drafts.renamePath(draftId, oldPrompt, newPrompt);
      }
      applyRecord(updated);
      onNameChanged(updated.name);
      onDraftChanged();
      // The versions already in the rail keep the path each was taken at
      // (DHS-FR-06); the live prompt's path is what moved.
      void loadHistory();
    } catch (e) {
      // Refused against this field, which stays open — nothing moved, on disk
      // or in the record.
      setNameError(String(e));
    }
  };

  /**
   * NAW-FR-28: **Archive** retires the draft and closes the tab; **Restore**
   * brings it back and leaves the tab open on the draft it just returned to
   * view. One operation, two directions.
   *
   * The unsaved prompt is written *first* and the status moved only if that
   * write landed: the tab closes behind an archive, and a close that discarded
   * the author's last paragraph because the write was skipped is the one way
   * this action can lose work (NAW-FR-16, NAW-FR-23).
   */
  const setStatus = async (status: DraftStatus) => {
    if (!record) return;
    if (dirty && !(await saveCurrent())) return;
    try {
      const updated = await api.setDraftStatus(draftId, status);
      applyRecord(updated);
      onDraftChanged();
      if (status === "archived") onArchived();
    } catch (e) {
      setError(String(e));
    }
  };

  /**
   * NAW-FR-44 / DRS-FR-18: the draft's graduation run, re-read whenever
   * anything about drafts or about a run changes. Never cached beyond that: the
   * lock is the *run's* state, and a tab holding its own copy would go on
   * offering to edit a prompt a run had taken.
   */
  const reloadGraduation = useCallback(async () => {
    try {
      const run = await api.getDraftGraduation(draftId);
      setGraduation(
        run
          ? {
              runId: run.id,
              state: run.state,
              locked: !isTerminalRun(run.state),
              graduated: run.commits.length > 0 && run.state !== "discarded",
            }
          : null,
      );
    } catch {
      // No project, or no queue yet. Neither is a lock, and neither is
      // something to report over an editing surface.
      setGraduation(null);
    }
  }, [draftId]);

  useEffect(() => {
    void reloadGraduation();
  }, [reloadGraduation, draftsRevision]);

  useEffect(() => {
    let cancelled = false;
    const subscription = onGraduationRunChanged(() => {
      if (!cancelled) void reloadGraduation();
    });
    return () => {
      cancelled = true;
      void subscription.then((un: () => void) => un()).catch(() => {});
    };
  }, [reloadGraduation]);

  /**
   * NAW-FR-16 / DRS-FR-KQTW: whether the draft's `graduated` status **stands**.
   *
   * Read from the run the queue holds for the draft wherever there is one, and
   * from the record only where there is none. The run is the authority because
   * the status is released when every run that published for the draft reaches
   * `discarded` or `failed`, and because the run is what this tab follows: the
   * record is read when the tab mounts, so a draft whose run the author has just
   * discarded would go on rendering read-only until the tab was reopened if the
   * record alone answered.
   */
  const graduated = graduation
    ? graduation.graduated === true
    : record?.status === "graduated";
  /**
   * NAW-FR-44: the prompt is read-only while a run holds the draft, and while
   * the `graduated` status stands — the same rule from two directions, because
   * what an author may do to the prompt is the same in both.
   */
  const locked = graduated || graduation?.locked === true;
  /**
   * NAW-FR-UEWC: a GitHub-shadow draft is read-only for its whole life, run or
   * no run. It is kept apart from `locked`, because Graduate stays enabled on
   * it (NAW-FR-AXFQ) while a run's lock disables Graduate.
   */
  const shadow = record ? isGithubShadow(record) : false;
  const shadowIssue = record?.githubIssue ?? null;
  const readOnlyPrompt = locked || shadow;

  /**
   * NAW-FR-TSQE: the tab's one publication read, followed by the
   * `"draft publication changed"` event. It serves the action's enablement, its
   * disabled reason, the standing attempt, and the section below the document.
   */
  // NAW-FR-AXFQ: a shadow draft offers no publication, so its tab reads none.
  const publication = useDraftPublication(shadow ? null : draftId);
  /**
   * NAW-FR-VBHT: what a disabled **Publish to GitHub** says, stated on hover
   * and in the accessible name. The reason is the backend's; the local-asset
   * case additionally lists every affected path (NAW-FR-PNCL).
   */
  /**
   * NAW-FR-GKBP: the unsaved prompt is written **first**.
   *
   * The backend publishes the *saved* prompt, so an author who types a
   * paragraph and publishes at once would otherwise post the previous save —
   * and the local-asset check would read the previous save's images too. A
   * write that fails publishes nothing, on the same terms Archive uses.
   */
  const beginPublication = async () => {
    if (dirty && !(await saveCurrent())) return;
    await publication.begin();
  };

  // DCR-FR-01 / DDS-FR-ZRPT: the review, in the document column. What it
  // holds, what each decision invokes, and what a refusal leaves standing all
  // live in `useDraftReview` — a hook rather than a component, because what it
  // produces is what the *editing surface* renders rather than a surface of its
  // own.
  const review = useDraftReview({
    draftId,
    proposal: pendingProposal,
    locked: readOnlyPrompt,
    viewingVersion: viewing !== null,
    path: pendingProposal?.path ?? null,
    flushBeforeApply,
    reloadAfterApply,
    reloadHistory: () => void loadHistory(),
    onDiscussFocus: () => setDiscussFocus((n) => n + 1),
    // DCR-FR-15: the roster the decision's own comment is dispatched against.
    agents: discussions.agents,
  });

  /**
   * DCR-FR-25: the candidate buffer is dropped when the draft is **graduated**.
   *
   * A graduated draft keeps every proposal it held and simply takes no further
   * write to its prompt (per `../../specifications/core/DRS-draft-storage.md`
   * DRS-FR-19, DRS-FR-20), so what goes is this session's unsaved editing of a
   * candidate rather than the proposal — which is still on disk and still reads
   * as it did. Held here rather than in the review, because the review is
   * closed by then and a buffer nothing can decide would otherwise stand until
   * the project closed.
   */
  useEffect(() => {
    if (!graduated) return;
    for (const proposal of proposals) {
      hunkCandidateBuffers.dropProposal(proposal.id);
    }
  }, [graduated, proposals]);

  /**
   * NAW-FR-57 / NAW-FR-59: tell the shell whether this tab's find panels are
   * reachable, so the Edit menu's two items are enabled exactly where the
   * accelerators are (SNV-FR-43).
   *
   * They are reachable over the **live prompt** of a draft that takes edits and
   * nowhere else: a version being read is read-only and gains no editing search
   * surface (NAW-FR-09), a draft its graduation holds takes no edit at all
   * (NAW-FR-44), and a draft whose storage is not the single prompt has no
   * editing surface to search (NAW-FR-41).
   */
  useEffect(() => {
    drafts.setPromptEditable(
      draftId,
      viewing === null && !readOnlyPrompt && !inconsistent && docKey !== null,
    );
  }, [drafts, draftId, viewing, readOnlyPrompt, inconsistent, docKey]);

  const archived = record?.status === "archived";
  const entries = history?.entries ?? [];
  const versionCount = versionCountOf(history);
  /**
   * NAW-FR-13: this reports rather than asks — there is no Save control to
   * forget. It is rendered **in** the band above the page rather than over it:
   * the trailing corner of that band is where the rendering toggle sits, in the
   * Editor's action row and in the reading's strip alike, and a report floated
   * into that corner lands on top of the control.
   */
  const writeReport = dirty ? "Saving…" : saved ? "Saved" : "";

  /**
   * NAW-FR-41 / DRS-FR-15: an inconsistent draft renders a state naming the
   * inconsistency, offers no editing surface and no rail, presents no file of it
   * as the prompt, and states that the draft is deleted from the Drafts panel —
   * that being the one thing left to do with it.
   */
  if (inconsistent) return <InconsistentDraft name={name} />;

  return (
    <div className="draft-workspace">
      {/* NAW-FR-05: the tab's chrome carries the draft's name and the rail's
          toggle, and nothing else. There is no permanently open text field, no
          lifecycle button, and no status control here — every action a draft
          affords is behind the one floating control (NAW-FR-27). */}
      <div className="draft-workspace__chrome" data-testid="draft-workspace-chrome">
        {/* NAW-FR-08: the rail's toggle sits at the head of this row and carries
            the number of versions the draft holds, so the count is legible
            without showing the rail. Placed here rather than beside the editing
            surface so that a hidden rail costs the surface no width at all: the
            whole rail goes, and no gutter is left behind to hold this. */}
        <button
          className="draft-rail__toggle"
          aria-expanded={railShown}
          aria-label={railShown ? "Hide History" : "Show History"}
          title={
            versionCount === null
              ? "Version history"
              : `${versionCount} ${
                  versionCount === 1 ? "version" : "versions"
                } of this draft`
          }
          onClick={toggleRail}
        >
          <Icon.History size={13} />
          {versionCount !== null && (
            <span className="draft-rail__count">{versionCount}</span>
          )}
        </button>
        {renaming ? (
          <span className="draft-workspace__rename">
            <input
              className="input input--sm"
              aria-label="Draft name"
              autoFocus
              value={nameText}
              onChange={(e) => {
                setNameText(e.target.value);
                setNameError(null);
              }}
              // A field already showing a refusal abandons the rename on blur
              // rather than re-issuing it, so the message never outlives the
              // field it belongs to.
              onBlur={() => (nameError ? cancelName() : void commitName())}
              onKeyDown={(e) => {
                if (e.key === "Enter") void commitName();
                if (e.key === "Escape") cancelName();
              }}
            />
            {nameError && (
              <p className="t-ui-xs draft-workspace__name-error" role="alert">
                {nameError}
              </p>
            )}
          </span>
        ) : (
          <button
            className="draft-workspace__name"
            // NAW-FR-UEWC: the in-place rename is inert on a shadow draft.
            onClick={() => !shadow && setRenaming(true)}
            aria-disabled={shadow || undefined}
            title={
              shadow
                ? "This draft mirrors a GitHub issue, so it cannot be renamed"
                : "Rename this draft"
            }
          >
            <Icon.Diamond size={12} /> {name}
          </button>
        )}
        {/* NAW-FR-16: an archived draft is marked beside its label, because it
            opens and edits exactly as an active one does and nothing else in
            the tab would say which it is. */}
        {archived && (
          <span className="draft-workspace__archived t-ui-xs">Archived</span>
        )}
        {/* NAW-FR-BJQX: the run's state, as a tag beside the draft's label, and as
            the route to it. A run is where an author acts on a graduation
            (GRU-FR-MYFA), so this says which one holds the prompt and takes them
            there rather than trying to be a second run surface. */}
        {graduation && (
          <button
            className="draft-workspace__graduation t-ui-xs"
            data-testid="draft-graduation-state"
            title="Go to this draft's graduation run"
            onClick={() => onOpenRun(graduation.runId)}
          >
            <Icon.Diamond size={11} /> {stateLabel(graduation.state)}
          </button>
        )}
        {/* NAW-FR-CBUJ: the draft's publication, as a tag beside the draft's
            label on the same terms as the archived marker and the run state. A
            tag that names a record is the route to that issue (NAW-FR-XRLD);
            the records themselves are read in Draft Information. */}
        <DraftPublicationTag publication={publication} />
        {/* NAW-FR-BCHZ: a shadow draft's tag names its repository and issue
            number, and opens the issue. */}
        {shadow && shadowIssue && (
          <GithubShadowTag draftId={draftId} issue={shadowIssue} />
        )}
        {/* NAW-FR-35 / DCR-FR-03: a draft carrying an undecided proposal says
            so, and the marker takes the author to the first change still to be
            decided. It clears the moment the proposal is resolved.

            The live dot is the one thing on this surface that animates, and it
            stops under a reduced-motion preference. */}
        {pendingProposal !== undefined && (
          <button
            className="draft-workspace__proposal t-ui-xs"
            data-testid="draft-pending-proposal"
            title={`Review the ${undecidedCount(pendingProposal.counts)} change${
              undecidedCount(pendingProposal.counts) === 1 ? "" : "s"
            } proposed to ${pendingProposal.path}`}
            onClick={() => {
              const first = undecidedHunks(pendingProposal)[0];
              if (first) setFocusedHunk(draftId, first.id);
              openReview(pendingProposal.id);
            }}
          >
            <span className="dot dot--live" aria-hidden="true" />
            {undecidedCount(pendingProposal.counts)} proposed{" "}
            {undecidedCount(pendingProposal.counts) === 1 ? "change" : "changes"}
          </button>
        )}

        <div className="spacer" />

        {/* DDS-FR-PNXR: the three presets, and the accelerator that cycles
            them, at the trailing edge of the tab's own row. They arrange two
            columns, so they stand down while one of them is hidden. */}
        {!view.hidden && (
          <>
            <RatioControl ratio={view.ratio} onChange={setRatio} />
            <kbd className="draft-workspace__kbd t-meta" title="Cycle the split">
              ⌘\
            </kbd>
          </>
        )}

        {/* DDS-FR-XQMF: the way to the whole window for the draft, and the way
            back to the conversation. One control in both states, so hiding the
            column never puts the discussion out of reach. */}
        <button
          type="button"
          className="btn btn--ghost btn--icon-sm draft-workspace__discussion-toggle"
          aria-pressed={!view.hidden}
          data-testid="draft-discussion-toggle"
          title={view.hidden ? "Show the discussion" : "Hide the discussion"}
          aria-label={view.hidden ? "Show the discussion" : "Hide the discussion"}
          onClick={toggleDiscussion}
        >
          <Icon.Comment size={13} />
        </button>
      </div>

      {error && <div className="draft-workspace__error">{error}</div>}


      {/* NAW-FR-44: why the prompt cannot be edited. Stated rather than left to
          be discovered by typing into a surface that silently takes nothing.

          The live region is rendered whether or not it holds anything, because
          a region that mounts with its own message is announced by nothing: the
          announcement is a *change* to a region already being watched. That is
          what carries the read-only state changing in both directions — a run
          taking the draft, and a run being discarded releasing it again
          (DRS-FR-KQTW). */}
      <div aria-live="polite" data-testid="draft-lock-announcement">
        {/* NAW-FR-UEWC: why a shadow draft's prompt cannot be edited. */}
        {shadow && <GithubShadowBanner issue={shadowIssue} />}
        {locked && (
          <div className="draft-workspace__locked t-ui-xs" role="note">
            {graduated
              ? "This draft has been graduated. Its prompt is kept as the record of what the specification was written from, and is read-only."
              : "A graduation run is answering this prompt, so it is read-only until the run finishes."}
          </div>
        )}
      </div>

      {/* NAW-FR-LQAF / NAW-FR-HZSW: the publication band, in the same place the
          lock note stands and on the same terms — present only while an attempt
          or a failure stands. */}
      <DraftPublicationBand publication={publication} />

      {/* NAW-FR-05 / NAW-FR-08 / DDS-FR-KTVW: the History rail, the document
          column and the discussion column. When the History rail is hidden
          nothing of it remains, so the two columns have the tab's full width —
          the toggle that brings it back lives in the action row above. */}
      <div className="draft-workspace__body" data-rail={railShown ? "on" : "off"}>
        {railShown && (
          <DraftHistoryRail
            history={history}
            entries={entries}
            error={historyError}
            viewing={viewing}
            liveRowRef={liveRowRef}
            onRetry={() => void loadHistory()}
            onBackToLive={backToLive}
            onRead={(entry) => void readVersion(entry)}
          />
        )}

        {/* DDS-FR-KTVW / DDS-FR-PNXR: the two columns and the splitter between
            them, as one grid. Fractions rather than pixels, so the split keeps
            its proportion when the window changes size, and the columns are
            inset from the frame so neither runs into it.

            One grid rather than two floating panes: both columns are always
            visible, and a layout in which either could cover the other is the
            arrangement this surface exists to replace. */}
        <div
          className="dds-split"
          ref={splitRef}
          // DDS-FR-XQMF: one column takes the whole tab while the other is
          // hidden. The splitter goes with it — there is nothing to drag
          // between when there is only one column.
          style={{
            gridTemplateColumns: view.hidden ? "1fr" : gridTemplate(view.ratio),
          }}
          data-discussion={view.hidden ? "hidden" : "shown"}
          data-testid="draft-discussion-split"
        >
        {/* NAW-FR-27: the field the page is set on, and the corner of it the
            action control floats in. The collapsed control has a strip of that
            field held open for it below the page, so the chrome the tab always
            carries covers nothing; what the control opens floats over the page
            rather than pushing it.

            `data-overlay` gives the page more to SCROLL while one is open —
            enough that its last line comes out from under whichever surface is
            covering it. It changes the scroll range and not the sheet, so the
            page's box stays exactly where it is. */}
        <div
          className="draft-editor"
          ref={draftEditorRef}
          data-overlay={overlayOpen ? "on" : "off"}
          /* NAW-FR-14 / CMT-FR-64: the field's margin state and its
             arrangement, on the element that owns the field.

             The draft's **discussions** are no longer here — they are the
             column beside this one (DDS-FR-KTVW), and they lend the page's
             field nothing. What can still open a margin is a thread anchored to
             a passage of the prompt, which this tab does not yet serve; until it
             does, the field keeps its whole measure for the writing. */
          data-rail={anchoredThreads.length > 0 ? "open" : "closed"}
          data-comments={arrangement}
        >
          {/* NAW-FR-51 / NAW-FR-54: the typed refusal of an insertion, rendered
              inline where the author is looking, and the live region that
              announces every image outcome — an insertion, a refusal, and a
              removal alike.

              **Once**, here in the band around the page rather than inside the
              surface: they stand whichever of the two renderings is showing and
              whether or not a version is being read, so a second copy in the
              reading's own strip would put two `alert`s on the page for one
              refusal and read it out twice. The refusal is words rather than a
              tone, so it is legible without colour. */}
          {imageError !== null && (
            <p className="draft-workspace__image-error" role="alert">
              {imageError}
            </p>
          )}
          {/* A polite live region rather than a `status` role: the action row
              already carries one for what the control has to say, and a second
              would make "the tab's status" ambiguous to a reader traversing it.
              `aria-live` announces the change on its own. */}
          <span
            className="sr-only"
            aria-live="polite"
            data-testid="draft-image-status"
          >
            {imageStatus}
          </span>
          {/* NAW-FR-09 / NAW-FR-39: while a version is showing, a strip above
              the page states that the reading is read-only and carries the
              return to the live prompt. Read-only is conveyed in words and in
              accessible semantics rather than by colour or fill alone. */}
          {viewing !== null && (
            <div className="draft-reading" role="status">
              {/* NAW-FR-10: the way back leads the strip, ahead of what the
                  strip says — it is the one control here, and a control the
                  author reaches for at the end of every reading belongs where
                  the eye starts rather than at the far edge of the tab. */}
              <button className="btn btn--sm" onClick={returnToLive}>
                ← Back to live prompt
              </button>
              <span className="t-ui-xs draft-reading__state">
                Reading a past version · read-only
              </span>
              <div className="spacer" />
              {/* NAW-FR-13: the live prompt's own write report, which an
                  autosave can land while a version is being read. In the strip's
                  flow, ahead of the toggle. */}
              {writeReport !== "" && (
                <span className="editor__report t-ui-xs" data-dirty={dirty}>
                  {writeReport}
                </span>
              )}
              {/* NAW-FR-09 / EDT-FR-17: a version is read in the same two modes
                  the prompt is edited in, behind the same toggle in the same
                  position. It changes only the reading — the live prompt's own
                  mode is exactly as it was left (NAW-FR-10). */}
              {viewError === null && (
                <button
                  className="btn btn--ghost btn--icon"
                  aria-label={
                    readingMode === "wysiwyg"
                      ? "View as Markdown source"
                      : "View as rich text"
                  }
                  title={
                    readingMode === "wysiwyg"
                      ? "View as Markdown source"
                      : "View as rich text"
                  }
                  data-active={readingMode === "text"}
                  onClick={() =>
                    setViewMode(readingMode === "wysiwyg" ? "text" : "wysiwyg")
                  }
                >
                  {readingMode === "wysiwyg" ? (
                    <Icon.Code size={15} />
                  ) : (
                    <Icon.Doc size={15} />
                  )}
                </button>
              )}
            </div>
          )}

          {/* ACT-FR-09: while the review bar stands, the column's foot ends in
              the bar rather than in a strip of field held open for the action
              control. The control keeps its corner and floats over the bar. */}
          <div
            className="draft-editor__surface"
            data-foot={review !== undefined ? "bar" : "field"}
          >
            {viewing !== null ? (
              viewError !== null ? (
                /* NAW-FR-41: an inline statement in place of the reading,
                   naming what went wrong and offering a retry and the return.
                   The live prompt's buffer and dirty state are untouched. */
                <div className="draft-editor__empty" role="alert">
                  <p className="t-ui-sm">⚠ This version could not be read.</p>
                  <p className="t-ui-xs">{viewError}</p>
                  <div className="draft-editor__empty-actions">
                    <button
                      className="btn btn--sm"
                      onClick={() => void readVersion(viewing)}
                    >
                      Try again
                    </button>
                    <button className="btn btn--sm" onClick={returnToLive}>
                      Live prompt
                    </button>
                  </div>
                </div>
              ) : (
                /* NAW-FR-09: that version's prompt, read-only, **on the same
                   page in the same measure** and in the same two modes — the
                   sheet, the inset, the frontmatter region and the typographic
                   roles the Editor sets the live prompt on (`EDT-editor.md`
                   EDT-FR-17, EDT-FR-63), so the only thing that changes between
                   reading a version and editing the prompt is whether the
                   surface takes a keystroke. */
                <DraftVersionReading
                  text={viewText ?? ""}
                  mode={readingMode}
                  label={`Read-only past version of ${viewing.path} — ${sourceLabelOf(viewing)}, ${standingOf(viewing, entries)}`}
                />
              )
            ) : docKey !== null ? (
              /* NAW-FR-11: the Editor's own two-mode Markdown surface — the same
                 WYSIWYG default, the same raw-Markdown toggle, the same
                 frontmatter region, the same formatting toolbar and the same
                 Find panels an artifact is edited with. Keyed on the document so
                 a rename gives a fresh Tiptap instance; that costs nothing,
                 because the buffer, dirty flag, mode and history live in the
                 shell's store (NAW-FR-12). */
              <Editor
                key={docKey}
                artifactId={docKey}
                artifactName={selected ?? undefined}
                sessions={drafts.docs}
                // NAW-FR-14: a draft's threads are served in the draft scope of
                // the thread operations, which this surface does not yet
                // distinguish — so the rail stays off rather than reading and
                // writing artifact-scoped threads against a draft's key.
                showComments={false}
                // DDS-FR-QMBC: the prompt marks its fragment discussions and
                // offers Comment on a selection; the discussions themselves
                // render in the column.
                fragmentHost={fragmentsGroup.host}
                // NAW-FR-27 / ACT-FR-01: the tab carries exactly one action
                // control, and it governs the DRAFT — so the embedded Editor
                // mounts none of its own.
                showActions={false}
                // NAW-FR-13: the prompt writes itself, so the action row carries
                // a report of whether the pending edit has landed in place of
                // the unsaved badge an artifact's Save-owning row carries.
                report={writeReport}
                // NAW-FR-44: read-only while a run holds the prompt, and for
                // good once the draft is `graduated`. The surface takes no
                // edits at all rather than accepting them and refusing the
                // write, which would lose whatever was typed.
                readOnly={readOnlyPrompt}
                onEdit={() => setEditTick((n) => n + 1)}
                /* NAW-FR-50 / NAW-FR-52: what this tab lends the surface about
                   images — the store a pasted or dropped picture goes through,
                   and the resolution that draws one the prompt references. An
                   Editor tab on a project file lends neither, which is what
                   keeps a paste there unchanged (EDT-FR-86). */
                imageHost={imageHost}
                /* NAW-FR-54: an insertion, a refusal, and a removal are each
                   announced through the tab's existing accessibility feedback,
                   in the same way its other outcomes are. */
                announce={announceImage}
                /* DCR-FR-01 / DDS-FR-ZRPT: the proposal is reviewed IN this
                   document — the hunks in the prose they change, the chip on
                   the one under review, and the gutter map on the scroller's
                   edge. There is no modal and no second surface. */
                review={review}
              />
            ) : (
              /* The moment before the record has landed. A draft always holds
                 its one prompt (NAW-FR-06), so this is never an empty state the
                 author can act in — and there is nothing here to act with. */
              <div className="draft-editor__empty" aria-busy="true" />
            )}
          </div>
        </div>

        {/* DDS-FR-PNXR: the splitter, dragged and also moved by keyboard. It
            stands only while there are two columns to split (DDS-FR-XQMF). */}
        <Splitter
          ratio={view.ratio}
          onChange={setRatio}
          boundsRef={splitRef}
          hidden={view.hidden}
        />

        {/* DDS-FR-KTVW / DDS-FR-NWRL: the draft's conversation, in the tab
            rather than in a floating window over it. It holds no presentation
            instance — there is nothing to attach, detach, minimize or maximize,
            because the conversation is already where it is read.

            DDS-FR-XQMF: while it is hidden it is out of the layout and out of
            the accessibility tree, but it is **not** unmounted. An author part
            way through a reply who hides the column to read the draft must find
            that reply where they left it, with the discussion they had chosen
            and how far they had scrolled it — all of which an unmount would
            silently discard. */}
        <DiscussionColumn
          hidden={view.hidden}
          draftId={draftId}
          discussions={draftDiscussions}
          selectedThreadId={focusedThreadId}
          onSelectThread={setFocusedThreadId}
          identity={discussions.identity}
          identityBlock={
            identityBlock
              ? { message: identityBlock.message, route: identityBlock.route }
              : null
          }
          agents={discussions.agents}
          onReply={discussions.reply}
          onSetLock={discussions.setLock}
          onSetResolved={discussions.setResolved}
          errors={discussions.errors}
          blocked={starting}
          anchor={anchorBar}
          onOpenDiscussion={openDiscussion}
          onOpened={fragmentsGroup.opened}
          openingFragment={fragmentsGroup.openingFragment}
          onCancelFragment={fragmentsGroup.cancelOpening}
          availabilityOf={fragmentsGroup.availabilityOf}
          onFocusFragment={fragmentsGroup.focusFragment}
          ownerLabel={name}
          focusSignal={discussFocus}
        />
        </div>

        {/* NAW-FR-27 / NAW-FR-28 / ACT-FR-09: the one control every tab
        carries, over the three actions a draft affords. It is a child of
        the tab's body rather than of the document column, because the
        corner it keeps is the tab's own: it holds the same place while the
        discussion column is shown, hidden, and resized. Its two states, its
        dismissal and its composer are all `ACT-action-control.md`'s; what
        this tab settles is which entries a draft puts in it and what
        Graduate and Archive do. */}
        <ActionControl
          discussions={discussions}
          composer={control.composer}
          onComposerChange={control.setComposer}
          attachments={control.attachments}
          /* CVP-FR-08 / CVP-FR-64: the same owner the rail's own cards name,
             so a discussion created here reads the draft's name rather than
             its opaque id wherever it is presented (per the CommentRail's
             ownerLabel/ownerSurface above). */
          ownerLabel={name}
          ownerSurface="draft"
          itemNoun="draft"
          controlLabel="Draft actions"
          // NAW-FR-34: enabled whenever a draft is open, on the same
          // unconditional terms Graduate is — a draft always holds its prompt,
          // so there is always material for an agent to be asked about, and an
          // empty prompt is a thing an author may well want to discuss.
          canDiscuss
          /* ACT-FR-QWNP / ACT-FR-23: with the column shown the conversation is
             already beside the draft, so the action has nothing left to do and
             says so rather than reading as a control that does nothing. */
          discussUnavailable={
            // NAW-FR-AXFQ: a shadow draft offers no Discuss.
            shadow
              ? SHADOW_DISCUSS_REASON
              : view.hidden
                ? undefined
                : "The discussion is already beside the draft"
          }
          /* ACT-FR-QWNP / DDS-FR-JWNC: the conversation is a column of this tab
             rather than a composer to open. The action is only reachable while
             that column is hidden, so it brings the column back — by the route
             the action row's own control takes, which is what keeps the two in
             agreement and persists the state (per DDS-FR-XQMF) — and then puts
             the caret in it. */
          onDiscuss={() => {
            setDraftDiscussionHidden(draftId, false);
            setDiscussFocus((n) => n + 1);
          }}
          // NAW-FR-32 / ACT-FR-19: this tab lends the rail a margin, so the
          // discussions pin at its head and no floating panel is rendered.
          discussionsInRail
          blocked={starting}
          graduate={{
            label: "Graduate",
            onActivate: () => setStarting(true),
            // NAW-FR-17 / NAW-FR-17: no precondition on status or content —
            // a draft always holds its one prompt, and an empty prompt is a
            // thing the author may graduate as readily as a full one. The one
            // thing that disables it is a run already holding this draft, or
            // a draft a run has already graduated.
            disabled: locked,
            title: locked
              ? graduated
                ? "This draft has already been graduated"
                : "A graduation run is already working on this draft"
              : "Write the specification this prompt describes",
          }}
          publish={{
            label: "Publish to GitHub",
            onActivate: () => void beginPublication(),
            /* NAW-FR-ZQMX: enabled only where the one read reports the draft
               publishable. NAW-FR-VBHT: the reason is that read's, taken
               unchanged rather than composed here. NAW-FR-44: a `graduated`
               draft is not disabled by its status — publication changes the
               draft's metadata rather than its prompt. */
            disabled:
              shadow ||
              publication.busy ||
              !publication.view?.eligibility.publishable,
            title: shadow
              ? SHADOW_PUBLISH_REASON
              : publicationActionTitle(publication),
          }}
          archive={{
            label: archived ? "Restore" : "Archive",
            onActivate: () => void setStatus(archived ? "active" : "archived"),
            // NAW-FR-AXFQ: neither Archive nor Restore on a shadow draft.
            disabled: locked || shadow,
            title: shadow ? SHADOW_ARCHIVE_REASON : undefined,
          }}
          // NAW-FR-30 / CMT-FR-32: the margin's card menus join this tab's
          // floating surfaces under one rule — at most one of them all is open
          // at any moment. Each side reports its openings to the other.
          dismissSignal={dismissControl}
          onOverlayChange={setOverlayOpen}
          // NAW-FR-32 / CMT-FR-53: focused at the head of the margin, not
          // merely present — the conversation is where it will be read from
          // the moment it exists.
          onDiscussionOpened={setFocusedThreadId}
        />
      </div>

      {/* NAW-FR-17 / GRU-FR-GLSO: the start dialog is `GRV-graduation-review.md`'s,
          and opens from this tab's Graduate action and from nowhere else. What
          happens after it — the queue, the run, and the review — is
          `GRD-graduation.md`'s and this tab neither drives nor renders it. */}
      {/* NAW-FR-DWKA / NAW-FR-EOTB: the remote picker and the recovery choice,
          each standing only where the flow opened it. */}
      <PublicationOverlays publication={publication} />

      {starting && (
        <GraduationStart
          draftId={draftId}
          draftName={name}
          onClose={() => setStarting(false)}
          onStarted={(run) => {
            setStarting(false);
            // NAW-FR-20: the draft is not ended by graduating. What changed is
            // that a run now holds it, so the panel and this tab re-read the
            // lock rather than the tab closing.
            onDraftChanged();
            void reloadGraduation();
            onGraduationStarted(run.id);
          }}
        />
      )}

    </div>
  );
}
