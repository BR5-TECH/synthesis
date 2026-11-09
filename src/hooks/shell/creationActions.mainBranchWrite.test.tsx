/**
 * The creation of an artifact on a main branch asks for the confirmation once
 * per branch (`NFI-new-file.md` NFI-FR-BHKR, `NTA-new-typed-artifact.md`
 * NTA-FR-PVSC, `MBW-main-branch-write.md` MBW-FR-DBGA, MBW-FR-ILFN).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
}));

import { createCreationActions } from "./creationActions";
import { MainBranchWriteHost } from "../../components/MainBranchWrite";
import { resetLogBufferForTest } from "../../logging";
import { mainGuard, scriptedReads } from "../../test/mainBranchWriteFixtures";

const NODE = { id: "a.md", name: "a.md", path: "a.md", nodeKind: "file", artifactType: "spec" };

function actions() {
  const deps = {
    openArtifact: vi.fn(),
    setRevealArtifactId: vi.fn(),
    setSearchOpen: vi.fn(),
    setProgressOverlayOpen: vi.fn(),
    setNewFileSeed: vi.fn(),
    setNewFolderSeed: vi.fn(),
    setNewTypedArtifactSeed: vi.fn(),
  };
  return { deps, ...createCreationActions(deps) };
}

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

beforeEach(() => {
  resetLogBufferForTest();
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "create_file" || cmd === "create_typed_file") return Promise.resolve(NODE);
    if (cmd === "append_log_records") return Promise.resolve();
    return Promise.reject(new Error(`unexpected command ${cmd}`));
  });
});
afterEach(cleanup);

describe("the creation of an artifact on a main branch (MBW-FR-DBGA)", () => {
  it("NFI-FR-BHKR, NTA-FR-PVSC, MBW-FR-ILFN: the first creation waits for the confirmation, and the next one on the same branch asks nothing", async () => {
    const reads = scriptedReads(mainGuard());
    const { deps, submitNewFile, submitNewTypedArtifact } = actions();
    render(<MainBranchWriteHost />);

    let first!: Promise<unknown>;
    act(() => {
      first = submitNewFile({ location: null, name: "a.md" });
    });
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("You asked to create an artifact.");
    expect(calls("create_file")).toHaveLength(0);
    fireEvent.click(screen.getByRole("button", { name: "Confirm write to main" }));
    await expect(first).resolves.toEqual({ ok: true });
    expect(calls("create_file")).toHaveLength(1);
    expect(deps.openArtifact).toHaveBeenCalledTimes(1);

    await expect(
      submitNewTypedArtifact({ location: null, name: "b.md", artifactType: "spec" }),
    ).resolves.toEqual({ ok: true });
    expect(calls("create_typed_file")).toHaveLength(1);
    expect(reads.confirms).toEqual(["main"]);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("NFI-FR-BHKR, MBW-FR-XBJS: a cancel creates nothing, opens nothing, and closes the window", async () => {
    scriptedReads(mainGuard());
    const { deps, submitNewFile } = actions();
    render(<MainBranchWriteHost />);

    let result!: Promise<unknown>;
    act(() => {
      result = submitNewFile({ location: null, name: "a.md" });
    });
    await screen.findByRole("dialog");
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));

    await expect(result).resolves.toEqual({ ok: true });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(calls("create_file")).toHaveLength(0);
    expect(deps.openArtifact).not.toHaveBeenCalled();
    expect(deps.setNewFileSeed).toHaveBeenCalledWith(null);
  });
});
