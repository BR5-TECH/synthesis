/**
 * Confirming the removal of a work stream
 * (`../../../specifications/ui/WSS-work-stream-selector.md` WSS-FR-UFZP,
 * WSS-FR-BRMT, WSS-FR-NHCV, WSS-FR-PKQD).
 *
 * For a stream holding commits its base branch does not, the confirmation
 * says how many and offers to merge instead or to delete anyway. **Delete
 * anyway** goes on to the same path check and passes `force`. The
 * confirmation first reads the stream's uncommitted paths. It
 * warns where there are any, and it names the discard on its confirm action.
 * It carries no way to remove a stream that a run holds, that has a
 * non-terminal run, or that is the active worktree.
 */

import { useEffect, useRef, useState } from "react";

import * as api from "../../api";
import { logInfo, logWarn } from "../../logging";
import { STREAM_ERRORS, type WorkStreamSummary } from "../../types";
import { refusalText, splitRefusal } from "./refusals";
import { namedPaths } from "./rows";

/**
 * WSS-FR-RWLB: focus the first button of a confirmation step when it mounts.
 * The step replaces the control that opened it, so without this the focus
 * falls to the page body.
 */
function useFirstControlFocus() {
  const ref = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    ref.current?.querySelector<HTMLElement>("button")?.focus();
  }, []);
  return ref;
}

type PathRead =
  | { state: "loading" }
  | { state: "failed"; message: string }
  | { state: "ready"; paths: string[] };

export function DeleteConfirm({
  summary,
  onCancel,
  onRefused,
  onSettled,
  onMergeInstead,
}: {
  summary: WorkStreamSummary;
  onCancel: () => void;
  onRefused: (text: string) => void;
  onSettled: () => void;
  onMergeInstead: () => void;
}) {
  const { stream, aheadOfBase } = summary;
  // WSS-FR-UFZP: the author chose to lose the unmerged commits.
  const [anyway, setAnyway] = useState(false);
  const warningRef = useFirstControlFocus();

  // WSS-FR-UFZP: a stream carrying commits its base does not hold is warned
  // about first. Merging keeps them; deleting anyway passes `force`.
  if (aheadOfBase > 0 && !anyway) {
    return (
      <div className="stream-select__confirm" role="group" ref={warningRef}>
        <p role="alert">
          {stream.name} holds {aheadOfBase} commit
          {aheadOfBase === 1 ? "" : "s"} that {stream.baseBranch} does not.
          Deleting it would lose them.
        </p>
        <div className="modal__actions">
          <button className="btn btn--ghost btn--sm" onClick={onCancel}>
            Cancel
          </button>
          <button className="btn btn--default btn--sm" onClick={onMergeInstead}>
            Merge instead
          </button>
          <button
            className="btn btn--danger btn--sm"
            data-testid={`stream-delete-anyway-${stream.id}`}
            onClick={() => {
              logInfo(["frontend"], "work stream delete anyway chosen", {
                streamId: stream.id,
                aheadOfBase,
              });
              setAnyway(true);
            }}
          >
            Delete anyway
          </button>
        </div>
      </div>
    );
  }

  return (
    <DeleteWithDiscardCheck
      summary={summary}
      force={anyway}
      onCancel={onCancel}
      onRefused={onRefused}
      onSettled={onSettled}
    />
  );
}

