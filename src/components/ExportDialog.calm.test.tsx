/**
 * The CALM pictures of an export (`EXD-export-dialog.md` EXD-FR-ESQB,
 * EXD-FR-UKLK, EXD-FR-QWZO, EXD-FR-ZQMX).
 *
 * The dialog lists the CALM sources of the saved file, makes the print
 * drawing of each with the real CALM read, and gives the drawings to the
 * export. The backend is a strict `invoke` mock.
 */
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const listeners = new Map<string, Set<(e: { payload: unknown }) => void>>();
function emitEvent(name: string, payload: unknown) {
  [...(listeners.get(name) ?? [])].forEach((h) => h({ payload }));
}
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, cb: (e: { payload: unknown }) => void) => {
    const set = listeners.get(event) ?? new Set();
    set.add(cb);
    listeners.set(event, set);
    return () => set.delete(cb);
  }),
}));

import { ExportDialogHost } from "./ExportDialog";
import {
  closeExportDialog,
  openExportDialog,
  resetExportDialogForTest,
} from "../state/exportDialog";
import { resetToasts, visibleToasts } from "../state/toasts";
import { flushLogs, resetLogBufferForTest } from "../logging";
import { readCalm } from "../calm/read";
import { CALM_TEMPLATES } from "../calm/templates";
import type { CalmPicture } from "../types";

type Handler = (args: Record<string, unknown>) => unknown;
let handlers: Record<string, Handler> = {};

const PATH = "docs/guide.md";
const DEST = "/Users/me/Desktop/guide.pdf";
const THREE_TIER = CALM_TEMPLATES.find((t) => t.key === "three-tier")!.source;
const SECRET = "secret-architecture-word";
/** A CALM source that is not valid JSON, with a word that must not reach a log. */
const BROKEN = `{ "nodes": [ "${SECRET}" `;
/** A CALM source whose read gives a graph with no nodes. */
const EMPTY = JSON.stringify({ nodes: [], relationships: [] });

/**
 * The first read loads the CALM libraries, and the first drawing loads the
 * canvas library. Both loads happen here, under the hook limit.
 */
beforeAll(async () => {
  await readCalm(CALM_TEMPLATES[0].source);
  await import("./CalmDiagram/printSvg");
}, 30_000);

beforeEach(() => {
  resetLogBufferForTest();
  resetExportDialogForTest();
  resetToasts();
  listeners.clear();
  invokeMock.mockReset();
  handlers = { list_export_calm_sources: () => [] };
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown>) => {
    if (cmd === "append_log_records") return undefined;
    const handler = handlers[cmd];
    if (!handler) throw new Error(`unexpected command ${cmd}`);
    return handler(args ?? {});
  });
});

afterEach(() => {
  cleanup();
  resetExportDialogForTest();
});

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

interface LoggedRecord {
  level: string;
  message: string;
  fields: Record<string, unknown>;
}

/** The records that reached the backend log. */
async function records(): Promise<LoggedRecord[]> {
  flushLogs();
  await waitFor(() =>
    expect(invokeMock.mock.calls.some((c) => c[0] === "append_log_records")).toBe(true),
  );
  return invokeMock.mock.calls
    .filter((c) => c[0] === "append_log_records")
    .flatMap((c) => (c[1] as { records: LoggedRecord[] }).records);
}

async function startExport() {
  render(<ExportDialogHost />);
  await act(async () => {
    await openExportDialog(PATH, {});
  });
  await screen.findByRole("dialog", { name: "Export document" });
  handlers.browse_for_save_path = () => ({ selected: { path: DEST } });
  fireEvent.click(screen.getByRole("button", { name: "Choose…" }));
  await screen.findByText(DEST);
  fireEvent.click(screen.getByRole("button", { name: "Export" }));
}

/** The arguments of the one `export_document` call. */
async function exportCall(): Promise<{ exportId: string; calmPictures: CalmPicture[] }> {
  await waitFor(
    () => expect(invokeMock.mock.calls.some((c) => c[0] === "export_document")).toBe(true),
    { timeout: 10_000 },
  );
  return invokeMock.mock.calls.find((c) => c[0] === "export_document")![1];
}

async function loggedText(): Promise<string> {
  flushLogs();
  await waitFor(() =>
    expect(invokeMock.mock.calls.some((c) => c[0] === "append_log_records")).toBe(true),
  );
  return JSON.stringify(
    invokeMock.mock.calls.filter((c) => c[0] === "append_log_records").map((c) => c[1]),
  );
}

