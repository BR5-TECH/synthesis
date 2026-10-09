/**
 * AII-FR-ZQTB: the confirmation a failed gateway check opens over the Agentic AI
 * section.
 *
 * Some gateways serve Claude Code and do not answer `GET /v1/models` — for
 * example a gateway in front of Amazon Bedrock. The binary has verified, so the
 * author can accept the gateway without the check (AIC-FR-KWMV). Every route
 * that dismisses the dialog invokes no operation, so it settles with a boolean
 * and starts nothing itself.
 */
import { useLayoutEffect, useRef } from "react";

interface GatewayCheckFailedDialogProps {
  /** The failure, as the status line words it (`aiErrorMessage`). */
  message: string;
  /** `true` for Accept anyway, `false` for every route that dismisses. */
  onSettle: (accept: boolean) => void;
}

export function GatewayCheckFailedDialog({ message, onSettle }: GatewayCheckFailedDialogProps) {
  const surfaceRef = useRef<HTMLDivElement>(null);
  const cancelRef = useRef<HTMLButtonElement>(null);

  // AII-FR-ZQTB: Cancel takes focus, so a reflexive Enter accepts nothing.
  // Layout effects run in the commit that opens the dialog, so focus and the
  // Escape listener are in place before the author can press a key.
  useLayoutEffect(() => {
    cancelRef.current?.focus();
  }, []);

  // Escape dismisses. Captured on the document so that it answers wherever
  // focus is, and stopped so that no control under the dialog also acts on it.
  useLayoutEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.stopPropagation();
        onSettle(false);
        return;
      }
      // The dialog is modal, so Tab cycles between its two actions and never
      // reaches the controls under the scrim.
      if (e.key !== "Tab") return;
      const actions = Array.from(
        surfaceRef.current?.querySelectorAll<HTMLButtonElement>("button") ?? [],
      );
      if (actions.length === 0) return;
      const at = actions.indexOf(document.activeElement as HTMLButtonElement);
      const step = e.shiftKey ? -1 : 1;
      e.preventDefault();
      actions[(at + step + actions.length) % actions.length].focus();
    };
    document.addEventListener("keydown", onKey, true);
    return () => document.removeEventListener("keydown", onKey, true);
  }, [onSettle]);

  return (
    <div
      className="scrim"
      data-testid="gateway-check-failed-scrim"
      onMouseDown={(e) => {
        if (!surfaceRef.current?.contains(e.target as Node)) onSettle(false);
      }}
    >
      <div
        className="modal"
        role="dialog"
        aria-modal="true"
        aria-labelledby="gateway-check-failed-title"
        aria-describedby="gateway-check-failed-body"
        data-testid="gateway-check-failed"
        ref={surfaceRef}
      >
        <div className="modal__head">
          <div className="modal__title" id="gateway-check-failed-title">
            The gateway check failed
          </div>
        </div>
        <div className="modal__body" id="gateway-check-failed-body">
          <p className="t-p" data-testid="gateway-check-failed-message">
            {message}
          </p>
          <p className="t-p">
            Claude Code verified. You can use this gateway without the check. The
            model list is then the bundled one.
          </p>
        </div>
        <div className="modal__actions">
          <button
            ref={cancelRef}
            className="btn btn--ghost"
            data-testid="gateway-check-failed-cancel"
            onClick={() => onSettle(false)}
          >
            Cancel
          </button>
          <button
            className="btn btn--primary"
            data-testid="gateway-check-failed-accept"
            onClick={() => onSettle(true)}
          >
            Accept anyway
          </button>
        </div>
      </div>
    </div>
  );
}
