/**
 * The parts both Indexing sections render: the progress of the model download
 * and of an embedding job (GLS-FR-RQFK, SET-FR-UNRY), with the words for each
 * job and state.
 */
import type {
  EmbeddingJob,
  EmbeddingJobKind,
  EmbeddingsIndexState,
  EmbeddingsStatus,
} from "../types";

/** EMB-FR-WJCW: what each job is called where the author reads it. */
export const JOB_LABELS: Record<EmbeddingJobKind, string> = {
  first_indexing: "Embedding specifications",
  open_check: "Checking embeddings",
  branch_update: "Checking embeddings",
  root_update: "Checking embeddings",
  model_switch: "Switching embedding model",
  stale_rebuild: "Switching embedding model",
  reindex: "Reindexing embeddings",
  incremental: "Updating embeddings",
};

/** EMB-FR-BLHF: the index state, in words. */
export const STATE_LABELS: Record<EmbeddingsIndexState, string> = {
  off: "Off — search uses BM25 only",
  preparing: "Preparing — search uses BM25 only until this ends",
  ready: "Ready",
  updating: "Ready — updating changed chunks",
  unavailable: "Unavailable — search uses BM25 only",
};

function megabytes(bytes: number): string {
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

/** GLS-FR-RQFK: the model download, in bytes. */
export function DownloadProgress({ status }: { status: EmbeddingsStatus }) {
  if (status.modelState !== "downloading") return null;
  const download = status.download;
  const total = download?.totalBytes;
  return (
    <div className="indexing-progress" data-testid="embeddings-download">
      <span className="t-ui-xs">
        Downloading the model
        {download
          ? ` — ${megabytes(download.completedBytes)}${total ? ` of ${megabytes(total)}` : ""}`
          : "…"}
      </span>
      <progress
        aria-label="Model download"
        max={total || undefined}
        value={total ? download?.completedBytes : undefined}
      />
    </div>
  );
}

/** GLS-FR-RQFK, SET-FR-UNRY: the running job and its count of chunks. */
export function JobProgress({ job }: { job: EmbeddingJob | undefined }) {
  if (!job) return null;
  const known = job.total !== undefined && job.total > 0;
  return (
    <div className="indexing-progress" data-testid="embeddings-job">
      <span className="t-ui-xs">
        {JOB_LABELS[job.kind]}
        {known ? ` — ${job.completed ?? 0} of ${job.total} chunks` : "…"}
      </span>
      <progress
        aria-label={JOB_LABELS[job.kind]}
        max={known ? job.total : undefined}
        value={known ? job.completed ?? 0 : undefined}
      />
    </div>
  );
}
