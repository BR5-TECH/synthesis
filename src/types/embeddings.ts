// Embeddings index (EMB-embeddings-index.md)
//
// The shapes of the embeddings commands and of `"embeddings status changed"`.
// Re-exported from `./index`, so `from "../types"` resolves them.

/** EMB contract surface: the Global switch and the selected model. */
export interface EmbeddingsSettings {
  enabled: boolean;
  /** A model id of `"list embedding models"`. */
  model: string;
}

/** EMB-FR-MHHY: the backend that runs a model. */
export type EmbeddingBackend = "candle" | "onnx";

/** EMB-FR-MIDG: one model of the picker. */
export interface EmbeddingModel {
  id: string;
  name: string;
  dimension: number;
  backend: EmbeddingBackend;
  recommended: boolean;
  default: boolean;
}

/** GSS-FR-DSUG / EMB-FR-WQGF: the per-project switch. */
export type ProjectEmbeddingsMode = "use_global" | "on" | "off";

/** EMB-FR-HRYV: what a Reindex embeds again. */
export type ReindexScope = "current_branch" | "whole_trace";

export type EmbeddingModelState =
  | "not_downloaded"
  | "downloading"
  | "loading"
  | "ready"
  | "failed";

/** EMB-FR-BLHF: the state of the open project's vector index. */
export type EmbeddingsIndexState =
  | "off"
  | "preparing"
  | "ready"
  | "updating"
  | "unavailable";

export type EmbeddingJobKind =
  | "first_indexing"
  | "open_check"
  | "branch_update"
  | "root_update"
  | "model_switch"
  | "stale_rebuild"
  | "reindex"
  | "incremental";

export interface EmbeddingJob {
  kind: EmbeddingJobKind;
  completed?: number;
  total?: number;
}

export interface EmbeddingsProjectStatus {
  mode: ProjectEmbeddingsMode;
  /** EMB-FR-WQGF: whether embeddings work for the project. */
  working: boolean;
  state: EmbeddingsIndexState;
  job?: EmbeddingJob;
  git: "repository" | "none";
  branch?: string;
  detached: boolean;
  rootBranch?: string;
  rootTip?: string;
  specChunks: number;
  documentChunks: number;
  lastError?: string;
}

/** EMB-FR-ZFGT: `"get embeddings status"` and `"embeddings status changed"`. */
export interface EmbeddingsStatus {
  enabled: boolean;
  model: string;
  modelState: EmbeddingModelState;
  download?: { completedBytes: number; totalBytes?: number };
  modelError?: string;
  project?: EmbeddingsProjectStatus;
}
