/**
 * The prompt change review modal (`PCR-prompt-change-review.md`).
 *
 * Where an author decides what to do with a change an agent has proposed to a
 * **prompt the project already holds**: the proposed version against the file as
 * it stood when the review opened, read in the same three shapes and two
 * renderings every other comparison is read in (PCR-FR-06), with the agent's
 * reasoning above it and Accept and Reject — and a place to say why — pinned at
 * its foot.
 *
 * ## Why the base does not move
 *
 * PCR-FR-05. The base is a reading taken at a moment, and moving it under a
 * decision would move the diff the decision is about. A file rewritten after the
 * review opened leaves the comparison exactly as it was; what the modal does is
 * **say so**, and **Accept** stays enabled. This is the one place this surface
 * departs from `DCR-draft-change-review.md`, whose base conflict exists because
 * a draft's acceptance records a version of the prompt it replaced. A published
 * file's does not, and being refused for having kept working is a worse answer
 * than being told plainly what Accept will do.
 *
 * ## Why the candidate is not the artifact's editing session
 *
 * PCR-FR-20. The candidate holds its own buffer and its own undo history, and no
 * keystroke in it reaches the artifact's buffer, its dirty state, or its write
 * schedule — which is what lets this surface promise that a rewrite the author is
 * still shaping has cost their project nothing (PCR-FR-22).
 */
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import {
  applyPromptChangeProposal,
  completePromptChangeDecision,
  declinePromptChangeProposal,
  dispatchAgentTurn,
  loadArtifactContentsById,
  loadPromptChangeProposalContent,
} from "../api";
import { hunksBetween } from "../diff/localHunks";
import { replaceLineRange } from "../diff/targetEdit";
import { promptCandidateBuffers } from "../state/candidateBuffers";
import { onArtifactChangedExternally } from "../events";
import type { EditSessionStore } from "../state/editSessions";
import type { TargetEditing } from "./DiffTarget";
import { useHistoryAccelerators } from "../hooks/useHistoryAccelerators";
import { logError, logInfo, logWarn } from "../logging";
import { originFor } from "../state/discussionOrigin";
import { loggableTurnFailure } from "./CommentRail/messages";
import {
  closePromptReview,
  usePromptProposals,
  useReviewingPrompt,
} from "../state/promptProposals";
import { useDiffModes } from "../state/diffModes";
import { formatRelative } from "./ProjectPicker";
import { DiffComparisonBody, DiffModeToolbar } from "./DiffView";
import { Icon } from "./icons";
import { CandidateWriteState, authorHandle, messageFor } from "./PromptChangeReviewParts";
import { PROMPT_PROPOSAL_ERRORS, participantTitle } from "../types";
import type {
  ConversationOrigin,
  Discussion,
  ProjectAgent,
  PromptChangeProposal,
  PromptDecisionOutcome,
} from "../types";
import {
  refreshThread,
  useConversationThread,
} from "../state/conversationThreads";
import { useProjectAgents } from "../state/agentRegistry";
import { listProjectAgents } from "../api";
import { agentRoster } from "./agentTags";
import { decisionTargets } from "../state/activeAgents";

/**
 * PCR-FR-04: the **base** revision, captured at the moment the review opens and
 * held for as long as it is showing.
 *
 * The candidate is deliberately not here. It is the proposal's own material and
 * lives in `promptCandidateBuffers` keyed by the proposal, so it survives this
 * modal being dismissed and reopened (PCR-FR-21) — holding it in component state
 * would put the agent's original text back on every reopening.
 */
interface Base {
  /** The artifact's text as it stood when the review opened. */
  text: string;
  /** The checksum that reading carried, so a later event can be compared to it. */
  checksum: string;
}

export function PromptChangeReview({
  artifactId,
  sessions,
}: {
  artifactId: string;
  /**
   * PCR-FR-11: the artifact's editing session, quiesced before the decision is
   * invoked and reset only on success. The tab owns it, so the tab passes it in
   * — this modal never creates one of its own.
   */
  sessions: EditSessionStore;
}) {
  const reviewing = useReviewingPrompt();
  const proposals = usePromptProposals(artifactId);
  const proposal = proposals.find((p) => p.id === reviewing);

  // PCR-FR-03: reachable only from the Editor tab open on that proposal's
  // artifact. A request naming another file's proposal belongs to that file's
  // tab, so this one renders nothing for it.
  if (proposal === undefined || proposal.artifactId !== artifactId) return null;
  return <ReviewModal key={proposal.id} proposal={proposal} sessions={sessions} />;
}

