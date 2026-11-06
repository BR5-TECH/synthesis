/**
 * The writes the author asks for outside an open overlay, which show the
 * standalone confirmation first (MBW-FR-DBGA): **+ Draft** and **New Draft**
 * on a folder, **Delete** of a draft (DRP-FR-MKCP), and **Push**
 * (CHG-FR-ATLJ).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, renderHook, screen, waitFor } from "@testing-library/react";

import { MainBranchWriteHost } from ".";
import { CommitMessageModal } from "../CommitMessageModal";
import { createDraftTabActions } from "../../hooks/shell/draftActions";
import { createDraftActions, type DraftActionDeps } from "../draftsActions";
import { useGitTransferState } from "../../hooks/useGitTransfer";
import { resetLogBufferForTest } from "../../logging";
import { mainGuard, scriptedReads } from "../../test/mainBranchWriteFixtures";
import type { DraftSummary } from "../../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
}));

const DRAFT = { id: "d-1", name: "Untitled", promptPath: "Untitled.md" };

beforeEach(() => {
  resetLogBufferForTest();
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "create_draft") return Promise.resolve({ draft: DRAFT, file: "Untitled.md" });
    if (cmd === "delete_draft") return Promise.resolve();
    if (cmd === "push_current_branch") return Promise.resolve();
    if (cmd === "commit_paths")
      return Promise.resolve({ commitId: "abc1234", committedPaths: ["a.md"] });
    if (cmd === "get_upstream_sync_state")
      return Promise.resolve({ hasRemote: true, hasUpstream: true, ahead: 1, behind: 0 });
    if (cmd === "append_log_records") return Promise.resolve();
    return Promise.reject(new Error(`unexpected command ${cmd}`));
  });
});
afterEach(cleanup);

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

function draftTabActions() {
  return createDraftTabActions({
    drafts: { select: vi.fn(), setPromptPath: vi.fn(), get: vi.fn() } as never,
    tabsRef: { current: [] },
    setTabs: vi.fn(),
    setActiveTab: vi.fn(),
    activateTab: vi.fn(),
    bumpDrafts: vi.fn(),
    specMap: { dispatch: vi.fn() } as never,
  });
}

function draftActions(onDraftDeleted = vi.fn()) {
  const deps = {
    panel: { expandAll: vi.fn() },
    drafts: [],
    childFolders: new Map(),
    inline: null,
    inlineText: "",
    openInline: vi.fn(),
    closeInline: vi.fn(),
    setInlineError: vi.fn(),
    setOverlay: vi.fn(),
    setSelected: vi.fn(),
    setAnnouncement: vi.fn(),
    focusNext: { current: null },
    run: async (_key: string, work: () => Promise<void>) => {
      await work();
      return true;
    },
    reload: async () => null,
    onOpenDraft: vi.fn(),
    onCreateDraft: vi.fn(),
    onDraftDeleted,
    onDraftRenamed: vi.fn(),
    onDraftChanged: vi.fn(),
  } as unknown as DraftActionDeps;
  return { actions: createDraftActions(deps), deps };
}

describe("the drafts panel (DRP-FR-MKCP)", () => {
  it("DRP-FR-MKCP, MBW-FR-DBGA: + Draft and New Draft on a folder read the guard first, and create nothing until the author confirms", async () => {
    const reads = scriptedReads(mainGuard());
    render(<MainBranchWriteHost />);
    const { createDraft } = draftTabActions();

    const cancelled = createDraft({ folder: "Specs" });
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("You asked to create a draft.");
    expect(calls("create_draft")).toHaveLength(0);
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await act(async () => {
      await expect(cancelled).resolves.toBeNull();
    });
    expect(calls("create_draft")).toHaveLength(0);

    const created = createDraft({});
    await screen.findByRole("dialog");
    fireEvent.click(screen.getByRole("button", { name: "Confirm write to main" }));
    await act(async () => {
      await expect(created).resolves.toEqual({ id: "d-1", name: "Untitled" });
    });
    expect(calls("create_draft")).toHaveLength(1);
    expect(reads.confirms).toEqual(["main"]);
  });

  it("DRP-FR-MKCP: Delete of a draft deletes nothing until the author confirms", async () => {
    scriptedReads(mainGuard());
    render(<MainBranchWriteHost />);
    const onDraftDeleted = vi.fn();
    const { actions, deps } = draftActions(onDraftDeleted);
    const draft = { id: "d-1", name: "Untitled" } as DraftSummary;

    const first = actions.doDeleteDraft(draft);
    await screen.findByRole("dialog");
    expect(calls("delete_draft")).toHaveLength(0);
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await act(async () => {
      await first;
    });
    expect(calls("delete_draft")).toHaveLength(0);
    expect(onDraftDeleted).not.toHaveBeenCalled();
    expect(deps.setAnnouncement).not.toHaveBeenCalled();

    const second = actions.doDeleteDraft(draft);
    await screen.findByRole("dialog");
    fireEvent.click(screen.getByRole("button", { name: "Confirm write to main" }));
    await act(async () => {
      await second;
    });
    expect(calls("delete_draft")).toHaveLength(1);
    expect(onDraftDeleted).toHaveBeenCalledWith("d-1");
  });
});

describe("the push (CHG-FR-ATLJ)", () => {
  it("CHG-FR-ATLJ: a push reads the guard of the current branch before it invokes push current branch, and a cancel pushes nothing", async () => {
    const reads = scriptedReads(mainGuard());
    render(<MainBranchWriteHost />);
    const { result } = renderHook(() => useGitTransferState({ enabled: true }));
    await waitFor(() => expect(result.current.sync).not.toBeNull());

    let outcome: unknown;
    const cancelled = result.current.pushBranch().then((value) => (outcome = value));
    await screen.findByRole("dialog");
    expect(result.current.running).toBe(false);
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await act(async () => {
      await cancelled;
    });
    expect(outcome).toMatchObject({ ok: false, cause: "cancelled" });
    expect(calls("push_current_branch")).toHaveLength(0);

    const pushed = result.current.pushBranch().then((value) => (outcome = value));
    await screen.findByRole("dialog");
    fireEvent.click(screen.getByRole("button", { name: "Confirm write to main" }));
    await act(async () => {
      await pushed;
    });
    expect(outcome).toEqual({ ok: true });
    expect(calls("push_current_branch")).toHaveLength(1);
    expect(reads.reads[0]).toEqual({ branch: null, refresh: false });
  });

  it("CHG-FR-ATLJ: a confirmation that the commit window took for the same branch covers the push", async () => {
    scriptedReads(mainGuard({ confirmed: true }));
    render(<MainBranchWriteHost />);
    const { result } = renderHook(() => useGitTransferState({ enabled: true }));
    let outcome: unknown;
    await act(async () => {
      outcome = await result.current.pushBranch();
    });
    expect(outcome).toEqual({ ok: true });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("CHG-FR-ATLJ, MBW-FR-ILFN: the confirmation that the commit window took covers the push step of Commit & Push", async () => {
    const reads = scriptedReads(mainGuard());
    render(<MainBranchWriteHost />);
    const onCommitted = vi.fn();
    render(
      <CommitMessageModal
        files={[{ path: "a.md", untracked: false }]}
        onClose={vi.fn()}
        onCommitted={onCommitted}
      />,
    );
    fireEvent.click(await screen.findByRole("button", { name: "Confirm write to main" }));
    fireEvent.change(screen.getByLabelText("Commit message"), { target: { value: "msg" } });
    await waitFor(() => expect(screen.getByRole("button", { name: /^Commit/ })).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: /^Commit/ }));
    await waitFor(() => expect(onCommitted).toHaveBeenCalled());

    const { result } = renderHook(() => useGitTransferState({ enabled: true }));
    let outcome: unknown;
    await act(async () => {
      outcome = await result.current.pushBranch();
    });
    expect(outcome).toEqual({ ok: true });
    expect(screen.queryByRole("dialog", { name: /Write to/ })).not.toBeInTheDocument();
    expect(reads.confirms).toEqual(["main"]);
    expect(calls("push_current_branch")).toHaveLength(1);
  });
});
