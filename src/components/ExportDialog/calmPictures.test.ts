import { beforeAll, beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

import { logWarn } from "../../logging";
import { readCalm } from "../../calm/read";
import { CALM_TEMPLATES } from "../../calm/templates";
import { drawCalmPictures } from "./calmPictures";

const sourceOf = (key: string) => CALM_TEMPLATES.find((t) => t.key === key)!.source;

beforeAll(async () => {
  // The first read loads the CALM libraries, and the first drawing loads the
  // canvas library.
  await readCalm(CALM_TEMPLATES[0].source);
  await import("../CalmDiagram/printSvg");
}, 30_000);

beforeEach(() => {
  invokeMock.mockReset();
  vi.mocked(logWarn).mockClear();
});

function listing(sources: string[]) {
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "list_export_calm_sources") return sources;
    throw new Error(`unexpected command ${cmd}`);
  });
}

describe("the drawing of the CALM pictures", () => {
  it(
    "EXD-FR-UKLK, EXD-FR-QWZO: each source is drawn in order, and the progress counts each one",
    async () => {
      listing([sourceOf("minimal"), "not json", sourceOf("flow")]);
      const seen: Array<[number, number]> = [];
      const pictures = await drawCalmPictures("docs/a.md", (done, total) => seen.push([done, total]));
      expect(seen).toEqual([
        [0, 3],
        [1, 3],
        [2, 3],
        [3, 3],
      ]);
      expect(pictures.map((p) => p.source)).toEqual([sourceOf("minimal"), sourceOf("flow")]);
      expect(pictures.every((p) => p.svg.startsWith("<svg"))).toBe(true);
    },
    15_000,
  );

  it("EXD-FR-UKLK: a file with no CALM source gives no picture", async () => {
    listing([]);
    const seen: Array<[number, number]> = [];
    expect(await drawCalmPictures("docs/a.md", (done, total) => seen.push([done, total]))).toEqual([]);
    expect(seen).toEqual([[0, 0]]);
  });

  it("EXD-FR-UKLK: a list call that fails rejects with its typed error", async () => {
    invokeMock.mockImplementation(async () => Promise.reject("not_doc_artifact"));
    await expect(drawCalmPictures("docs/a.md", () => {})).rejects.toBe("not_doc_artifact");
  });

  it("EXD-FR-UKLK, EXD-FR-ZQMX: a drawing that fails gives no picture, logs the step, and the next source is still drawn", async () => {
    vi.resetModules();
    vi.doMock("../CalmDiagram/printSvg", async (original) => {
      const actual = await original<typeof import("../CalmDiagram/printSvg")>();
      let first = true;
      return {
        ...actual,
        calmPrintSvg: (...args: Parameters<typeof actual.calmPrintSvg>) => {
          if (first) {
            first = false;
            throw new Error("the drawing failed");
          }
          return actual.calmPrintSvg(...args);
        },
      };
    });
    try {
      const fresh = await import("./calmPictures");
      const logging = await import("../../logging");
      listing([sourceOf("minimal"), sourceOf("flow")]);
      const seen: Array<[number, number]> = [];
      const pictures = await fresh.drawCalmPictures("docs/a.md", (done, total) => seen.push([done, total]));
      expect(pictures.map((p) => p.source)).toEqual([sourceOf("flow")]);
      expect(seen).toEqual([
        [0, 2],
        [1, 2],
        [2, 2],
      ]);
      expect(logging.logWarn).toHaveBeenCalledWith(
        ["frontend"],
        "a CALM diagram of a document export has no picture",
        { step: "draw" },
      );
    } finally {
      vi.doUnmock("../CalmDiagram/printSvg");
      vi.resetModules();
    }
  }, 30_000);

  it("EXD-FR-ZQMX: a source with an error logs its step alone", async () => {
    listing(["not json"]);
    expect(await drawCalmPictures("docs/a.md", () => {})).toEqual([]);
    expect(logWarn).toHaveBeenCalledWith(["frontend"], "a CALM diagram of a document export has no picture", {
      step: "parse",
    });
  });
});
