// Shared fixtures for the Documents panel tests. The test files mock the Tauri
// modules and the logging module with the mocks that live here.
import { act } from "@testing-library/react";
import { vi } from "vitest";
import type {
  DocumentEntry,
  DocumentFormat,
  DocumentSource,
  DocumentsSnapshot,
  PickDocumentSourcesMode,
  PickDocumentSourcesResult,
} from "../../types";

export const invokeMock = vi.fn();
export const logInfoMock = vi.fn();
export const logWarnMock = vi.fn();
export const logErrorMock = vi.fn();

type Handler = (event: { payload: DocumentsSnapshot }) => void;
const handlers: Handler[] = [];

export const listenMock = vi.fn((name: string, handler: Handler) => {
  if (name === "documents-changed") handlers.push(handler);
  return Promise.resolve(() => {
    const at = handlers.indexOf(handler);
    if (at >= 0) handlers.splice(at, 1);
  });
});

export const loggingMock = {
  logDebug: vi.fn(),
  logInfo: (...args: unknown[]) => logInfoMock(...args),
  logWarn: (...args: unknown[]) => logWarnMock(...args),
  logError: (...args: unknown[]) => logErrorMock(...args),
  flushLogs: vi.fn(),
};

export function resetPanelFixtures(): void {
  invokeMock.mockReset();
  listenMock.mockClear();
  logInfoMock.mockReset();
  logWarnMock.mockReset();
  logErrorMock.mockReset();
  handlers.length = 0;
}

let counter = 0;

/** A document entry for `path`, with a stable fake id. */
export function doc(path: string, patch: Partial<DocumentEntry> = {}): DocumentEntry {
  counter += 1;
  const name = path.split("/").pop() ?? path;
  const lower = name.toLowerCase();
  const format: DocumentFormat = lower.endsWith(".pdf")
    ? "pdf"
    : lower.endsWith(".txt")
      ? "text"
      : "markdown";
  return {
    id: `doc-${String(counter).padStart(32, "0")}`,
    path,
    name,
    format,
    status: "available",
    revision: `rev${counter}`,
    ...patch,
  };
}

export function src(
  kind: DocumentSource["kind"],
  path: string,
  patch: Partial<DocumentSource> = {},
): DocumentSource {
  return { kind, path, status: "available", ...patch };
}

export function snap(
  sources: DocumentSource[],
  documents: DocumentEntry[],
): DocumentsSnapshot {
  return { sources, documents };
}

export interface Backend {
  list: () => Promise<DocumentsSnapshot> | DocumentsSnapshot;
  pick?: (mode: PickDocumentSourcesMode) => Promise<PickDocumentSourcesResult>;
  remove?: (path: string) => Promise<DocumentsSnapshot> | DocumentsSnapshot;
}

/** Point `invoke` at a small fake of the Documents commands. */
export function serve(backend: Backend): void {
  invokeMock.mockImplementation((cmd: string, args?: Record<string, unknown>) => {
    if (cmd === "list_documents") return Promise.resolve(backend.list());
    if (cmd === "pick_document_sources" && backend.pick)
      return backend.pick(args?.mode as PickDocumentSourcesMode);
    if (cmd === "remove_document_source" && backend.remove)
      return Promise.resolve(backend.remove(args?.path as string));
    return Promise.reject(new Error(`unexpected command ${cmd}`));
  });
}

export const calls = (cmd: string) =>
  invokeMock.mock.calls.filter((call) => call[0] === cmd);

/** Send a `documents changed` event to every listener. */
export async function emitSnapshot(next: DocumentsSnapshot): Promise<void> {
  await act(async () => {
    for (const handler of [...handlers]) handler({ payload: next });
  });
}

export const listenerCount = () => handlers.length;

/** What every log call of a test carried, as one string. */
export function loggedText(): string {
  return JSON.stringify([
    ...logInfoMock.mock.calls,
    ...logWarnMock.mock.calls,
    ...logErrorMock.mock.calls,
  ]);
}
