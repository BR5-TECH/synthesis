/**
 * The Terminal tab (`../../../specifications/ui/TRM-terminals.md` TRM-FR-XBVB …
 * TRM-FR-XVFQ).
 *
 * The terminal itself lives in `state/terminalViews`, outside React, so it
 * stays attached while the tab is hidden. This view puts its element into the
 * page and shows the states around it: starting, busy target, exit, target
 * gone, and output that is gone.
 */
import { useEffect, useRef } from "react";
import { useTerminalsSnapshot } from "../../hooks/shell/useTerminalTabs";
import { focusTerminalView, hideTerminalView, showTerminalView } from "../../state/terminalViews";
import type { TerminalSummary } from "../../types";
import { Icon } from "../icons";

/** TRM-FR-FFPL: the line under the output of an ended session. */
export function exitLine(session: TerminalSummary): string {
  if (session.endReason === "start_failed")
    return session.endDetail ? `The terminal did not start: ${session.endDetail}` : "The terminal did not start.";
  if (session.endReason === "vm_stopped") return "The VM stopped. The shell in it ended.";
  return session.exitCode === undefined
    ? "The process exited."
    : `The process exited with code ${session.exitCode}.`;
}

export function TerminalTab({ terminalId }: { terminalId: string }) {
  const { sessions } = useTerminalsSnapshot();
  const session = sessions.find((s) => s.id === terminalId);
  const hostRef = useRef<HTMLDivElement | null>(null);
  // TRM-FR-HQDM: an ended session with no history has nothing to show.
  const noOutput = session !== undefined && session.state === "exited" && !session.hasHistory;

  useEffect(() => {
    const host = hostRef.current;
    if (!host || noOutput) return;
    showTerminalView(terminalId, host);
    focusTerminalView(terminalId);
    return () => hideTerminalView(terminalId);
  }, [terminalId, noOutput]);

  return (
    <div className="terminal-tab" data-state={session?.state ?? "unknown"}>
      {session?.state === "target_gone" && (
        // TRM-FR-XVJT: the tab stays as a read-only view of the history.
        <div className="terminal-banner terminal-banner--gone" role="status">
          <Icon.AlertTriangle size={12} aria-hidden="true" />
          <span>
            <strong>Target gone.</strong> The worktree or work stream of this terminal is removed. This is a read-only
            view of its output.
          </span>
        </div>
      )}
      {session?.busyRunId && session.state !== "target_gone" && (
        // TRM-FR-XVFQ: a persistent warning; input still goes to the session.
        <div className="terminal-banner terminal-banner--busy" role="note">
          <Icon.AlertTriangle size={12} aria-hidden="true" />
          <span>A graduation run is working in this target.</span>
        </div>
      )}
      <div className="terminal-tab__surface">
        {noOutput ? (
          <div className="terminal-tab__gone-output">
            <p>{exitLine(session)}</p>
            <p>The output of this terminal is gone.</p>
          </div>
        ) : (
          <div ref={hostRef} className="terminal-view" data-terminal-id={terminalId} />
        )}
        {session?.state === "starting" && (
          // TRM-FR-GMGF: "Starting" over the start output.
          <div className="terminal-tab__starting" role="status">
            <span className="terminal-tab__spinner" aria-hidden="true" />
            Starting
          </div>
        )}
      </div>
      {session?.state === "exited" && !noOutput && (
        <div className="terminal-tab__exit" role="status">
          {exitLine(session)}
        </div>
      )}
    </div>
  );
}
