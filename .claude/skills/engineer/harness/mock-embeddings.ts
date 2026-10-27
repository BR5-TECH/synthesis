/*
 * Embeddings index stand-in (EMB-embeddings-index.md), for UI auditing only.
 *
 * The Global and Project Indexing sections (GLS-FR-JDUM, SET-FR-QCUJ) and the
 * Semantic mode of the search bar (SCH-FR-12). `?embeddings=` picks the start
 * state: `off` (the default), `ready`, `downloading`, `failed`, or `job` (ready,
 * with a running first indexing). A save, a retry, and a Reindex emit
 * `"embeddings-status-changed"` with the whole status.
 */

import { NOT_HANDLED } from "./mock-github-polling";

type ModelState = "not_downloaded" | "downloading" | "loading" | "ready" | "failed";
type Mode = "use_global" | "on" | "off";
type Job = { kind: string; completed?: number; total?: number };

const MODELS = [
  { id: "BGESmallENV15", name: "BGE-small-en-v1.5", dimension: 384, backend: "onnx", recommended: true, default: true },
  { id: "AllMiniLML6V2", name: "all-MiniLM-L6-v2", dimension: 384, backend: "onnx", recommended: true, default: false },
  { id: "EmbeddingGemma300M", name: "EmbeddingGemma 300M", dimension: 768, backend: "onnx", recommended: true, default: false },
  { id: "BGEM3", name: "BGE-M3", dimension: 1024, backend: "onnx", recommended: true, default: false },
  { id: "GTEBaseENV15", name: "gte-base-en-v1.5", dimension: 768, backend: "onnx", recommended: false, default: false },
  {
    id: "SnowflakeArcticEmbedLV2Q",
    name: "snowflake-arctic-embed-l-v2.0-quantized-multilingual-long-context",
    dimension: 1024,
    backend: "candle",
    recommended: false,
    default: false,
  },
];

const start = new URLSearchParams(location.search).get("embeddings") ?? "off";

let enabled = start !== "off";
let model = "BGESmallENV15";
let modelState: ModelState =
  start === "off"
    ? "not_downloaded"
    : start === "downloading" || start === "failed"
      ? start
      : "ready";
let projectMode: Mode = "use_global";
let job: Job | undefined =
  start === "job" ? { kind: "first_indexing", completed: 412, total: 1280 } : undefined;

function working(): boolean {
  return projectMode !== "off" && enabled;
}

function status() {
  const on = working();
  return {
    enabled,
    model,
    modelState,
    ...(modelState === "downloading"
      ? { download: { completedBytes: 48_234_496, totalBytes: 133_169_152 } }
      : {}),
    ...(modelState === "failed"
      ? { modelError: "the download from huggingface.co timed out after 30 seconds" }
      : {}),
    project: {
      mode: projectMode,
      working: on,
      state: !on ? "off" : modelState !== "ready" ? "preparing" : job ? "updating" : "ready",
      ...(on && job ? { job } : {}),
      git: "repository",
      branch: "feature/new-artifact-window",
      detached: false,
      rootBranch: "main",
      rootTip: "7dd5558a9c3e41f0b2d6",
      specChunks: on ? 1280 : 0,
      documentChunks: on ? 64 : 0,
    },
  };
}

function emit() {
  const fire = (globalThis as Record<string, unknown>).__fireBusEvent as
    | ((name: string, payload?: unknown) => void)
    | undefined;
  queueMicrotask(() => fire?.("embeddings-status-changed", status()));
}

/** SCH-FR-03 / SCH-FR-LTSO: one Spec, one Document, and one Draft hit. */
const SEMANTIC_HITS = [
  {
    id: "specifications/ui/LIB-library.md",
    name: "LIB-library.md",
    path: "specifications/ui/LIB-library.md",
    ordinal: 1,
    group: "artifact",
    matchKind: "content",
    subtype: "spec",
    editContext: "standalone",
    snippet:
      "The Library panel shows every artifact of the project as a tree, grouped by type and then by folder. A row opens its artifact in the Editor; a folder row expands and collapses. The tree follows the scan as it changes, and a filter narrows it to the rows whose name contains the text, keeping the parents of each match so that the author can see where it lies.",
  },
  {
    id: `doc-${"b2".repeat(16)}`,
    documentId: `doc-${"b2".repeat(16)}`,
    name: "guide.md",
    path: "/Users/demo/reference/guide.md",
    ordinal: 2,
    group: "document",
    matchKind: "content",
    snippet:
      "Supercalifragilisticexpialidocious-identifier-without-any-break-opportunity-at-all-that-runs-past-the-row-width-and-must-wrap-anywhere-to-stay-inside-two-lines-of-the-snippet-region-of-the-result-row-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx",
  },
  {
    id: "d-1",
    draftId: "d-1",
    name: "Worktree cleanup command",
    path: "UI/Components",
    ordinal: 3,
    group: "draft",
    matchKind: "content",
    snippet: "Remove stale worktrees.",
  },
];

let semanticSeq = 0;

export function embeddingsInvoke(cmd: string, a: Record<string, any>): unknown {
  switch (cmd) {
    case "load_embeddings_settings":
      return { enabled, model };
    case "save_embeddings_settings": {
      const next = a.settings as { enabled: boolean; model: string };
      if (!MODELS.some((m) => m.id === next.model)) throw "unknown_model";
      enabled = next.enabled;
      model = next.model;
      if (enabled && modelState === "not_downloaded") modelState = "downloading";
      emit();
      return { enabled, model };
    }
    case "list_embedding_models":
      return MODELS;
    case "retry_embeddings_model_download":
      modelState = "downloading";
      emit();
      return status();
    case "load_project_embeddings_mode":
      return projectMode;
    case "save_project_embeddings_mode":
      projectMode = a.mode as Mode;
      emit();
      return projectMode;
    case "get_embeddings_status":
      return status();
    case "start_reindex":
      if (!working()) throw "embeddings_not_working";
      job = { kind: "reindex", completed: 0, total: 1344 };
      emit();
      return status();
    case "start_search": {
      if (a.mode !== "semantic") return NOT_HANDLED;
      const id = `semantic-${++semanticSeq}`;
      setTimeout(() => {
        const fire = (globalThis as any).__fireBusEvent;
        fire?.("search-results", { searchId: id, hits: SEMANTIC_HITS });
        fire?.("search-ended", { searchId: id, reason: "completed", total: SEMANTIC_HITS.length });
      }, 30);
      return id;
    }
    default:
      return NOT_HANDLED;
  }
}
