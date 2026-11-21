/**
 * TRM-FR-DPWA: the state marker of a Terminal tab. An exited session and a
 * session with a gone target show it beside the tab's label.
 */
import { useTerminalsSnapshot } from "../../hooks/shell/useTerminalTabs";

export function TerminalTabMark({ terminalId }: { terminalId: string }) {
  const { sessions } = useTerminalsSnapshot();
  const state = sessions.find((s) => s.id === terminalId)?.state;
  if (state !== "exited" && state !== "target_gone") return null;
  const text = state === "exited" ? "exited" : "target gone";
  return (
    <span className="tab__terminal-state" data-state={state} aria-label={text} title={text}>
      {text}
    </span>
  );
}