describe("the CALM pictures of an export", () => {
  it("EXD-FR-ESQB, EXD-FR-QWZO: while the dialog draws, every control is disabled and Escape keeps the dialog open", async () => {
    const listing = deferred<unknown>();
    handlers.list_export_calm_sources = () => listing.promise;
    handlers.export_document = () => ({ destination: DEST, warnings: [] });
    await startExport();
    await waitFor(() =>
      expect(invokeMock.mock.calls.some((c) => c[0] === "list_export_calm_sources")).toBe(true),
    );
    expect(screen.getByText("Starting the export…")).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: ".pdf" })).toBeDisabled();
    expect(screen.getByRole("checkbox", { name: /frontmatter/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Choose…" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Export" })).toBeDisabled();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.getByRole("dialog", { name: "Export document" })).toBeInTheDocument();
    expect(invokeMock.mock.calls.some((c) => c[0] === "export_document")).toBe(false);
    await act(async () => {
      listing.resolve([]);
      await listing.promise;
    });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("EXD-FR-UKLK, EXD-FR-OOTQ: another overlay that closes the dialog while it draws leaves the export to finish with a toast", async () => {
    const listing = deferred<unknown>();
    handlers.list_export_calm_sources = () => listing.promise;
    handlers.export_document = () => ({ destination: DEST, warnings: [] });
    await startExport();
    await waitFor(() =>
      expect(invokeMock.mock.calls.some((c) => c[0] === "list_export_calm_sources")).toBe(true),
    );
    act(() => closeExportDialog("other-overlay"));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await act(async () => {
      listing.resolve([THREE_TIER]);
      await listing.promise;
    });
    const call = await exportCall();
    expect(call.calmPictures.map((p) => p.source)).toEqual([THREE_TIER]);
    await waitFor(() =>
      expect(visibleToasts().find((t) => t.title === "Document exported")?.body).toBe(
        "guide.pdf was written.",
      ),
    );
  });

  it(
    "EXD-FR-ESQB, EXD-FR-UKLK, EXD-FR-QWZO: Export lists the CALM sources, draws each, shows the drawing, and gives the drawings to the export",
    async () => {
      const pending = deferred<unknown>();
      handlers.list_export_calm_sources = () => [THREE_TIER, BROKEN];
      handlers.export_document = () => pending.promise;
      await startExport();
      const call = await exportCall();
      expect(invokeMock).toHaveBeenCalledWith("list_export_calm_sources", { path: PATH });
      const list = invokeMock.mock.calls.findIndex((c) => c[0] === "list_export_calm_sources");
      const run = invokeMock.mock.calls.findIndex((c) => c[0] === "export_document");
      expect(list).toBeLessThan(run);
      // EXD-FR-UKLK: the source with an error gets no picture.
      expect(call.calmPictures.map((p) => p.source)).toEqual([THREE_TIER]);
      expect(call.calmPictures[0].svg.startsWith("<svg")).toBe(true);
      expect(call.calmPictures[0].svg).toContain("Shop API");
      // EXD-FR-QWZO: the drawing shows until the backend reports its stage.
      expect(await screen.findByText("Drawing CALM diagrams 2 of 2…")).toBeInTheDocument();
      expect(screen.getByRole("button", { name: "Export" })).toBeDisabled();
      act(() => {
        emitEvent("document-export-progress", {
          exportId: call.exportId,
          stage: "diagrams",
          done: 1,
          total: 2,
        });
      });
      expect(await screen.findByText("Rendering diagrams 1 of 2…")).toBeInTheDocument();
      pending.resolve({ destination: DEST, warnings: [{ kind: "diagram_failed", index: 2 }] });
      // EXD-FR-UKLK: the export states the diagram with no picture (EXD-FR-YALT).
      expect(await screen.findByText(/stayed as code: diagram 2\./)).toBeInTheDocument();
    },
    20_000,
  );

  it("EXD-FR-UKLK: a file with no CALM fence gives the export no picture and shows no drawing", async () => {
    const pending = deferred<unknown>();
    handlers.export_document = () => pending.promise;
    await startExport();
    const call = await exportCall();
    expect(call.calmPictures).toEqual([]);
    expect(await screen.findByText("Starting the export…")).toBeInTheDocument();
    expect(screen.queryByText(/Drawing CALM diagrams/)).toBeNull();
    pending.resolve({ destination: DEST, warnings: [] });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("EXD-FR-UKLK: a list call that fails is an export that fails, with its typed error", async () => {
    handlers.list_export_calm_sources = () => Promise.reject("not_found");
    await startExport();
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The file is not in the project any more.",
    );
    expect(invokeMock.mock.calls.some((c) => c[0] === "export_document")).toBe(false);
    expect(screen.getByRole("button", { name: "Export" })).toBeEnabled();
    expect(await loggedText()).toContain('"failure":"not_found"');
  });

  it(
    "EXD-FR-ZQMX: the drawing logs the counts and the step of each source with no picture, and no CALM text",
    async () => {
      handlers.list_export_calm_sources = () => [THREE_TIER, BROKEN, EMPTY];
      handlers.export_document = () => ({ destination: DEST, warnings: [] });
      await startExport();
      await exportCall();
      const all = await records();
      const drawn = all.filter((r) => r.message === "the CALM pictures of a document export were drawn");
      expect(drawn).toHaveLength(1);
      expect(drawn[0].level).toBe("DEBUG");
      expect(drawn[0].fields).toEqual({ sources: 3, pictures: 1, durationMs: expect.any(Number) });
      const missing = all.filter((r) => r.message === "a CALM diagram of a document export has no picture");
      expect(missing.map((r) => [r.level, r.fields])).toEqual([
        ["WARN", { step: "parse" }],
        ["WARN", { step: "no_nodes" }],
      ]);
      const logged = JSON.stringify(all);
      expect(logged).not.toContain(SECRET);
      expect(logged).not.toContain("Shop API");
    },
    20_000,
  );
});
