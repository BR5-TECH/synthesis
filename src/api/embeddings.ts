/**
 * The embeddings index (`EMB-embeddings-index.md`): its settings, its models,
 * its status, and the Reindex action.
 *
 * One part of the typed wrappers over the backend's `#[tauri::command]` set;
 * `./index.ts` carries the whole rule these files are written under.
 */
import { invoke } from "@tauri-apps/api/core";
import type {
  EmbeddingModel,
  EmbeddingsSettings,
  EmbeddingsStatus,
  ProjectEmbeddingsMode,
  ReindexScope,
} from "../types";

/** GLS-FR-JDUM: the Global switch and the selected model. */
export const loadEmbeddingsSettings = () =>
  invoke<EmbeddingsSettings>("load_embeddings_settings");

/**
 * EMB-FR-JFOV: persist the Global switch and the model. Rejects with
 * `unknown_model`, and then nothing is stored.
 */
export const saveEmbeddingsSettings = (settings: EmbeddingsSettings) =>
  invoke<EmbeddingsSettings>("save_embeddings_settings", { settings });

/** EMB-FR-MIDG: every model, the recommended four first. */
export const listEmbeddingModels = () =>
  invoke<EmbeddingModel[]>("list_embedding_models");

/** EMB-FR-KEGS: try a failed model download again. */
export const retryEmbeddingsModelDownload = () =>
  invoke<EmbeddingsStatus>("retry_embeddings_model_download");

/** GSS-FR-DSUG: the open project's mode. Rejects with `no_project_open`. */
export const loadProjectEmbeddingsMode = () =>
  invoke<ProjectEmbeddingsMode>("load_project_embeddings_mode");

/** EMB-FR-CTXC: persist the open project's mode. */
export const saveProjectEmbeddingsMode = (mode: ProjectEmbeddingsMode) =>
  invoke<ProjectEmbeddingsMode>("save_project_embeddings_mode", { mode });

/** EMB-FR-ZFGT: the status of the model and of the open project. */
export const getEmbeddingsStatus = () =>
  invoke<EmbeddingsStatus>("get_embeddings_status");

/**
 * EMB-FR-HRYV: queue a Reindex. Rejects with `no_project_open` or
 * `embeddings_not_working`.
 */
export const startReindex = (scope: ReindexScope) =>
  invoke<EmbeddingsStatus>("start_reindex", { scope });
