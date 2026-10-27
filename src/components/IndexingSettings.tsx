/**
 * Global settings → **Indexing** (GLS-FR-JDUM through GLS-FR-UWFA).
 *
 * The Global embeddings switch, the model picker, and the progress of the model
 * download and of the embedding jobs. BM25 search is always on; embeddings only
 * improve how specifications and documents rank.
 *
 * Every change applies at once through `"save embeddings settings"`, so the
 * section holds no dirty state and takes no part in the save-before-close sweep
 * (GLS-FR-UWFA). Changes are written one at a time, so a later write never
 * carries a value that an earlier change replaced.
 */
import { useEffect, useRef, useState } from "react";

import * as api from "../api";
import { useEmbeddingsStatus } from "../hooks/useEmbeddingsStatus";
import { logInfo, logWarn } from "../logging";
import type { EmbeddingModel, EmbeddingsSettings } from "../types";
import { Modal } from "./DraftsPanelParts";
import { DownloadProgress, JobProgress } from "./IndexingParts";

function errorText(e: unknown): string {
  const raw = e instanceof Error ? e.message : String(e);
  return raw === "unknown_model" ? "This model is not available in this build." : raw;
}

/** GLS-FR-KRZU: a picker entry with its marks. */
function modelLabel(model: EmbeddingModel): string {
  const marks = [model.recommended ? "Recommended" : null, model.default ? "Default" : null]
    .filter(Boolean)
    .join(", ");
  return `${model.name} — ${model.dimension} dimensions${marks ? ` (${marks})` : ""}`;
}

