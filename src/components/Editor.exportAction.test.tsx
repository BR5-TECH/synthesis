/**
 * **Export to...** on the Editor tab (`EDT-editor.md` EDT-FR-64, EDT-FR-65;
 * `ACT-action-control.md` ACT-FR-04; `EXD-export-dialog.md` EXD-FR-UQNO,
 * EXD-FR-CDYS).
 *
 * The real Editor is mounted on a strict backend, so the entry shows exactly
 * where the file is a Markdown file whose resolved artifact type is `doc`.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";

import { Editor } from "./Editor";
import { EditSessionStore } from "../state/editSessions";
import { resetLogBufferForTest } from "../logging";
import {
  exportDialogRequest,
  resetExportDialogForTest,
} from "../state/exportDialog";
import { resetToasts } from "../state/toasts";
import { BODY, wireBackend } from "../test/editorCommentsFixtures";
import type { Backend } from "../test/editorCommentsFixtures";
import type { EditorProps } from "./Editor/props";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

beforeEach(() => {
  resetLogBufferForTest();
  resetExportDialogForTest();
  resetToasts();
  invokeMock.mockReset();
});
afterEach(() => {
  cleanup();
  resetExportDialogForTest();
});

async function mount(artifactId: string, artifactType: EditorProps["artifactType"]) {
  const sessions = new EditSessionStore();
  const backend: Backend = { load: { body: BODY, checksum: "ck1" }, threads: [], calls: [] };
  wireBackend(invokeMock, backend);
  render(
    <Editor
      artifactId={artifactId}
      artifactName={artifactId}
      artifactType={artifactType}
      sessions={sessions}
    />,
  );
  await screen.findByLabelText(artifactId.endsWith(".md") ? "artifact body" : "Source");
  return sessions;
}

function entries() {
  fireEvent.click(screen.getByRole("button", { name: "Actions" }));
  return within(screen.getByRole("menu"))
    .getAllByRole("menuitem")
    .map((item) => item.textContent?.trim());
}

describe("Export to... on the Editor tab", () => {
  it("EXD-FR-UQNO, EDT-FR-64, ACT-FR-04: a Markdown DOC artifact offers Discuss, then Export to...", async () => {
    await mount("docs/guide.md", "doc");
    expect(entries()).toEqual(["Discuss", "Export to..."]);
  });

  it.each([
    ["a Markdown file of another type", "docs/guide.md", "spec"],
    ["a Markdown file of no type", "notes.md", undefined],
    ["a source file assigned the DOC type", "docs/guide.txt", "doc"],
  ] as const)(
    "EXD-FR-UQNO, EDT-FR-65: %s offers no Export to...",
    async (_label, artifactId, artifactType) => {
      await mount(artifactId, artifactType);
      expect(entries()).toEqual(["Discuss"]);
    },
  );

  it("EXD-FR-CDYS: Export to... writes the file before the dialog opens on it", async () => {
    const sessions = await mount("docs/guide.md", "doc");
    const flush = vi.spyOn(sessions, "flush");
    fireEvent.click(screen.getByRole("button", { name: "Actions" }));
    fireEvent.click(screen.getByRole("menuitem", { name: "Export to..." }));
    await waitFor(() => expect(exportDialogRequest()?.path).toBe("docs/guide.md"));
    expect(flush).toHaveBeenCalledWith("docs/guide.md");
    expect(exportDialogRequest()?.format).toBe("pdf");
  });
});
