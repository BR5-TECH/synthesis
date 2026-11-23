/**
 * The label field of a terminal (`../../../specifications/ui/TRM-terminals.md`
 * TRM-FR-SHKD, TRM-FR-LIRN, TRM-FR-MOVS): the row of the Terminals panel and
 * the terminal tab show it in place of the label while the author renames.
 */
import { useLayoutEffect, useRef, useState } from "react";
import { renameTerminal } from "../../state/terminalActions";

/** TRS-FR-BTMT: the most characters a name holds. */
const MAX_LABEL_LENGTH = 64;

export function TerminalLabelField({
  terminalId,
  label,
  className,
  onDone,
}: {
  terminalId: string;
  label: string;
  className?: string;
  /** The field closed. `restoreFocus` is true for Escape and for a save by Enter, which put focus back. */
  onDone: (restoreFocus: boolean) => void;
}) {
  const [text, setText] = useState(label);
  const [error, setError] = useState<string | null>(null);
  const inputRef = useRef<HTMLInputElement | null>(null);
  // A commit runs once: Enter and the blur that follows it do not both call.
  const busy = useRef(false);
  const closed = useRef(false);

  useLayoutEffect(() => {
    // TRM-FR-SHKD: the focus is in the field, with all of its text selected.
    inputRef.current?.focus();
    inputRef.current?.select();
  }, []);

  const close = (restoreFocus: boolean) => {
    if (closed.current) return;
    closed.current = true;
    onDone(restoreFocus);
  };

  /** `byKey` is true for Enter: the focus is still in the field and goes back to the row or the tab. */
  const commit = async (byKey: boolean) => {
    if (busy.current || closed.current) return;
    busy.current = true;
    const failure = await renameTerminal(terminalId, text);
    busy.current = false;
    if (failure === null) close(byKey && document.activeElement === inputRef.current);
    else setError(failure);
  };

  return (
    <span className={`terminal-label-field${className ? ` ${className}` : ""}`}>
      <input
        ref={inputRef}
        className="input input--sm terminal-label-field__input"
        aria-label="Terminal name"
        aria-invalid={error !== null || undefined}
        value={text}
        maxLength={MAX_LABEL_LENGTH}
        onChange={(e) => {
          setText(e.target.value);
          setError(null);
        }}
        onClick={(e) => e.stopPropagation()}
        onMouseDown={(e) => e.stopPropagation()}
        onDoubleClick={(e) => e.stopPropagation()}
        onContextMenu={(e) => e.stopPropagation()}
        onKeyDown={(e) => {
          // TRM-FR-SHKD: no key in the field reaches the row or the tab.
          e.stopPropagation();
          if (e.key === "Enter") {
            e.preventDefault();
            void commit(true);
          } else if (e.key === "Escape") {
            e.preventDefault();
            close(true);
          }
        }}
        onBlur={() => {
          // TRM-FR-LIRN: a blur saves. After a refused save, the author left the
          // field with its error on screen, so it closes with no second call.
          if (error === null) void commit(false);
          else close(false);
        }}
      />
      {error && (
        <span className="terminal-label-field__error" role="alert">
          {error}
        </span>
      )}
    </span>
  );
}
