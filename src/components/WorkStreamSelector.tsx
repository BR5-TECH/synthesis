/**
 * The top-chrome work-stream selector
 * (`../../specifications/ui/WSS-work-stream-selector.md`).
 *
 * A **work stream** is a named branch and working copy the application owns and
 * that lives longer than one graduation run. Runs execute in it and commit onto
 * it, so this control answers "which streams does this project hold, what is in
 * each, and which agent is busy in one?" — and it is where a stream is made,
 * merged back into the branch it came from, and removed. A merge Git settles
 * completes here at once. A merge Git cannot settle becomes a merge run, which
 * this surface shows the status of and opens in Runs (WSS-FR-OFCU,
 * WSS-FR-AWRS).
 *
 * It sits immediately after the worktree selector (WSS-FR-JVUF) because a
 * stream is a checkout of the same repository, and the author moves between the
 * two the same way.
 *
 * The component performs no Git operation and computes nothing about the
 * repository: it renders what `"list work streams"` reports, and every
 * `"work streams changed"` event reloads that listing rather than the surface
 * patching what it thinks its own act did (WSS-FR-JBYF).
 */

import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import * as api from "../api";
import { onWorkStreamUpdateProgress, onWorkStreamsChanged } from "../events";
import { logError, logInfo, logWarn } from "../logging";
import type {
  StreamMergePublication,
  StreamMergeResult,
  StreamUpdateStrategy,
  WorkStreamSummary,
} from "../types";
import { STREAM_ERRORS } from "../types";
import { Icon } from "./icons";

export interface WorkStreamSelectorProps {
  /**
   * Whether the open project's content root sits inside a Git repository.
   * Nothing renders at all when it does not (WSS-FR-JVUF).
   */
  inRepository: boolean;
  /** The branch the active worktree stands on, which pre-fills the dialog. */
  activeBranch?: string | null;
  /**
   * WSS-FR-XZRD / SNV-FR-56: called when this dropdown opens, so every other
   * floating overlay of the main window closes rather than coexisting with it.
   */
  onOpen?: () => void;
  /**
   * WSS-FR-YCAL: open a stream's working copy as the project's content root.
   * The caller owns the switch itself — flush, close every tab, invoke — so the
   * ordering has one implementation (per `WTS-worktree-selector.md` WTS-FR-22).
   */
  onActivate: (path: string) => Promise<{ ok: true } | { ok: false; error?: string }>;
  /**
   * WSS-FR-TQBN / CMW-FR-KRVP: the author chose to commit the merge, so the
   * commit-message window opens with this stream. The shell owns that window,
   * which answers with **the message alone** and closes at once; the merge is
   * this surface's to start, to show and to report (WSS-FR-HGWL).
   */
  onRequestMergeCommit: (
    streamId: string,
    streamName: string,
  ) => Promise<string | null>;
  /** WSS-FR-OMAP: route to the Changes panel, where uncommitted work is settled. */
  onOpenChanges?: () => void;
  /**
   * WSS-FR-AWRS: open the bottom panel on the Runs surface with its graduation
   * section active and this run selected — the result a `run` address has
   * (per `NTF-notifications.md` NTF-FR-17). The shell owns the navigation.
   */
  onOpenRun: (runId: string) => void;
}

import { StreamUpdateResolution, updateSubject } from "./StreamUpdateResolution";
import { refusalText, splitRefusal } from "./WorkStreamSelector/refusals";
import { NewStreamDialog } from "./WorkStreamSelector/NewStreamDialog";
import { StreamRow } from "./WorkStreamSelector/StreamRow";
import type {
  MergeOutcome,
  RowSurface,
  RunningMerge,
  RunningUpdate,
} from "./WorkStreamSelector/rows";

export { refusalText } from "./WorkStreamSelector/refusals";
export { namedPaths } from "./WorkStreamSelector/rows";

