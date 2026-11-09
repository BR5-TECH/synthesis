/**
 * The inline section of an overlay that is already open (MBW-FR-URIL,
 * MBW-FR-IOGF, MBW-FR-WCBQ), in each overlay that holds one: the commit message
 * window (CMW-FR-RMCO), the merge confirmation of the work stream selector
 * (WSS-FR-DBGA), and the graduation start dialog (GSD-FR-YSMC). The pull
 * reconcile dialog holds none (MBW-FR-HXTE).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { CommitMessageModal } from "../CommitMessageModal";
import { MergeConfirm } from "../WorkStreamSelector/MergeConfirm";
import { GraduationStart } from "../GraduationStart";
import { PullReconcileDialog } from "../PullReconcileDialog";
import { resetLogBufferForTest } from "../../logging";
import { onNewWorktreeRequested } from "../../state/newWorktreeRequests";
import { mainGuard, scriptedReads, unconfirmedRefusal } from "../../test/mainBranchWriteFixtures";
import type { PullOutcome, WorkStreamSummary } from "../../types";
import { otherCommand } from "../../test/invokeMocks";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
}));

/** Answers of the commands each overlay sends. A test replaces one. */
let answers: Record<string, (args: Record<string, unknown>) => Promise<unknown>>;

beforeEach(() => {
  resetLogBufferForTest();
  answers = {
    commit_paths: async () => ({ commitId: "abc1234", committedPaths: ["a.md"] }),
    list_work_streams: async () => [summary],
    start_graduation: async () => ({ id: "g-1" }),
    reconcile_pull: async () => ({ state: "merged", commit: "c".repeat(40), updatedPaths: [] }),
    get_upstream_sync_state: async () => ({ hasRemote: true, hasUpstream: true, ahead: 2, behind: 1 }),
  };
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string, args: Record<string, unknown>) =>
    answers[cmd] ? answers[cmd](args ?? {}) : otherCommand(cmd),
  );
});
afterEach(cleanup);

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

const summary: WorkStreamSummary = {
  stream: {
    id: "s-1",
    name: "editor-work",
    projectKey: "/p",
    branch: "synthesis/stream/s-1",
    baseBranch: "main",
    baseRevision: "a91bc04",
    worktreePath: "/tmp/s-1",
    createdAt: "2026-09-06T09:00:00Z",
    isMissing: false,
  },
  queuedRunCount: 0,
  aheadOfBase: 2,
  behindBase: 0,
  baseTipRevision: "a91bc04",
  missingCommits: [],
  branchMissing: false,
};

const diverged: PullOutcome = {
  state: "diverged",
  branch: "main",
  upstream: "origin/main",
  localTip: "1".repeat(40),
  remoteTip: "2".repeat(40),
  ahead: 2,
  behind: 1,
  updatedPaths: [],
  refresh: {
    context: { repositoryRoot: "/repo", activeWorktreePath: "/repo", worktrees: [], branches: [] },
    remoteState: "refreshed",
  },
};

function commitWindow() {
  const onClose = vi.fn();
  const onCommitted = vi.fn();
  render(
    <CommitMessageModal
      files={[{ path: "a.md", untracked: false }]}
      onClose={onClose}
      onCommitted={onCommitted}
    />,
  );
  return { onClose, onCommitted };
}

