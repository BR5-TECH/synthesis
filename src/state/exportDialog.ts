/**
 * EXD-FR-CDYS, EXD-FR-UDHD, EXD-FR-XNLZ: which file the Export dialog is open
 * on now, and the write that comes before it opens.
 *
 * A module store rather than React state, because two different surfaces open
 * the dialog — the action control of an Editor tab and the export request card
 * of a conversation — and the window draws it once, over both. The window tells
 * the store what to close before the dialog opens (SNV-FR-56), and which edit
 * sessions to write first.
 */
import { useSyncExternalStore } from "react";

import { logInfo, logWarn } from "../logging";
import { showToast } from "./toasts";
import type { ExportFormat, ExportRequest } from "../types";
import type { FlushResult } from "./editSessions/types";

/** What the store needs of an edit session store (EDT-FR-70). */
export interface ExportSessions {
  flush(artifactId: string, opts?: { force?: boolean }): Promise<FlushResult>;
  get(artifactId: string): { error: string | null } | undefined;
}

/** One opening of the dialog. */
export interface ExportDialogRequest {
  /** A new number for each opening, so each opening starts fresh. */
  id: number;
  /** The project-relative path of the DOC artifact. */
  path: string;
  /** EXD-FR-IVAE: the format the dialog opens with. */
  format: ExportFormat;
  /** EXD-FR-DHAN: the export request a card opened the dialog for. */
  request: ExportRequest | null;
  /** Where focus goes back when the dialog closes. */
  opener: HTMLElement | null;
}

/** Why the dialog closed. Only for the log. */
export type ExportCloseReason = "done" | "cancel" | "other-overlay" | "window";

let current: ExportDialogRequest | null = null;
let nextId = 1;
/** EXD-FR-DHAN / EXD-FR-OOTQ: what the mounted dialog does when another surface closes it. */
let closeHandler: { id: number; handle: () => void } | null = null;
let opening: (() => void) | null = null;
let shellSessions: ExportSessions | null = null;
const listeners = new Set<() => void>();

function emit(): void {
  for (const listener of [...listeners]) listener();
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function exportDialogRequest(): ExportDialogRequest | null {
  return current;
}

/** The dialog of the window, or `null` while it is closed. */
export function useExportDialogRequest(): ExportDialogRequest | null {
  return useSyncExternalStore(subscribe, exportDialogRequest, exportDialogRequest);
}

/** SNV-FR-56: set what the window closes before the dialog opens. */
export function setExportDialogOpening(next: (() => void) | null): void {
  opening = next;
}

/** EDT-FR-70: set the edit sessions a card writes before the dialog opens. */
export function setExportSessions(next: ExportSessions | null): void {
  shellSessions = next;
}

/**
 * EXD-FR-DHAN / EXD-FR-OOTQ: the mounted dialog of opening `id` sets what it
 * does when another overlay or the window closes it, and clears it with `null`.
 *
 * The store calls it from the close path, and not the dialog from an effect
 * cleanup, because React can run a cleanup on a dialog that stays mounted.
 */
export function setExportDialogCloseHandler(id: number, handle: (() => void) | null): void {
  if (handle !== null) closeHandler = { id, handle };
  else if (closeHandler?.id === id) closeHandler = null;
}

/** The reason a write that failed gives the author (EXD-FR-CBIW). */
function saveFailure(result: FlushResult, sessions: ExportSessions, path: string): string {
  if (result.blocked === "conflict") {
    return "The file changed on disk. Choose which version to keep, then export again.";
  }
  return sessions.get(path)?.error ?? "The document could not be saved.";
}

/**
 * EXD-FR-CDYS / EXD-FR-UDHD: write the file's buffer at once, and open the
 * dialog only when that write succeeds. A write that fails opens nothing and
 * shows its error in a toast (EXD-FR-CBIW). Resolves whether the dialog opened.
 */
export async function openExportDialog(
  path: string,
  options: {
    format?: ExportFormat;
    request?: ExportRequest | null;
    sessions?: ExportSessions;
    opener?: HTMLElement | null;
  } = {},
): Promise<boolean> {
  const sessions = options.sessions ?? shellSessions;
  if (sessions) {
    let result: FlushResult;
    try {
      result = await sessions.flush(path);
    } catch {
      result = { ok: false, blocked: "error", artifactId: path };
    }
    if (!result.ok) {
      logWarn(["frontend"], "export did not open: the document was not saved", {
        blocked: result.blocked ?? "error",
        fromRequest: options.request != null,
      });
      showToast(
        {
          key: "document-export-save",
          level: "Error",
          title: "Could not export the document",
          body: saveFailure(result, sessions, path),
          address: null,
        },
        { priority: true },
      );
      return false;
    }
  }
  // SNV-FR-56: the dialog is one of the window's mutually exclusive overlays.
  opening?.();
  // EXD-FR-DHAN: a dialog that is still open ends its request before the new one.
  if (current !== null) closeExportDialog("other-overlay");
  current = {
    id: nextId++,
    path,
    format: options.format ?? options.request?.format ?? "pdf",
    request: options.request ?? null,
    opener: options.opener ?? null,
  };
  logInfo(["frontend"], "export dialog opened", {
    fromRequest: options.request != null,
  });
  emit();
  return true;
}

/**
 * Close the dialog. EXD-FR-OOTQ: another overlay and the window close it also
 * while an export runs. The export continues, and the dialog's close handler
 * ends a request when the export ends (EXD-FR-DHAN).
 */
export function closeExportDialog(reason: ExportCloseReason): void {
  if (current === null) return;
  const opener = current.opener;
  const handler = closeHandler?.id === current.id ? closeHandler : null;
  current = null;
  if (handler !== null) closeHandler = null;
  if (reason === "other-overlay" || reason === "window") handler?.handle();
  emit();
  if (reason === "done" || reason === "cancel") opener?.focus?.();
}

/** Test support: forget every opening and every registration. */
export function resetExportDialogForTest(): void {
  current = null;
  closeHandler = null;
  opening = null;
  shellSessions = null;
  nextId = 1;
  emit();
}
