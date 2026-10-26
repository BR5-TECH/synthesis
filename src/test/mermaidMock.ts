/**
 * A stand-in for the Mermaid library in jsdom, which has no layout for Mermaid
 * to measure with.
 *
 * Each test file mocks the module with it:
 *
 *     vi.mock("mermaid", async () => (await import("../test/mermaidMock")).mermaidModule);
 *
 * A render returns a small SVG that names its source, its theme, and its id.
 * Like real Mermaid output, it is HTML-serialised: its label holds a `<br>`
 * with no close and a `&nbsp;`, which an XML parser rejects (EDT-FR-LXFD).
 * A source that contains `INVALID` fails with a parse error, in `parse` and in
 * `render` alike, as the library does. `holdNextRender`
 * makes the next render wait until the test releases it, so a test can order
 * renders without a timer.
 */
import { vi } from "vitest";

interface Held {
  release: () => void;
}

let theme = "default";
const holds: Array<{ promise: Promise<void> }> = [];

/** What the next render does instead of the default, when a test sets it. */
let nextRender: ((id: string, source: string) => Promise<{ svg: string }>) | null = null;
/** What the next parse does instead of the default, when a test sets it. */
let nextParse: ((source: string) => Promise<unknown>) | null = null;

function escapeXml(text: string): string {
  return text.replace(/[&<>"]/g, (ch) =>
    ch === "&" ? "&amp;" : ch === "<" ? "&lt;" : ch === ">" ? "&gt;" : "&quot;",
  );
}

/** The SVG the stand-in renders for one source, as markup. */
export function fakeSvg(id: string, source: string, themeName = theme): string {
  return (
    `<svg xmlns="http://www.w3.org/2000/svg" id="${id}" data-theme="${themeName}"` +
    ` viewBox="0 0 200 100" data-source="${escapeXml(source)}"><text>${escapeXml(source)}</text>` +
    '<foreignObject width="10" height="10"><div xmlns="http://www.w3.org/1999/xhtml">' +
    '<span class="nodeLabel"><p>Label<br>two&nbsp;lines</p></span></div></foreignObject></svg>'
  );
}

export const mermaidApi = {
  initialize: vi.fn((config: { theme?: string }) => {
    theme = config.theme ?? "default";
  }),
  parse: vi.fn(async (source: string): Promise<unknown> => {
    const custom = nextParse;
    nextParse = null;
    if (custom) return custom(source);
    if (source.includes("INVALID")) {
      throw new Error("Parse error on line 1: INVALID");
    }
    return { diagramType: "flowchart" };
  }),
  render: vi.fn(async (id: string, source: string): Promise<{ svg: string }> => {
    const held = holds.shift();
    if (held) await held.promise;
    const custom = nextRender;
    nextRender = null;
    if (custom) return custom(id, source);
    if (source.includes("INVALID")) {
      throw new Error("Parse error on line 1: INVALID");
    }
    return { svg: fakeSvg(id, source) };
  }),
};

export const mermaidModule = { default: mermaidApi };

/** The next render waits until `release` is called. */
export function holdNextRender(): Held {
  let release!: () => void;
  const promise = new Promise<void>((resolve) => {
    release = resolve;
  });
  holds.push({ promise });
  return { release };
}

/** The next render does `impl` instead of the default. */
export function overrideNextRender(
  impl: (id: string, source: string) => Promise<{ svg: string }>,
): void {
  nextRender = impl;
}

/** The next parse does `impl` instead of the default. */
export function overrideNextParse(impl: (source: string) => Promise<unknown>): void {
  nextParse = impl;
}

/** Restores the stand-in to its first state. Call it in `beforeEach`. */
export function resetMermaidMock(): void {
  theme = "default";
  holds.length = 0;
  nextRender = null;
  nextParse = null;
  mermaidApi.initialize.mockClear();
  mermaidApi.parse.mockClear();
  mermaidApi.render.mockClear();
}
