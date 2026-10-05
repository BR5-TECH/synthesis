/**
 * The main window's GitHub polling (`GIT-git.md` GIT-FR-CVSB, GIT-FR-SLRD) and
 * the actions of the Ready tasks section (GIT-FR-NQTZ, GIT-FR-FTHT).
 *
 * Mounted in `App` rather than in the Git panel: the panel is mounted only
 * while it shows, and the schedule must run whether the panel is shown,
 * hidden, or on another section. The claim flow lives here for the same
 * reason — a claim started from the panel must still acknowledge once the
 * start dialog opens, even if the author hides the panel meanwhile.
 */
import { useCallback, useEffect, useRef, useState } from "react";

import * as api from "../api";
import { onGithubPollingChanged } from "../events";
import { logInfo, logWarn } from "../logging";
import {
  githubPollingErrorMessage,
  intervalMs,
  refusalCode,
  timedPollingEnabled,
} from "../state/githubPolling";
import type { GithubPollingView } from "../types";

/** The action in flight on one issue's row (GIT-FR-TZUI). */
export type ReadyTaskAction = "claim" | "retry";

export interface GithubPollingOptions {
  /** The main window shows a project (not the picker). */
  active: boolean;
  /** The open project, or null. */
  projectKey: string | null;
  /** Bumped on every worktree switch and in-place checkout. */
  contentRootEpoch: number;
  /** Bumped whenever the draft set moves; shadow rows come from it. */
  draftsRevision: number;
  /**
   * GIT-FR-NQTZ / GIT-FR-OLNA: open the shell's graduation start dialog for a
   * draft. Resolves `true` once the dialog is open, and `false` where it never
   * opened because another overlay took its place first.
   */
  openGraduationStart: (draftId: string, draftName: string) => Promise<boolean>;
}

export interface GithubPollingController {
  view: GithubPollingView | null;
  /** The view could not be read; displayable text. */
  readError: string | null;
  /** A poll this window started is in flight. */
  pollInFlight: boolean;
  /** A manual Refresh was refused; displayable text. */
  refreshError: string | null;
  rowBusy: ReadonlyMap<number, ReadyTaskAction>;
  rowErrors: ReadonlyMap<number, string>;
  reload: () => Promise<void>;
  refresh: () => Promise<void>;
  claim: (issueNumber: number) => Promise<void>;
  retry: (issueNumber: number) => Promise<void>;
  graduateShadow: (draftId: string, draftName: string) => void;
  openTaskIssue: (issueNumber: number) => void;
  openShadowIssue: (draftId: string, url: string, issueNumber: number) => void;
}

