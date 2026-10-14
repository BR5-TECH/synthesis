/**
 * The New Artifact tab's publication state (`NAW-new-artifact.md` NAW-FR-GKBP
 * … NAW-FR-UDPM, served by `../../specifications/core/GHP-github-publication.md`).
 *
 * One read on mount and one per `"draft publication changed"` (NAW-FR-TSQE):
 * the action's enablement, its disabled reason, the standing attempt, and the
 * publication section all come from that single call rather than from separate
 * queries.
 *
 * Every mutation is **single-flight** (NAW-FR-JBHV): one gesture is one attempt
 * however impatiently it is repeated.
 */
import { useCallback, useEffect, useRef, useState } from "react";
import {
  cancelDraftPublicationAttempt,
  cancelDraftPublicationConflict,
  getDraftPublication,
  listPublicationRemotes,
  loadPublicationMetadata,
  openPublicationIssue,
  publishDraftToGithub,
  resolveDraftPublicationConflict,
  retryDraftPublication,
  updateDraftOnGithub,
} from "../api";
import { onDraftPublicationChanged } from "../events";
import { logError, logInfo, logWarn } from "../logging";
import { githubPollingErrorMessage } from "../state/githubPolling";
import { publicationErrorText, updateErrorText } from "../state/publicationChoice";
import { showToast } from "../state/toasts";
import type {
  DraftPublicationView,
  PublicationChoiceInput,
  PublicationMetadata,
  PublicationMismatch,
  PublicationOutcome,
  PublicationRecoveryChoice,
  PublicationRemoteResolution,
} from "../types";

/** NTF-FR-RMJT: the key of the toast a failed update shows. */
export const UPDATE_FAILED_KEY = "draft-update-failed";
/** NTF-FR-RMJT: the title of that toast. */
export const UPDATE_FAILED_TITLE = "Issue not updated";

/** NAW-FR-AQBE: the closed issue an update waits on. */
export interface ClosedIssuePrompt {
  issueNumber: number;
  issueUrl: string;
  repository: string;
}

/** NAW-FR-EOTB: the issue a recovery choice is being offered over. */
export interface RecoveryPrompt {
  issueNumber: number;
  issueUrl: string;
  marker: string;
  /** What differs from the draft and the saved choice (GHP-FR-HRUN). */
  mismatches: PublicationMismatch[];
}

/**
 * NAW-FR-HNVR: the publication chooser while it is open. The metadata is read
 * once when it opens (`ready`), or its read failed outright (`failed`), in
 * which case the chooser still offers root publication (NAW-FR-TCQB).
 */
export interface PublicationChooserState {
  remoteName: string;
  persist: boolean;
  status: "loading" | "ready" | "failed";
  metadata: PublicationMetadata | null;
  error: string | null;
}

export interface UseDraftPublicationResult {
  view: DraftPublicationView | null;
  /** NAW-FR-DWKA: the picker's data while it is open, or `null`. */
  remotes: PublicationRemoteResolution | null;
  recovery: RecoveryPrompt | null;
  /** NAW-FR-HNVR: the publication chooser while it is open, or `null`. */
  chooser: PublicationChooserState | null;
  /** NAW-FR-JBHV: a call is in flight, so every affordance is inert. */
  busy: boolean;
  /**
   * NAW-FR-KDMW: a call that REACHES GitHub is in flight, which is the only
   * condition under which a publication is in progress. `busy` is every call,
   * an abandon among them, and an abandon is not a publication.
   */
  publishing: boolean;
  /** NAW-FR-HZSW: the typed error of the last failure, rendered inline. */
  error: string | null;
  /** NAW-FR-AQBE: the closed-issue dialog while it is open, or `null`. */
  closedIssue: ClosedIssuePrompt | null;
  /** NAW-FR-KMLN: update the issue of the current record. */
  update: () => Promise<void>;
  /** NAW-FR-AQBE: **Update closed issue**. */
  confirmClosedUpdate: () => Promise<void>;
  /** NAW-FR-AQBE: **Cancel**, Escape, or the close control; invokes nothing. */
  cancelClosedUpdate: () => void;
  /** NAW-FR-DWKA: begin — enumerate, then publish directly or open the picker. */
  begin: () => Promise<void>;
  closePicker: () => void;
  confirmPicker: (remoteName: string, persist: boolean) => Promise<void>;
  /** NAW-FR-VUCK: dismiss the chooser; invokes nothing. */
  closeChooser: () => void;
  /** NAW-FR-RBTE: publish with the choice the author made. */
  confirmChooser: (choice: PublicationChoiceInput) => Promise<void>;
  retry: () => Promise<void>;
  abandon: () => Promise<void>;
  answerRecovery: (choice: PublicationRecoveryChoice) => Promise<void>;
  cancelRecovery: () => Promise<void>;
  openIssue: (url: string) => void;
}

