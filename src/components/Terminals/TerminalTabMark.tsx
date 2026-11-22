/**
 * The marks beside a Terminal tab's label: the persistent marker of a session
 * that survives an IDE restart (TRM-FR-PHTL), and the state marker of an
 * exited session and of a session with a gone target (TRM-FR-DPWA).
 */
import { useTerminalsSnapshot } from "../../hooks/shell/useTerminalTabs";
import { TerminalPersistMark } from "./TerminalPersistMark";

export function TerminalTabMark({ terminalId }: { terminalId: string }) {
  const { sessions } = useTerminalsSnapshot();
  const session = sessions.find((s) => s.id === terminalId);
  const state = session?.state;
  const persistent = session?.persistent === true && <TerminalPersistMark className="tab__terminal-persist" />;
  if (state !== "exited" && state !== "target_gone") return persistent || null;
  const text = state === "exited" ? "exited" : "target gone";
  return (
    <>
      {persistent}
      <span className="tab__terminal-state" data-state={state} aria-label={text} title={text}>
        {text}
      </span>
    </>
  );
}
