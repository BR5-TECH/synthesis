/**
 * How the Export dialog closes (`EXD-export-dialog.md` EXD-FR-DHAN,
 * EXD-FR-OOTQ, EXD-FR-VFFP): under React's StrictMode, by another overlay
 * while an export runs, and with a resolution that did not reach the
 * conversation.
 *
 * The dialog is mounted through its window host and opened through its store.
 * The backend is a strict `invoke` mock.
 */
import { StrictMode } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

const dispatchMock = vi.fn(async () => {});
vi.mock("./DraftDiscussion/decisionDispatch", () => ({
  dispatchDecision: (...args: unknown[]) => dispatchMock(...(args as [])),
}));

import { ExportDialogHost } from "./ExportDialog";
import { exportErrorText } from "./ExportDialog/ExportDialog";
import {
  closeExportDialog,
  exportDialogRequest,
  openExportDialog,
  resetExportDialogForTest,
} from "../state/exportDialog";
import { resetToasts, visibleToasts } from "../state/toasts";
import { resetLogBufferForTest } from "../logging";
import type { ExportRequest } from "../types";

type Handler = (args: Record<string, unknown>) => unknown;
let handlers: Record<string, Handler> = {};

beforeEach(() => {
  resetLogBufferForTest();
  resetExportDialogForTest();
  resetToasts();
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
const DEST = "/Users/me/Desktop/guide.docx";

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

function resolvesWithComment() {
  handlers.resolve_export_request = (args) => ({
    request: request({ state: args.outcome as ExportRequest["state"] }),
    commentId: "c-outcome",
    commentFailed: false,
  });
}

const resolutions = () => invokeMock.mock.calls.filter(([cmd]) => cmd === "resolve_export_request");

async function open(strict = false) {
  render(strict ? <StrictMode><ExportDialogHost /></StrictMode> : <ExportDialogHost />);
  await act(async () => {
    await openExportDialog(PATH, { request: request() });
  });
  return screen.findByRole("dialog", { name: "Export document" });
}

async function startExport(result: Promise<unknown>) {
  handlers.export_document = () => result;
  handlers.browse_for_save_path = () => ({ selected: { path: DEST } });
  fireEvent.click(screen.getByRole("button", { name: "Choose…" }));
  await screen.findByText(DEST);
  fireEvent.click(screen.getByRole("button", { name: "Export" }));
  await screen.findByText("Starting the export…");
}

describe("a dialog whose shell unmounts first", () => {
  it("EXD-FR-DHAN, EXD-FR-OOTQ: the window close after the unmount still resolves the request", async () => {
    resolvesWithComment();
    await open();
    // A project that closes unmounts the shell before the window effect runs.
    cleanup();
    expect(resolutions()).toHaveLength(0);
    act(() => closeExportDialog("window"));
    await waitFor(() => expect(resolutions()).toHaveLength(1));
    expect(resolutions()[0]).toEqual([
      "resolve_export_request",
      { requestId: "req-1", outcome: "cancelled" },
    ]);
    expect(exportDialogRequest()).toBeNull();
  });
});

describe("the dialog under StrictMode", () => {
  it("EXD-FR-DHAN: a dialog opened from a request keeps the request pending, and Cancel resolves it once", async () => {
    resolvesWithComment();
    await open(true);
    // The settled state is the dialog itself: give every effect a turn to run.
    await act(async () => {
      await Promise.resolve();
    });
    expect(resolutions()).toHaveLength(0);
    expect(exportDialogRequest()).not.toBeNull();

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    await waitFor(() => expect(resolutions()).toHaveLength(1));
    expect(resolutions()[0]).toEqual([
      "resolve_export_request",
      { requestId: "req-1", outcome: "cancelled" },
    ]);
  });

  it("EXD-FR-DHAN, EXD-FR-YTQG: a success closes the dialog and resolves the request as exported", async () => {
    resolvesWithComment();
    await open(true);
    const pending = deferred<unknown>();
    await startExport(pending.promise);
    await act(async () => {
      pending.resolve({ destination: DEST, warnings: [] });
      await pending.promise;
    });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    await waitFor(() => expect(resolutions()).toHaveLength(1));
    expect(resolutions()[0][1]).toEqual({
      requestId: "req-1",
      outcome: "exported",
      destination: DEST,
    });
  });
});

describe("another overlay while an export runs", () => {
  it("EXD-FR-OOTQ, EXD-FR-DHAN: the dialog closes, and the export that ends later shows a toast and resolves the request", async () => {
    resolvesWithComment();
    await open();
    const pending = deferred<unknown>();
    await startExport(pending.promise);

    act(() => closeExportDialog("other-overlay"));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(resolutions()).toHaveLength(0);

    await act(async () => {
      pending.resolve({ destination: DEST, warnings: [] });
      await pending.promise;
    });
    await waitFor(() => expect(resolutions()).toHaveLength(1));
    expect(resolutions()[0][1]).toEqual({
      requestId: "req-1",
      outcome: "exported",
      destination: DEST,
    });
    const toast = visibleToasts().find((t) => t.title === "Document exported");
    expect(toast?.level).toBe("Info");
    expect(toast?.body).toBe("guide.docx was written.");
    await waitFor(() => expect(dispatchMock).toHaveBeenCalledTimes(1));
  });

  it("EXD-FR-OOTQ: an export with warnings that ends after the close names the diagrams that stayed as code", async () => {
    resolvesWithComment();
    await open();
    const pending = deferred<unknown>();
    await startExport(pending.promise);
    act(() => closeExportDialog("window"));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());

    await act(async () => {
      pending.resolve({
        destination: DEST,
        warnings: [
          { kind: "diagram_failed", index: 1 },
          { kind: "diagram_failed", index: 3 },
        ],
      });
      await pending.promise;
    });
    await waitFor(() =>
      expect(visibleToasts().some((t) => t.title === "Document exported")).toBe(true),
    );
    const toast = visibleToasts().find((t) => t.title === "Document exported");
    expect(toast?.level).toBe("Warn");
    expect(toast?.body).toBe("guide.docx was written. 2 diagrams stayed as code.");
    await waitFor(() => expect(resolutions()).toHaveLength(1));
    expect(resolutions()[0][1]).toMatchObject({ outcome: "exported" });
  });

  it("EXD-FR-OOTQ, EXD-FR-DHAN: an export that fails after the close shows its error in a toast and resolves the request as failed", async () => {
    resolvesWithComment();
    await open();
    const pending = deferred<unknown>();
    await startExport(pending.promise);
    act(() => closeExportDialog("other-overlay"));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());

    await act(async () => {
      pending.reject("write_failed");
      await pending.promise.catch(() => {});
    });
    await waitFor(() => expect(resolutions()).toHaveLength(1));
    expect(resolutions()[0][1]).toEqual({
      requestId: "req-1",
      outcome: "failed",
      error: "The file could not be written to the destination.",
    });
    const toast = visibleToasts().find((t) => t.title === "Could not export the document");
    expect(toast?.level).toBe("Error");
    expect(toast?.body).toBe("The file could not be written to the destination.");
  });

  it("EXD-FR-DHAN: a new opening ends the request of the dialog it replaces", async () => {
    resolvesWithComment();
    await open();
    await act(async () => {
      await openExportDialog(PATH);
    });
    await waitFor(() => expect(resolutions()).toHaveLength(1));
    expect(resolutions()[0][1]).toEqual({ requestId: "req-1", outcome: "cancelled" });
    expect(exportDialogRequest()?.request).toBeNull();
  });
});

describe("a resolution that did not reach the conversation", () => {
  it("EXD-FR-VFFP: commentFailed shows an error toast that says the conversation was not told, and brings nobody back", async () => {
    handlers.resolve_export_request = () => ({
      request: request({ state: "cancelled" }),
      commentFailed: true,
    });
    await open();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() =>
      expect(visibleToasts().some((t) => t.key === "export-request-req-1")).toBe(true),
    );
    const toast = visibleToasts().find((t) => t.key === "export-request-req-1");
    expect(toast?.level).toBe("Error");
    expect(toast?.body).toBe("The conversation was not told what happened.");
    expect(dispatchMock).not.toHaveBeenCalled();
  });
});

describe("the error text", () => {
  it("EXD-FR-RNJL: a read failure and a bad format each have their own text", () => {
    expect(exportErrorText("read_failed")).toBe("The file could not be read.");
    expect(exportErrorText("invalid_format")).toBe("The format is not .pdf or .docx.");
  });
});
