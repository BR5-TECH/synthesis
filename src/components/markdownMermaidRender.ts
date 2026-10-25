/**
 * EDT-FR-JIBV / EDT-FR-JBZU: the rendering of a diagram fence's source into an
 * SVG, apart from any editing surface.
 *
 * The Mermaid library is bundled with the application and loaded on first use,
 * so a file without a diagram costs nothing and a file with one renders offline.
 * Diagram source is untrusted: every render runs at Mermaid's strict security
 * level, and what comes back is parsed in an inert document, sanitised, and
 * handed over as a node — the caller never assigns markup through `innerHTML`.
 */
import type * as Mermaid from "mermaid";

import { logDebug, logWarn } from "../logging";

type MermaidModule = typeof Mermaid;

/** The two themes the application sets on `<html data-theme>`. */
export type DiagramTheme = "light" | "dark";

/** What one render of one source produced. */
export type DiagramResult =
  | {
      state: "rendered";
      svg: string;
      /** The id Mermaid scoped the SVG's own ids and styles to. */
      id: string;
      /** A short tag for the SVG, so a surface knows when it changed. */
      tag: string;
    }
  | {
      state: "error";
      /** EDT-FR-XKMP: the step that failed, as one sentence. */
      summary: string;
      /** EDT-FR-XKMP: the reason the failed step gave, when it gave one. */
      detail?: string;
      /**
       * The library could not be loaded. Such a result is shown, but it is
       * forgotten at the next edit (`forgetTransientFailures`), so a later
       * render tries the load again.
       */
      transient?: boolean;
    };

/** A short, stable tag for a text: its length and a 32-bit hash. */
export function textTag(text: string): string {
  let hash = 5381;
  for (let i = 0; i < text.length; i++) {
    hash = ((hash << 5) + hash + text.charCodeAt(i)) | 0;
  }
  return `${text.length}:${(hash >>> 0).toString(36)}`;
}

let loading: Promise<MermaidModule> | null = null;

/**
 * The Mermaid module, loaded once. A failed load is forgotten, so a later
 * render tries again rather than failing for the rest of the session.
 */
export function loadMermaid(): Promise<MermaidModule> {
  if (loading === null) {
    loading = import("mermaid");
    loading.catch(() => {
      loading = null;
    });
  }
  return loading;
}

/** EDT-FR-JIBV: the theme the application currently shows. */
export function currentTheme(): DiagramTheme {
  if (typeof document === "undefined") return "light";
  return document.documentElement.dataset.theme === "dark" ? "dark" : "light";
}

/**
 * EDT-FR-JIBV: calls `listener` each time the application theme changes, and
 * returns the function that stops it.
 */
export function subscribeTheme(listener: (theme: DiagramTheme) => void): () => void {
  if (typeof MutationObserver === "undefined" || typeof document === "undefined") {
    return () => {};
  }
  let last = currentTheme();
  const observer = new MutationObserver(() => {
    const next = currentTheme();
    if (next === last) return;
    last = next;
    listener(next);
  });
  observer.observe(document.documentElement, {
    attributes: true,
    attributeFilter: ["data-theme"],
  });
  return () => observer.disconnect();
}

/**
 * Results by theme and source. Bounded, because every keystroke in a fence is a
 * new source and the old ones are not shown again.
 */
const CACHE_LIMIT = 64;
const cache = new Map<string, DiagramResult>();
interface PendingRender {
  result: Promise<DiagramResult | null>;
  /** Every caller's interest; the render runs when any one still holds. */
  wanted: Array<() => boolean>;
}
const pending = new Map<string, PendingRender>();

const cacheKey = (source: string, theme: DiagramTheme) => `${theme}\u0000${source}`;

/**
 * The result already known for this source and theme, if there is one. A read
 * counts as a use, so a diagram that is on screen is the last one evicted.
 */
export function cachedDiagram(
  source: string,
  theme: DiagramTheme,
): DiagramResult | undefined {
  const key = cacheKey(source, theme);
  const known = cache.get(key);
  if (known) remember(key, known);
  return known;
}

/**
 * The diagrams each live surface shows, by owner. A shown diagram is never
 * evicted: with more diagrams on screen than the cache holds, evicting one
 * would make its surface ask for it again, which evicts the next, without end.
 */
const shown = new Map<object, Set<string>>();

/** EDT-FR-MOYS: the diagrams `owner` shows now, kept until it shows others. */
export function showDiagrams(
  owner: object,
  diagrams: Iterable<readonly [source: string, theme: DiagramTheme]>,
): void {
  const keys = new Set<string>();
  for (const [source, theme] of diagrams) keys.add(cacheKey(source, theme));
  shown.set(owner, keys);
}

/** `owner` shows no diagram any more. */
export function releaseDiagrams(owner: object): void {
  shown.delete(owner);
}

