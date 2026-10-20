import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";

import * as api from "../../api";
import { isGithubShadow, participantName } from "../../types";
import type {
  DraftRecord,
  DraftStatus,
  DiscussionTarget,
  AttachmentInput,
} from "../../types";
import { useDraftDiscussionWiring } from "./useDraftDiscussionWiring";
import { GraduationStart } from "../GraduationStart";
import type { EditMode } from "../../state/editHistory";
import { AUTOSAVE_DELAY_MS } from "../../state/editSessions";
import { DiscussionColumn, Splitter, gridTemplate } from "../DraftDiscussion";
import { DraftHistoryRail } from "./DraftHistoryRail";
import { GithubShadowBanner } from "./GithubShadow";
import { useDraftReview } from "./useDraftReview";
import { useHunkDiscussion } from "./useHunkDiscussion";
import { useDraftPublication } from "../../hooks/useDraftPublication";
import { DraftPublicationBand, PublicationOverlays } from "../DraftPublication";
import { isRefusal, versionCountOf } from "./labels";

// Re-exported so the wording stays testable from where it always was, and so a
// reader who follows a citation to this tab still finds them.
export {
  imageRefusal,
  liveMarkerFor,
  sourceLabelOf,
  standingOf,
  versionCountOf,
} from "./labels";
import { revealHunk } from "../../state/draftDiscussion";
import { useDiscussionControl } from "../../hooks/useDiscussionControl";
import { identityBlockFor } from "../../hooks/useComments";
import {
  hunkPosition,
  undecidedHunks,
  useDraftProposals,
  useReviewing,
} from "../../state/draftProposals";
import { hunkCandidateBuffers } from "../../state/candidateBuffers";
import type { WorkspaceProps } from "./workspaceProps";
import { InconsistentDraft } from "./InconsistentDraft";
import { DraftActionControl } from "./DraftActionControl";
import { DraftChrome } from "./DraftChrome";
import { DraftDocumentColumn } from "./DraftDocumentColumn";
import { useDraftGraduation } from "./useDraftGraduation";
import { useDraftHistory } from "./useDraftHistory";
import { useDraftImages } from "./useDraftImages";
import { useDraftRename } from "./useDraftRename";
import { useDraftSplit } from "./useDraftSplit";

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
  const [saved, setSaved] = useState(false);
  /**
   * Bumped on every edit the Editor reports, so the autosave timer below is
   * restarted by the *last* keystroke of a burst rather than the first.
   */
  const [editTick, setEditTick] = useState(0);

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

  const { splitRef, view, setRatio, toggleDiscussion } = useDraftSplit(draftId);

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
        if (hunkId !== null) revealHunk(draftId, hunkId);
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

  const {
    history,
    historyError,
    viewing,
    viewText,
    viewError,
    viewMode,
    setViewMode,
    liveRowRef,
    dropReading,
    backToLive,
    returnToLive,
    loadHistory,
    readVersion,
  } = useDraftHistory(draftId, record);
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
      dropReading();
      void drafts.docs.reload(drafts.key(draftId, path));
    },
    [drafts, draftId, dropReading],
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

  const { imageStatus, imageError, imageHost, announceImage } = useDraftImages({
    draftId,
    dirty,
    editTick,
    saveCurrent,
  });

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

  const toggleRail = useCallback(() => {
    // NAW-FR-10: hiding the rail returns to the live prompt, so the author
    // cannot be left reading a version with no rail to leave it by.
    if (railShown) backToLive();
    drafts.setRailShown(draftId, !railShown);
  }, [railShown, backToLive, drafts, draftId]);

  const rename = useDraftRename({
    draftId,
    name,
    drafts,
    record,
    dirty,
    saveCurrent,
    applyRecord,
    loadHistory,
    onNameChanged,
    onDraftChanged,
  });

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

  const { graduation, reloadGraduation } = useDraftGraduation(
    draftId,
    draftsRevision,
  );

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
  /** NAW-FR-KMLN: an update also writes the unsaved prompt first. */
  const beginUpdate = async () => {
    if (dirty && !(await saveCurrent())) return;
    await publication.update();
  };

  // DCR-FR-01 / DDS-FR-ZRPT: the review, in the document column. What it
  // holds, what each decision invokes, and what a refusal leaves standing all
  // live in `useDraftReview` — a hook rather than a component, because what it
  // produces is what the *editing surface* renders rather than a surface of its
  // own.
  // DCR-FR-MTFD / DDS-FR-RHKW: Discuss on a change opens a discussion of its
  // own, in the column beside the document.
  const hunkDiscussion = useHunkDiscussion({
    draftId,
    proposal: pendingProposal,
    discussions: draftDiscussions,
    selectDiscussion: setFocusedThreadId,
    focusComposer: () => setDiscussFocus((n) => n + 1),
  });
  const review = useDraftReview({
    draftId,
    proposal: pendingProposal,
    locked: readOnlyPrompt,
    viewingVersion: viewing !== null,
    path: pendingProposal?.path ?? null,
    flushBeforeApply,
    reloadAfterApply,
    reloadHistory: () => void loadHistory(),
    onDiscussHunk: hunkDiscussion.discuss,
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
      <DraftChrome
        draftId={draftId}
        name={name}
        railShown={railShown}
        versionCount={versionCount}
        onToggleRail={toggleRail}
        rename={rename}
        shadow={shadow}
        shadowIssue={shadowIssue}
        archived={archived}
        graduation={graduation}
        onOpenRun={onOpenRun}
        publication={publication}
        pendingProposal={pendingProposal}
        view={view}
        onRatioChange={setRatio}
        onToggleDiscussion={toggleDiscussion}
      />

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
        <DraftDocumentColumn
          drafts={drafts}
          docKey={docKey}
          selected={selected}
          overlayOpen={overlayOpen}
          imageError={imageError}
          imageStatus={imageStatus}
          imageHost={imageHost}
          announceImage={announceImage}
          viewing={viewing}
          viewText={viewText}
          viewError={viewError}
          entries={entries}
          readingMode={readingMode}
          onReadingMode={setViewMode}
          onReturnToLive={returnToLive}
          onRead={(entry) => void readVersion(entry)}
          writeReport={writeReport}
          dirty={dirty}
          readOnly={readOnlyPrompt}
          onEdit={() => setEditTick((n) => n + 1)}
          fragmentHost={fragmentsGroup.host}
          review={review}
        />

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
          onSetResolved={discussions.setResolved}
          errors={discussions.errors}
          blocked={starting}
          anchor={anchorBar}
          onOpenDiscussion={openDiscussion}
          onOpened={(created) => {
            fragmentsGroup.opened(created);
            hunkDiscussion.opened(created);
          }}
          openingHunk={hunkDiscussion.opening}
          onCancelHunk={hunkDiscussion.cancel}
          openingFragment={fragmentsGroup.openingFragment}
          onCancelFragment={fragmentsGroup.cancelOpening}
          availabilityOf={fragmentsGroup.availabilityOf}
          onFocusFragment={fragmentsGroup.focusFragment}
          ownerLabel={name}
          focusSignal={discussFocus}
        />
        </div>

        {/* NAW-FR-27 / NAW-FR-28 / ACT-FR-09: the one control every tab
        carries, over the actions a draft affords. It is a child of
        the tab's body rather than of the document column, because the
        corner it keeps is the tab's own: it holds the same place while the
        discussion column is shown, hidden, and resized. Its two states, its
        dismissal and its composer are all `ACT-action-control.md`'s; what
        this tab settles is which entries a draft puts in it and what
        Graduate and Archive do. */}
        <DraftActionControl
          draftId={draftId}
          name={name}
          drafts={drafts}
          control={control}
          discussionHidden={view.hidden}
          shadow={shadow}
          locked={locked}
          graduated={graduated}
          archived={archived}
          blocked={starting}
          publication={publication}
          onGraduate={() => setStarting(true)}
          onPublish={() => void beginPublication()}
          onUpdate={() => void beginUpdate()}
          onToggleArchive={() => void setStatus(archived ? "active" : "archived")}
          onDiscussFocus={() => setDiscussFocus((n) => n + 1)}
          dismissSignal={dismissControl}
          onOverlayChange={setOverlayOpen}
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
