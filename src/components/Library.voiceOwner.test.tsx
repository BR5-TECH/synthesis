/**
 * A Library delete ends voice mode when the deleted file, or a folder that
 * holds it, owns the voice discussion (`VOM-voice-mode.md` VOM-FR-LSBQ,
 * `CVP-conversation-presentation.md` CVP-FR-51, `LCM-library-context-menu.md`
 * LCM-FR-11).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => undefined),
}));
vi.mock("../voice/voiceController", () => ({ noteOwnerDeleted: vi.fn() }));

import { baseTree, expandedAll, makeTreeStubs, renderLibrary } from "../test/libraryFixtures";
import { resetPanelReveals } from "../state/panelReveal";
import { noteOwnerDeleted } from "../voice/voiceController";

const { mockInvoke } = makeTreeStubs(invokeMock);

beforeEach(() => {
  resetPanelReveals();
  invokeMock.mockReset();
  vi.mocked(noteOwnerDeleted).mockReset();
});

afterEach(() => {
  cleanup();
});

function deleteFrom(name: string) {
  fireEvent.contextMenu(screen.getByText(name));
  const menu = document.querySelector(".menu") as HTMLElement;
  fireEvent.click(within(menu).getByText("Delete"));
}

describe("a Library delete and voice mode", () => {
  it("VOM-FR-LSBQ, CVP-FR-51: a deleted file names itself as the deleted owner", async () => {
    mockInvoke(
      { load_project_tree: () => baseTree(), delete_path: () => undefined },
      expandedAll(baseTree()),
    );
    renderLibrary();
    await screen.findByText("LIB-library.md");
    deleteFrom("LIB-library.md");
    await waitFor(() =>
      expect(noteOwnerDeleted).toHaveBeenCalledWith({
        kind: "artifact",
        artifactId: "specifications/LIB-library.md",
      }),
    );
  });

  it("VOM-FR-LSBQ, LCM-FR-11: a folder deleted with its contents names itself as the deleted owner", async () => {
    mockInvoke(
      {
        load_project_tree: () => baseTree(),
        delete_path: (args) => {
          if (!(args as { recursive: boolean }).recursive) {
            throw new Error("directory not empty: specifications");
          }
          return undefined;
        },
      },
      expandedAll(baseTree()),
    );
    renderLibrary();
    await screen.findByText("specifications");
    deleteFrom("specifications");
    const dialog = await screen.findByRole("dialog", { name: "Delete folder" });
    expect(noteOwnerDeleted).not.toHaveBeenCalled();
    fireEvent.click(within(dialog).getByRole("button", { name: "Delete" }));
    await waitFor(() =>
      expect(noteOwnerDeleted).toHaveBeenCalledWith({
        kind: "artifact",
        artifactId: "specifications",
      }),
    );
  });

  it("VOM-FR-LSBQ: a refused delete ends nothing", async () => {
    mockInvoke(
      {
        load_project_tree: () => baseTree(),
        delete_path: () => {
          throw new Error("permission denied");
        },
      },
      expandedAll(baseTree()),
    );
    renderLibrary();
    await screen.findByText("AGENTS.md");
    deleteFrom("AGENTS.md");
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("delete_path", { path: "AGENTS.md", recursive: false }),
    );
    expect(noteOwnerDeleted).not.toHaveBeenCalled();
  });
});
