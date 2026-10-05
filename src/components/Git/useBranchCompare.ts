import { useCallback, useEffect, useState } from "react";
import * as api from "../../api";
import { logDebug, logWarn } from "../../logging";
import type { BranchComparison, CommitFile, DiffPayload } from "../../types";
import { parseRejection } from "./errors";
import { useLoad } from "./useLoad";

/** A branch row the author selected (GIT-FR-LNEI). */
export interface BranchRef {
  name: string;
  kind: "local" | "remote";
}

/**
 * GIT-FR-GDMG, GIT-FR-SION: the Branches section's large view state.
 *
 * Selecting a branch reads what it changed against its base, and selecting a
 * file reads that file's diff. The file list and the diff have their own
 * request slots, so a failure in one leaves the other usable, and a newer
 * selection supersedes the response of an older one. Another branch clears the
 * file and the diff.
 */
export function useBranchCompare(branch: BranchRef | null) {
  const comparison = useLoad<BranchComparison>();
  const diff = useLoad<DiffPayload>();
  const [filePath, setFilePath] = useState<string | null>(null);

  const { run: runComparison, reset: resetComparison } = comparison;
  const { run: runDiff, reset: resetDiff } = diff;

  const loadComparison = useCallback(
    (target: BranchRef) => {
      logDebug(["frontend", "backend"], "branch comparison read started", {
        kind: target.kind,
      });
      void runComparison(async () => {
        try {
          const read = await api.listBranchCompareFiles(target.name, target.kind);
          const files = read?.files ?? [];
          logDebug(["frontend"], "branch comparison read finished", {
            count: files.length,
            sameAsBase: read?.sameAsBase ?? false,
          });
          return { ...read, sameAsBase: read?.sameAsBase ?? false, files };
        } catch (e) {
          logWarn(["frontend", "backend"], "branch comparison read failed", {
            code: parseRejection(e).code,
          });
          throw e;
        }
      });
    },
    [runComparison],
  );

  const loadDiff = useCallback(
    (target: BranchRef, path: string) => {
      logDebug(["frontend", "backend"], "branch comparison diff read started");
      void runDiff(async () => {
        try {
          const read = await api.getBranchCompareFileDiff(target.name, target.kind, path);
          return { isBinary: read?.isBinary ?? false, hunks: read?.hunks ?? [] };
        } catch (e) {
          logWarn(["frontend", "backend"], "branch comparison diff read failed", {
            code: parseRejection(e).code,
          });
          throw e;
        }
      });
    },
    [runDiff],
  );

  const key = branch ? `${branch.kind}:${branch.name}` : null;
  useEffect(() => {
    setFilePath(null);
    resetDiff();
    if (branch === null) {
      resetComparison();
      return;
    }
    loadComparison(branch);
    // The selection is identified by its key.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key]);

  const selectFile = useCallback(
    (path: string) => {
      if (branch === null) return;
      setFilePath(path);
      loadDiff(branch, path);
    },
    [branch, loadDiff],
  );

  const retryFiles = useCallback(() => {
    if (branch !== null) loadComparison(branch);
  }, [branch, loadComparison]);
  const retryDiff = useCallback(() => {
    if (branch !== null && filePath !== null) loadDiff(branch, filePath);
  }, [branch, filePath, loadDiff]);

  // The file list is the comparison's files, in the slot shape the view reads.
  const files = {
    ...comparison.state,
    data: (comparison.state.data?.files ?? null) as CommitFile[] | null,
  };

  return {
    comparison: comparison.state,
    files,
    diff: diff.state,
    filePath,
    selectFile,
    retryFiles,
    retryDiff,
  };
}

export type BranchCompareController = ReturnType<typeof useBranchCompare>;