function isShown(key: string): boolean {
  for (const keys of shown.values()) if (keys.has(key)) return true;
  return false;
}

function remember(key: string, result: DiagramResult): void {
  cache.delete(key);
  cache.set(key, result);
  if (cache.size <= CACHE_LIMIT) return;
  for (const candidate of Array.from(cache.keys())) {
    if (cache.size <= CACHE_LIMIT) break;
    if (candidate !== key && !isShown(candidate)) cache.delete(candidate);
  }
}

/** Forgets each failed library load, so the next render tries it again. */
export function forgetTransientFailures(): void {
  for (const [key, result] of cache) {
    if (result.state === "error" && result.transient) cache.delete(key);
  }
}

/**
 * Mermaid keeps one global configuration and its renders share temporary DOM,
 * so renders run one at a time, in the order they were asked for.
 */
let queue: Promise<unknown> = Promise.resolve();
let configuredTheme: DiagramTheme | null = null;
let nextId = 0;

/** EDT-FR-XKMP: one sentence for each step that can fail. */
export const DIAGRAM_ERROR = {
  syntax: "The Mermaid source is not valid.",
  draw: "Mermaid cannot draw the diagram.",
  read: "The application cannot read the drawing that Mermaid made.",
  load: "The diagram library cannot be loaded.",
} as const;

type DiagramStep = keyof typeof DIAGRAM_ERROR;

/** The trimmed text of an error, or undefined when it has none. */
function errorDetail(error: unknown): string | undefined {
  const text = error instanceof Error ? error.message : String(error);
  return text.trim() || undefined;
}

function diagramError(
  step: DiagramStep,
  detail: string | undefined,
): Extract<DiagramResult, { state: "error" }> {
  return detail === undefined
    ? { state: "error", summary: DIAGRAM_ERROR[step] }
    : { state: "error", summary: DIAGRAM_ERROR[step], detail };
}

async function renderNow(
  source: string,
  theme: DiagramTheme,
): Promise<DiagramResult> {
  let mermaid: MermaidModule["default"];
  try {
    mermaid = (await loadMermaid()).default;
  } catch (error) {
    const detail = errorDetail(error);
    logWarn(["frontend"], "the diagram library could not be loaded", {
      error: detail ?? "",
    });
    return { ...diagramError("load", detail), transient: true };
  }
  const id = `synthesis-mermaid-${++nextId}`;
  const started = performance.now();
  // The step that runs now, so a failure names it (EDT-FR-XKMP).
  let step: DiagramStep = "draw";
  try {
    if (configuredTheme !== theme) {
      configuredTheme = null;
      mermaid.initialize({
        startOnLoad: false,
        // EDT-FR-JBZU: the source is untrusted, so no script, no click
        // handler, and no unsanitised label markup reaches the document.
        securityLevel: "strict",
        suppressErrorRendering: true,
        theme: theme === "dark" ? "dark" : "default",
      });
      configuredTheme = theme;
    }
    step = "syntax";
    await mermaid.parse(source);
    step = "draw";
    const { svg } = await mermaid.render(id, source);
    // EDT-FR-XTEB: an SVG that cannot be shown is an error, not an empty
    // figure.
    const read = readDiagramSvg(svg, id);
    if ("reason" in read) {
      logDebug(["frontend"], "a diagram rendered to an SVG that cannot be read", {
        length: source.length,
        svgLength: svg.length,
        theme,
      });
      return diagramError("read", read.reason);
    }
    logDebug(["frontend"], "a diagram rendered", {
      length: source.length,
      svgLength: svg.length,
      theme,
      ms: Math.round(performance.now() - started),
    });
    return { state: "rendered", svg, id, tag: textTag(svg) };
  } catch (error) {
    // The message can quote the source, which is user content, so only its
    // shape is logged.
    logDebug(["frontend"], "a diagram did not render", {
      step,
      length: source.length,
      theme,
    });
    return diagramError(step, errorDetail(error));
  } finally {
    // Mermaid measures in a temporary container; a failed render can leave it.
    document.getElementById(`d${id}`)?.remove();
  }
}

/**
 * EDT-FR-MOYS: the diagram one source describes, in one theme.
 *
 * `wanted` is asked again when the render's turn comes. A source nobody shows
 * any more — an intermediate state of a fence being typed — is then skipped
 * rather than rendered, so a burst of edits costs one render, not one each.
 * A skipped render resolves to `null` and is not remembered.
 */
