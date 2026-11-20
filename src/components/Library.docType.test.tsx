/**
 * The Document type in the Project panel (`LIB-library.md` LIB-FR-04,
 * LIB-FR-08, LIB-FR-19; `LCM-library-context-menu.md` LCM-FR-04, LCM-FR-07):
 * it is offered, tagged, and filtered on the terms of the other eight types.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));
// No log flush timer may outlive its test.
vi.mock("../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
  flushLogs: vi.fn(),
}));

import {
  baseTree,
  expandedAll,
  file,
  folder,
  makeTreeStubs,
  renderLibrary,
} from "../test/libraryFixtures";
import { wireNode } from "../test/treeCommands";
import { resetPanelReveals } from "../state/panelReveal";
import type { TreeNode } from "../types";

const { mockInvoke } = makeTreeStubs(invokeMock);

/** The base tree with its `specifications` folder typed as Document. */
/** The `specifications` node an assignment of Document returns (LCM-FR-04). */
function typedNode(): TreeNode {
  const child = (baseTree().children ?? []).find((c) => c.path === "specifications")!;
  return wireNode({ ...child, artifactType: "doc", typeSource: "assigned" });
}

beforeEach(() => {
  resetPanelReveals();
  invokeMock.mockReset();
});
afterEach(cleanup);

describe("the Document type in the Project panel", () => {
  it("LCM-FR-04, LCM-FR-07, LIB-FR-08: the submenu offers Document with its icon and tag, and a folder takes it at folder scope", async () => {
    mockInvoke(
      {
        tree: () => baseTree(),
        assign_artifact_type: () => typedNode(),
      },
      expandedAll(baseTree()),
    );
    renderLibrary();

    fireEvent.contextMenu(await screen.findByText("specifications"));
    const menu = document.querySelector(".menu") as HTMLElement;
    fireEvent.click(within(menu).getByText("Artifact Type"));
    const sub = menu.querySelector(".menu--sub") as HTMLElement;
    const entry = within(sub).getByText("Document").closest(".menu-item") as HTMLElement;
    expect(entry.querySelector("svg")).not.toBeNull();
    expect(within(entry).getByText("DOC")).toHaveAttribute("data-type", "doc");
    // The ninth type is the last type row, above the divider and Clear Type.
    const labels = Array.from(sub.querySelectorAll(".menu-item")).map((el) => el.textContent);
    expect(labels[labels.length - 2]).toContain("Document");

    fireEvent.click(entry);
    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("assign_artifact_type", {
        path: "specifications",
        artifactType: "doc",
        scope: "folder",
      }),
    );
    // LCM-FR-04 / LIB-FR-08: the returned node re-tags the folder at once.
    const row = (await screen.findByText("specifications")).closest(".tree-row") as HTMLElement;
    await waitFor(() => expect(within(row).getByText("DOC")).toHaveAttribute("data-type", "doc"));
  });

  it("LIB-FR-04, LIB-FR-19, LIB-FR-08: a tree that holds a Document file offers the Document lens and tags the file", async () => {
    const tree = folder("", true, [
      folder("guides", true, [file("guides/intro.md", "doc", "inherited")]),
    ]);
    mockInvoke({ tree: () => tree }, expandedAll(tree));
    renderLibrary();

    const row = (await screen.findByText("intro.md")).closest(".tree-row") as HTMLElement;
    expect(within(row).getByText("DOC")).toHaveAttribute("data-type", "doc");
    expect(screen.getByRole("radio", { name: "Document" })).toBeInTheDocument();
  });
});
