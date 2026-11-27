import { useCallback } from "react";
import * as api from "../../api";
import { logInfo, logWarn } from "../../logging";
import { errorCode, upsertTerminal } from "../../state/terminals";
import { failureText } from "../Terminals/NewTerminalMenu";
import type { TerminalSummary, TreeNode } from "../../types";

/**
 * LCM-FR-PKIH: the folder the terminal entries use, relative to the worktree
 * root: the folder itself, or the folder that holds the file. The tree root is
 * `""`.
 */
export function terminalFolderOf(node: TreeNode): string {
  if (node.nodeKind === "folder") return node.path;
  const cut = node.path.lastIndexOf("/");
  return cut < 0 ? "" : node.path.slice(0, cut);
}

/** LCM-FR-WNPH: the text of a failed outside-app hand-off. */
function handOffText(err: unknown): string {
  const text = String(err);
  if (text.includes(api.ERR_NO_TERMINAL_FOUND)) return "No terminal application was found.";
  if (text.includes(api.ERR_PATH_NOT_FOUND)) return "This item is no longer on disk.";
  return `The item did not open: ${text}`;
}

type Entry = "system_terminal" | "terminal_tab" | "file_manager";

/** The typed code of a failed hand-off, for a log record. An untyped error can carry a path, so it is `unknown`. */
function handOffCode(err: unknown): string {
  const text = String(err);
  const known = [api.ERR_NO_TERMINAL_FOUND, api.ERR_PATH_NOT_FOUND, "path escapes project root", "terminal_start_failed", "reveal_failed"];
  return known.find((code) => text.includes(code)) ?? "unknown";
}

/** LCM-FR-WNPH: one WARN record. It names the entry and the error, never a path. */
function logFailure(entry: Entry, error: string) {
  logWarn(["frontend"], "project panel: open in failed", { entry, error });
}

/**
 * The three **Open In** entries (LCM-FR-PAUK, LCM-FR-XWFO, LCM-FR-VTLR). A
 * failure goes to `setError`, the panel's inline error (LCM-FR-WNPH).
 */
export function useOpenIn(
  setError: (error: string | null) => void,
  onOpenTerminal?: (session: TerminalSummary) => void,
) {
  const systemTerminal = useCallback(
    async (node: TreeNode) => {
      try {
        await api.openInSystemTerminal(node.path);
        setError(null);
      } catch (e) {
        logFailure("system_terminal", handOffCode(e));
        setError(handOffText(e));
      }
    },
    [setError],
  );

  const revealInFileManager = useCallback(
    async (node: TreeNode) => {
      try {
        await api.revealInFileManager(node.path);
        setError(null);
      } catch (e) {
        logFailure("file_manager", handOffCode(e));
        setError(handOffText(e));
      }
    },
    [setError],
  );

  const terminalTab = useCallback(
    async (node: TreeNode) => {
      try {
        // LCM-FR-XWFO: a work-stream copy has a `work_stream` target, so the
        // target comes from the backend, not from the worktree path.
        const targets = await api.listTerminalTargets();
        const active = targets.find((t) => t.isActiveWorktree);
        if (!active) {
          logFailure("terminal_tab", "no_active_target");
          setError("This worktree has no terminal target.");
          return;
        }
        const folder = terminalFolderOf(node);
        const session = await api.createTerminal(active.target, "host_shell", null, folder === "" ? null : folder);
        upsertTerminal(session);
        logInfo(["frontend"], "project panel: terminal tab created", { terminalId: session.id });
        setError(null);
        onOpenTerminal?.(session);
      } catch (e) {
        logFailure("terminal_tab", errorCode(e));
        setError(failureText(e));
      }
    },
    [setError, onOpenTerminal],
  );

  return { systemTerminal, revealInFileManager, terminalTab };
}