export function WorkStreamSelector({
  inRepository,
  activeBranch,
  onOpen,
  onActivate,
  onRequestMergeCommit,
  onOpenChanges,
  onOpenRun,
}: WorkStreamSelectorProps) {
  const [open, setOpen] = useState(false);
  const [streams, setStreams] = useState<WorkStreamSummary[] | null>(null);
  const [surface, setSurface] = useState<RowSurface>({ kind: "none" });
  const [creating, setCreating] = useState(false);
  const [busyRow, setBusyRow] = useState<string | null>(null);
  /** WSS-FR-HGWL: the merge calls this surface is waiting on, keyed by stream. */
  const [merging, setMerging] = useState<Record<string, RunningMerge>>({});
  /**
   * WSS-FR-KMHD / WSS-FR-PLVE: what the last settled merge call of each stream
   * answered. Kept until the dropdown closes or the author starts another merge,
   * update or delete of that stream, and never read as a record.
   */
  const [outcomes, setOutcomes] = useState<Record<string, MergeOutcome>>({});
  /** WSS-FR-BDMU: the updates this surface has running, keyed by stream. */
  const [updating, setUpdating] = useState<Record<string, RunningUpdate>>({});
  /** WSS-FR-OMAP: the complete path set a dirty side was refused with. */
  const [dirtyRefusal, setDirtyRefusal] = useState<{
    streamId: string;
    paths: string[];
  } | null>(null);
  /** WSS-FR-QSAF: the same, for an update this surface started. */
  const [updateDirty, setUpdateDirty] = useState<{
    streamId: string;
    code: string;
    paths: string[];
  } | null>(null);
  /** WSS-FR-CJYE: the stale-revision refusal, in the window it was made from. */
  const [updateStale, setUpdateStale] = useState<{
    streamId: string;
    text: string;
  } | null>(null);
  /** WSS-FR-CRJD / WSS-FR-YCAL: a refusal is rendered on the row it is about. */
  const [rowError, setRowError] = useState<{ id: string; text: string } | null>(
    null,
  );
  /** WSS-FR-KDVU: the stream whose update resolution window stands open. */
  const [resolvingStream, setResolvingStream] = useState<string | null>(null);
  /** True while a call the resolution window made is still out. */
  const [resolving, setResolving] = useState(false);
  /** WSS-FR-NPXC: a refusal of a resolution action, in the window it was made from. */
  const [resolveError, setResolveError] = useState<string | null>(null);
  const rootRef = useRef<HTMLDivElement | null>(null);
  /** Whether this surface is still mounted, for the merges it is waiting on. */
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  /** Which listing request is the newest, so a late older answer is dropped. */
  const listingSeq = useRef(0);

  const reload = useCallback(() => {
    if (!inRepository) {
      setStreams(null);
      return;
    }
    const mine = ++listingSeq.current;
    api
      .listWorkStreams()
      .then((list) => {
        // A merge run moves through many states, each of which asks for a
        // listing, so an older answer that lands late must not overwrite a newer.
        if (mounted.current && mine === listingSeq.current) setStreams(list);
      })
      .catch((reason) => {
        // A project outside a repository answers the typed refusal rather than
        // an empty list, and either way there is nothing to list.
        logError(["frontend"], "work streams could not be listed", {
          reason: String(reason),
        });
        setStreams([]);
      });
  }, [inRepository]);

  useEffect(reload, [reload]);

  // WSS-FR-JBYF: every event reloads the listing, whoever caused it.
  useEffect(() => {
    const pending = onWorkStreamsChanged(() => reload());
    return () => {
      void pending.then((un) => un());
    };
  }, [reload]);

  // WSS-FR-BDMU / WSS-FR-FVKO: a running update reports which turn it has
  // reached, whichever window started it. An update belongs
  // to the stream rather than to the window that asked for it, so a report for a
  // stream this surface is not already tracking starts tracking it.
  useEffect(() => {
    const pending = onWorkStreamUpdateProgress((report) => {
      if (!mounted.current || report.turn === 0) return;
      setUpdating((live) => ({
        ...live,
        [report.streamId]: {
          streamName: live[report.streamId]?.streamName ?? "",
          progress: report,
        },
      }));
    });
    return () => {
      void pending.then((un) => un());
    };
  }, []);

  // WSS-FR-XZRD: a click outside closes the dropdown, and so does Escape.
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (!rootRef.current?.contains(e.target as Node)) close();
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") close();
    };
    window.addEventListener("mousedown", onDown);
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("mousedown", onDown);
      window.removeEventListener("keydown", onKey);
    };
  }, [open]);

  const close = () => {
    setOpen(false);
    setSurface({ kind: "none" });
    setRowError(null);
    setDirtyRefusal(null);
    setUpdateDirty(null);
    setUpdateStale(null);
    // WSS-FR-KMHD: a settled merge's answer stays until the dropdown closes.
    setOutcomes({});
    // `merging` and `updating` are deliberately kept: each outlives the dropdown
    // it was started from, and reopening must show it still running
    // (WSS-FR-HGWL, WSS-FR-BDMU).
  };

  const live = streams ?? [];
  /**
   * Put the update resolution window down, and give the keyboard back.
   *
   * The dropdown closed as the window opened (WSS-FR-KDVU), so the row that
   * opened it no longer exists to take the focus. The control that opens the
   * dropdown does, and it is where the author's next act starts.
   */
  const closeResolution = () => {
    setResolvingStream(null);
    setResolveError(null);
    rootRef.current
      ?.querySelector<HTMLElement>('[data-testid="stream-selector"]')
      ?.focus();
  };

  /**
   * WSS-FR-KDVU / WSS-FR-FVKO: the stream and update record the window is
   * about, read from the listing so the window redraws from the record on every
   * reload. A record that has gone closes the window with it.
   */
  const resolutionFor = (() => {
    if (!resolvingStream) return null;
    const summary = live.find((s) => s.stream.id === resolvingStream);
    if (!summary?.update) return null;
    return { summary, subject: updateSubject(summary.update) };
  })();
  const busyCount = live.filter((s) => s.stream.busyRunId).length;

  /** WSS-FR-DGAG: how many streams, and whether one is busy. */
  const label = useMemo(() => {
    const count = `${live.length} stream${live.length === 1 ? "" : "s"}`;
    return busyCount > 0 ? `${count} · ${busyCount} busy` : count;
  }, [live.length, busyCount]);

  if (!inRepository) return null;

  /**
   * WSS-FR-HGWL: start a merge and show it while the call is out.
   *
   * The call answers once Git has settled the merge: at once for a merge Git
   * settles, and after the handoff for one it cannot (WKS-FR-QNHF). The row
   * carries the running state for the length of the call alone, and the call's
   * answer for as long as the dropdown stays open. What a conflict became — the
   * merge run — is read from the listing, never from this answer.
   */
  const startMerge = (
    summary: WorkStreamSummary,
    publication: StreamMergePublication,
  ) => {
    const id = summary.stream.id;
    // WSS-FR-XRHT: one reconciliation of a stream is startable at a time.
    // Without this a second start would replace the running call's entry and
    // then delete it when it was refused.
    if (merging[id] || updating[id]) return;
    setSurface({ kind: "none" });
    setRowError(null);
    setDirtyRefusal(null);
    setOutcomes((held) => withoutKey(held, id));
    setMerging((live) => ({
      ...live,
      [id]: { streamName: summary.stream.name },
    }));
    logInfo(["frontend"], "work stream merge started", {
      streamId: id,
      publication: publication.kind,
    });
    api
      .mergeWorkStream(id, publication)
      .then((result) => {
        if (!mounted.current) return;
        settleMerge(id);
        acceptMergeResult(id, result);
      })
      .catch((reason) => {
        if (!mounted.current) return;
        settleMerge(id);
        reportRefusal(summary, reason);
      });
  };

  /**
   * WSS-FR-KMHD / WSS-FR-PLVE: keep what the call answered on the row.
   *
   * A clean merge and a stream that held nothing make no run. A conflict made
   * one, and the listing the settle reloaded names it.
   */
  const acceptMergeResult = (streamId: string, result: StreamMergeResult) => {
    logInfo(["frontend"], "work stream merge answered", {
      streamId,
      kind: result.kind,
      paths:
        result.kind === "merged"
          ? result.mergedPaths.length
          : result.kind === "conflicted"
            ? result.conflictedPaths.length
            : 0,
    });
    setOutcomes((held) => ({ ...held, [streamId]: result }));
  };

  /**
   * WSS-FR-TQBN / SNV-FR-56: take the message, then start the merge.
   *
   * The dropdown closes first. The commit message window is a modal overlay of
   * the main window (per `CMW-commit-message.md` CMW-FR-01), and a dropdown
   * left open paints over its scrim — covering the window's own heading and
   * putting stream rows in front of it as click targets.
   */
  const startMergeWithMessage = (summary: WorkStreamSummary) => {
    const { id, name } = summary.stream;
    close();
    void onRequestMergeCommit(id, name).then((message) => {
      // CMW-FR-09: a dismissed window starts nothing.
      if (!mounted.current || message === null) return;
      startMerge(summary, { kind: "commit", message });
    });
  };

  const settleMerge = (streamId: string) => {
    // A merge outlives the surface that started it, so an answer arriving after
    // this surface has gone reloads nothing and sets no state.
    if (!mounted.current) return;
    setMerging((live) => {
      const next = { ...live };
      delete next[streamId];
      return next;
    });
    reload();
  };

  /**
   * WSS-FR-PMYA: drive an update that stopped.
   *
   * Each of these answers with the update's own record, so the row and the
   * window both redraw from what the backend holds rather than from what this
   * surface attempted (WSS-FR-FVKO).
   */
  const driveUpdate = (streamId: string, call: () => Promise<unknown>) => {
    setResolving(true);
    setResolveError(null);
    // The promise is handed back rather than swallowed: the escalation form
    // reads it to decide whether the author's answers were accepted, and a set
    // reported as accepted when it was refused loses everything they typed
    // (per `GEA-graduation-escalation-answering.md` GEA-FR-TEMG).
    return call()
      .then(() => {
        if (mounted.current) reload();
      })
      .catch((reason) => {
        const text = String(reason instanceof Error ? reason.message : reason);
        logWarn(["frontend"], "work stream update action refused", {
          streamId,
          reason: splitRefusal(text)[0] || "unknown",
        });
        if (mounted.current) setResolveError(refusalText(text));
        throw reason;
      })
      .finally(() => {
        if (mounted.current) setResolving(false);
      });
  };

  /** WSS-FR-NPXC: every typed refusal renders on the stream's own row. */
  const reportRefusal = (summary: WorkStreamSummary, reason: unknown) => {
    const id = summary.stream.id;
    const text = String(reason instanceof Error ? reason.message : reason);
    const [code, detail] = splitRefusal(text);
    logWarn(["frontend"], "work stream merge refused", {
      streamId: id,
      reason: code || "unknown",
    });
    // WSS-FR-OMAP: an uncommitted side is settled in the Changes panel, so the
    // paths are named and the author is routed there — this surface neither
    // commits nor discards their work.
    if (code === STREAM_ERRORS.dirty || code === STREAM_ERRORS.baseDirty) {
      setSurface({ kind: "merge", streamId: id });
      setDirtyRefusal({ streamId: id, paths: detail ? detail.split(", ") : [] });
      setOpen(true);
      return;
    }
    // GRB-FR-MWTC: a cancellation reads as what the author did rather than as a
    // failure they did not cause; `refusalText` carries that wording with every
    // other typed refusal.
    setRowError({ id, text: refusalText(text) });
  };

  /**
   * WSS-FR-WPGR: start an update with the revision the window displayed.
   *
   * The call is not awaited by anything the author is looking at: an update may
   * spend three semantic turns, so the window closes at once and the row
   * carries the state from here on.
   */
  const startUpdate = (
    summary: WorkStreamSummary,
    strategy: StreamUpdateStrategy,
  ) => {
    const id = summary.stream.id;
    // WSS-FR-XRHT: one reconciliation of a stream is startable at a time.
    if (updating[id] || merging[id]) return;
    setSurface({ kind: "none" });
    setRowError(null);
    setOutcomes((held) => withoutKey(held, id));
    setUpdateDirty(null);
    setUpdateStale(null);
    setUpdating((live) => ({
      ...live,
      [id]: { streamName: summary.stream.name, progress: null },
    }));
    logInfo(["frontend"], "work stream update started", {
      streamId: id,
      strategy,
    });
    // WKS-FR-MJEB: the call answers at once with the update's record. What the
    // update settles reaches this surface through `"work streams changed"`.
    api
      .updateWorkStream(id, strategy, summary.baseTipRevision)
      .then(() => {
        if (!mounted.current) return;
        settleUpdate(id);
      })
      .catch((reason) => {
        if (!mounted.current) return;
        settleUpdate(id);
        reportUpdateRefusal(summary, reason);
      });
  };

  const settleUpdate = (streamId: string) => {
    if (!mounted.current) return;
    setUpdating((live) => {
      const next = { ...live };
      delete next[streamId];
      return next;
    });
    reload();
  };

  /** WSS-FR-ZWCB: every typed refusal of an update renders on its own row. */
  const reportUpdateRefusal = (summary: WorkStreamSummary, reason: unknown) => {
    const id = summary.stream.id;
    const text = String(reason instanceof Error ? reason.message : reason);
    const [code, detail] = splitRefusal(text);
    logWarn(["frontend"], "work stream update refused", {
      streamId: id,
      reason: code || "unknown",
    });
    // WSS-FR-QSAF: an uncommitted worktree is settled in the Changes panel, and
    // the refusal names which of the two worktrees holds that work.
    if (
      code === STREAM_ERRORS.dirty ||
      code === STREAM_ERRORS.baseDirty ||
      code === STREAM_ERRORS.baseNotCheckedOut
    ) {
      setSurface({ kind: "update", streamId: id });
      setUpdateDirty({
        streamId: id,
        code,
        paths: detail ? detail.split(", ") : [],
      });
      setOpen(true);
      return;
    }
    // WSS-FR-CJYE: a base branch that moved is told in the window that read it,
    // and the author reopens Update for a new revision. Nothing is re-run here.
    if (code === STREAM_ERRORS.staleBaseRevision) {
      setSurface({ kind: "update", streamId: id });
      setUpdateStale({ streamId: id, text: refusalText(text) });
      setOpen(true);
      return;
    }
    setRowError({ id, text: refusalText(text) });
  };

  /** WSS-FR-BDMU: stop an update that is running. */
  const cancelUpdate = (streamId: string) => {
    logInfo(["frontend"], "work stream update cancellation requested", {
      streamId,
    });
    void api.cancelWorkStreamUpdate(streamId).catch((reason) => {
      // The typed code alone, as every other refusal of this surface is logged:
      // a raw error may carry a path of the author's own.
      logWarn(["frontend"], "work stream update cancellation failed", {
        streamId,
        reason: splitRefusal(String(reason))[0] || "unknown",
      });
    });
  };

  const activate = async (summary: WorkStreamSummary) => {
    // WSS-FR-YCAL / WSS-FR-HGWL: a busy stream is not selectable, and neither
    // is one whose merge call is out. The row already says so, so this is the
    // guard rather than the explanation.
    if (summary.stream.busyRunId || summary.stream.isMissing) return;
    if (merging[summary.stream.id] || updating[summary.stream.id]) return;
    setBusyRow(summary.stream.id);
    const outcome = await onActivate(summary.stream.worktreePath);
    setBusyRow(null);
    if (outcome.ok) {
      close();
      return;
    }
    if (outcome.error) {
      setRowError({ id: summary.stream.id, text: refusalText(outcome.error) });
    }
  };

  return (
    <div className="stream-select" ref={rootRef}>
      <button
        type="button"
        className="stream-select__control"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-label={`Work streams: ${label}`}
        data-testid="stream-selector"
        onClick={() => {
          if (open) {
            close();
            return;
          }
          // WSS-FR-XZRD: opening closes every other floating overlay.
          onOpen?.();
          reload();
          setOpen(true);
        }}
      >
        <Icon.Graduate size={12} />
        <span className="stream-select__label">{label}</span>
      </button>

      {open && (
        <div className="stream-select__menu menu" role="menu" data-testid="stream-menu">
          {/* WSS-FR-SOAS: the listing scrolls inside its own region. */}
          <div className="stream-select__list" data-testid="stream-list">
            {live.length === 0 && (
              <p className="stream-select__empty t-muted">
                This project holds no work stream yet.
              </p>
            )}
            {live.map((summary) => (
              <StreamRow
                key={summary.stream.id}
                summary={summary}
                surface={surface}
                busy={busyRow === summary.stream.id}
                error={
                  rowError?.id === summary.stream.id ? rowError.text : null
                }
                merge={merging[summary.stream.id] ?? null}
                mergeOutcome={outcomes[summary.stream.id] ?? null}
                update={updating[summary.stream.id] ?? null}
                dirty={
                  dirtyRefusal?.streamId === summary.stream.id
                    ? dirtyRefusal.paths
                    : null
                }
                updateDirty={
                  updateDirty?.streamId === summary.stream.id
                    ? { code: updateDirty.code, paths: updateDirty.paths }
                    : null
                }
                updateStale={
                  updateStale?.streamId === summary.stream.id
                    ? updateStale.text
                    : null
                }
                onActivate={() => void activate(summary)}
                onOpenSurface={(next) => {
                  setRowError(null);
                  setDirtyRefusal(null);
                  setUpdateDirty(null);
                  setUpdateStale(null);
                  setSurface(next);
                }}
                onStartMerge={startMerge}
                onStartMergeWithMessage={startMergeWithMessage}
                onStartUpdate={startUpdate}
                onCancelUpdate={cancelUpdate}
                onOpenRun={(runId) => {
                  // WSS-FR-AWRS: the dropdown closes, and the run opens in
                  // Runs. The surface acts on no run itself (WSS-FR-NRCQ).
                  logInfo(["frontend"], "a merge run was opened in Runs", {
                    streamId: summary.stream.id,
                  });
                  close();
                  onOpenRun(runId);
                }}
                onResolveUpdate={() => {
                  setResolveError(null);
                  close();
                  setResolvingStream(summary.stream.id);
                }}
                onRefused={(text) => {
                  // WSS-FR-KMHD: starting a delete of the stream ends the answer
                  // of its last merge call.
                  setOutcomes((held) => withoutKey(held, summary.stream.id));
                  setRowError({ id: summary.stream.id, text });
                }}
                onSettled={() => {
                  setOutcomes((held) => withoutKey(held, summary.stream.id));
                  setSurface({ kind: "none" });
                  reload();
                }}
                onOpenChanges={() => {
                  close();
                  onOpenChanges?.();
                }}
              />
            ))}
          </div>

          {/* WSS-FR-PDFX: outside the scrolling region, so it is reachable
              however long the listing is. */}
          <div className="menu-sep" />
          <button
            type="button"
            role="menuitem"
            className="btn btn--ghost btn--sm"
            data-testid="stream-new"
            onClick={() => {
              setOpen(false);
              setCreating(true);
            }}
          >
            <Icon.Plus size={12} /> New stream…
          </button>
        </div>
      )}

      {/* WSS-FR-KDVU / WSS-FR-ZMPC: the update resolution window. It stands
          outside the dropdown, so putting the dropdown down leaves it open and
          the author can read the files the update could not settle.
          WSS-FR-PMYA: every act reaches the update operations alone. */}
      {resolutionFor && (
        <StreamUpdateResolution
          streamName={resolutionFor.summary.stream.name}
          streamBranch={resolutionFor.summary.stream.branch}
          subject={resolutionFor.subject}
          busy={resolving}
          onClose={closeResolution}
          onSend={(answers) =>
            driveUpdate(resolutionFor.subject.streamId, () =>
              api.answerWorkStreamUpdateEscalation(
                resolutionFor.subject.streamId,
                answers,
              ),
            )
          }
          onRetry={() => {
            // The refusal is rendered by `driveUpdate`; nothing waits on the
            // rejection here, so it is settled rather than left unhandled.
            void driveUpdate(resolutionFor.subject.streamId, () =>
              api.retryWorkStreamUpdate(resolutionFor.subject.streamId),
            ).catch(() => {});
          }}
          onCancelUpdate={() => cancelUpdate(resolutionFor.subject.streamId)}
          onDismissUpdate={() => {
            void driveUpdate(resolutionFor.subject.streamId, () =>
              api.clearWorkStreamUpdate(resolutionFor.subject.streamId),
            )
              .then(() => closeResolution())
              // A dismissal the backend refused leaves the window standing with
              // the refusal on it, rather than closing over a record that is
              // still there.
              .catch(() => {});
          }}
          onError={setResolveError}
          error={resolveError}
        />
      )}

      {creating && (
        <NewStreamDialog
          activeBranch={activeBranch ?? ""}
          onClose={() => setCreating(false)}
          onCreated={() => {
            setCreating(false);
            reload();
          }}
        />
      )}
    </div>
  );
}


/** A copy of a keyed record without one key. */
function withoutKey<T>(held: Record<string, T>, key: string): Record<string, T> {
  if (!(key in held)) return held;
  const next = { ...held };
  delete next[key];
  return next;
}
