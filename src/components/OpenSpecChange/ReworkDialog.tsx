/**
 * The Rework… dialog of a change workspace on a stream
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-LWFQ,
 * OCW-FR-FEOA, OCW-FR-SCIA, OCW-FR-FEBP).
 *
 * A modal over the tab that names the change and the stream and holds one
 * instruction field. Start rework invokes `"start change rework"`; a refusal
 * stays in the dialog with the instruction kept. The instruction is the
 * author's text, so no record holds it.
 */
import { useState } from "react";

import * as api from "../../api";
import { logInfo, logWarn } from "../../logging";
import { noteStartedRun, rereadChangeRunQueue } from "../../state/openspec/changeRunQueue";
import { Modal } from "../DraftsPanelParts";
import { startRefusal, type StartRefusal } from "./changeRunStart";
import { knownRunTitle } from "./useChangeRuns";
import { RefusalRoutes } from "./ChangeRunActions";

export function ReworkDialog({
  streamId,
  streamName,
  change,
  returnFocus,
  onClose,
  onThread,
}: {
  streamId: string;
  streamName: string;
  change: string;
  /** OCW-FR-FEBP: focus returns to Rework… when the dialog closes. */
  returnFocus: HTMLElement | null;
  onClose: () => void;
  /** OCW-FR-FEOA / OCW-FR-SCIA: select this thread in the discussion column. */
  onThread: (threadId: string) => void;
}) {
  const [instruction, setInstruction] = useState("");
  const [busy, setBusy] = useState(false);
  const [refusal, setRefusal] = useState<StartRefusal | null>(null);
  const blank = instruction.trim() === "";

  // OCW-FR-FEBP: Escape and Cancel start nothing; while a start is in flight
  // the dialog stays.
  const close = () => {
    if (!busy) onClose();
  };

  const start = async () => {
    if (blank || busy) return;
    setBusy(true);
    setRefusal(null);
    logInfo(["frontend"], "a rework run is being started", { streamId });
    try {
      const started = await api.startChangeRework(streamId, change, instruction);
      noteStartedRun(started.run);
      logInfo(["frontend"], "a rework run was started", { streamId, runId: started.run.id });
      void rereadChangeRunQueue();
      onThread(started.threadId);
      onClose();
    } catch (e) {
      const refused = startRefusal(e, knownRunTitle);
      setRefusal(refused);
      // OCW-FR-SCIA: a failure that returns a thread id selects that thread.
      if (refused.threadId) onThread(refused.threadId);
      logWarn(["frontend"], "a rework run was refused", {
        streamId,
        code: refused.code,
        withThread: refused.threadId !== null,
      });
      setBusy(false);
    }
  };

  return (
    <Modal
      label={`Rework ${change}`}
      returnFocus={returnFocus}
      onClose={close}
      closeOnScrim={false}
      className="openspec-rework"
    >
      <div className="modal__head">
        <div className="modal__title">Rework {change}</div>
      </div>
      <div className="modal__body openspec-rework__body" data-testid="openspec-rework-dialog">
        <p className="t-ui-sm openspec-rework__what">
          A rework run revises the change <span className="openspec-rework__name">{change}</span> on
          the work stream <span className="openspec-rework__name">{streamName}</span>. Its result
          comes to you as one proposal in the discussion of the change.
        </p>
        <label className="openspec-rework__field">
          <span className="t-eyebrow">Instruction</span>
          {/* OCW-FR-FEBP: the first focus stop of the dialog, so it takes the
              focus when the dialog opens. */}
          <textarea
            className="input openspec-rework__instruction"
            rows={6}
            tabIndex={0}
            value={instruction}
            disabled={busy}
            placeholder="What to change in the proposal, the specs, the design, or the tasks"
            onChange={(e) => setInstruction(e.target.value)}
          />
        </label>
        {refusal && (
          <div className="openspec-rework__refusal" role="alert" data-testid="openspec-rework-refusal">
            <span>{refusal.sentence}</span>
            <RefusalRoutes refusal={refusal} />
          </div>
        )}
      </div>
      <div className="modal__actions">
        <button type="button" className="btn btn--sm btn--ghost" disabled={busy} onClick={close}>
          Cancel
        </button>
        <button
          type="button"
          className="btn btn--sm btn--primary"
          disabled={blank || busy}
          onClick={() => void start()}
        >
          Start rework
        </button>
      </div>
    </Modal>
  );
}
