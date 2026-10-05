import { useCallback, useEffect, useRef, useState } from "react";
import * as api from "../../api";
import { logDebug, logWarn } from "../../logging";
import type { CommitFile, CommitHistory, DiffPayload } from "../../types";
import { parseRejection } from "./errors";
import { useLoad } from "./useLoad";

/**
 * GIT-FR-KPTE through GIT-FR-TFAU: the Log section's state.
 *
 * It lives in the panel's shell, not in the section, so the selected commit,
 * the selected file and the diff stay while the author looks at another
 * section (GIT-FR-11). The panel remounts when the project or the active
 * worktree changes, which discards all of it (GIT-FR-09).
 *
 * Each region — history, files, diff — has its own request slot, so one
 * region's failure leaves the others usable and a newer selection supersedes
 * the response of an older one (GIT-FR-TFAU, GIT-FR-EPSV).
 */
export function useGitLog(enabled: boolean) {
  const history = useLoad<CommitHistory>();
  const files = useLoad<CommitFile[]>();
  const diff = useLoad<DiffPayload>();
  const [commitId, setCommitId] = useState<string | null>(null);
  const [filePath, setFilePath] = useState<string | null>(null);

  const { run: runHistory } = history;
  const loadHistory = useCallback(() => {
    logDebug(["frontend", "backend"], "commit history read started");
    void runHistory(async () => {
      try {
        const read = await api.listCommitHistory();
        const commits = read?.commits ?? [];
        logDebug(["frontend"], "commit history read finished", {
          count: commits.length,
        });
        return { ...read, isDetached: read?.isDetached ?? false, commits };
      } catch (e) {
        logWarn(["frontend", "backend"], "commit history read failed", {
          code: parseRejection(e).code,
        });
        throw e;
      }
    });
  }, [runHistory]);

  // Loaded once, when the section is first shown.
  const started = useRef(false);
  useEffect(() => {
    if (!enabled || started.current) return;
    started.current = true;
    loadHistory();
  }, [enabled, loadHistory]);

  const { run: runFiles } = files;
  const { run: runDiff, reset: resetDiff } = diff;

  const loadFiles = useCallback(
    (id: string) => {
      logDebug(["frontend", "backend"], "commit files read started");
      void runFiles(async () => {
        try {
          const read = (await api.listCommitFiles(id)) ?? [];
          logDebug(["frontend"], "commit files read finished", { count: read.length });
          return read;
        } catch (e) {
          logWarn(["frontend", "backend"], "commit files read failed", {
            code: parseRejection(e).code,
          });
          throw e;
        }
      });
    },
    [runFiles],
  );

  const loadDiff = useCallback(
    (id: string, path: string) => {
      logDebug(["frontend", "backend"], "commit file diff read started");
      void runDiff(async () => {
        try {
          const read = await api.getCommitFileDiff(id, path);
          return {
            isBinary: read?.isBinary ?? false,
            hunks: read?.hunks ?? [],
          };
        } catch (e) {
          logWarn(["frontend", "backend"], "commit file diff read failed", {
            code: parseRejection(e).code,
          });
          throw e;
        }
      });
    },
    [runDiff],
  );

  /** GIT-FR-DXNC / GIT-FR-JRYS: another commit clears the file and the diff. */
  const selectCommit = useCallback(
    (id: string) => {
      if (id === commitId) return;
      setCommitId(id);
      setFilePath(null);
      resetDiff();
      loadFiles(id);
    },
    [commitId, loadFiles, resetDiff],
  );

  const selectFile = useCallback(
    (path: string) => {
      if (commitId === null) return;
      setFilePath(path);
      loadDiff(commitId, path);
    },
    [commitId, loadDiff],
  );

  const retryFiles = useCallback(() => {
    if (commitId !== null) loadFiles(commitId);
  }, [commitId, loadFiles]);
  const retryDiff = useCallback(() => {
    if (commitId !== null && filePath !== null) loadDiff(commitId, filePath);
  }, [commitId, filePath, loadDiff]);

  return {
    history: history.state,
    files: files.state,
    diff: diff.state,
    commitId,
    filePath,
    selectCommit,
    selectFile,
    retryHistory: loadHistory,
    retryFiles,
    retryDiff,
  };
}

export type GitLogController = ReturnType<typeof useGitLog>;