function DeleteWithDiscardCheck({
  summary,
  force,
  onCancel,
  onRefused,
  onSettled,
}: {
  summary: WorkStreamSummary;
  /** WSS-FR-UFZP: the author chose **Delete anyway**. */
  force: boolean;
  onCancel: () => void;
  onRefused: (text: string) => void;
  onSettled: () => void;
}) {
  const { stream } = summary;
  const streamId = stream.id;
  const [running, setRunning] = useState(false);
  const [read, setRead] = useState<PathRead>({ state: "loading" });
  const [attempt, setAttempt] = useState(0);
  // WSS-FR-PKQD: the paths a `stream_dirty` refusal carried. They replace what
  // the confirmation read, because the working copy changed since then.
  const [refusedPaths, setRefusedPaths] = useState<string[] | null>(null);
  const mounted = useRef(true);
  // WSS-FR-RWLB: the step takes focus when it opens, so the control that
  // opened it going away leaves no one on the page body.
  const stepRef = useFirstControlFocus();

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  // WSS-FR-BRMT: opening the confirmation reads the uncommitted paths.
  useEffect(() => {
    let current = true;
    setRead({ state: "loading" });
    api
      .getWorkStreamUncommittedPaths(streamId)
      .then((paths) => {
        if (!current) return;
        logInfo(["frontend"], "work stream uncommitted paths read", {
          streamId,
          count: paths.length,
        });
        setRead({ state: "ready", paths });
      })
      .catch((reason) => {
        if (!current) return;
        const text = String(reason instanceof Error ? reason.message : reason);
        logWarn(["frontend"], "work stream uncommitted paths read failed", {
          streamId,
          reason: splitRefusal(text)[0] || "unknown",
        });
        setRead({ state: "failed", message: refusalText(text) });
      });
    return () => {
      current = false;
    };
  }, [streamId, attempt]);

  const paths =
    refusedPaths ?? (read.state === "ready" ? read.paths : null);
  // WSS-FR-NHCV: a refusal that named no path still means work would be lost.
  const discards = refusedPaths !== null || (paths !== null && paths.length > 0);
  const ready = refusedPaths !== null || read.state === "ready";

  const confirm = () => {
    setRunning(true);
    logInfo(["frontend"], "work stream delete confirmed", {
      streamId,
      discardUncommitted: discards,
      force,
    });
    api
      .deleteWorkStream(streamId, force, discards)
      .then(() => {
        if (!mounted.current) return;
        setRunning(false);
        onSettled();
      })
      .catch((reason) => {
        if (!mounted.current) return;
        setRunning(false);
        const text = String(reason instanceof Error ? reason.message : reason);
        const [code, detail] = splitRefusal(text);
        logWarn(["frontend"], "work stream delete refused", {
          streamId,
          reason: code || "unknown",
        });
        // WSS-FR-PKQD: a dirty refusal shows the discard warning again, with
        // the paths it carries, and lets the author confirm.
        if (code === STREAM_ERRORS.dirty) {
          setRefusedPaths(detail ? detail.split(", ") : []);
          return;
        }
        onRefused(refusalText(text));
      });
  };

  return (
    <div
      className="stream-select__confirm"
      role="group"
      aria-busy={!ready}
      ref={stepRef}
    >
      <p>
        Delete {stream.name}? Its branch and its working copy are removed. Its
        runs stay in the project's history.
        {force && (
          <>
            {" "}
            The {summary.aheadOfBase === 1 ? "commit" : "commits"} that{" "}
            {stream.baseBranch} does not hold are lost.
          </>
        )}
      </p>

      {read.state === "loading" && refusedPaths === null && (
        <p className="t-muted" role="status">
          Checking for uncommitted changes…
        </p>
      )}
      {read.state === "failed" && refusedPaths === null && (
        <>
          <p role="alert">
            Could not check for uncommitted changes. {read.message}
          </p>
          <div className="modal__actions">
            <button
              type="button"
              className="btn btn--ghost btn--sm"
              onClick={() => setAttempt((n) => n + 1)}
            >
              Retry
            </button>
          </div>
        </>
      )}
      {discards && paths && (
        <p role="alert" data-testid={`stream-delete-discard-${stream.id}`}>
          Deleting {stream.name} discards{" "}
          {paths.length > 0
            ? `${paths.length} uncommitted change${paths.length === 1 ? "" : "s"}`
            : "its uncommitted changes"}{" "}
          in its working copy.
          {paths.length > 0 && (
            <>
              {" "}
              <span title={paths.join(", ")}>{namedPaths(paths)}</span>
            </>
          )}
        </p>
      )}

      <div className="modal__actions">
        <button
          className="btn btn--ghost btn--sm"
          disabled={running}
          onClick={onCancel}
        >
          Cancel
        </button>
        <button
          className="btn btn--default btn--sm"
          disabled={running || !ready}
          onClick={confirm}
        >
          {discards ? "Discard changes and delete" : "Delete"}
        </button>
      </div>
    </div>
  );
}

/** WSS-FR-XZRO / WSS-FR-CRJD: exactly two inputs, and nothing else. */
