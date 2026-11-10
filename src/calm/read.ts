/**
 * `CLM-calm-model.md`: the read of a CALM document, and the results that are
 * kept by source text.
 *
 * A read runs its steps in order (CLM-FR-ODQZ): parse, schema, references,
 * graph, layout. The first step that fails ends the read. Every surface of
 * `CAD-calm-architecture-diagrams.md` asks for a read here, so the same source
 * reads once and shows the same way everywhere.
 */
import { logDebug, logWarn } from "../logging";
import { buildGraph } from "./graph";
import { layoutGraph } from "./layout";
import { parseCalm } from "./parse";
import { checkReferences } from "./references";
import { CALM_ERROR, type CalmError, type CalmResult } from "./types";

type Engine = typeof import("./engine");

let loading: Promise<Engine> | null = null;
const importEngine = (): Promise<Engine> => import("./engine");
let loader = importEngine;

/** Replaces the import of the libraries of a read. For tests only. */
export function setCalmEngineLoaderForTest(next: (() => Promise<Engine>) | null): void {
  loader = next ?? importEngine;
  loading = null;
}

/** CLM-FR-HPJT: the libraries of a read, loaded once; a failed load is tried again. */
function loadEngine(): Promise<Engine> {
  if (loading === null) {
    loading = loader();
    loading.catch(() => {
      loading = null;
    });
  }
  return loading;
}

/** A short, stable tag for a text: its length and a 32-bit hash. */
export function calmTag(text: string): string {
  let hash = 5381;
  for (let i = 0; i < text.length; i++) {
    hash = ((hash << 5) + hash + text.charCodeAt(i)) | 0;
  }
  return `${text.length}:${(hash >>> 0).toString(36)}`;
}

/** CLM-FR-IAHL: the most results that are kept. */
export const CALM_CACHE_LIMIT = 64;
const cache = new Map<string, CalmResult>();
const pending = new Map<string, PendingRead>();
/** CLM-FR-IAHL: the sources that each live surface shows, by owner. */
const shown = new Map<object, Set<string>>();

const isShown = (source: string) => {
  for (const sources of shown.values()) if (sources.has(source)) return true;
  return false;
};

function remember(source: string, result: CalmResult): void {
  cache.delete(source);
  cache.set(source, result);
  if (cache.size <= CALM_CACHE_LIMIT) return;
  for (const candidate of Array.from(cache.keys())) {
    if (cache.size <= CALM_CACHE_LIMIT) break;
    if (candidate !== source && !isShown(candidate)) cache.delete(candidate);
  }
}

/** CLM-FR-IAHL: the kept result of `source`. A read of it counts as a use. */
export function cachedCalm(source: string): CalmResult | undefined {
  const known = cache.get(source);
  if (known) remember(source, known);
  return known;
}

/** CLM-FR-IAHL: the sources that `owner` shows now, kept until it shows others. */
export function showCalm(owner: object, sources: Iterable<string>): void {
  shown.set(owner, new Set(sources));
}

/** `owner` shows no CALM diagram any more. */
export function releaseCalm(owner: object): void {
  shown.delete(owner);
}

/** CLM-FR-HPJT: forget each failed library load, so the next read tries again. */
export function forgetCalmTransientFailures(): void {
  for (const [source, result] of cache) {
    if (result.state === "error" && result.transient) cache.delete(source);
  }
}

/** Clears every kept result and every pending read. For tests only. */
export function resetCalmForTest(): void {
  cache.clear();
  pending.clear();
  shown.clear();
  loading = null;
}

const errorText = (error: unknown) =>
  (error instanceof Error ? error.message : String(error)).trim() || undefined;

const errorName = (error: unknown) => (error instanceof Error ? error.name : typeof error);

const loadError = (error: unknown): CalmError => ({
  state: "error",
  step: "load",
  summary: CALM_ERROR.load,
  detail: errorText(error),
  transient: true,
});

/** What a read learns on its way, for its log record. */
interface ReadFacts {
  nodes: number;
}

/**
 * CLM-FR-ODQZ: the steps of one read. Null when no caller wants the result
 * any more: a fence that changed again while the read ran does not pay for
 * the schema check and the layout of a source it no longer shows.
 */
