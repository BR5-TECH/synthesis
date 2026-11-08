/**
 * The Export dialog and the export request card (`EXD-export-dialog.md`,
 * `CMT-comments.md` CMT-FR-HFXL).
 *
 * The dialog is mounted through its window host and opened through its store,
 * exactly as the action control and the card open it. The backend is a strict
 * `invoke` mock, and the events are delivered by hand.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";

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

const dispatchMock = vi.fn(async () => {});
vi.mock("./DraftDiscussion/decisionDispatch", () => ({
  dispatchDecision: (...args: unknown[]) => dispatchMock(...(args as [])),
}));

import { ExportDialogHost } from "./ExportDialog";
import { ExportRequestCard } from "./ExportDialog/ExportRequestCard";
import { CommentAttachmentList } from "./CommentAttachments";
import {
  closeExportDialog,
  exportDialogRequest,
  openExportDialog,
  resetExportDialogForTest,
  setExportDialogOpening,
  setExportSessions,
  type ExportSessions,
} from "../state/exportDialog";
import { resetToasts, visibleToasts } from "../state/toasts";
import { flushLogs, resetLogBufferForTest } from "../logging";
import type { ExportRequest } from "../types";

type Handler = (args: Record<string, unknown>) => unknown;
let handlers: Record<string, Handler> = {};

function wire(next: Record<string, Handler>) {
  handlers = next;
}

beforeEach(() => {
  resetLogBufferForTest();
  resetExportDialogForTest();
  resetToasts();
  listeners.clear();
  dispatchMock.mockReset();
  invokeMock.mockReset();
  handlers = {};
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

const PATH = "docs/guide.md";
const DEST = "/Users/me/Desktop/guide.pdf";

function request(over: Partial<ExportRequest> = {}): ExportRequest {
  return {
    id: "req-1",
    path: PATH,
    format: "docx",
    discussionId: "d1",
    target: { kind: "artifact", artifactId: "notes.md" },
    state: "pending",
    createdAt: "2026-11-07T10:00:00Z",
    ...over,
  };
}

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

async function openDialog(options: Parameters<typeof openExportDialog>[1] = {}) {
  render(<ExportDialogHost />);
  await act(async () => {
    await openExportDialog(PATH, options);
  });
  return screen.findByRole("dialog", { name: "Export document" });
}

async function chooseDestination(path = DEST) {
  handlers.browse_for_save_path = () => ({ selected: { path } });
  fireEvent.click(screen.getByRole("button", { name: "Choose…" }));
  await screen.findByText(path);
}

const exportButton = () => screen.getByRole("button", { name: "Export" });

describe("the dialog", () => {
  it("EXD-FR-DPHQ, EXD-FR-IVAE: shows the file, the format, the frontmatter choice, the destination, and the two buttons", async () => {
    const dialog = await openDialog();
    expect(within(dialog).getByTestId("export-dialog-file")).toHaveTextContent("guide.md");
    expect(within(dialog).getByRole("radio", { name: ".pdf" })).toBeChecked();
    expect(within(dialog).getByRole("radio", { name: ".docx" })).not.toBeChecked();
    expect(
      within(dialog).getByRole("checkbox", { name: "Include frontmatter as a metadata table" }),
    ).not.toBeChecked();
    expect(within(dialog).getByTestId("export-destination")).toHaveTextContent(
      "No destination chosen",
    );
    expect(within(dialog).getByRole("button", { name: "Choose…" })).toBeEnabled();
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeEnabled();
    expect(within(dialog).getByRole("button", { name: "Export" })).toBeInTheDocument();
  });

  it("EXD-FR-IVAE: a request's format is the format the dialog opens with", async () => {
    handlers.get_export_request = () => request();
    const dialog = await openDialog({ request: request({ format: "docx" }) });
    expect(within(dialog).getByRole("radio", { name: ".docx" })).toBeChecked();
  });

  it("EXD-FR-BKFL: Export is disabled until a destination is chosen, and says why", async () => {
    await openDialog();
    expect(exportButton()).toBeDisabled();
    expect(exportButton()).toHaveAttribute("title", "Choose a destination first");
    await chooseDestination();
    expect(exportButton()).toBeEnabled();
  });

  it("EXD-FR-LQAW: Choose… asks for the base name with the format's extension, and a cancel keeps the destination", async () => {
    await openDialog();
    await chooseDestination();
    expect(invokeMock).toHaveBeenCalledWith("browse_for_save_path", { defaultName: "guide.pdf" });
    handlers.browse_for_save_path = () => "cancelled";
    fireEvent.click(screen.getByRole("button", { name: "Choose…" }));
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.filter((c) => c[0] === "browse_for_save_path"),
      ).toHaveLength(2),
    );
    expect(screen.getByTestId("export-destination")).toHaveTextContent(DEST);
  });

  it("EXD-FR-LQAW: a change of format replaces the destination's extension", async () => {
    await openDialog();
    await chooseDestination();
    fireEvent.click(screen.getByRole("radio", { name: ".docx" }));
    expect(screen.getByTestId("export-destination")).toHaveTextContent(
      "/Users/me/Desktop/guide.docx",
    );
    handlers.browse_for_save_path = () => ({ selected: { path: "/tmp/out" } });
    fireEvent.click(screen.getByRole("button", { name: "Choose…" }));
    await screen.findByText("/tmp/out.docx");
    // The log flush can call `append_log_records` at any moment, so the
    // browse calls are read by their command name rather than by order.
    const browses = invokeMock.mock.calls.filter(([cmd]) => cmd === "browse_for_save_path");
    expect(browses[browses.length - 1]).toEqual(["browse_for_save_path", { defaultName: "guide.docx" }]);
  });

  it("EXD-FR-ESQB, EXD-FR-QWZO: Export runs the conversion, disables every control, ignores Escape, and shows its own progress", async () => {
    const pending = deferred<unknown>();
    handlers.export_document = () => pending.promise;
    await openDialog();
    fireEvent.click(screen.getByRole("checkbox", { name: "Include frontmatter as a metadata table" }));
    await chooseDestination();
    fireEvent.click(exportButton());
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith(
        "export_document",
        expect.objectContaining({
          path: PATH,
          format: "pdf",
          includeFrontmatter: true,
          destination: DEST,
        }),
      ),
    );
    const call = invokeMock.mock.calls.find((c) => c[0] === "export_document")!;
    const exportId = (call[1] as { exportId: string }).exportId;
    expect(exportId).not.toBe("");
    expect(await screen.findByText("Starting the export…")).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: ".pdf" })).toBeDisabled();
    expect(screen.getByRole("checkbox", { name: /frontmatter/ })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Choose…" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeDisabled();
    expect(exportButton()).toBeDisabled();

    act(() => {
      emitEvent("document-export-progress", { exportId, stage: "diagrams", done: 2, total: 5 });
    });
    expect(await screen.findByText("Rendering diagrams 2 of 5…")).toBeInTheDocument();
    act(() => {
      emitEvent("document-export-progress", {
        exportId: "another",
        stage: "writing",
        done: 0,
        total: 0,
      });
    });
    expect(screen.queryByText("Writing the file…")).not.toBeInTheDocument();
    act(() => {
      emitEvent("document-export-progress", { exportId, stage: "converting", done: 0, total: 0 });
    });
    expect(await screen.findByText("Converting to PDF…")).toBeInTheDocument();

    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.getByRole("dialog", { name: "Export document" })).toBeInTheDocument();

    await act(async () => {
      pending.resolve({ destination: DEST, warnings: [] });
      await pending.promise;
    });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("EXD-FR-YTQG: a success with no warning closes the dialog and names the file in a toast", async () => {
    handlers.export_document = () => ({ destination: DEST, warnings: [] });
    await openDialog();
    await chooseDestination();
    fireEvent.click(exportButton());
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(exportDialogRequest()).toBeNull();
    const toast = visibleToasts().find((t) => t.title === "Document exported");
    expect(toast?.level).toBe("Info");
    expect(toast?.body).toContain("guide.pdf");
  });

  it("EXD-FR-YALT: a success with warnings stays open, names the diagrams, and offers Close alone", async () => {
    handlers.export_document = () => ({
      destination: DEST,
      warnings: [
        { kind: "diagram_failed", index: 2 },
        { kind: "diagram_failed", index: 4 },
      ],
    });
    await openDialog();
    await chooseDestination();
    fireEvent.click(exportButton());
    expect(await screen.findByText(/2 diagrams could not be rendered/)).toHaveTextContent(
      "diagram 2, diagram 4",
    );
    expect(screen.getByText("guide.pdf was written.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Export" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Cancel" })).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("EXD-FR-RNJL: a failure stays open, shows the error, and Export tries again", async () => {
    let attempts = 0;
    handlers.export_document = () => {
      attempts += 1;
      if (attempts === 1) return Promise.reject("conversion_failed");
      return { destination: DEST, warnings: [] };
    };
    await openDialog();
    await chooseDestination();
    fireEvent.click(exportButton());
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "The document could not be converted.",
    );
    expect(screen.getByRole("dialog", { name: "Export document" })).toBeInTheDocument();
    expect(exportButton()).toBeEnabled();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeEnabled();
    fireEvent.click(exportButton());
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(attempts).toBe(2);
  });

  it("EXD-FR-SVLO: Escape and Cancel close the dialog while no export runs", async () => {
    await openDialog();
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    await act(async () => {
      await openExportDialog(PATH);
    });
    fireEvent.click(await screen.findByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("EXD-FR-XNLZ: opening closes the window's other overlays, and another overlay closes the dialog", async () => {
    const closeOthers = vi.fn();
    setExportDialogOpening(closeOthers);
    await openDialog();
    expect(closeOthers).toHaveBeenCalledTimes(1);
    act(() => closeExportDialog("other-overlay"));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("EXD-FR-MRTI: the dialog logs the open, the start, and the failure, and no record carries a path", async () => {
    handlers.export_document = () => Promise.reject("write_failed");
    await openDialog();
    await chooseDestination();
    fireEvent.click(exportButton());
    await screen.findByRole("alert");
    flushLogs();
    await waitFor(() =>
      expect(invokeMock.mock.calls.some((c) => c[0] === "append_log_records")).toBe(true),
    );
    const logged = JSON.stringify(
      invokeMock.mock.calls.filter((c) => c[0] === "append_log_records").map((c) => c[1]),
    );
    expect(logged).toContain("export dialog opened");
    expect(logged).toContain("document export started");
    expect(logged).toContain("document export failed");
    expect(logged).toContain("write_failed");
    expect(logged).not.toContain("guide");
    expect(logged).not.toContain("/Users/me");
  });
});

describe("the write before the dialog", () => {
  function sessions(result: Awaited<ReturnType<ExportSessions["flush"]>>, error: string | null = null) {
    const flush = vi.fn(async () => result);
    const store: ExportSessions = { flush, get: () => ({ error }) };
    return { store, flush };
  }

  it("EXD-FR-CDYS: the file's buffer is written before the dialog opens", async () => {
    const order: string[] = [];
    const flush = vi.fn(async (id: string) => {
      order.push(`flush ${id}`);
      return { ok: true };
    });
    setExportDialogOpening(() => order.push("open"));
    const opened = await openExportDialog(PATH, { sessions: { flush, get: () => undefined } });
    expect(opened).toBe(true);
    expect(order).toEqual([`flush ${PATH}`, "open"]);
    expect(exportDialogRequest()?.path).toBe(PATH);
  });

  it("EXD-FR-CBIW: a write that fails opens no dialog and shows its error in a toast", async () => {
    const { store } = sessions({ ok: false, blocked: "error", artifactId: PATH }, "disk full");
    const opened = await openExportDialog(PATH, { sessions: store });
    expect(opened).toBe(false);
    expect(exportDialogRequest()).toBeNull();
    const toast = visibleToasts().find((t) => t.key === "document-export-save");
    expect(toast?.level).toBe("Error");
    expect(toast?.body).toBe("disk full");
  });

  it("EXD-FR-UDHD: a card's Export… writes the pending edit through the window's sessions, then opens the dialog with the request", async () => {
    const { store, flush } = sessions({ ok: true });
    setExportSessions(store);
    handlers.get_export_request = () => request();
    render(
      <>
        <ExportRequestCard attachment={{ kind: "exportRequest", requestId: "req-1", path: PATH }} />
        <ExportDialogHost />
      </>,
    );
    fireEvent.click(await screen.findByRole("button", { name: "Export…" }));
    const dialog = await screen.findByRole("dialog", { name: "Export document" });
    expect(flush).toHaveBeenCalledWith(PATH);
    expect(within(dialog).getByRole("radio", { name: ".docx" })).toBeChecked();
    expect(exportDialogRequest()?.request?.id).toBe("req-1");
  });

  it("EXD-FR-UDHD, EXD-FR-CBIW: a card whose write fails opens no dialog", async () => {
    const { store } = sessions({ ok: false, blocked: "conflict", artifactId: PATH });
    setExportSessions(store);
    handlers.get_export_request = () => request();
    render(
      <ExportRequestCard attachment={{ kind: "exportRequest", requestId: "req-1", path: PATH }} />,
    );
    fireEvent.click(await screen.findByRole("button", { name: "Export…" }));
    await waitFor(() =>
      expect(visibleToasts().some((t) => t.key === "document-export-save")).toBe(true),
    );
    expect(exportDialogRequest()).toBeNull();
  });
});

describe("a dialog opened from a request", () => {
  async function openFromRequest() {
    handlers.resolve_export_request = (args) => ({
      request: request({ state: args.outcome as ExportRequest["state"] }),
      commentId: "c-outcome",
    });
    return openDialog({ request: request() });
  }

  it("EXD-FR-DHAN, EXD-FR-VFFP: a success resolves the request as exported with the destination, and brings the agents back", async () => {
    handlers.export_document = () => ({ destination: DEST, warnings: [] });
    await openFromRequest();
    await chooseDestination("/Users/me/Desktop/guide.docx");
    fireEvent.click(exportButton());
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("resolve_export_request", {
        requestId: "req-1",
        outcome: "exported",
        destination: DEST,
      }),
    );
    await waitFor(() => expect(dispatchMock).toHaveBeenCalledTimes(1));
    expect(dispatchMock).toHaveBeenCalledWith(
      { id: "req-1", threadId: "d1", target: { kind: "artifact", artifactId: "notes.md" } },
      { commentId: "c-outcome" },
      undefined,
      [],
      "",
    );
  });

  it("EXD-FR-DHAN: closing with no failed attempt resolves the request as cancelled", async () => {
    await openFromRequest();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("resolve_export_request", {
        requestId: "req-1",
        outcome: "cancelled",
      }),
    );
  });

  it("EXD-FR-DHAN: closing after a failed attempt resolves the request as failed with the last error", async () => {
    handlers.export_document = () => Promise.reject("write_failed");
    await openFromRequest();
    await chooseDestination("/Users/me/Desktop/guide.docx");
    fireEvent.click(exportButton());
    await screen.findByRole("alert");
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("resolve_export_request", {
        requestId: "req-1",
        outcome: "failed",
        error: "The file could not be written to the destination.",
      }),
    );
  });

  it("EXD-FR-DHAN, EXD-FR-XNLZ: a dialog that another overlay closes still resolves the request as cancelled", async () => {
    await openFromRequest();
    act(() => closeExportDialog("other-overlay"));
    await waitFor(() => expect(exportDialogRequest()).toBeNull());
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("resolve_export_request", {
        requestId: "req-1",
        outcome: "cancelled",
      }),
    );
  });

  it("EXD-FR-DHAN: a dialog that closes after a success with warnings resolves the request as exported", async () => {
    // The request asks for `docx`, so the destination carries that extension.
    const docxDest = "/Users/me/Desktop/guide.docx";
    handlers.export_document = () => ({
      destination: docxDest,
      warnings: [{ kind: "diagram_failed", index: 1 }],
    });
    await openFromRequest();
    await chooseDestination(docxDest);
    fireEvent.click(exportButton());
    await screen.findByRole("button", { name: "Close" });
    act(() => closeExportDialog("window"));
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("resolve_export_request", {
        requestId: "req-1",
        outcome: "exported",
        destination: docxDest,
      }),
    );
    const resolutions = invokeMock.mock.calls.filter(([cmd]) => cmd === "resolve_export_request");
    expect(resolutions).toHaveLength(1);
  });

  it("EXD-FR-VFFP: a resolution that fails shows its error in a toast and brings nobody back", async () => {
    handlers.resolve_export_request = () => Promise.reject("already_resolved");
    await openDialog({ request: request() });
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() =>
      expect(visibleToasts().some((t) => t.key === "export-request-req-1")).toBe(true),
    );
    expect(visibleToasts().find((t) => t.key === "export-request-req-1")?.level).toBe("Error");
    expect(dispatchMock).not.toHaveBeenCalled();
  });

  it("EXD-FR-DHAN: a dialog opened from the action control resolves nothing", async () => {
    await openDialog();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(invokeMock.mock.calls.some((c) => c[0] === "resolve_export_request")).toBe(false);
  });
});

describe("the export request card", () => {
  it("EXD-FR-UQUD: shows the file name, the requested format, and the state it reads", async () => {
    handlers.get_export_request = () => request({ format: "pdf" });
    render(
      <ExportRequestCard
        attachment={{ kind: "exportRequest", requestId: "req-1", path: PATH, format: "pdf" }}
      />,
    );
    const card = screen.getByTestId("export-request-card");
    expect(card).toHaveTextContent("guide.md");
    expect(card).toHaveTextContent("PDF");
    expect(await within(card).findByText("Waiting for you")).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledWith("get_export_request", { requestId: "req-1" });
  });

  it("EXD-FR-UQUD: a request with no format reads any format", async () => {
    handlers.get_export_request = () => request({ format: undefined });
    render(
      <ExportRequestCard attachment={{ kind: "exportRequest", requestId: "req-1", path: PATH }} />,
    );
    expect(await screen.findByText("Any format")).toBeInTheDocument();
  });

  it("EXD-FR-WLUG: the card follows its request through each state, and only a pending card has a button", async () => {
    handlers.get_export_request = () => request();
    render(
      <ExportRequestCard attachment={{ kind: "exportRequest", requestId: "req-1", path: PATH }} />,
    );
    expect(await screen.findByRole("button", { name: "Export…" })).toBeInTheDocument();
    act(() => {
      emitEvent("export-request-changed", request({ id: "other", state: "cancelled" }));
    });
    expect(screen.getByText("Waiting for you")).toBeInTheDocument();
    act(() => {
      emitEvent("export-request-changed", request({ state: "exported", destination: DEST }));
    });
    expect(await screen.findByText(`Exported to ${DEST}`)).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Export…" })).not.toBeInTheDocument();
    act(() => {
      emitEvent("export-request-changed", request({ state: "cancelled" }));
    });
    expect(await screen.findByText("Cancelled")).toBeInTheDocument();
    act(() => {
      emitEvent("export-request-changed", request({ state: "failed", error: "disk full" }));
    });
    expect(await screen.findByText("Failed: disk full")).toBeInTheDocument();
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });

  it("EXD-FR-UQUD: a request that can not be read says so and offers no button", async () => {
    handlers.get_export_request = () => Promise.reject("request_not_found");
    render(
      <ExportRequestCard attachment={{ kind: "exportRequest", requestId: "gone", path: PATH }} />,
    );
    expect(
      await screen.findByText("This export request is no longer available"),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button")).not.toBeInTheDocument();
  });

  it("CMT-FR-HFXL: a comment's export_request attachment renders as the card, with no chip", async () => {
    handlers.get_export_request = () => request();
    render(
      <CommentAttachmentList
        threadId="d1"
        attachments={[{ kind: "exportRequest", requestId: "req-1", path: PATH, format: "docx" }]}
      />,
    );
    expect(await screen.findByTestId("export-request-card")).toHaveTextContent("DOCX");
    expect(screen.queryByRole("link")).not.toBeInTheDocument();
    expect(await screen.findByText("Waiting for you")).toBeInTheDocument();
  });
});
