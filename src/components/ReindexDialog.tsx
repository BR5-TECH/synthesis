/**
 * The Reindex confirmation (SET-FR-WZPS, SET-FR-HRRN).
 *
 * Two scope choices, Current branch first and selected. Each says what it
 * embeds again (EMB-FR-HRYV). Confirming calls `"start reindex (scope)"`;
 * Cancel, Escape, and a click outside call nothing.
 */
import { useState } from "react";

import * as api from "../api";
import { Modal } from "./DraftsPanelParts";
import { logInfo, logWarn } from "../logging";
import type { EmbeddingsProjectStatus, ReindexScope } from "../types";

export interface ReindexDialogProps {
  project: EmbeddingsProjectStatus;
  onClose: () => void;
  /** Where focus goes when the dialog closes: the button that opened it. */
  returnFocus?: HTMLElement | null;
}

function errorText(e: unknown): string {
  const raw = e instanceof Error ? e.message : String(e);
  if (raw === "embeddings_not_working") return "Embeddings do not work for this project.";
  if (raw === "no_project_open") return "No project is open.";
  return raw;
}

/** EMB-FR-HRYV, EMB-FR-QNWF, EMB-FR-MGML: what each scope embeds again here. */
function describe(scope: ReindexScope, project: EmbeddingsProjectStatus): string {
  if (scope === "whole_trace") {
    if (project.git === "none") return "Embeds the whole project and the documents again.";
    if (project.detached) return "Embeds the default branch and the documents again.";
    return "Embeds the default branch, this branch's changes, and the documents again.";
  }
  if (project.git === "none") return "Embeds the whole project and the documents again.";
  if (project.detached) return "On a detached HEAD, this embeds the documents again only.";
  return "Embeds this branch's changes and the documents again. The default branch stays.";
}

export function ReindexDialog({ project, onClose, returnFocus }: ReindexDialogProps) {
  const [scope, setScope] = useState<ReindexScope>("current_branch");
  const [error, setError] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  const confirm = async () => {
    setSubmitting(true);
    setError(null);
    try {
      await api.startReindex(scope);
      logInfo(["frontend"], "reindex started", { scope });
      onClose();
    } catch (e) {
      setSubmitting(false);
      setError(errorText(e));
      logWarn(["frontend"], "reindex not started", { scope, error: errorText(e) });
    }
  };

  const choices: [ReindexScope, string][] = [
    ["current_branch", "Current branch"],
    ["whole_trace", "Whole trace"],
  ];

  // The shared modal takes focus on the first choice, keeps Tab inside the
  // dialog, closes on Escape and on the scrim, and returns focus to the opener.
  return (
    <Modal label="Reindex embeddings" onClose={onClose} returnFocus={returnFocus}>
      <div data-testid="reindex-dialog">
        <div className="modal__head">
          <div className="modal__title" id="reindex-title">
            Reindex embeddings
          </div>
        </div>
        <div className="modal__body">
          <fieldset style={{ border: 0, padding: 0, margin: 0 }}>
            <legend className="t-label">Scope</legend>
            {choices.map(([value, label]) => (
              <label key={value} style={{ display: "flex", gap: 8, alignItems: "flex-start", marginTop: 8 }}>
                <input
                  type="radio"
                  name="reindex-scope"
                  value={value}
                  checked={scope === value}
                  onChange={() => setScope(value)}
                />
                <span>
                  <span style={{ display: "block" }}>{label}</span>
                  <span className="t-ui-xs t-muted">{describe(value, project)}</span>
                </span>
              </label>
            ))}
          </fieldset>
          <p className="t-ui-xs t-muted" style={{ marginTop: 12 }}>
            The work runs in the background. Search uses BM25 only until it ends.
          </p>
          {error && (
            <p className="t-ui-xs" role="alert" style={{ color: "var(--danger)" }}>
              {error}
            </p>
          )}
        </div>
        <div className="modal__actions">
          <button type="button" className="btn btn--ghost" onClick={onClose}>
            Cancel
          </button>
          <button
            type="button"
            className="btn btn--primary"
            disabled={submitting}
            onClick={() => void confirm()}
          >
            Reindex
          </button>
        </div>
      </div>
    </Modal>
  );
}