async function runRead(
  source: string,
  stillWanted: () => boolean,
  facts: ReadFacts,
): Promise<CalmResult | null> {
  const parsed = parseCalm(source);
  if (!parsed.ok) return parsed.error;
  let engine: Engine;
  try {
    engine = await loadEngine();
  } catch (error) {
    logWarn(["frontend"], "the CALM libraries cannot be loaded", { error: errorName(error) });
    return loadError(error);
  }
  if (!stillWanted()) return null;
  let problems: string[] | null;
  try {
    problems = engine.validateCalm(parsed.document);
  } catch (error) {
    // The bundled schema did not compile: the library cannot be used.
    logWarn(["frontend"], "the CALM schema check cannot run", { error: errorName(error) });
    return loadError(error);
  }
  if (problems !== null) {
    return {
      state: "error",
      step: "schema",
      summary: CALM_ERROR.schema,
      detail: problems.join("\n"),
    };
  }
  const missing = checkReferences(parsed.document);
  if (missing) return missing;
  const graph = buildGraph(parsed.document);
  if ("state" in graph) return graph;
  facts.nodes = graph.nodes.length;
  if (!stillWanted()) return null;
  let elk;
  try {
    elk = await engine.layoutEngine();
  } catch (error) {
    logWarn(["frontend"], "the CALM layout engine cannot be loaded", { error: errorName(error) });
    return loadError(error);
  }
  try {
    const layout = await layoutGraph(elk, graph);
    return { state: "rendered", layout, tag: calmTag(source) };
  } catch (error) {
    logWarn(["frontend"], "the CALM layout failed", {
      error: errorName(error),
      nodes: facts.nodes,
    });
    return {
      state: "error",
      step: "layout",
      summary: CALM_ERROR.layout,
      detail: errorText(error),
    };
  }
}

interface PendingRead {
  result: Promise<CalmResult | null>;
  /** Every caller's interest; the read goes on while any one still holds. */
  wanted: Array<() => boolean>;
  /** The read stopped because nobody wanted it; a new caller starts afresh. */
  dropped: boolean;
}

const always = () => true;

/**
 * CLM-FR-ODQZ, CLM-FR-IAHL: the result of `source`, from what is kept or from
 * a new read. Each read that is not kept logs one record (CLM-FR-JIWR). The
 * record names the step, the number of nodes, and the duration, and holds no
 * text of the document. With `wanted`, the read can stop early and give null
 * when no caller wants it any more.
 */
export function readCalm(source: string): Promise<CalmResult>;
export function readCalm(source: string, wanted: () => boolean): Promise<CalmResult | null>;
export function readCalm(source: string, wanted?: () => boolean): Promise<CalmResult | null> {
  const known = cachedCalm(source);
  if (known) return Promise.resolve(known);
  const running = pending.get(source);
  if (running && !running.dropped) {
    running.wanted.push(wanted ?? always);
    return running.result;
  }
  const started = performance.now();
  const facts: ReadFacts = { nodes: 0 };
  const entry: PendingRead = { result: Promise.resolve(null), wanted: [wanted ?? always], dropped: false };
  const stillWanted = () => {
    if (entry.wanted.some((check) => check())) return true;
    entry.dropped = true;
    return false;
  };
  entry.result = runRead(source, stillWanted, facts)
    .catch((error): CalmResult => {
      logWarn(["frontend"], "a CALM read failed unexpectedly", { error: errorName(error) });
      return {
        state: "error",
        step: "layout",
        summary: CALM_ERROR.layout,
        detail: errorText(error),
      };
    })
    .then((result) => {
      if (pending.get(source) === entry) pending.delete(source);
      // CLM-FR-HPJT: a failed library load is not kept, so the next read of
      // the same source, on any surface, tries the load again.
      if (result && !(result.state === "error" && result.transient)) remember(source, result);
      logDebug(["frontend"], "a CALM read ended", {
        step: result === null ? "dropped" : result.state === "rendered" ? "done" : result.step,
        nodes: facts.nodes,
        durationMs: Math.round(performance.now() - started),
      });
      return result;
    });
  pending.set(source, entry);
  return entry.result;
}