function ReviewModal({
  proposal,
  sessions,
}: {
  proposal: PromptChangeProposal;
  sessions: EditSessionStore;
}) {
  const modes = useDiffModes();
  /**
   * PCR-FR-14: the conversation the proposal was made in, which is what its
   * active agents are read out of (CTA-FR-LCFU).
   *
   * Taken from the cache every surface that lists conversations publishes into,
   * so a rail already showing this artifact answers for it and no read is issued
   * (CVP-FR-53); a review opened where nothing has published it reads it once.
   * Held in a ref as well, because a decision is settled from a promise callback
   * and must route on the conversation as it stands then.
   */
  const thread = useConversationThread(proposal.threadId);
  const threadRef = useRef<Discussion | undefined>(thread);
  threadRef.current = thread;
  /** AGT-FR-36/37: the roster as it stands, so `@all` means the room as it now is. */
  const agents = useProjectAgents();
  const [base, setBase] = useState<Base | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [feedback, setFeedback] = useState("");
  const [deciding, setDeciding] = useState(false);
  const [decisionError, setDecisionError] = useState<string | null>(null);
  /**
   * PCR-FR-15: **Accept** disabled because the target no longer resolves, or no
   * longer resolves as a prompt. **Reject** stays enabled, because declining is
   * the one thing still left to do with such a proposal.
   */
  const [unappliable, setUnappliable] = useState<string | null>(null);
  /**
   * PCR-FR-15: the one answer that is **not** a refusal — the acceptance landed
   * and its decision comment is still owed. Nothing here is ever allowed to read
   * "your file has not changed", because it has.
   */
  const [owed, setOwed] = useState(false);
  /**
   * PCR-FR-05 / PCR-FR-24: the artifact has been rewritten since the review
   * opened. The event moves nothing — this line is the whole of what it does.
   */
  const [fileChanged, setFileChanged] = useState(false);
  const surfaceRef = useRef<HTMLDivElement | null>(null);
  const feedbackRef = useRef<HTMLTextAreaElement | null>(null);

  const pending = proposal.state === "pending";
  const artifactId = proposal.artifactId;

  /**
   * DFV-FR-47: whether the derivation is **held** because the rich candidate has
   * the caret. Declared here because both the traversal below and the mode
   * switch have to release it.
   */
  const [held, setHeld] = useState(false);
  /**
   * Bumped whenever the candidate is replaced wholesale rather than edited —
   * which is the one thing a rich run cannot detect for itself.
   */
  const [seed, setSeed] = useState(0);
  // A mode switch unmounts whatever held the caret, and a browser does not
  // reliably raise blur for an element it is removing.
  useEffect(() => {
    setHeld(false);
  }, [modes.visualization, modes.rendering]);

  /**
   * PCR-FR-20: undo and redo behave here as they do in a Diff tab (DFV-FR-50),
   * over the **candidate's own** history — a candidate belongs to a proposal and
   * not to a file, so no traversal here reaches the artifact's own history
   * (EDT-FR-84).
   */
  const traverse = useCallback(
    (direction: "undo" | "redo") => {
      if (!pending) return;
      setHeld(false);
      setSeed((n) => n + 1);
      promptCandidateBuffers.traverse(proposal.id, direction);
    },
    [pending, proposal.id],
  );
  useHistoryAccelerators(surfaceRef, traverse);

  /**
   * PCR-FR-02: dismissal decides nothing and changes no file of the project. The
   * one thing it does invoke is the candidate's own outstanding write, brought
   * forward so an edit is not lost to a dismissal (PCR-FR-22) — and that write
   * reaches proposal storage and never the artifact.
   */
  const dismiss = useCallback(() => {
    void promptCandidateBuffers.flush(proposal.id);
    closePromptReview();
  }, [proposal.id]);

  // PCR-FR-21: the candidate buffer is the proposal's, not this modal's, so it
  // is read from the store rather than held here.
  const candidateVersion = useSyncExternalStore(
    promptCandidateBuffers.subscribe,
    promptCandidateBuffers.getVersion,
  );
  const candidate = promptCandidateBuffers.get(proposal.id);

  /**
   * PCR-FR-09: the base is captured when the modal opens and retained for as
   * long as it is showing, and the candidate is fetched then too **unless the
   * store already holds a buffer for this proposal**, in which case the buffer
   * is what renders (PCR-FR-21). Switching among the six mode combinations
   * re-renders from held text without a further round-trip.
   */
  useEffect(() => {
    let cancelled = false;
    setBase(null);
    setLoadError(null);
    void Promise.all([
      loadPromptChangeProposalContent(proposal.id),
      loadArtifactContentsById(artifactId),
    ])
      .then(([content, contents]) => {
        if (cancelled) return;
        promptCandidateBuffers.adopt(proposal.id, content.content, content.checksum);
        setBase({ text: contents.body, checksum: contents.checksum });
      })
      .catch((error: unknown) => {
        if (cancelled) return;
        logError(["frontend"], "could not read a proposed prompt change", {
          proposalId: proposal.id,
          artifactId,
          error: String(error),
        });
        setLoadError("This proposed change could not be read.");
      });
    return () => {
      cancelled = true;
    };
  }, [proposal.id, artifactId]);

  /**
   * PCR-FR-11 / PCR-FR-24 / EDT-FR-84: no external-change modal is raised over
   * this tab for as long as the review is showing. The hold is released when the
   * modal goes, which raises a divergence that arrived meanwhile — the reason
   * for passing it over was that the file was about to be replaced whole, and a
   * dismissal means that did not happen.
   */
  useEffect(() => sessions.holdExternalChanges(artifactId), [sessions, artifactId]);

  /**
   * PCR-FR-05 / PCR-FR-24: the event moves nothing. The captured base stays on
   * screen, the comparison is not re-derived, the candidate is untouched, both
   * decisions stay enabled, and no conflict is raised — all it does is put up
   * the standing line saying the file has changed since the review opened.
   */
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void onArtifactChangedExternally((payload) => {
      if (payload.artifactId !== artifactId) return;
      setFileChanged((changed) => changed || payload.checksum !== baseRef.current);
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [artifactId]);
  // Read at delivery time rather than captured: the base arrives after the
  // subscription is established, and a listener rebuilt when it lands would miss
  // an event in the gap.
  const baseRef = useRef<string | null>(null);
  baseRef.current = base?.checksum ?? null;

  /**
   * PCR-FR-07: which renderings apply is the file's own affair, decided exactly
   * as `DFV-diff-viewer.md` decides it. A prompt artifact whose name ends `.md`
   * is Markdown and both rendering toggles are enabled; one whose name does not
   * is read in Source with the rendering group disabled (DFV-FR-18), the
   * disablement being display-only so the stored preference is untouched.
   *
   * The Flow reading of DFV-FR-17 is never reached: the target's resolved type is
   * `prompt` by the tool's own rule (PPC-FR-06) and never `flow`.
   */
  const richApplies = proposal.path.toLowerCase().endsWith(".md");
  const rendering = richApplies && modes.rendering === "rich" ? "rich" : "source";

  const candidateText = candidate?.text ?? null;
  /**
   * DFV-FR-47: the derivation is **held** while the rich candidate has the
   * caret. The marking does not go stale meanwhile: it is carried as decorations
   * that map through the author's own edits, and blur re-derives at once.
   */
  const shownRef = useRef(candidateText);
  if (!held) shownRef.current = candidateText;
  const shown = shownRef.current;
  const payload = useMemo(
    () => (base === null || shown === null ? null : hunksBetween(base.text, shown)),
    [base, shown],
  );

  /**
   * PCR-FR-20: the candidate's editing surface, on exactly the target-only terms
   * `DFV-diff-viewer.md` sets (DFV-FR-41).
   *
   * Offered only while the proposal is **pending**: a decision is made once, and
   * the text a decided proposal holds is the record of what was decided
   * (DCR-FR-17).
   */
  const editing = useMemo<TargetEditing | undefined>(() => {
    if (!pending || candidate == null) return undefined;
    return {
      // The name identifies the file, says which revision the keystrokes reach,
      // and names the proposing agent — so a screen-reader user landing in it
      // knows it belongs to a proposal rather than to the file.
      label: `${proposal.path} — proposal candidate from ${authorHandle(proposal)} (editable)`,
      seed,
      currentText: () => promptCandidateBuffers.get(proposal.id)?.text ?? "",
      disabled: candidate.conflict !== null || deciding,
      replaceLines: (from, to, replacement) => {
        const buffer = promptCandidateBuffers.get(proposal.id);
        if (!buffer) return;
        promptCandidateBuffers.edit(
          proposal.id,
          replaceLineRange(buffer.text, from, to, replacement),
        );
      },
    };
    // `candidateVersion` rather than `candidate`: the buffer is one object
    // mutated in place, so a memo keyed on it alone would answer with the state
    // the modal opened in.
  }, [pending, candidate, candidateVersion, proposal, deciding, seed]);

  /**
   * Focus the modal on open, so Escape and the controls are reachable at once —
   * and give it back on dismissal to whatever opened it.
   *
   * Without the second half, dismissing drops focus to `<body>` and a keyboard
   * user's next Tab restarts from the top of the document rather than from the
   * indication or the rail control they came in through.
   */
  useEffect(() => {
    const opener = document.activeElement;
    surfaceRef.current?.focus();
    return () => {
      if (!(opener instanceof HTMLElement)) return;
      // Only where it is still on screen: the control that opened the review is
      // routinely gone by the time it closes, the pending indication clearing
      // with the decision (PCR-FR-26).
      if (opener.isConnected) opener.focus();
    };
  }, []);

  /**
   * PCR-FR-02: the feedback field is one line until there is more than one line
   * in it, and grows to the cap the stylesheet sets before it starts scrolling.
   */
  useEffect(() => {
    const field = feedbackRef.current;
    if (field === null) return;
    field.style.height = "auto";
    const border = field.offsetHeight - field.clientHeight;
    field.style.height = `${field.scrollHeight + border}px`;
  }, [feedback]);

  /** PCR-FR-02: Escape dismisses, wherever focus has got to. */
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") dismiss();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [dismiss]);

  /**
   * PCR-FR-22: keep what is on screen, over whatever storage now holds.
   *
   * The stored checksum has to be read first: the write was refused precisely
   * because storage moved on, so keeping the author's text means writing it
   * against the baseline that refusal was about rather than the stale one.
   */
  const resolveCandidate = async (resolution: "keep" | "take") => {
    try {
      const stored = await loadPromptChangeProposalContent(proposal.id);
      promptCandidateBuffers.resolveCandidate(proposal.id, resolution, stored);
    } catch (error: unknown) {
      logError(["frontend"], "could not re-read a candidate to resolve it", {
        proposalId: proposal.id,
        error: String(error),
      });
    }
  };

  /**
   * PCR-FR-10 / PCR-FR-11: the decision, in the order the requirement fixes.
   *
   * The session is **quiesced** before the decision is invoked, so no scheduled
   * write of the author's own buffer races the replacement; in the same act any
   * external-change modal standing over that artifact is dismissed with neither
   * of its resolutions taken. Then the decision. Then, **on success only**, the
   * session is reset to what the file now holds.
   */
  const decide = (accept: boolean) => {
    if (deciding) return;
    setDeciding(true);
    setDecisionError(null);
    setOwed(false);
    const call = accept
      ? applyPromptChangeProposal
      : declinePromptChangeProposal;
    // Whether the session was actually put into the quiesced state, so the
    // failure path below lifts only what it took. A decision abandoned because
    // the candidate is not safe to decide over never reached the quiesce, and
    // never passed an external change over either.
    let quiesced = false;
    // PCR-FR-10 / PCR-FR-22: bring the candidate's own outstanding write forward
    // first, so what an acceptance applies is the candidate **as the author last
    // edited it** rather than as the agent composed it. A flush that reports the
    // candidate unsafe — a failed write, or an unresolved conflict — abandons
    // the decision rather than deciding over a candidate the author cannot be
    // sure of.
    void promptCandidateBuffers
      .flush(proposal.id)
      .then(async (safe) => {
        if (!safe) throw new Error(CANDIDATE_UNSAFE);
        if (!accept) return;
        // EDT-FR-81 / EDT-FR-84: nothing of the author's buffer may be written
        // behind the replacement, and the modal answering a question about that
        // buffer must not hold this decision behind it.
        await sessions.quiesce([artifactId]);
        quiesced = true;
        sessions.dismissConflict(artifactId);
      })
      .then(() => call(proposal.id, feedback.trim() === "" ? null : feedback))
      .then(async (outcome) => {
        // EXC-FR-UVJY: reset **on success only** — the buffer replaced, the dirty
        // indicator cleared, the undo history discarded, and the checksum the
        // acceptance produced adopted as the new baseline — so every tab bound
        // to that artifact renders the accepted text at once, without a reload
        // and without any external-change dialog appearing for the acceptance's
        // own write.
        if (accept) await sessions.resetRestored(artifactId);
        logInfo(["frontend"], "decided a proposed prompt change", {
          proposalId: proposal.id,
          artifactId,
          accepted: accept,
        });
        // PCR-FR-14: one fresh turn for each distinct agent the conversation is
        // currently addressed to, naming the decision's comment as what
        // addressed it, and none at all where it is addressed to nobody
        // (CTA-FR-LCFU, CTA-FR-QUXJ).
        // PCR-FR-14: the dispatch is best-effort, the decision having already
        // landed — so nothing it raises reaches the author, and nothing it
        // raises escapes as an unhandled rejection either.
        void dispatchDecision(
          proposal,
          outcome,
          threadRef.current,
          agents,
          feedback,
        ).catch((error: unknown) => {
          logWarn(["ai", "frontend"], "could not tell the agents what was decided", {
            proposalId: proposal.id,
            threadId: proposal.threadId,
            error: String(error),
          });
        });
        // PCR-FR-21: the candidate buffer is dropped by the decision, not by the
        // modal going — a decided proposal has nothing left to edit.
        promptCandidateBuffers.drop(proposal.id);
        closePromptReview();
      })
      .catch((error: unknown) => {
        const reason = String(error);
        // PCR-FR-11: a failed acceptance resets nothing and **lifts the
        // quiesce**, leaving the buffer, the dirty indicator, the undo history,
        // and the baseline exactly as they were — the backend never wrote the
        // file, so the session is still describing what is actually on disk.
        //
        // An external change that arrived while the review stood and was passed
        // over on the way in is raised as an ordinary external-change modal once
        // the quiesce lifts, the replacement it was passed over for not having
        // happened (EDT-FR-84). An `acceptance_incomplete` is not such a case —
        // the replacement did happen — so this runs only where the answer is a
        // refusal.
        if (quiesced) {
          sessions.unquiesce(artifactId);
          if (!reason.includes(PROMPT_PROPOSAL_ERRORS.acceptanceIncomplete)) {
            sessions.raiseHeldExternalChange(artifactId);
          }
        }
        // PCR-FR-15: an `acceptance_incomplete` is **not a refusal**. The change
        // has been applied and the conversation has not yet been told, so
        // nothing here is allowed to read "your file has not changed" — and no
        // turn is dispatched, there being nothing yet for one to answer.
        if (reason.includes(PROMPT_PROPOSAL_ERRORS.acceptanceIncomplete)) {
          void sessions.resetRestored(artifactId);
          logWarn(["frontend"], "a prompt change was applied with its comment owed", {
            proposalId: proposal.id,
            artifactId,
          });
          setOwed(true);
          return;
        }
        if (
          reason.includes(PROMPT_PROPOSAL_ERRORS.artifactNotFound) ||
          reason.includes(PROMPT_PROPOSAL_ERRORS.notAPrompt)
        ) {
          setUnappliable(messageFor(reason));
        }
        logError(["frontend"], "a proposed prompt change could not be decided", {
          proposalId: proposal.id,
          accepted: accept,
          error: reason,
        });
        setDecisionError(messageFor(reason));
      })
      .finally(() => setDeciding(false));
  };

  /**
   * PCR-FR-15: the retry an owed decision comment offers invokes
   * **`complete_prompt_change_decision`** and nothing else — never a second
   * acceptance, which would be a second decision rather than the completion of
   * one.
   */
  const retryOwed = () => {
    if (deciding) return;
    setDeciding(true);
    void completePromptChangeDecision(proposal.id)
      .then((outcome) => {
        if (outcome.commentId === undefined) {
          // The append failed again: the statement stands with the retry still
          // offered, the acceptance still landed, and nothing said about the
          // file having changed back.
          logWarn(["frontend"], "a prompt decision comment is still owed", {
            proposalId: proposal.id,
          });
          return;
        }
        // PCR-FR-14: the retry that completes the owed comment is what
        // dispatches it, on exactly these terms, from the outcome that call
        // returns. It routes off the appended comment, which `dispatchDecision`
        // reads for itself — the owed body was journalled when the acceptance
        // landed and carries feedback this modal may no longer hold.
        // PCR-FR-14: the dispatch is best-effort, the decision having already
        // landed — so nothing it raises reaches the author, and nothing it
        // raises escapes as an unhandled rejection either.
        void dispatchDecision(
          proposal,
          outcome,
          threadRef.current,
          agents,
          feedback,
        ).catch((error: unknown) => {
          logWarn(["ai", "frontend"], "could not tell the agents what was decided", {
            proposalId: proposal.id,
            threadId: proposal.threadId,
            error: String(error),
          });
        });
        promptCandidateBuffers.drop(proposal.id);
        closePromptReview();
      })
      .catch((error: unknown) => {
        logWarn(["frontend"], "a prompt decision comment could not be completed", {
          proposalId: proposal.id,
          error: String(error),
        });
      })
      .finally(() => setDeciding(false));
  };

  /**
   * PCR-FR-22: both decisions are disabled while the outcome of activating one
   * would be ambiguous — a candidate write in flight or failed, and an
   * unresolved candidate conflict.
   */
  const candidateBlocked =
    candidate != null &&
    (candidate.saving || candidate.error !== null || candidate.conflict !== null);
  const blockedReason =
    candidate?.conflict != null
      ? "This candidate has been rewritten elsewhere — resolve that first."
      : candidate?.error != null
        ? "This candidate could not be saved, so deciding it now would be ambiguous."
        : candidate?.saving
          ? "Saving this candidate…"
          : undefined;

  return (
    // PCR-FR-01: a scrim that covers the Editor tab's own bounds and stops at
    // its edges, so the shell around it — the activity rail, the panels, the tab
    // strip — stays lit and reachable while the author reads.
    // PCR-FR-02: a pointer-down outside dismisses and invokes nothing.
    <div
      className="draft-review-scrim"
      data-testid="prompt-change-review"
      onClick={(e) => {
        if (e.target === e.currentTarget) dismiss();
      }}
    >
      <div
        className="modal draft-review"
        role="dialog"
        aria-modal="true"
        aria-label={`Proposed change to ${proposal.path}`}
        ref={surfaceRef}
        tabIndex={-1}
        onKeyDown={(e) => {
          if (e.key !== "Escape") return;
          e.stopPropagation();
          dismiss();
        }}
      >
        {/* PCR-FR-02: the head names the file, the agent, and when — with the
            agent's own rationale beneath as prose. What differs from the draft
            review is only that the path is project-relative. */}
        <div className="draft-review__head">
          <div className="draft-review__title">
            <Icon.Diff size={13} />
            <span className="draft-review__path">{proposal.path}</span>
            <span className="draft-review__by">
              {authorHandle(proposal)} · {formatRelative(proposal.createdAt)}
            </span>
            <button
              type="button"
              className="draft-review__dismiss"
              aria-label="Close review"
              onClick={dismiss}
            >
              <Icon.X size={12} />
            </button>
          </div>
          {participantTitle(proposal.agent) && (
            <div
              className="draft-review__agent-title"
              data-testid="prompt-review-agent-title"
            >
              {participantTitle(proposal.agent)}
            </div>
          )}
          <p className="draft-review__rationale">{proposal.rationale}</p>
        </div>

        <DiffModeToolbar
          visualization={modes.visualization}
          rendering={rendering}
          richApplies={richApplies}
          // PCR-FR-23: the write state sits at the row's trailing end and says
          // only whether the edit has reached PROPOSAL storage. "Candidate
          // saved" is never allowed to read as "file updated", which is why the
          // standing line in the foot says the other half.
          writeState={
            pending && candidate != null ? (
              <CandidateWriteState buffer={candidate} />
            ) : undefined
          }
        />

        {/* PCR-FR-06: the comparison, in the Diff tab's own modes and marking. */}
        <div
          className="draft-review__body"
          data-page={rendering === "rich" ? "on" : "off"}
          onFocusCapture={(event) => {
            if (rendering !== "rich" || editing == null) return;
            if ((event.target as HTMLElement).closest?.(".diff-run")) setHeld(true);
          }}
          onBlurCapture={() => setHeld(false)}
        >
          <DiffComparisonBody
            payload={payload}
            revisions={
              base === null || shown === null
                ? null
                : { old: base.text, new: shown, isBinary: false }
            }
            visualization={modes.visualization}
            rendering={rendering}
            flow={false}
            isBinary={false}
            /* DFV-FR-57: the project-relative path is the name the Source
               rendering's language is read from. */
            fileName={proposal.path}
            error={loadError}
            awaiting={(base === null || candidate == null) && loadError === null}
            emptyLabel="This proposal matches the file as it stands."
            editing={editing}
            // PCR-FR-02: an empty candidate says what accepting it would do and
            // leaves Accept enabled — it is a change the author is entitled to
            // accept rather than a state to be corrected first.
            emptyTargetLabel="This proposal would leave the file empty. Type the version you do want, then accept it."
          />
        </div>

        <div className="draft-review__foot">
          {/* PCR-FR-22: a candidate storage has moved on under raises an explicit
              conflict over the **candidate**, offering to keep what is on screen
              or to take what storage holds, and choosing neither. */}
          {candidate?.conflict != null && (
            <div className="draft-review__conflict" role="alert">
              <p>
                This candidate has been rewritten somewhere else since you opened
                it. Nothing has been overwritten.
              </p>
              <div className="draft-review__conflict-actions">
                <button
                  type="button"
                  className="btn btn--sm"
                  onClick={() => void resolveCandidate("keep")}
                >
                  Keep what is on screen
                </button>
                <button
                  type="button"
                  className="btn btn--sm"
                  onClick={() => void resolveCandidate("take")}
                >
                  Take the stored version
                </button>
              </div>
            </div>
          )}
          {/* PCR-FR-23: the standing line that says which of the two things has
              happened — in words, and announced when it changes, so nobody has
              to infer from a saved indicator that their file was rewritten. */}
          {pending && promptCandidateBuffers.isEdited(proposal.id) && (
            <p className="draft-review__edited" role="status" aria-live="polite">
              You have edited this candidate. Editing it has not changed the
              file.
            </p>
          )}
          {/* PCR-FR-05: the file has moved on, and Accept still does what the
              review says it will do. */}
          {pending && fileChanged && !owed && (
            <p
              className="draft-review__edited"
              role="status"
              aria-live="polite"
              data-testid="prompt-review-file-changed"
            >
              This file has changed since you opened this review. Accept replaces
              it whole with the version below.
            </p>
          )}
          {unappliable !== null && (
            <p className="draft-review__warning" role="status">
              {unappliable}
            </p>
          )}
          {/* PCR-FR-15: the typed refusal, inline above the foot's controls.
              Where the refusal is one that also disables **Accept** the standing
              line above already states it, so it is not said twice. */}
          {decisionError !== null && !owed && unappliable === null && (
            <p className="draft-review__error" role="alert">
              {decisionError}
            </p>
          )}
          {/* PCR-FR-15: the one answer that is not a refusal, stated as what it
              is. It never says the file is unchanged, because it is not, and it
              never withdraws the acceptance, because the commit point was
              reached. */}
          {owed && (
            <div className="draft-review__owed" role="status">
              {/* PCR-FR-15: what is stated is that the change landed and that
                  the conversation has not been told. It names no agent: who a
                  decision reaches is the conversation's active set at the moment
                  it is told (PCR-FR-14, per `CMT-comments.md` CTA-FR-LCFU), which
                  is not the proposing agent wherever the author has since turned
                  to somebody else. */}
              <p data-testid="prompt-review-comment-owed">
                ✓ The change was applied to this file. The conversation has not
                yet been told, so no agent has been asked to continue.
              </p>
              <div className="draft-review__owed-actions">
                <button
                  type="button"
                  className="btn"
                  disabled={deciding}
                  onClick={retryOwed}
                >
                  Try again
                </button>
                <button type="button" className="btn" onClick={dismiss}>
                  Close
                </button>
              </div>
            </div>
          )}
          {/* DCR-FR-17: a decided proposal opens for reading — a statement of
              which way it went, and a dismissal, in place of the controls. */}
          {owed ? null : !pending ? (
            <div className="draft-review__decided">
              <span data-testid="prompt-review-decided">
                {proposal.state === "accepted" ? "Accepted" : "Rejected"}
                {proposal.decidedAt
                  ? ` ${formatRelative(proposal.decidedAt)}`
                  : ""}
                .
              </span>
              <button type="button" className="btn" onClick={dismiss}>
                Close
              </button>
            </div>
          ) : (
            <div className="draft-review__controls">
              <div className="draft-review__actions">
                {/* DCR-FR-28: two independently reachable controls, each with
                    its own accessible name and each stating its disabled
                    reason. */}
                <button
                  type="button"
                  className="btn"
                  onClick={() => decide(false)}
                  disabled={deciding || candidateBlocked}
                  title={blockedReason}
                >
                  Reject
                </button>
                <button
                  type="button"
                  className="btn btn--primary"
                  onClick={() => decide(true)}
                  disabled={deciding || candidateBlocked || unappliable !== null}
                  title={unappliable ?? blockedReason}
                >
                  Accept
                </button>
              </div>
              <textarea
                ref={feedbackRef}
                className="draft-review__feedback"
                placeholder="Say why, if you want to…"
                aria-label="Feedback"
                rows={1}
                value={feedback}
                onChange={(e) => setFeedback(e.target.value)}
                disabled={deciding}
              />
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

/**
 * PCR-FR-14: tell the conversation what was decided, on **exactly the terms any
 * human comment in it dispatches on** (per `CMT-comments.md` CTA-FR-LCFU,
 * CTA-FR-QUXJ).
 *
 * The decision's comment is one the author wrote (PCP-FR-16), so a turn goes to
 * each distinct agent of the conversation's active set as it stands with that
 * comment appended, and none at all where that set is empty. A decision carrying
 * feedback that names agents is a tagged comment like any other and reaches
 * exactly those; a decision carrying none is an untagged comment and reaches
 * whoever the author was last addressing there — which is the proposing agent
 * wherever the author has not since turned to somebody else, and is not it where
 * they have. There is no route by which a proposal's own agent is dispatched to
 * **for having proposed**.
 *
 * Both inputs are read **again** here rather than taken from what this surface
 * happened to be holding, and both readings matter. The conversation must carry
 * the decision's own comment for its feedback to settle the set: the body of an
 * owed comment was journalled when the acceptance landed and carries feedback
 * this modal may no longer hold (PCR-FR-15), and even on the ordinary path the
 * `"discussion changed"` event that would have delivered it has no ordering
 * against the decision call's own response. The roster must be the project's as
 * it stands, and this review can be opened from a notification without any
 * surface having published one — an empty roster resolves no tag at all, which
 * would silently send the decision to nobody. A read that fails falls back to
 * what was held, which is `decisionTargets`' second reading.
 *
 * Best-effort and never surfaced: the decision has already landed, and an agent
 * that could not be re-dispatched is a conversation that stalls rather than a
 * change the author has to make again. It is logged, because a stalled
 * conversation is otherwise invisible to anyone debugging it. One agent's
 * failure neither suppresses another's turn nor repeats it (CMT-FR-79).
 *
 * Nothing is dispatched when the decision's comment could not be appended —
 * there is then no comment for a turn to answer, and the retry of PCR-FR-15 is
 * what dispatches once it is.
 */
async function dispatchDecision(
  proposal: PromptChangeProposal,
  outcome: PromptDecisionOutcome,
  held: Discussion | undefined,
  agents: readonly ProjectAgent[],
  feedback: string,
): Promise<void> {
  if (outcome.commentId === undefined) return;
  const [refreshed, enrolled] = await Promise.all([
    refreshThread(proposal.threadId),
    agents.length > 0 ? agents : currentRoster(),
  ]);
  const thread = refreshed ?? held;
  const roster = agentRoster([...enrolled]);
  const targets = decisionTargets(thread, outcome.commentId, feedback, roster);
  if (targets.length === 0) {
    // PCR-FR-14: an ordinary outcome rather than a failure — the decision is not
    // refused or retried on this account. Recorded because a decision that told
    // nobody is otherwise indistinguishable from one whose dispatch was lost.
    logInfo(["ai", "frontend"], "a decision reached no agent", {
      proposalId: proposal.id,
      threadId: proposal.threadId,
      conversationRead: thread === undefined ? "unavailable" : "read",
      enrolled: roster.nicknames.length,
    });
    return;
  }
  // AGC-FR-05: the held thread's own origin. A thread that could not be read
  // still names its discussion and owner, and the backend reads the target and
  // the fragment target again from the discussion that id names.
  const origin: ConversationOrigin = thread
    ? originFor(thread)
    : {
        discussionId: proposal.threadId,
        target: { kind: "artifact", artifactId: proposal.artifactId },
        fragmentTarget: null,
      };
  for (const nickname of targets) {
    void dispatchAgentTurn({
      nickname,
      origin,
      triggerCommentId: outcome.commentId,
    }).catch((error: unknown) => {
      // Not surfaced (PCR-FR-14). The log carries the typed code only: the raw
      // error can quote the argument it refused (see `loggableTurnFailure`).
      const raw = error instanceof Error ? error.message : String(error);
      logWarn(["ai", "frontend"], "could not tell an agent what was decided", {
        proposalId: proposal.id,
        nickname,
        failure: loggableTurnFailure(raw),
      });
    });
  }
}

/**
 * The project's enrolment, for a decision taken before any surface published it.
 *
 * `useProjectAgents` is a read of a cache the chrome's own roster control fills
 * (AGT-FR-02); it never loads anything itself. A review opened from a
 * notification (PCR-FR-18) can therefore reach a decision while that cache is
 * still empty, and an empty roster resolves no tag at all — so a decision that
 * should have reached the whole room would quietly reach nobody.
 */
async function currentRoster(): Promise<readonly ProjectAgent[]> {
  // A read that fails, or that answers with anything other than a list, is the
  // same thing here: no enrolment this decision can resolve a tag against. Both
  // resolve to an empty roster rather than to a raised error, because this whole
  // path is best-effort — see `dispatchDecision`.
  return listProjectAgents()
    .then((enrolled) => (Array.isArray(enrolled) ? enrolled : []))
    .catch(() => []);
}

/**
 * Why a decision stood down: the candidate is not in a state anyone can decide
 * over (PCR-FR-22). Not surfaced as prose — the foot is already saying why, and
 * the controls are already disabled.
 */
const CANDIDATE_UNSAFE = "candidate_unsafe";
