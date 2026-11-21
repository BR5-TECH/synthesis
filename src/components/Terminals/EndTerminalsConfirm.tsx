/**
 * The confirmation before a close ends running terminals (`TRM-terminals.md`
 * TRM-FR-JJEY). It states how many running terminals end. Cancel changes
 * nothing.
 */
import { Modal } from "../DraftsPanelParts";

export function EndTerminalsConfirm({
  running,
  onConfirm,
  onCancel,
  returnFocus,
}: {
  running: number;
  onConfirm: () => void;
  onCancel: () => void;
  returnFocus?: HTMLElement | null;
}) {
  const noun = running === 1 ? "terminal" : "terminals";
  return (
    <Modal label="End running terminals" onClose={onCancel} returnFocus={returnFocus} className="end-terminals">
      <div className="modal__head">
        <div className="modal__title">
          End {running} running {noun}?
        </div>
      </div>
      <p className="modal__body">
        {running === 1
          ? "1 running terminal ends. Its process stops and its output is gone."
          : `${running} running terminals end. Their processes stop and their output is gone.`}
      </p>
      <div className="modal__actions">
        <button type="button" className="btn btn--sm" onClick={onCancel}>
          Cancel
        </button>
        <button type="button" className="btn btn--danger btn--sm" onClick={onConfirm}>
          End terminals
        </button>
      </div>
    </Modal>
  );
}
