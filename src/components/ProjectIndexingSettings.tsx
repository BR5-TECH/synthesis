/**
 * Project settings → **Indexing** (SET-FR-QCUJ through SET-FR-VGQB).
 *
 * The project's embeddings switch — Use Global, On, Off — with the index status
 * of the open project and the Reindex action. The model picker and the model
 * download stay in the Global Indexing section (SET-FR-VGQB).
 *
 * A choice persists at once through `"save project embeddings mode"`, outside
 * the section dirty state and the save-before-close sweep (SET-FR-DHIZ).
 */
import { useEffect, useRef, useState } from "react";

import * as api from "../api";
import { useEmbeddingsStatus } from "../hooks/useEmbeddingsStatus";
import { logWarn } from "../logging";
import { formatSectionAddress } from "../settingsWindow";
import type { EmbeddingsProjectStatus, ProjectEmbeddingsMode } from "../types";
import { JobProgress, STATE_LABELS } from "./IndexingParts";
import { ReindexDialog } from "./ReindexDialog";

const MODE_CHOICES: [ProjectEmbeddingsMode, string][] = [
  ["use_global", "Use Global"],
  ["on", "On"],
  ["off", "Off"],
];

function errorText(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

/** SET-FR-MKDJ: why embeddings do not work for this project, when they do not. */
function notWorkingReason(globalEnabled: boolean, mode: ProjectEmbeddingsMode): string | null {
  if (mode === "off") return "Embeddings are off for this project.";
  if (!globalEnabled) {
    return "Embeddings are off in Global settings. On does nothing until you turn them on there.";
  }
  return null;
}

/** SET-FR-UNRY: where the index stands in Git. */
function gitLine(project: EmbeddingsProjectStatus): string {
  if (project.git === "none") return "Not a Git repository: one layer from the working copy.";
  const root = project.rootBranch
    ? `Default branch ${project.rootBranch}${project.rootTip ? ` at ${project.rootTip.slice(0, 8)}` : ""}`
    : "No default branch";
  const head = project.detached
    ? "a detached HEAD (the default branch only)"
    : project.branch
      ? `branch ${project.branch}`
      : "no branch yet";
  return `${root}; working on ${head}.`;
}

export function ProjectIndexingSettings() {
  const status = useEmbeddingsStatus();
  const [mode, setMode] = useState<ProjectEmbeddingsMode | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [reindexOpen, setReindexOpen] = useState(false);
  const reindexButton = useRef<HTMLButtonElement | null>(null);
  const writes = useRef<Promise<unknown>>(Promise.resolve());
  /** The last mode the backend stored, which a refusal restores. */
  const confirmed = useRef<ProjectEmbeddingsMode | null>(null);
  const latest = useRef<ProjectEmbeddingsMode | null>(null);

  // SET-FR-QCUJ: read on mount.
  useEffect(() => {
    let cancelled = false;
    api
      .loadProjectEmbeddingsMode()
      .then((m) => {
        if (cancelled) return;
        confirmed.current = m;
        latest.current = m;
        setMode(m);
      })
      .catch((e) => {
        if (cancelled) return;
        setError(errorText(e));
        logWarn(["frontend"], "project embeddings mode could not be read", { error: errorText(e) });
      });
    return () => {
      cancelled = true;
    };
  }, []);

  /** SET-FR-DHIZ: persist at once; a refusal restores the previous choice. */
  const choose = (next: ProjectEmbeddingsMode) => {
    latest.current = next;
    setMode(next);
    setError(null);
    writes.current = writes.current
      .catch(() => undefined)
      .then(() => api.saveProjectEmbeddingsMode(next))
      .then((saved) => {
        confirmed.current = saved;
      })
      .catch((e) => {
        if (latest.current === next) {
          latest.current = confirmed.current;
          setMode(confirmed.current);
        }
        setError(errorText(e));
        logWarn(["frontend"], "project embeddings mode not saved", { mode: next, error: errorText(e) });
      });
  };

  const openGlobal = () => {
    void api.openSettingsWindow(formatSectionAddress("global", "indexing")).catch((e) => {
      logWarn(["frontend"], "global indexing settings could not be opened", { error: errorText(e) });
    });
  };

  const globalEnabled = status?.enabled ?? false;
  const effectiveMode = mode ?? "use_global";
  const reason = notWorkingReason(globalEnabled, effectiveMode);
  const project = status?.project;

  return (
    <div data-testid="project-indexing-settings">
      <p className="t-p" style={{ marginBottom: 20, maxWidth: 560 }}>
        Whether embeddings improve search in this project. The choice belongs to
        you: it is stored on this machine, is the same in every worktree, and is
        never committed.
      </p>

      <fieldset style={{ border: 0, padding: 0, margin: "0 0 12px" }}>
        <legend className="t-label">Embeddings</legend>
        {MODE_CHOICES.map(([value, label]) => (
          <label key={value} style={{ display: "flex", alignItems: "center", gap: 8 }}>
            <input
              type="radio"
              name="project-embeddings-mode"
              value={value}
              checked={effectiveMode === value}
              disabled={mode === null}
              onChange={() => choose(value)}
            />
            <span>{label}</span>
          </label>
        ))}
      </fieldset>

      {/* SET-FR-MKDJ: whether they work, and why not. */}
      {status && (
        <p className="t-ui-xs t-muted" data-testid="project-indexing-working" style={{ maxWidth: 560 }}>
          {reason ?? "Embeddings work for this project."}{" "}
          {!globalEnabled && (
            <button type="button" className="btn btn--sm btn--ghost" onClick={openGlobal}>
              Open Global Indexing
            </button>
          )}
        </p>
      )}

      {/* SET-FR-UNRY: the index status of the open project. */}
      {project && (
        <div style={{ marginTop: 16, maxWidth: 560 }} data-testid="project-indexing-status">
          <p className="t-ui-sm">{STATE_LABELS[project.state]}</p>
          <p className="t-ui-xs t-muted">{gitLine(project)}</p>
          <p className="t-ui-xs t-muted">
            {project.specChunks} specification chunks and {project.documentChunks} document chunks
            have vectors.
          </p>
          <JobProgress job={project.job} />
          {project.lastError && (
            <p className="t-ui-xs" role="alert" style={{ color: "var(--danger)" }}>
              The last embedding job failed: {project.lastError}
            </p>
          )}
        </div>
      )}

      <div style={{ marginTop: 16 }}>
        {/* SET-FR-HRRN: disabled while embeddings do not work, with the reason
            stated above. */}
        <button
          ref={reindexButton}
          type="button"
          className="btn btn--default"
          disabled={!project?.working}
          onClick={() => setReindexOpen(true)}
        >
          Reindex…
        </button>
      </div>

      {error && (
        <p className="t-ui-xs" role="alert" style={{ marginTop: 12, color: "var(--danger)" }}>
          {error}
        </p>
      )}

      {reindexOpen && project && (
        <ReindexDialog
          project={project}
          onClose={() => setReindexOpen(false)}
          returnFocus={reindexButton.current}
        />
      )}
    </div>
  );
}