const EMPTY: UseDraftPublicationResult = {
  view: null,
  remotes: null,
  recovery: null,
  chooser: null,
  busy: false,
  publishing: false,
  error: null,
  closedIssue: null,
  update: async () => {},
  confirmClosedUpdate: async () => {},
  cancelClosedUpdate: () => {},
  begin: async () => {},
  closePicker: () => {},
  confirmPicker: async () => {},
  closeChooser: () => {},
  confirmChooser: async () => {},
  retry: async () => {},
  abandon: async () => {},
  answerRecovery: async () => {},
  cancelRecovery: async () => {},
  openIssue: () => {},
};

export function useDraftPublication(
  draftId: string | null,
): UseDraftPublicationResult {
  const [view, setView] = useState<DraftPublicationView | null>(null);
  const [remotes, setRemotes] = useState<PublicationRemoteResolution | null>(
    null,
  );
  const [recovery, setRecovery] = useState<RecoveryPrompt | null>(null);
  const [chooser, setChooser] = useState<PublicationChooserState | null>(null);
  // A chooser closed before its read lands must not be reopened by the answer.
  const chooserRequest = useRef(0);
  const [busy, setBusy] = useState(false);
  const [publishing, setPublishing] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [closedIssue, setClosedIssue] = useState<ClosedIssuePrompt | null>(
    null,
  );
  // NAW-FR-JBHV: the guard is a ref rather than the state above, because two
  // activations in one tick would both read the same stale `busy`.
  const inFlight = useRef(false);

  const reload = useCallback(async () => {
    if (!draftId) return;
    try {
      setView(await getDraftPublication(draftId));
    } catch (e) {
      // A read that fails leaves the previous figures rather than blanking the
      // section; the action simply stays as it was until the next event.
      logError(["frontend"], "draft publication could not be read", {
        draftId,
        error: String(e),
      });
    }
  }, [draftId]);

  useEffect(() => {
    if (!draftId) {
      setView(null);
      return;
    }
    void reload();
  }, [draftId, reload]);

  // NAW-FR-TSQE: follow the event rather than polling.
  useEffect(() => {
    if (!draftId) return;
    let cancelled = false;
    const subscription = onDraftPublicationChanged((payload) => {
      if (!cancelled && payload.draftId === draftId) void reload();
    });
    return () => {
      cancelled = true;
      void subscription.then((unlisten) => unlisten());
    };
  }, [draftId, reload]);

  /**
   * One guarded call, with the outcome folded back into the tag and the band.
   *
   * `publishes` marks the calls that reach GitHub to create or update the
   * issue. NAW-FR-KDMW turns on that distinction: only those calls make a
   * publication "in progress", and an attempt found in the read is one that
   * stopped rather than one that is running.
   */
  const run = useCallback(
    async (
      work: () => Promise<PublicationOutcome | void>,
      publishes = false,
    ) => {
      if (!draftId || inFlight.current) return;
      inFlight.current = true;
      setBusy(true);
      if (publishes) setPublishing(true);
      setError(null);
      try {
        const outcome = await work();
        if (outcome && outcome.kind === "recoveryRequired") {
          setRecovery({
            issueNumber: outcome.issueNumber,
            issueUrl: outcome.issueUrl,
            marker: outcome.marker,
            mismatches: outcome.mismatches ?? [],
          });
        } else {
          setRecovery(null);
        }
      } catch (e) {
        setError(publicationErrorText(String(e)));
      } finally {
        inFlight.current = false;
        setBusy(false);
        setPublishing(false);
        await reload();
      }
    },
    [draftId, reload],
  );

  /**
   * NAW-FR-HNVR: open the chooser and read its metadata once. A read that fails
   * outright leaves the chooser open on its root option (NAW-FR-TCQB, GHP-FR-UXOT).
   */
  const openChooser = useCallback(
    async (remoteName: string, persist: boolean) => {
      if (!draftId) return;
      const request = ++chooserRequest.current;
      setChooser({
        remoteName,
        persist,
        status: "loading",
        metadata: null,
        error: null,
      });
      try {
        const metadata = await loadPublicationMetadata(draftId, remoteName);
        if (request !== chooserRequest.current) return;
        setChooser({ remoteName, persist, status: "ready", metadata, error: null });
      } catch (e) {
        logError(["frontend", "remote"], "publication metadata could not be read", {
          draftId,
          error: String(e),
        });
        if (request !== chooserRequest.current) return;
        setChooser({
          remoteName,
          persist,
          status: "failed",
          metadata: null,
          error: githubPollingErrorMessage(e),
        });
      }
    },
    [draftId],
  );

  const begin = useCallback(async () => {
    if (!draftId || inFlight.current) return;
    inFlight.current = true;
    setBusy(true);
    setError(null);
    let resolution: PublicationRemoteResolution | null = null;
    try {
      resolution = await listPublicationRemotes(draftId);
    } catch (e) {
      setError(String(e));
    } finally {
      inFlight.current = false;
      setBusy(false);
    }
    if (!resolution) return;
    // NAW-FR-DWKA: exactly one configured remote and a selection means there is
    // nothing to choose between, so the picker is not shown at all and the
    // chooser opens for that remote.
    if (resolution.remotes.length === 1 && resolution.selection) {
      await openChooser(resolution.selection, false);
      return;
    }
    setRemotes(resolution);
  }, [draftId, openChooser]);

  const confirmPicker = useCallback(
    async (remoteName: string, persist: boolean) => {
      if (!draftId) return;
      setRemotes(null);
      await openChooser(remoteName, persist);
    },
    [draftId, openChooser],
  );

  const closeChooser = useCallback(() => {
    chooserRequest.current += 1;
    setChooser(null);
  }, []);

  const confirmChooser = useCallback(
    async (choice: PublicationChoiceInput) => {
      // Checked before the chooser closes: a confirm that `run` would refuse
      // must not take the chooser away with it.
      if (!draftId || !chooser || inFlight.current) return;
      const { remoteName, persist } = chooser;
      chooserRequest.current += 1;
      setChooser(null);
      logInfo(["frontend"], "publishing draft to GitHub", {
        draftId,
        remoteName,
        persistRemote: persist,
        subIssue: choice.parentIssueNumber !== null,
      });
      await run(
        () =>
          publishDraftToGithub({
            draftId,
            remoteName,
            persistRemote: persist,
            publicationChoice: choice,
          }),
        true,
      );
    },
    [draftId, chooser, run],
  );

  const retry = useCallback(
    () => run(() => retryDraftPublication(draftId as string), true),
    [draftId, run],
  );

  const abandon = useCallback(
    () => run(() => cancelDraftPublicationAttempt(draftId as string)),
    [draftId, run],
  );

  const answerRecovery = useCallback(
    (choice: PublicationRecoveryChoice) =>
      run(() => resolveDraftPublicationConflict(draftId as string, choice), true),
    [draftId, run],
  );

  const cancelRecovery = useCallback(async () => {
    // The dialog closes when the cancel lands, not before: a cancel the backend
    // refused would otherwise leave the attempt `awaiting_choice` with the one
    // surface that answers it gone (NAW-FR-EOTB). `run` clears `recovery`
    // itself on a non-recovery outcome.
    await run(() => cancelDraftPublicationConflict(draftId as string));
  }, [draftId, run]);

  /**
   * NAW-FR-KMLN / NAW-FR-HKYL: one update call, single-flight. The tag and the
   * band do not take part: an update is not a publication, so it sets neither
   * `publishing` nor `error` (NAW-FR-PTYO, NAW-FR-KMFT).
   */
  const runUpdate = useCallback(
    async (updateClosed: boolean) => {
      if (!draftId || inFlight.current) return;
      inFlight.current = true;
      setBusy(true);
      logInfo(["frontend", "remote"], "updating draft issue on GitHub", {
        draftId,
        updateClosed,
      });
      try {
        const outcome = await updateDraftOnGithub(draftId, updateClosed);
        if (outcome.kind === "issueClosed") {
          const current = view?.current;
          setClosedIssue({
            issueNumber: outcome.issueNumber,
            issueUrl: outcome.issueUrl,
            repository: current
              ? `${current.repositoryOwner}/${current.repositoryName}`
              : "",
          });
        }
      } catch (e) {
        // NAW-FR-KMFT / NTF-FR-RMJT: the toast carries the displayable text;
        // the code alone goes to the log, which holds no prompt text.
        logWarn(["frontend", "remote"], "draft issue update failed", {
          draftId,
          error: String(e),
        });
        showToast(
          {
            key: UPDATE_FAILED_KEY,
            level: "Error",
            title: UPDATE_FAILED_TITLE,
            body: updateErrorText(String(e)),
            address: null,
          },
          { priority: true },
        );
      } finally {
        inFlight.current = false;
        setBusy(false);
        await reload();
      }
    },
    [draftId, reload, view],
  );

  const update = useCallback(() => runUpdate(false), [runUpdate]);

  const confirmClosedUpdate = useCallback(async () => {
    if (inFlight.current) return;
    setClosedIssue(null);
    await runUpdate(true);
  }, [runUpdate]);

  const openIssue = useCallback(
    (url: string) => {
      if (!draftId) return;
      void openPublicationIssue(draftId, url).catch((e) =>
        logError(["frontend"], "publication issue could not be opened", {
          draftId,
          error: String(e),
        }),
      );
    },
    [draftId],
  );

  if (!draftId) return EMPTY;

  return {
    view,
    remotes,
    recovery,
    chooser,
    busy,
    publishing,
    error,
    closedIssue,
    update,
    confirmClosedUpdate,
    cancelClosedUpdate: () => setClosedIssue(null),
    begin,
    closePicker: () => setRemotes(null),
    confirmPicker,
    closeChooser,
    confirmChooser,
    retry,
    abandon,
    answerRecovery,
    cancelRecovery,
    openIssue,
  };
}
