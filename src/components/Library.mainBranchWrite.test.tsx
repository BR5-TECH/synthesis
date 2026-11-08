/**
 * The Library writes that ask for the confirmation on a main branch
 * (`../../specifications/ui/LCM-library-context-menu.md` LCM-FR-TDWE,
 * `../../specifications/ui/MBW-main-branch-write.md` MBW-FR-DBGA,
 * MBW-FR-ILFN).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import { MainBranchWriteHost } from "./MainBranchWrite";
import { baseTree, expandedAll, makeTreeStubs, renderLibrary } from "../test/libraryFixtures";
import { resetPanelReveals } from "../state/panelReveal";
import { currentMainBranchWrite } from "../state/mainBranchWrite";
import { mainGuard, scriptedReads } from "../test/mainBranchWriteFixtures";

const { mockInvoke } = makeTreeStubs(invokeMock);

beforeEach(() => {
  resetPanelReveals();
  invokeMock.mockReset();
});
afterEach(cleanup);

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

async function renameFirstTo(name: string) {
  fireEvent.contextMenu(await screen.findByText("onboarding.md"));
  const menu = document.querySelector(".menu") as HTMLElement;
  fireEvent.click(within(menu).getByText("Rename"));
  const input = screen.getByLabelText("New name");
  fireEvent.change(input, { target: { value: name } });
  fireEvent.keyDown(input, { key: "Enter" });
}

describe("the Library writes on a main branch (LCM-FR-TDWE, MBW-FR-DBGA)", () => {
  it("LCM-FR-TDWE, MBW-FR-DBGA, MBW-FR-ILFN: a rename waits for the confirmation, runs after Confirm, and a later delete of the same branch asks no more", async () => {
    const reads = scriptedReads(mainGuard());
    mockInvoke(
      { load_project_tree: () => baseTree(), rename_path: () => undefined, delete_path: () => undefined },
      expandedAll(baseTree()),
    );
    renderLibrary();
    render(<MainBranchWriteHost />);

    await renameFirstTo("welcome.md");
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("You asked to rename an item.");
    expect(calls("rename_path")).toHaveLength(0);

    fireEvent.click(screen.getByRole("button", { name: "Confirm write to main" }));
    await waitFor(() => expect(calls("rename_path")).toHaveLength(1));
    expect(reads.confirms).toEqual(["main"]);

    fireEvent.contextMenu(await screen.findByText("AGENTS.md"));
    const menu = document.querySelector(".menu") as HTMLElement;
    fireEvent.click(within(menu).getByText("Delete"));
    await waitFor(() => expect(calls("delete_path")).toHaveLength(1));
    expect(currentMainBranchWrite()).toBeNull();
    expect(reads.confirms).toEqual(["main"]);
  });

  it("LCM-FR-TDWE, MBW-FR-XBJS: a cancel of the confirmation invokes no delete", async () => {
    scriptedReads(mainGuard());
    mockInvoke(
      { load_project_tree: () => baseTree(), delete_path: () => undefined },
      expandedAll(baseTree()),
    );
    renderLibrary();
    render(<MainBranchWriteHost />);

    fireEvent.contextMenu(await screen.findByText("AGENTS.md"));
    const menu = document.querySelector(".menu") as HTMLElement;
    fireEvent.click(within(menu).getByText("Delete"));
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("You asked to delete an artifact.");
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(currentMainBranchWrite()).toBeNull());
    expect(calls("delete_path")).toHaveLength(0);
  });

  it("LCM-FR-TDWE: a protected branch shows the same confirmation with the GitHub line, and Confirm runs the rename", async () => {
    scriptedReads(mainGuard({ protection: "protected" }));
    mockInvoke(
      { load_project_tree: () => baseTree(), rename_path: () => undefined },
      expandedAll(baseTree()),
    );
    renderLibrary();
    render(<MainBranchWriteHost />);

    await renameFirstTo("welcome.md");
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("GitHub protects main and may refuse a push.");
    fireEvent.click(screen.getByRole("button", { name: "Confirm write to main" }));
    await waitFor(() => expect(calls("rename_path")).toHaveLength(1));
  });

  it("LCM-FR-TDWE, MBW-FR-DBGA, MBW-FR-XBJS: a paste waits for the confirmation, a cancel invokes no copy, and a confirm invokes it once", async () => {
    const reads = scriptedReads(mainGuard());
    mockInvoke(
      { load_project_tree: () => baseTree(), copy_path_into_folder: () => undefined },
      expandedAll(baseTree()),
    );
    renderLibrary();
    render(<MainBranchWriteHost />);
    await screen.findByText("specifications");

    fireEvent.contextMenu(screen.getByText("onboarding.md"));
    fireEvent.click(within(document.querySelector(".menu") as HTMLElement).getByText("Copy"));

    const paste = () => {
      fireEvent.contextMenu(screen.getByText("specifications"));
      fireEvent.click(within(document.querySelector(".menu") as HTMLElement).getByText("Paste"));
    };

    paste();
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("You asked to paste an artifact.");
    expect(calls("copy_path_into_folder")).toHaveLength(0);
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(currentMainBranchWrite()).toBeNull());
    expect(calls("copy_path_into_folder")).toHaveLength(0);
    expect(reads.confirms).toEqual([]);

    paste();
    fireEvent.click(await screen.findByRole("button", { name: "Confirm write to main" }));
    await waitFor(() => expect(calls("copy_path_into_folder")).toHaveLength(1));
    expect(reads.confirms).toEqual(["main"]);
  });

  it("MBW-FR-QKVA: a tree on a main branch that is only read asks for no confirmation", async () => {
    const reads = scriptedReads(mainGuard());
    mockInvoke({ load_project_tree: () => baseTree() }, expandedAll(baseTree()));
    const { onOpenArtifact } = renderLibrary();
    render(<MainBranchWriteHost />);

    fireEvent.click(await screen.findByText("onboarding.md"));
    fireEvent.click(screen.getByText("AGENTS.md"));
    fireEvent.click(screen.getByText("review.flow"));

    expect(currentMainBranchWrite()).toBeNull();
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(reads.confirms).toEqual([]);
    expect(onOpenArtifact).toHaveBeenCalled();
  });
});
