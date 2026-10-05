// Shared fixtures for the Document text viewer tests. The test files mock the
// Tauri modules and the logging module with the mocks that live here.
import { act } from "@testing-library/react";
import { vi } from "vitest";
import type {
  DocumentEntry,
  DocumentFormat,
  DocumentsSnapshot,
  DocumentText,
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

export const ID = "doc-0123456789abcdef0123456789abcdef";

export function resetFixtures(): void {
  invokeMock.mockReset();
  listenMock.mockClear();
  logInfoMock.mockReset();
  logWarnMock.mockReset();
  logErrorMock.mockReset();
  handlers.length = 0;
}

export function documentText(
  text: string,
  format: DocumentFormat = "markdown",
  revision = "r1",
): DocumentText {
  return { id: ID, name: "notes.md", format, text, revision };
}

/** Make `read_document` answer with the given text. */
export function readReturns(value: DocumentText): void {
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "read_document") return Promise.resolve(value);
    throw new Error(`unexpected command ${cmd}`);
  });
}

/** Make `read_document` refuse with the given error. */
export function readRejects(error: unknown): void {
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "read_document") return Promise.reject(error);
    throw new Error(`unexpected command ${cmd}`);
  });
}

export const readCalls = () =>
  invokeMock.mock.calls.filter((call) => call[0] === "read_document");

export function entry(
  patch: Partial<DocumentEntry> = {},
): DocumentEntry {
  return {
    id: ID,
    path: "/refs/notes.md",
    name: "notes.md",
    format: "markdown",
    status: "available",
    revision: "r1",
    ...patch,
  };
}

/** Send a `documents changed` event to every listener. */
export async function emitDocuments(
  documents: DocumentEntry[],
): Promise<void> {
  await act(async () => {
    for (const handler of [...handlers]) {
      handler({ payload: { sources: [], documents } });
    }
  });
}

export const listenerCount = () => handlers.length;