export function renderDiagram(
  source: string,
  theme: DiagramTheme,
  wanted: () => boolean = () => true,
): Promise<DiagramResult | null> {
  const key = cacheKey(source, theme);
  const known = cache.get(key);
  if (known) return Promise.resolve(known);
  const inFlight = pending.get(key);
  if (inFlight) {
    inFlight.wanted.push(wanted);
    return inFlight.result;
  }
  const entry: PendingRender = { result: Promise.resolve(null), wanted: [wanted] };
  const run = queue.then(async (): Promise<DiagramResult | null> => {
    try {
      if (!entry.wanted.some((stillWanted) => stillWanted())) return null;
    } catch (error) {
      logDebug(["frontend"], "a diagram interest check failed", {
        error: error instanceof Error ? error.name : typeof error,
      });
      return null;
    }
    const result = await renderNow(source, theme);
    remember(key, result);
    return result;
  });
  entry.result = run.finally(() => {
    pending.delete(key);
  });
  queue = entry.result.catch(() => undefined);
  pending.set(key, entry);
  return entry.result;
}

let copies = 0;

/**
 * Elements that can run script, load a document, retarget a link, or change
 * the document they are put in (`base`, `meta`). A `template` goes too: its
 * content is a separate fragment that the sanitising pass does not walk.
 */
const UNSAFE_ELEMENTS = [
  "script",
  "iframe",
  "object",
  "embed",
  "link",
  "animate",
  "set",
  "base",
  "meta",
  "form",
  "template",
];

/** A style that imports, also when a CSS escape spells the at-rule. */
const IMPORTING_STYLE = /@(?:import|\\)/i;

/** An attribute that can run script or load a script URL. */
function unsafeAttribute(name: string, value: string): boolean {
  const lower = name.toLowerCase();
  if (lower.startsWith("on")) return true;
  if (["href", "xlink:href", "src", "action", "formaction"].includes(lower)) {
    // Browsers drop control characters and whitespace inside a scheme.
    // eslint-disable-next-line no-control-regex
    const compact = value.replace(/[\u0000-\u0020]/g, "").toLowerCase();
    // A raster image is safe; an SVG image is a document that can hold script.
    if (/^data:image\/(?!svg)/.test(compact)) return false;
    return /^(javascript|data|vbscript):/.test(compact);
  }
  if (lower === "style") {
    return IMPORTING_STYLE.test(value) || /url\(\s*["']?\s*javascript:/i.test(value);
  }
  return false;
}

/**
 * EDT-FR-JBZU, EDT-FR-LXFD: an SVG element for a rendered diagram, imported
 * into this document, or the reason there is none. The element is new on each
 * call, so two surfaces that show one diagram never share a node.
 *
 * Mermaid writes its SVG with HTML serialisation: a label in a
 * `foreignObject` holds `<br>` and `&nbsp;`, which are not XML. So the markup
 * is parsed as HTML, in a document that `DOMParser` makes inert: no script in
 * it runs and nothing in it loads.
 *
 * Mermaid has already sanitised the SVG at its strict level. This removes, as a
 * second layer, every element that can run script, load a document, or change
 * this document, every event attribute, every script URL, and every style that
 * imports. Each copy
 * also gets its own ids, because one diagram shown twice would otherwise put
 * each id, and each `url(#…)` that points at it, in the page two times.
 */
function readDiagramSvg(svg: string, id: string): { element: Element } | { reason: string } {
  const unique = `${id}-c${++copies}`;
  const scoped = id ? svg.split(id).join(unique) : svg;
  const parsed = new DOMParser().parseFromString(scoped, "text/html");
  const root = parsed.body.firstElementChild;
  // The HTML parser puts an `<svg>` tag in the SVG namespace.
  if (!(root instanceof SVGElement) || root.localName !== "svg") {
    return { reason: "The output of Mermaid has no <svg> element." };
  }
  // By local name, so a prefixed element (`svg:script`) is caught too.
  for (const el of Array.from(root.getElementsByTagName("*"))) {
    const name = el.localName.toLowerCase();
    if (UNSAFE_ELEMENTS.includes(name)) el.remove();
    else if (name === "style" && IMPORTING_STYLE.test(el.textContent ?? "")) el.remove();
  }
  for (const el of [root, ...Array.from(root.getElementsByTagName("*"))]) {
    for (const attr of Array.from(el.attributes)) {
      if (unsafeAttribute(attr.name, attr.value)) el.removeAttribute(attr.name);
    }
  }
  return { element: document.importNode(root, true) };
}

/** EDT-FR-JBZU: the SVG element for a rendered diagram, or null. */
export function diagramElement(svg: string, id: string): Element | null {
  const read = readDiagramSvg(svg, id);
  return "element" in read ? read.element : null;
}

/** Test seam: forgets every result, the loaded library, and the queue. */
export function resetMermaidForTest(): void {
  loading = null;
  cache.clear();
  pending.clear();
  queue = Promise.resolve();
  configuredTheme = null;
  nextId = 0;
  copies = 0;
  shown.clear();
}