export function IndexingSettings() {
  const status = useEmbeddingsStatus();
  const [settings, setSettings] = useState<EmbeddingsSettings | null>(null);
  const [models, setModels] = useState<EmbeddingModel[] | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  /** GLS-FR-ZBQR: the model waiting for the author's confirmation. */
  const [pendingModel, setPendingModel] = useState<string | null>(null);
  const writes = useRef<Promise<unknown>>(Promise.resolve());
  const latest = useRef<EmbeddingsSettings | null>(null);
  /** The last value the backend stored, which a refusal restores. */
  const confirmed = useRef<EmbeddingsSettings | null>(null);
  const modelSelect = useRef<HTMLSelectElement | null>(null);

  // GLS-FR-JDUM: read on mount.
  useEffect(() => {
    let cancelled = false;
    Promise.all([api.loadEmbeddingsSettings(), api.listEmbeddingModels()])
      .then(([loaded, list]) => {
        if (cancelled) return;
        latest.current = loaded;
        confirmed.current = loaded;
        setSettings(loaded);
        setModels(list);
      })
      .catch((e) => {
        if (cancelled) return;
        setLoadError(errorText(e));
        logWarn(["frontend"], "embeddings settings could not be read", { error: errorText(e) });
      });
    return () => {
      cancelled = true;
    };
  }, []);

  /** GLS-FR-UWFA: persist at once; a refusal restores the previous value. */
  const persist = (next: EmbeddingsSettings) => {
    latest.current = next;
    setSettings(next);
    setError(null);
    writes.current = writes.current
      .catch(() => undefined)
      .then(() => api.saveEmbeddingsSettings(next))
      .then((saved) => {
        confirmed.current = saved;
        logInfo(["frontend"], "embeddings settings saved", {
          enabled: saved.enabled,
          model: saved.model,
        });
        // A later change already shown keeps showing until its own write ends.
        if (latest.current === next) {
          latest.current = saved;
          setSettings(saved);
        }
      })
      .catch((e) => {
        if (latest.current === next) {
          latest.current = confirmed.current;
          setSettings(confirmed.current);
        }
        setError(errorText(e));
        logWarn(["frontend"], "embeddings settings not saved", {
          enabled: next.enabled,
          model: next.model,
          error: errorText(e),
        });
      });
  };

  if (loadError) {
    return (
      <p className="t-p" role="alert">
        The indexing settings could not be read: {loadError}
      </p>
    );
  }
  if (!settings || !models) {
    return <p className="t-p">Reading the indexing settings…</p>;
  }

  const recommended = models.filter((m) => m.recommended);
  const others = models.filter((m) => !m.recommended);
  const chooseModel = (id: string) => {
    if (id === settings.model) return;
    // GLS-FR-ZBQR: while embeddings are on, a new model embeds every project
    // again, so the author confirms first. While they are off, nothing runs.
    if (settings.enabled) setPendingModel(id);
    else persist({ ...settings, model: id });
  };
  const pendingName = models.find((m) => m.id === pendingModel)?.name ?? pendingModel;

  return (
    <div data-testid="indexing-settings">
      <p className="t-p" style={{ marginBottom: 20, maxWidth: 560 }}>
        Search always ranks with BM25. Embeddings improve how specifications and
        documents rank, for agents and for the Semantic mode of the search bar.
        The model runs on this machine.
      </p>

      <label style={{ display: "flex", alignItems: "center", gap: 10 }}>
        <input
          type="checkbox"
          role="switch"
          checked={settings.enabled}
          onChange={(e) => persist({ ...settings, enabled: e.target.checked })}
        />
        <span>Use embeddings</span>
      </label>
      <p className="t-ui-xs t-muted" style={{ marginTop: 6, marginBottom: 20, maxWidth: 560 }}>
        Turning this on downloads the selected model once, then embeds the open
        project in the background. A project can turn embeddings off for itself
        in its own Indexing section.
      </p>

      <label className="t-label" htmlFor="embeddings-model">
        Model
      </label>
      <div style={{ marginTop: 6, marginBottom: 6 }}>
        <select
          ref={modelSelect}
          id="embeddings-model"
          className="input"
          value={settings.model}
          onChange={(e) => chooseModel(e.target.value)}
          style={{ maxWidth: 520, width: "100%" }}
        >
          <optgroup label="Recommended">
            {recommended.map((m) => (
              <option key={m.id} value={m.id}>
                {modelLabel(m)}
              </option>
            ))}
          </optgroup>
          <optgroup label="Other models">
            {others.map((m) => (
              <option key={m.id} value={m.id}>
                {modelLabel(m)}
              </option>
            ))}
          </optgroup>
        </select>
      </div>

      {status && (
        <div style={{ marginTop: 16, maxWidth: 560 }} data-testid="indexing-model-state">
          <DownloadProgress status={status} />
          {status.modelState === "loading" && <p className="t-ui-xs t-muted">Loading the model…</p>}
          {status.enabled && status.modelState === "ready" && (
            <p className="t-ui-xs t-muted">The model is ready.</p>
          )}
          {status.modelState === "failed" && (
            // GLS-FR-HEPP: the reason, and a way to try again.
            <div role="alert">
              <p className="t-ui-xs" style={{ color: "var(--danger)" }}>
                The model could not be downloaded: {status.modelError ?? "unknown error"}. Search
                uses BM25 only until the model is ready.
              </p>
              <button
                type="button"
                className="btn btn--default btn--sm"
                onClick={() => {
                  void api.retryEmbeddingsModelDownload().catch((e) => {
                    setError(errorText(e));
                    logWarn(["frontend"], "embeddings model download retry failed", {
                      error: errorText(e),
                    });
                  });
                }}
              >
                Retry
              </button>
            </div>
          )}
          {/* GLS-FR-RQFK: with no project open, the model state is all there is. */}
          {status.project && <JobProgress job={status.project.job} />}
        </div>
      )}

      {error && (
        <p className="t-ui-xs" role="alert" style={{ marginTop: 12, color: "var(--danger)" }}>
          {error}
        </p>
      )}

      {pendingModel && (
        // The shared modal keeps Tab inside the dialog, closes on Escape and
        // on the scrim, and returns focus to the model picker.
        <Modal
          label={`Switch to ${pendingName}?`}
          onClose={() => setPendingModel(null)}
          returnFocus={modelSelect.current}
        >
          <div data-testid="model-switch-confirm">
            <div className="modal__head">
              <div className="modal__title" id="model-switch-title">
                Switch to {pendingName}?
              </div>
            </div>
            <div className="modal__body">
              <p className="t-p">
                Every project embeds its specifications and documents again with
                the new model. Search uses BM25 only until that ends.
              </p>
            </div>
            <div className="modal__actions">
              <button
                type="button"
                className="btn btn--ghost"
                onClick={() => setPendingModel(null)}
              >
                Cancel
              </button>
              <button
                type="button"
                className="btn btn--primary"
                onClick={() => {
                  const id = pendingModel;
                  setPendingModel(null);
                  persist({ ...settings, model: id });
                }}
              >
                Switch model
              </button>
            </div>
          </div>
        </Modal>
      )}
    </div>
  );
}
