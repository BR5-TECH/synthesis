/**
 * The History of a change on a stream
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-RKCC,
 * OCW-FR-DRQD, OCW-FR-QCSL).
 *
 * The list of versions is read when History opens, on its Refresh, and on
 * `"change edit proposals changed"` and `"new change proposals changed"` with
 * the cause `committed` or `decided` for this stream and change. A version is
 * read when the author selects it. Nothing is read while History is closed.
 */
import { useCallback, useEffect, useRef, useState } from "react";

import * as api from "../../api";
import { onChangeEditProposalsChanged, onNewChangeProposalsChanged } from "../../events";
import { logDebug, logWarn } from "../../logging";
import type { ChangeVersion, ChangeVersionFile, ChangeVersionList } from "../../types";
import { refusalCode } from "./decisions";

/** OCW-FR-QCSL: the causes that can make a version. */
const MAKES_A_VERSION = new Set(["committed", "decided"]);

/** OCW-FR-DRQD: the version on show, and what its read gave. */
export interface ShownVersion {
  version: ChangeVersion;
  /** Null until the read answers. */
  files: ChangeVersionFile[] | null;
  error: string | null;
}

export interface ChangeHistory {
  list: ChangeVersionList | null;
  error: string | null;
  reading: boolean;
  refresh: () => void;
  shown: ShownVersion | null;
  show: (version: ChangeVersion) => void;
  back: () => void;
}

/** OCW-FR-RKCC / OCW-FR-DRQD: a refusal of a History read, in words. */
export function historyMessage(code: string): string {
  if (code.includes("unknown_version")) return "This version is no longer on the stream branch.";
  if (code.includes("change_not_found")) return "The stream branch holds no version of this change.";
  if (code.includes("unknown_stream")) return "The work stream is gone, so its History cannot be read.";
  return "The History could not be read.";
}

/** Subscribe to one event while mounted; a refused subscription is logged. */
function useFollow(
  subscribe: () => Promise<() => void>,
  active: boolean,
  what: string,
): void {
  useEffect(() => {
    if (!active) return;
    let stop: (() => void) | null = null;
    let cancelled = false;
    subscribe()
      .then((fn) => {
        if (cancelled) fn();
        else stop = fn;
      })
      .catch((e: unknown) => {
        logWarn(["frontend"], `could not follow ${what}`, { error: refusalCode(e).slice(0, 80) });
      });
    return () => {
      cancelled = true;
      stop?.();
    };
  }, [subscribe, active, what]);
}

export function useChangeHistory(streamId: string, change: string, open: boolean): ChangeHistory {
  const [list, setList] = useState<ChangeVersionList | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [reading, setReading] = useState(false);
  const [shown, setShown] = useState<ShownVersion | null>(null);
  const seq = useRef(0);
  const versionSeq = useRef(0);

  const read = useCallback(() => {
    const mine = ++seq.current;
    setReading(true);
    api.listChangeVersions(streamId, change).then(
      (answer) => {
        if (mine !== seq.current) return;
        setReading(false);
        setError(null);
        setList({
          versions: Array.isArray(answer?.versions) ? answer.versions : [],
          truncated: Boolean(answer?.truncated),
        });
        logDebug(["frontend"], "the versions of a change were listed", {
          streamId,
          versions: answer?.versions?.length ?? 0,
          truncated: Boolean(answer?.truncated),
        });
      },
      (e: unknown) => {
        if (mine !== seq.current) return;
        const code = refusalCode(e);
        setReading(false);
        setError(historyMessage(code));
        logWarn(["frontend"], "the versions of a change could not be listed", {
          streamId,
          code: code.slice(0, 80),
        });
      },
    );
  }, [streamId, change]);

  // OCW-FR-QCSL: read when History opens.
  useEffect(() => {
    if (open) read();
  }, [open, read]);

  const followEdits = useCallback(
    () =>
      onChangeEditProposalsChanged((payload) => {
        if (payload.streamId !== streamId || payload.change !== change) return;
        if (MAKES_A_VERSION.has(payload.cause)) read();
      }),
    [streamId, change, read],
  );
  const followSplit = useCallback(
    () =>
      onNewChangeProposalsChanged((payload) => {
        if (payload.streamId !== streamId || payload.change !== change) return;
        if (MAKES_A_VERSION.has(payload.cause)) read();
      }),
    [streamId, change, read],
  );
  useFollow(followEdits, open, "the edit proposals of a change for its History");
  useFollow(followSplit, open, "the split of a change for its History");

  /** OCW-FR-DRQD: show one version, from its own read. */
  const show = useCallback(
    (version: ChangeVersion) => {
      const mine = ++versionSeq.current;
      setShown({ version, files: null, error: null });
      api.readChangeVersion(streamId, change, version.commitId).then(
        (answer) => {
          if (mine !== versionSeq.current) return;
          setShown({ version, files: Array.isArray(answer?.files) ? answer.files : [], error: null });
          logDebug(["frontend"], "a version of a change was read", {
            streamId,
            files: answer?.files?.length ?? 0,
          });
        },
        (e: unknown) => {
          if (mine !== versionSeq.current) return;
          const code = refusalCode(e);
          setShown({ version, files: null, error: historyMessage(code) });
          logWarn(["frontend"], "a version of a change could not be read", {
            streamId,
            code: code.slice(0, 80),
          });
        },
      );
    },
    [streamId, change],
  );

  const back = useCallback(() => {
    versionSeq.current += 1;
    setShown(null);
  }, []);

  // Closing History shows the current change again.
  useEffect(() => {
    if (!open) back();
  }, [open, back]);

  return { list, error, reading, refresh: read, shown, show, back };
}