describe("the commit message window (CMW-FR-RMCO)", () => {
  it("CMW-FR-RMCO, MBW-FR-URIL, MBW-FR-IOGF, MBW-FR-WCBQ: the window reads the guard when it opens, shows the section, and keeps Commit unavailable until the author confirms", async () => {
    const reads = scriptedReads(mainGuard());
    const { onCommitted } = commitWindow();
    await userEvent.type(screen.getByLabelText("Commit message"), "a message");

    const section = await screen.findByTestId("main-branch-write-section");
    expect(section).toHaveAttribute("role", "alert");
    expect(section).toHaveTextContent("You asked to commit. The commit goes to main.");
    const commit = screen.getByRole("button", { name: /^Commit/ });
    expect(commit).toBeDisabled();
    expect(commit).toHaveAccessibleDescription(/main is the default branch/);
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(reads.reads).toEqual([{ branch: null, refresh: false }]);

    fireEvent.click(screen.getByRole("button", { name: "Confirm write to main" }));
    await waitFor(() => expect(commit).toBeEnabled());
    expect(screen.queryByTestId("main-branch-write-section")).not.toBeInTheDocument();
    fireEvent.click(commit);
    await waitFor(() => expect(onCommitted).toHaveBeenCalled());
    expect(calls("commit_paths")).toHaveLength(1);
  });

  it("MBW-FR-WCBQ, CMW-FR-04: a confirm by keyboard leaves focus in the window, on the message field", async () => {
    scriptedReads(mainGuard());
    commitWindow();
    await userEvent.type(screen.getByLabelText("Commit message"), "a message");
    const confirm = await screen.findByRole("button", { name: "Confirm write to main" });
    confirm.focus();
    await userEvent.keyboard("{Enter}");
    await waitFor(() =>
      expect(screen.queryByTestId("main-branch-write-section")).not.toBeInTheDocument(),
    );
    await waitFor(() => expect(screen.getByLabelText("Commit message")).toHaveFocus());
    expect(document.activeElement).not.toBe(document.body);
  });

  it("MBW-FR-YSMC: a commit the backend refuses for the check shows the section with the carried guard, and a confirm commits once more", async () => {
    const reads = scriptedReads(mainGuard({ isMain: false, isDefault: false, branch: "feature" }));
    let refused = false;
    answers.commit_paths = async () => {
      if (!refused) {
        refused = true;
        throw unconfirmedRefusal(mainGuard({ branch: "trunk" }));
      }
      return { commitId: "abc1234", committedPaths: ["a.md"] };
    };
    const { onCommitted } = commitWindow();
    await userEvent.type(screen.getByLabelText("Commit message"), "a message");
    // Enabled only once the read has landed (MBW-FR-IOGF).
    await waitFor(() => expect(screen.getByRole("button", { name: /^Commit/ })).toBeEnabled());
    expect(reads.reads).toHaveLength(1);
    fireEvent.click(screen.getByRole("button", { name: /^Commit/ }));

    const section = await screen.findByTestId("main-branch-write-section");
    expect(section).toHaveTextContent("trunk is the default branch");
    fireEvent.click(screen.getByRole("button", { name: "Confirm write to trunk" }));
    await waitFor(() => expect(onCommitted).toHaveBeenCalled());
    expect(calls("commit_paths")).toHaveLength(2);
    expect(reads.confirms).toEqual(["trunk"]);
  });

  it("MBW-FR-IOGF, MBW-FR-JZME: a protected branch shows the GitHub line, keeps Commit unavailable until Confirm, and offers New worktree", async () => {
    const reads = scriptedReads(mainGuard({ protection: "protected" }));
    const requested = vi.fn();
    const stop = onNewWorktreeRequested(requested);
    const { onClose } = commitWindow();
    await userEvent.type(screen.getByLabelText("Commit message"), "a message");
    const section = await screen.findByTestId("main-branch-write-section");
    expect(section).toHaveTextContent("GitHub protects main and may refuse a push.");
    expect(screen.getByRole("button", { name: /^Commit/ })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "New worktree" }));
    stop();
    expect(onClose).toHaveBeenCalled();
    expect(requested).toHaveBeenCalledTimes(1);
    expect(reads.confirms).toEqual([]);
  });

  it("MBW-FR-JZME, MBW-FR-IOGF: a confirm on a protected branch makes Commit available", async () => {
    const reads = scriptedReads(mainGuard({ protection: "protected" }));
    commitWindow();
    await userEvent.type(screen.getByLabelText("Commit message"), "a message");
    await screen.findByTestId("main-branch-write-section");
    fireEvent.click(screen.getByRole("button", { name: "Confirm write to main" }));
    await waitFor(() => expect(screen.getByRole("button", { name: /^Commit/ })).toBeEnabled());
    expect(reads.confirms).toEqual(["main"]);
  });

  it("MBW-FR-IOGF: while the guard read is under way, Commit is unavailable", async () => {
    let land!: () => void;
    const reads = scriptedReads(mainGuard({ isMain: false, isDefault: false, branch: "feature" }));
    const answer = reads.read.bind(reads);
    reads.read = (branch, refresh) =>
      new Promise((resolve) => {
        land = () => resolve(answer(branch, refresh));
      });
    commitWindow();
    await userEvent.type(screen.getByLabelText("Commit message"), "a message");
    const commit = screen.getByRole("button", { name: /^Commit/ });
    expect(commit).toBeDisabled();
    land();
    await waitFor(() => expect(commit).toBeEnabled());
  });

  it("MBW-FR-UNAN, MBW-FR-IOGF: a read that fails shows the section with the error and Try again, and Commit stays unavailable", async () => {
    const reads = scriptedReads(new Error("no project open"));
    commitWindow();
    await userEvent.type(screen.getByLabelText("Commit message"), "a message");
    const section = await screen.findByTestId("main-branch-write-section");
    expect(section).toHaveTextContent("no project open");
    expect(screen.getByRole("button", { name: /^Commit/ })).toBeDisabled();
    reads.answer = mainGuard({ isMain: false, isDefault: false, branch: "feature" });
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(screen.getByRole("button", { name: /^Commit/ })).toBeEnabled());
    expect(screen.queryByTestId("main-branch-write-section")).not.toBeInTheDocument();
  });

  it("CMW-FR-RMCO: a branch that is not a main branch shows no section", async () => {
    scriptedReads(mainGuard({ isMain: false, isDefault: false, branch: "feature" }));
    commitWindow();
    await userEvent.type(screen.getByLabelText("Commit message"), "a message");
    await waitFor(() => expect(screen.getByRole("button", { name: /^Commit/ })).toBeEnabled());
    expect(screen.queryByTestId("main-branch-write-section")).not.toBeInTheDocument();
  });
});