export function useGithubPolling({
  active,
  projectKey,
  contentRootEpoch,
  draftsRevision,
  openGraduationStart,
}: GithubPollingOptions): GithubPollingController {
  const [view, setView] = useState<GithubPollingView | null>(null);
  const [readError, setReadError] = useState<string | null>(null);
  const [pollInFlight, setPollInFlight] = useState(false);
  const [refreshError, setRefreshError] = useState<string | null>(null);
  const [rowBusy, setRowBusy] = useState<Map<number, ReadyTaskAction>>(new Map());
  const [rowErrors, setRowErrors] = useState<Map<number, string>>(new Map());

  /**
   * GIT-FR-SLRD / GPP-FR-ELWS: the session a result belongs to. Bumped when the
   * project or the content root changes, so a read or a poll that settles after
   * the switch changes nothing on screen.
   */
  const generation = useRef(0);
  /** The generation whose poll is in flight, so two never overlap. */
  const pollGeneration = useRef<number | null>(null);
  /** Mirrors `rowBusy` for the guard against a second action on one row. */
  const busyRef = useRef<Map<number, ReadyTaskAction>>(new Map());
  /** Read at event delivery time; the subscription is made once. */
  const rootRef = useRef({ projectKey, active });
  rootRef.current = { projectKey, active };
  const openStartRef = useRef(openGraduationStart);
  openStartRef.current = openGraduationStart;

  const setBusy = (issueNumber: number, action: ReadyTaskAction | null) => {
    const next = new Map(busyRef.current);
    if (action) next.set(issueNumber, action);
    else next.delete(issueNumber);
    busyRef.current = next;
    setRowBusy(next);
  };
  const setRowError = (issueNumber: number, message: string | null) =>
    setRowErrors((current) => {
      const next = new Map(current);
      if (message) next.set(issueNumber, message);
      else next.delete(issueNumber);
      return next;
    });

  /** GIT-FR-OGHO: read the view. No network call is made. */
  const reload = useCallback(async () => {
    const g = generation.current;
    try {
      const next = await api.getGithubPollingState();
      if (g !== generation.current) return;
      setView(next ?? null);
      setReadError(null);
    } catch (e) {
      logWarn(["frontend"], "github polling state could not be read", {
        code: refusalCode(e),
      });
      if (g === generation.current) setReadError(githubPollingErrorMessage(e));
    }
  }, []);
  const reloadRef = useRef(reload);
  reloadRef.current = reload;

  /**
   * GIT-FR-CVSB / GIT-FR-EZFL: run one poll. A poll already in flight for this
   * session starts no second one.
   */
  const poll = useCallback(
    async (cause: "launch" | "timer" | "refresh") => {
      const g = generation.current;
      if (pollGeneration.current === g) return;
      pollGeneration.current = g;
      setPollInFlight(true);
      if (cause === "refresh") setRefreshError(null);
      try {
        const next = await api.pollGithubReadyTasks();
        if (g !== generation.current || !next) return;
        setView(next);
        logInfo(["frontend", "remote"], "github poll settled", {
          cause,
          tasks: next.tasks.length,
          stale: next.stale,
          errorCode: next.lastErrorCode,
        });
      } catch (e) {
        const code = refusalCode(e);
        logWarn(["frontend", "remote"], "github poll refused", {
          operation: "poll",
          cause,
          code,
        });
        if (g !== generation.current) return;
        if (cause === "refresh") setRefreshError(githubPollingErrorMessage(e));
        void reload();
      } finally {
        if (pollGeneration.current === g) pollGeneration.current = null;
        if (g === generation.current) setPollInFlight(false);
      }
    },
    [reload],
  );

  /**
   * GIT-FR-CVSB: a project opening — or its active worktree changing, which
   * starts a new polling session (GPP-FR-DATH) — reads the view and runs the
   * launch poll where a Project, an interval, and a configuration that is not
   * `invalid` permit it. GIT-FR-SLRD: closing or switching the project drops
   * everything of the previous one.
   */
  useEffect(() => {
    const g = generation.current;
    setView(null);
    setReadError(null);
    setRefreshError(null);
    setPollInFlight(false);
    busyRef.current = new Map();
    setRowBusy(new Map());
    setRowErrors(new Map());
    if (!active || !projectKey) return;
    void (async () => {
      let first: GithubPollingView;
      try {
        first = await api.getGithubPollingState();
      } catch (e) {
        logWarn(["frontend"], "github polling state could not be read", {
          code: refusalCode(e),
        });
        if (g === generation.current) setReadError(githubPollingErrorMessage(e));
        return;
      }
      if (g !== generation.current || !first) return;
      setView(first);
      if (timedPollingEnabled(first)) void poll("launch");
    })();
    return () => {
      generation.current += 1;
    };
  }, [active, projectKey, contentRootEpoch, poll]);

  /**
   * GIT-FR-CVSB / GIT-FR-SLRD: the timed poll. Rescheduled whenever the
   * interval, the Project, or the configuration's validity changes — which the
   * view learns from `"github polling changed"` — and stopped with the
   * project.
   */
  const enabled = timedPollingEnabled(view);
  const minutes = view?.settings.intervalMinutes ?? null;
  const projectNodeId = view?.settings.projectNodeId ?? null;
  useEffect(() => {
    if (!active || !projectKey || !enabled) return;
    const ms = intervalMs(minutes);
    if (ms === null) return;
    const timer = setInterval(() => void poll("timer"), ms);
    return () => clearInterval(timer);
  }, [active, projectKey, contentRootEpoch, enabled, minutes, projectNodeId, poll]);

  /**
   * GIT-FR-OGHO: every change of the view is re-read. The notification raise
   * for new issues is the main window's notification surface's
   * (`useAppNotifications`, NTF-FR-JLXL).
   */
  useEffect(() => {
    let cancelled = false;
    let unlisten: (() => void) | undefined;
    void onGithubPollingChanged(() => {
      const root = rootRef.current;
      if (!root.active || !root.projectKey) return;
      void reloadRef.current();
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  /**
   * GPP-FR-UBDE: the shadow rows are read from the draft listing and never
   * cached, so a graduation that takes or releases a shadow draft is followed.
   */
  const seenRevision = useRef(draftsRevision);
  useEffect(() => {
    if (seenRevision.current === draftsRevision) return;
    seenRevision.current = draftsRevision;
    if (active && projectKey) void reload();
  }, [draftsRevision, active, projectKey, reload]);

  const refresh = useCallback(() => poll("refresh"), [poll]);

  /**
   * GIT-FR-NQTZ / GIT-FR-FTHT: claim (or retry), open the start dialog for the
   * returned draft, and acknowledge only once the dialog is open. A refusal
   * renders on the row and opens nothing.
   */
  const runClaim = async (issueNumber: number, action: ReadyTaskAction) => {
    if (busyRef.current.has(issueNumber)) return;
    const g = generation.current;
    setBusy(issueNumber, action);
    setRowError(issueNumber, null);
    try {
      let result;
      try {
        result =
          action === "claim"
            ? await api.claimGithubTask(issueNumber)
            : await api.retryGithubClaim(issueNumber);
      } catch (e) {
        logWarn(["frontend", "remote"], "github claim refused", {
          operation: action,
          code: refusalCode(e),
          issueNumber,
        });
        if (g === generation.current)
          setRowError(issueNumber, githubPollingErrorMessage(e));
        return;
      }
      if (g !== generation.current) return;
      logInfo(["frontend"], "github task claimed", {
        operation: action,
        issueNumber,
        draftId: result.draftId,
      });
      const opened = await openStartRef.current(result.draftId, result.draftName);
      if (!opened) {
        // The claim stays pending, with its Retry, until a dialog opens.
        logWarn(["frontend"], "graduation start dialog did not open after a claim", {
          issueNumber,
        });
        return;
      }
      // GPP-FR-ELWS: a project or worktree switch while the dialog opened
      // acknowledges nothing; the pending claim stays and Retry recovers it.
      if (g !== generation.current) {
        logWarn(["frontend"], "project changed before a claim was acknowledged", {
          issueNumber,
        });
        return;
      }
      try {
        await api.acknowledgeGithubClaim(issueNumber);
      } catch (e) {
        logWarn(["frontend"], "github claim acknowledgement refused", {
          operation: "acknowledge",
          code: refusalCode(e),
          issueNumber,
        });
        if (g === generation.current)
          setRowError(issueNumber, githubPollingErrorMessage(e));
      }
    } finally {
      if (g === generation.current) setBusy(issueNumber, null);
    }
  };

  /** GIT-FR-OLNA: a shadow row's Graduate opens the dialog and claims nothing. */
  const graduateShadow = (draftId: string, draftName: string) => {
    void openStartRef.current(draftId, draftName);
  };

  /** GIT-FR-LUSE: an unclaimed row's issue link. */
  const openTaskIssue = (issueNumber: number) => {
    setRowError(issueNumber, null);
    api.openGithubTaskIssue(issueNumber).catch((e) => {
      logWarn(["frontend"], "github task issue could not be opened", {
        code: refusalCode(e),
        issueNumber,
      });
      setRowError(issueNumber, githubPollingErrorMessage(e));
    });
  };

  /** GIT-FR-OZYT: a shadow row's issue link. */
  const openShadowIssue = (draftId: string, url: string, issueNumber: number) => {
    setRowError(issueNumber, null);
    api.openPublicationIssue(draftId, url).catch((e) => {
      logWarn(["frontend"], "shadow draft issue could not be opened", {
        code: refusalCode(e),
        draftId,
      });
      setRowError(issueNumber, githubPollingErrorMessage(e));
    });
  };

  return {
    view,
    readError,
    pollInFlight,
    refreshError,
    rowBusy,
    rowErrors,
    reload,
    refresh,
    claim: (issueNumber) => runClaim(issueNumber, "claim"),
    retry: (issueNumber) => runClaim(issueNumber, "retry"),
    graduateShadow,
    openTaskIssue,
    openShadowIssue,
  };
}
