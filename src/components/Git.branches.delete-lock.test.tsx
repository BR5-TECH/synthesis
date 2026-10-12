import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Git } from "./Git";
import type { BranchDeletionOutcome, BranchDeletionPlan, GitBranch } from "../types";
import { branchInformation, deletionPlan } from "../test/gitPanelFixtures";
import { flushLogs, resetLogBufferForTest } from "../logging";

// The lock notice of a branch deletion (GIT-FR-LUEY, GTC-FR-JUZP).

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
}));

const REASON = "claude agent agent-aea84 (pid 89346 start Sun Oct 11 14:19:41 2026)";

const OUTCOME: BranchDeletionOutcome = {
  branch: "feature/x",
  removedWorktreePath: "/work/feature-x",
  remote: { requested: false, state: "not_requested" },
};

let plan: BranchDeletionPlan;

beforeEach(() => {
  resetLogBufferForTest();
  plan = deletionPlan();
  const branches: GitBranch[] = [
    { name: "main", kind: "local", isCurrent: true },
    { name: "feature/x", kind: "local", isCurrent: false },
  ];
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown>) => {
    switch (cmd) {
      case "append_log_records":
        return undefined;
      case "list_branches":
        return branches;
      case "inspect_branch_deletion":
        return plan;
      case "delete_branch":
        return OUTCOME;
      case "delete_work_stream":
        return undefined;
      case "get_work_stream_uncommitted_paths":
        return [];
      case "get_branch_information":
        return branchInformation({ name: args.name as string });
      case "get_upstream_sync_state":
        return { hasRemote: true, hasUpstream: true, ahead: 0, behind: 0 };
      default:
        throw new Error(`unexpected command ${cmd}`);
    }
  });
});

afterEach(cleanup);

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

const openDialog = async () => {
  render(<Git onSwitchWorktree={async () => ({ ok: true as const })} canCheckOutBranches />);
  await userEvent.click(screen.getByText("Branches"));
  const section = await screen.findByTestId("git-branches");
  const row = (await within(section).findByText("feature/x")).closest<HTMLElement>(
    '[role="button"]',
  )!;
  row.focus();
  await userEvent.keyboard("{ContextMenu}");
  await userEvent.click(await screen.findByRole("menuitem", { name: "Delete" }));
  return screen.findByTestId("git-branch-delete");
};

describe("the lock notice (GIT-FR-LUEY)", () => {
  it("GIT-FR-LUEY: a locked worktree with a reason shows the reason and that the lock goes", async () => {
    plan = deletionPlan({ worktreeLock: { reason: REASON } });
    const dialog = await openDialog();

    const notice = within(dialog).getByTestId("git-delete-lock-notice");
    expect(notice.textContent).toBe(
      `This worktree is locked: ${REASON}. Deleting it removes the lock.`,
    );
    // Information, not the discard warning.
    expect(notice).toHaveAttribute("role", "note");
    expect(notice).not.toHaveClass("git-delete__warning");
    expect(within(dialog).queryByTestId("git-delete-dirty-warning")).toBeNull();
  });

  it("GIT-FR-LUEY: a locked worktree with no reason shows the short notice", async () => {
    plan = deletionPlan({ worktreeLock: {} });
    const dialog = await openDialog();

    expect(within(dialog).getByTestId("git-delete-lock-notice")).toHaveTextContent(
      /^This worktree is locked\. Deleting it removes the lock\.$/,
    );
  });

  it("GIT-FR-LUEY: an unlocked worktree shows no notice", async () => {
    const dialog = await openDialog();

    expect(within(dialog).getByTestId("git-delete-effect")).toBeInTheDocument();
    expect(within(dialog).queryByTestId("git-delete-lock-notice")).toBeNull();
  });

  it("GIT-FR-LUEY, GIT-FR-SIJB: the notice and the discard warning show together", async () => {
    plan = deletionPlan({ worktreeLock: { reason: REASON }, uncommittedPaths: ["a.txt"] });
    const dialog = await openDialog();

    expect(within(dialog).getByTestId("git-delete-lock-notice")).toBeInTheDocument();
    expect(within(dialog).getByTestId("git-delete-dirty-warning")).toBeInTheDocument();
  });

  it("GIT-FR-LUEY: confirming a locked worktree sends the same deletion as an unlocked one", async () => {
    plan = deletionPlan({ worktreeLock: { reason: REASON } });
    const dialog = await openDialog();

    await userEvent.click(within(dialog).getByRole("button", { name: "Delete branch" }));

    await waitFor(() => expect(calls("delete_branch")).toHaveLength(1));
    expect(calls("delete_branch")[0][1]).toEqual({
      name: "feature/x",
      deleteRemote: false,
      discardUncommitted: false,
    });
    await waitFor(() => expect(screen.queryByTestId("git-branch-delete")).toBeNull());
  });

  it("GIT-FR-LUEY, GIT-FR-XOLE: a locked stream branch shows the notice and confirming deletes only the work stream", async () => {
    plan = deletionPlan({
      worktreeLock: { reason: REASON },
      stream: { streamId: "s1", streamName: "Test", busyRunId: null, aheadOfBase: 0 },
    });
    const dialog = await openDialog();

    expect(within(dialog).getByTestId("git-delete-lock-notice")).toBeInTheDocument();
    const confirm = await within(dialog).findByRole("button", { name: "Delete work stream" });
    await waitFor(() => expect(confirm).toBeEnabled());
    await userEvent.click(confirm);

    await waitFor(() => expect(calls("delete_work_stream")).toHaveLength(1));
    expect(calls("delete_work_stream")[0][1]).toEqual({
      streamId: "s1",
      force: false,
      discardUncommitted: false,
    });
    expect(calls("delete_branch")).toHaveLength(0);
  });
});

describe("the lock in the records (GTC-FR-JUZP)", () => {
  it("GTC-FR-JUZP: the dialog record names the lock as a boolean and never the reason", async () => {
    plan = deletionPlan({ worktreeLock: { reason: REASON } });
    const dialog = await openDialog();
    await userEvent.click(within(dialog).getByRole("button", { name: "Delete branch" }));
    await waitFor(() => expect(calls("delete_branch")).toHaveLength(1));

    flushLogs();
    const records = calls("append_log_records").flatMap(
      (c) => (c[1] as { records: { message: string; fields: Record<string, unknown> }[] }).records,
    );
    const shown = records.find((r) => r.message === "branch deletion dialog shown");
    expect(shown?.fields).toMatchObject({ locked: true });
    expect(JSON.stringify(records)).not.toContain("pid 89346");
  });
});