describe("the merge confirmation (WSS-FR-DBGA)", () => {
  function confirmation() {
    const onStart = vi.fn();
    const onStartWithMessage = vi.fn();
    const onCreatePullRequest = vi.fn();
    render(
      <MergeConfirm
        summary={summary}
        dirty={null}
        onCancel={vi.fn()}
        onStart={onStart}
        onStartWithMessage={onStartWithMessage}
        onOpenChanges={vi.fn()}
        onCreatePullRequest={onCreatePullRequest}
      />,
    );
    return { onStart, onStartWithMessage, onCreatePullRequest };
  }

  it("WSS-FR-DBGA, MBW-FR-URIL: the guard of the base branch is read when it opens, and Merge stays unavailable until the author confirms", async () => {
    const reads = scriptedReads(mainGuard());
    const { onStart } = confirmation();
    await screen.findByTestId("main-branch-write-section");
    expect(reads.reads[0]).toEqual({ branch: "main", refresh: false });
    const leave = screen.getByRole("button", { name: "Leave uncommitted" });
    expect(leave).toBeDisabled();
    expect(screen.getByRole("button", { name: "Commit…" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Confirm write to main" }));
    await waitFor(() => expect(leave).toBeEnabled());
    fireEvent.click(leave);
    expect(onStart).toHaveBeenCalledWith(summary, { kind: "uncommitted" });
  });

  it("WSS-FR-DBGA, MBW-FR-JZME: a protected base branch shows the GitHub line, waits for the confirm, and points to Create a PR of the same row", async () => {
    const reads = scriptedReads(mainGuard({ protection: "protected" }));
    const { onCreatePullRequest } = confirmation();
    const section = await screen.findByTestId("main-branch-write-section");
    expect(section).toHaveTextContent("GitHub protects main and may refuse a push.");
    const leave = screen.getByRole("button", { name: "Leave uncommitted" });
    expect(leave).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Create a PR" }));
    expect(onCreatePullRequest).toHaveBeenCalledTimes(1);
    fireEvent.click(screen.getByRole("button", { name: "Confirm write to main" }));
    await waitFor(() => expect(leave).toBeEnabled());
    expect(reads.confirms).toEqual(["main"]);
  });
});

describe("the graduation start dialog (GSD-FR-YSMC)", () => {
  it("GSD-FR-YSMC, MBW-FR-URIL: the dialog reads the guard of the active worktree's branch when it opens, and the confirm stays unavailable until the author confirms", async () => {
    const reads = scriptedReads(mainGuard());
    const onStarted = vi.fn();
    render(
      <GraduationStart draftId="d-1" draftName="artifact" onClose={vi.fn()} onStarted={onStarted} />,
    );
    const section = await screen.findByTestId("main-branch-write-section");
    expect(section).toHaveTextContent("You asked to start a graduation.");
    expect(reads.reads[0]).toEqual({ branch: null, refresh: false });
    const graduate = screen.getByTestId("graduation-start-confirm");
    await waitFor(() => expect(screen.getByRole("combobox", { name: "Work stream" })).toBeInTheDocument());
    expect(graduate).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Confirm write to main" }));
    await waitFor(() => expect(graduate).toBeEnabled());
    fireEvent.click(graduate);
    await waitFor(() => expect(onStarted).toHaveBeenCalled());
    expect(calls("start_graduation")).toHaveLength(1);
  });

  it("GSD-FR-YSMC: the control of the case above: on a branch that is not a main branch, the confirm is available at the same point", async () => {
    scriptedReads(mainGuard({ isMain: false, isDefault: false, branch: "feature" }));
    render(
      <GraduationStart draftId="d-1" draftName="artifact" onClose={vi.fn()} onStarted={vi.fn()} />,
    );
    const graduate = screen.getByTestId("graduation-start-confirm");
    await waitFor(() => expect(screen.getByRole("combobox", { name: "Work stream" })).toBeInTheDocument());
    await waitFor(() => expect(graduate).toBeEnabled());
    expect(screen.queryByTestId("main-branch-write-section")).not.toBeInTheDocument();
  });

  it("MBW-FR-YSMC: a start the backend refuses for the check shows the section, and a confirm starts once more", async () => {
    scriptedReads(mainGuard({ isMain: false, isDefault: false, branch: "feature" }));
    let refused = false;
    answers.start_graduation = async () => {
      if (!refused) {
        refused = true;
        throw unconfirmedRefusal();
      }
      return { id: "g-2" };
    };
    const onStarted = vi.fn();
    render(
      <GraduationStart draftId="d-1" draftName="artifact" onClose={vi.fn()} onStarted={onStarted} />,
    );
    const graduate = screen.getByTestId("graduation-start-confirm");
    await waitFor(() => expect(graduate).toBeEnabled());
    fireEvent.click(graduate);
    await screen.findByTestId("main-branch-write-section");
    expect(screen.queryByTestId("graduation-start-error")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Confirm write to main" }));
    await waitFor(() => expect(onStarted).toHaveBeenCalled());
    expect(calls("start_graduation")).toHaveLength(2);
  });
});

describe("the pull reconcile dialog (MBW-FR-HXTE)", () => {
  it("MBW-FR-HXTE, MBW-FR-QKVA: a pull reconcile on a main branch reads no guard, shows no section, and runs at once", async () => {
    const reads = scriptedReads(mainGuard());
    render(<PullReconcileDialog outcome={diverged} onClose={vi.fn()} onOpenRun={vi.fn()} />);
    const merge = screen.getByTestId("pull-reconcile-merge");
    expect(merge).toBeEnabled();
    expect(screen.queryByTestId("main-branch-write-section")).not.toBeInTheDocument();
    fireEvent.click(merge);
    await waitFor(() => expect(calls("reconcile_pull")).toHaveLength(1));
    expect(reads.reads).toEqual([]);
    expect(reads.confirms).toEqual([]);
  });
});
