import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Git } from "./Git";
import type { BranchDeletionOutcome, BranchDeletionPlan, GitBranch } from "../types";
import { branchInformation, deferred, deletionPlan } from "../test/gitPanelFixtures";

// Deleting a branch with its worktree (GIT-FR-QYWP, GIT-FR-GAMV, GIT-FR-SIJB,
// GIT-FR-CTHN, GIT-FR-XOLE, GIT-FR-UDKY).

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
}));

let branches: GitBranch[];
let inspect: (name: string) => Promise<BranchDeletionPlan>;
let remove: (args: Record<string, unknown>) => Promise<BranchDeletionOutcome>;
let removeStream: (args: Record<string, unknown>) => Promise<void>;
let streamPaths: (id: string) => Promise<string[]>;
const onSwitchWorktree = vi.fn(async () => ({ ok: true as const }));

const OUTCOME: BranchDeletionOutcome = {
  branch: "feature/x",
  removedWorktreePath: "/work/feature-x",
  remote: { requested: false, state: "not_requested" },
};

beforeEach(() => {
  branches = [
    { name: "main", kind: "local", isCurrent: true },
    { name: "feature/x", kind: "local", isCurrent: false },
    { name: "origin/feature/x", kind: "remote", isCurrent: false },
  ];
  inspect = async () => deletionPlan();
  remove = async () => OUTCOME;
  removeStream = async () => undefined;
  streamPaths = async () => [];
  onSwitchWorktree.mockClear();
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown>) => {
    switch (cmd) {
      case "list_branches":
        return branches;
      case "inspect_branch_deletion":
        return inspect(args.name as string);
      case "delete_branch":
        return remove(args);
      case "delete_work_stream":
        return removeStream(args);
      case "get_work_stream_uncommitted_paths":
        return streamPaths(args.streamId as string);
      case "get_branch_information":
        return branchInformation({ name: args.name as string });
      case "get_upstream_sync_state":
        return { hasRemote: true, hasUpstream: true, ahead: 0, behind: 0 };
      default:
        return undefined;
    }
  });
});

afterEach(cleanup);

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);
// Scoped to the rail: a selected branch is named in the large view too.
const row = (name: string) =>
  within(screen.getByTestId("git-branches"))
    .getByText(name)
    .closest<HTMLElement>('[role="button"]')!;

const openBranches = async (token?: () => Promise<boolean>) => {
  render(
    <Git
      onSwitchWorktree={onSwitchWorktree}
      canCheckOutBranches
      onRequestGithubToken={token}
    />,
  );
  await userEvent.click(screen.getByText("Branches"));
  const section = await screen.findByTestId("git-branches");
  await within(section).findByText("feature/x");
  return section;
};

/** Open the row's menu from the keyboard and choose Delete. */
const chooseDelete = async (name = "feature/x") => {
  row(name).focus();
  await userEvent.keyboard("{ContextMenu}");
  await userEvent.click(await screen.findByRole("menuitem", { name: "Delete" }));
};

const openDialog = async (name = "feature/x") => {
  await chooseDelete(name);
  return screen.findByTestId("git-branch-delete");
};

const confirmButton = (dialog: HTMLElement) =>
  within(dialog).getByRole("button", { name: /^Delete/ });

describe("refusals read at the inspection (GIT-FR-GAMV)", () => {
  it.each([
    ["branch_in_primary_worktree", /Branch feature\/x is checked out in the primary worktree\. Check out another branch or switch worktree first\./],
    ["branch_in_active_worktree", /Branch feature\/x is checked out in the active worktree\. Check out another branch or switch worktree first\./],
  ])("GIT-FR-GAMV, GIT-FR-QYWP: %s shows beside the row, opens no confirmation and switches nothing", async (code, sentence) => {
    inspect = async () => {
      throw code;
    };
    await openBranches();
    await chooseDelete();

    const refusal = await screen.findByTestId("git-branch-refusal");
    expect(refusal).toHaveTextContent(sentence);
    // It stands beside the row it is about, in the same block.
    expect(row("feature/x").parentElement).toContainElement(refusal);
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(calls("delete_branch")).toHaveLength(0);
    expect(onSwitchWorktree).not.toHaveBeenCalled();
    expect(calls("inspect_branch_deletion")[0][1]).toEqual({ name: "feature/x" });
  });

  it("GIT-FR-UDKY: a new attempt clears the earlier refusal of the row", async () => {
    inspect = async () => {
      throw "branch_in_active_worktree";
    };
    await openBranches();
    await chooseDelete();
    await screen.findByTestId("git-branch-refusal");

    inspect = async () => deletionPlan();
    await chooseDelete();
    await screen.findByTestId("git-branch-delete");
    expect(screen.queryByTestId("git-branch-refusal")).toBeNull();
  });

  it("GIT-FR-UDKY: the row shows the request running, and every other control stays usable", async () => {
    const pending = deferred<BranchDeletionPlan>();
    inspect = () => pending.promise;
    const section = await openBranches();
    await chooseDelete();
    expect(await within(row("feature/x")).findByText("Checking…")).toBeInTheDocument();

    // The switch, the other rows and the other sections all still work.
    await userEvent.click(within(section).getByRole("radio", { name: "Remote" }));
    expect(within(section).getByText("origin/feature/x")).toBeInTheDocument();
    await userEvent.click(within(section).getByRole("radio", { name: "Local" }));
    await userEvent.click(within(section).getByText("main"));
    expect(onSwitchWorktree).not.toHaveBeenCalled();
    // Another row opens its own menu while the request runs.
    fireEvent.contextMenu(row("main"));
    expect(await screen.findByRole("menu")).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");

    await act(async () => pending.resolve(deletionPlan()));
    expect(await screen.findByTestId("git-branch-delete")).toBeInTheDocument();
  });

  it("GIT-FR-EPSV: an inspection that arrives after the author left the section opens nothing", async () => {
    const pending = deferred<BranchDeletionPlan>();
    inspect = () => pending.promise;
    await openBranches();
    await chooseDelete();
    await userEvent.click(screen.getByRole("tab", { name: "Commits" }));
    expect(screen.getByRole("tab", { name: "Commits" })).toHaveAttribute("aria-selected", "true");

    await act(async () => pending.resolve(deletionPlan()));
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("GIT-FR-EPSV: an inspection that arrives after another overlay opened does not replace it", async () => {
    const pending = deferred<BranchDeletionPlan>();
    inspect = () => pending.promise;
    await openBranches();
    await chooseDelete();
    // The author asks for the information of another branch meanwhile.
    row("main").focus();
    await userEvent.keyboard("{ContextMenu}");
    await userEvent.click(await screen.findByRole("menuitem", { name: "Information" }));
    await screen.findByTestId("git-branch-info");

    await act(async () => pending.resolve(deletionPlan()));
    expect(screen.queryByTestId("git-branch-delete")).toBeNull();
    expect(screen.getByTestId("git-branch-info")).toBeInTheDocument();
  });
});

describe("the confirmation (GIT-FR-QYWP)", () => {
  it("GIT-FR-08, GIT-FR-QYWP: names the branch, the worktree path and the effect, and deletes nothing before it is confirmed", async () => {
    await openBranches();
    const dialog = await openDialog();

    expect(within(dialog).getByText("Delete branch feature/x")).toBeInTheDocument();
    expect(within(dialog).getByTestId("git-delete-effect")).toHaveTextContent(
      "This removes the local branch feature/x and its worktree /work/feature-x.",
    );
    expect(dialog.contains(document.activeElement)).toBe(true);
    // Cancel holds the focus, so a stray Enter removes nothing.
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toHaveFocus();
    expect(calls("delete_branch")).toHaveLength(0);
  });

  it("GIT-FR-QYWP: a branch with no linked worktree says so", async () => {
    inspect = async () => deletionPlan({ worktree: undefined });
    await openBranches();
    const dialog = await openDialog();
    expect(within(dialog).getByTestId("git-delete-effect")).toHaveTextContent(
      "It has no linked worktree.",
    );
  });

  it("GIT-FR-QYWP: Cancel closes it, changes nothing and returns focus to the row", async () => {
    await openBranches();
    const dialog = await openDialog();
    await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(calls("delete_branch")).toHaveLength(0);
    expect(row("feature/x")).toHaveFocus();
    expect(screen.queryByTestId("git-delete-result")).toBeNull();
  });

  it("GIT-FR-QYWP: Escape and the backdrop close it and change nothing", async () => {
    await openBranches();
    await openDialog();
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(row("feature/x")).toHaveFocus();

    await openDialog();
    fireEvent.mouseDown(screen.getByTestId("git-overlay-backdrop"));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(calls("delete_branch")).toHaveLength(0);
    expect(calls("delete_work_stream")).toHaveLength(0);
  });

  it("GIT-FR-QYWP, GIT-FR-UDKY: confirming deletes without discarding, reloads the list and names the removed branch, the removed worktree and the remote state", async () => {
    await openBranches();
    const dialog = await openDialog();
    expect(within(dialog).queryByTestId("git-delete-dirty-warning")).toBeNull();
    const listed = calls("list_branches").length;

    branches = [{ name: "main", kind: "local", isCurrent: true }];
    await userEvent.click(confirmButton(dialog));

    const result = await screen.findByTestId("git-delete-result");
    expect(calls("delete_branch")[0][1]).toEqual({
      name: "feature/x",
      deleteRemote: false,
      discardUncommitted: false,
    });
    expect(result).toHaveTextContent("Removed local branch feature/x.");
    expect(result).toHaveTextContent("Removed worktree /work/feature-x.");
    expect(screen.getByTestId("git-delete-result-remote")).toHaveTextContent(
      "Remote branch: not requested.",
    );
    expect(screen.queryByRole("dialog")).toBeNull();
    // The list reloads and no longer holds the branch; focus is on the message.
    await waitFor(() => expect(calls("list_branches").length).toBe(listed + 1));
    await waitFor(() => expect(screen.queryByText("feature/x")).toBeNull());
    expect(result).toHaveFocus();
    expect(onSwitchWorktree).not.toHaveBeenCalled();

    await userEvent.click(within(result).getByRole("button", { name: "Dismiss" }));
    expect(screen.queryByTestId("git-delete-result")).toBeNull();
  });

  it("GIT-FR-UDKY: a typed refusal of the deletion shows in the confirmation, names the branch, and the confirmation stays open", async () => {
    remove = async () => {
      throw "branch_in_active_worktree";
    };
    await openBranches();
    const dialog = await openDialog();
    await userEvent.click(confirmButton(dialog));

    expect(await within(dialog).findByTestId("git-delete-error")).toHaveTextContent(
      "Branch feature/x is checked out in the active worktree. Check out another branch or switch worktree first.",
    );
    expect(screen.getByTestId("git-branch-delete")).toBeInTheDocument();
    expect(screen.queryByTestId("git-delete-result")).toBeNull();
    expect(onSwitchWorktree).not.toHaveBeenCalled();
  });

  it("GIT-FR-UDKY: a running deletion disables only its own controls and cannot be dismissed", async () => {
    const pending = deferred<BranchDeletionOutcome>();
    remove = () => pending.promise;
    await openBranches();
    const dialog = await openDialog();
    await userEvent.click(confirmButton(dialog));

    const busy = await within(dialog).findByRole("button", { name: "Deleting…" });
    expect(busy).toBeDisabled();
    expect(within(dialog).getByRole("button", { name: "Cancel" })).toBeDisabled();
    await userEvent.keyboard("{Escape}");
    expect(screen.getByTestId("git-branch-delete")).toBeInTheDocument();

    await act(async () => pending.resolve(OUTCOME));
    expect(await screen.findByTestId("git-delete-result")).toBeInTheDocument();
  });
});

describe("uncommitted paths (GIT-FR-SIJB)", () => {
  const paths = ["a.txt", "b.txt", "c.txt", "d.txt", "e.txt", "f.txt", "g.txt", "h.txt"];

  it("GIT-FR-SIJB: warns with the count and the first paths, counts the rest, and names the discard on the confirm action", async () => {
    inspect = async () => deletionPlan({ uncommittedPaths: paths });
    await openBranches();
    const dialog = await openDialog();

    const warning = within(dialog).getByTestId("git-delete-dirty-warning");
    expect(warning).toHaveTextContent("confirming discards 8 uncommitted paths");
    for (const p of paths.slice(0, 5)) expect(within(warning).getByText(p)).toBeInTheDocument();
    for (const p of paths.slice(5)) expect(within(warning).queryByText(p)).toBeNull();
    expect(warning).toHaveTextContent("and 3 more not listed");
    expect(
      within(dialog).getByRole("button", { name: "Delete and discard changes" }),
    ).toBeInTheDocument();
  });

  it("GIT-FR-SIJB: Cancel discards nothing, and confirming passes the discard", async () => {
    inspect = async () => deletionPlan({ uncommittedPaths: paths });
    await openBranches();
    let dialog = await openDialog();
    await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    expect(calls("delete_branch")).toHaveLength(0);

    dialog = await openDialog();
    await userEvent.click(
      within(dialog).getByRole("button", { name: "Delete and discard changes" }),
    );
    await screen.findByTestId("git-delete-result");
    expect(calls("delete_branch")[0][1]).toEqual({
      name: "feature/x",
      deleteRemote: false,
      discardUncommitted: true,
    });
  });

  it("GIT-FR-SIJB: a worktree_dirty refusal after the plan shows the same warning with the new paths, and a second confirmation discards", async () => {
    remove = vi
      .fn<(args: Record<string, unknown>) => Promise<BranchDeletionOutcome>>()
      .mockRejectedValueOnce("worktree_dirty: new1.txt, new2.txt")
      .mockResolvedValueOnce(OUTCOME);
    await openBranches();
    const dialog = await openDialog();
    expect(within(dialog).queryByTestId("git-delete-dirty-warning")).toBeNull();
    await userEvent.click(confirmButton(dialog));

    const warning = await within(dialog).findByTestId("git-delete-dirty-warning");
    expect(warning).toHaveTextContent("confirming discards 2 uncommitted paths");
    expect(within(warning).getByText("new1.txt")).toBeInTheDocument();
    expect(within(warning).getByText("new2.txt")).toBeInTheDocument();
    expect(screen.queryByTestId("git-delete-result")).toBeNull();
    expect(calls("delete_branch")).toHaveLength(1);

    await userEvent.click(
      within(dialog).getByRole("button", { name: "Delete and discard changes" }),
    );
    await screen.findByTestId("git-delete-result");
    expect(calls("delete_branch")).toHaveLength(2);
    expect(calls("delete_branch")[1][1]).toMatchObject({ discardUncommitted: true });
  });
});

describe("the remote branch (GIT-FR-CTHN)", () => {
  beforeEach(() => {
    inspect = async () => deletionPlan({ remoteBranch: "origin/feature/x" });
  });

  it("GIT-FR-CTHN: offers an unchecked checkbox, and deletes the remote only while it is checked", async () => {
    await openBranches();
    let dialog = await openDialog();
    const box = within(dialog).getByRole("checkbox", {
      name: "Also delete the remote branch origin/feature/x",
    });
    expect(box).not.toBeChecked();
    await userEvent.click(confirmButton(dialog));
    await screen.findByTestId("git-delete-result");
    expect(calls("delete_branch")[0][1]).toMatchObject({ deleteRemote: false });

    dialog = await openDialog();
    await userEvent.click(within(dialog).getByRole("checkbox"));
    remove = async () => ({
      ...OUTCOME,
      remote: { requested: true, state: "deleted", branch: "origin/feature/x" },
    });
    await userEvent.click(confirmButton(dialog));
    await waitFor(() => expect(calls("delete_branch")).toHaveLength(2));
    expect(calls("delete_branch")[1][1]).toMatchObject({ deleteRemote: true });
    await waitFor(() =>
      expect(screen.getByTestId("git-delete-result-remote")).toHaveTextContent(
        "Remote branch origin/feature/x: deleted.",
      ),
    );
  });

  it("GIT-FR-CTHN: offers no checkbox when the branch has no remote branch", async () => {
    inspect = async () => deletionPlan();
    await openBranches();
    const dialog = await openDialog();
    expect(within(dialog).queryByRole("checkbox")).toBeNull();
  });

  it("GIT-FR-CTHN, GIT-FR-UDKY: a failed remote deletion shows as a failure with its typed cause, never as deleted, and the local result still shows", async () => {
    remove = async () => ({
      ...OUTCOME,
      remote: {
        requested: true,
        state: "failed",
        branch: "origin/feature/x",
        error: "github_token_rejected",
      },
    });
    await openBranches();
    const dialog = await openDialog();
    await userEvent.click(within(dialog).getByRole("checkbox"));
    await userEvent.click(confirmButton(dialog));

    const result = await screen.findByTestId("git-delete-result");
    const remote = within(result).getByTestId("git-delete-result-remote");
    expect(remote).toHaveTextContent("FAILED");
    expect(remote).toHaveTextContent("GitHub rejected the token of this project.");
    expect(remote.textContent).not.toMatch(/deleted/i);
    expect(result).toHaveTextContent("Removed local branch feature/x.");
    expect(result).toHaveTextContent("Removed worktree /work/feature-x.");
  });

  it("GIT-FR-CTHN, GIT-FR-10: a missing token for the remote deletion shows in the confirmation and removes nothing", async () => {
    remove = async () => {
      throw "github_token_missing";
    };
    await openBranches();
    const dialog = await openDialog();
    await userEvent.click(within(dialog).getByRole("checkbox"));
    await userEvent.click(confirmButton(dialog));
    expect(await within(dialog).findByTestId("git-delete-error")).toHaveTextContent(
      "No GitHub token is stored. Add one in Global settings → GitHub.",
    );
    expect(screen.queryByTestId("git-delete-result")).toBeNull();
  });

  it("GIT-FR-CTHN, GIT-FR-10: a selection-required token opens the picker and the deletion runs once on confirmation", async () => {
    remove = vi
      .fn<(args: Record<string, unknown>) => Promise<BranchDeletionOutcome>>()
      .mockRejectedValueOnce("github_token_selection_required")
      .mockResolvedValueOnce(OUTCOME);
    const picker = vi.fn(async () => true);
    await openBranches(picker);
    const dialog = await openDialog();
    await userEvent.click(within(dialog).getByRole("checkbox"));
    await userEvent.click(confirmButton(dialog));

    await screen.findByTestId("git-delete-result");
    expect(picker).toHaveBeenCalledOnce();
    expect(calls("delete_branch")).toHaveLength(2);
  });

  it("GIT-FR-CTHN, GIT-FR-10: cancelling the picker abandons the deletion", async () => {
    remove = async () => {
      throw "github_token_selection_required";
    };
    const picker = vi.fn(async () => false);
    await openBranches(picker);
    const dialog = await openDialog();
    await userEvent.click(within(dialog).getByRole("checkbox"));
    await userEvent.click(confirmButton(dialog));

    expect(await within(dialog).findByTestId("git-delete-error")).toHaveTextContent(
      "No GitHub token was selected",
    );
    expect(calls("delete_branch")).toHaveLength(1);
  });
});

describe("a branch of a work stream (GIT-FR-XOLE)", () => {
  const STREAM_PLAN = () =>
    deletionPlan({
      remoteBranch: "origin/feature/x",
      stream: { streamId: "s1", streamName: "Stream One", busyRunId: null, aheadOfBase: 0 },
    });
  beforeEach(() => {
    inspect = async () => STREAM_PLAN();
  });

  it("GIT-FR-XOLE: warns that the branch belongs to a work stream and offers no remote deletion", async () => {
    await openBranches();
    const dialog = await openDialog();
    expect(within(dialog).getByTestId("git-delete-stream-warning")).toHaveTextContent(
      "Branch feature/x belongs to the work stream Stream One.",
    );
    expect(within(dialog).queryByRole("checkbox")).toBeNull();
    expect(within(dialog).getByTestId("git-delete-effect")).toHaveTextContent(
      "This removes the work stream Stream One, its branch feature/x and its worktree /work/feature-x.",
    );
  });

  it("GIT-FR-XOLE: confirming uses the work stream deletion and never the branch deletion", async () => {
    await openBranches();
    const dialog = await openDialog();
    await userEvent.click(within(dialog).getByRole("button", { name: "Delete work stream" }));

    const result = await screen.findByTestId("git-delete-result");
    expect(calls("delete_work_stream")[0][1]).toEqual({
      streamId: "s1",
      force: false,
      discardUncommitted: false,
    });
    expect(calls("delete_branch")).toHaveLength(0);
    expect(calls("get_work_stream_uncommitted_paths")[0][1]).toEqual({ streamId: "s1" });
    expect(result).toHaveTextContent("Removed work stream Stream One with its branch feature/x.");
    expect(screen.getByTestId("git-delete-result-remote")).toHaveTextContent("not requested");
  });

  it("GIT-FR-XOLE, GIT-FR-SIJB: the discarded paths are named from the work stream's own read, and the discard is passed", async () => {
    streamPaths = async () => ["x1.txt", "x2.txt"];
    await openBranches();
    const dialog = await openDialog();
    const warning = await within(dialog).findByTestId("git-delete-dirty-warning");
    expect(warning).toHaveTextContent("confirming discards 2 uncommitted paths");
    await userEvent.click(
      within(dialog).getByRole("button", { name: "Delete and discard changes" }),
    );
    await screen.findByTestId("git-delete-result");
    expect(calls("delete_work_stream")[0][1]).toMatchObject({ discardUncommitted: true });
    expect(calls("delete_branch")).toHaveLength(0);
  });

  it("GIT-FR-XOLE, GIT-FR-SIJB: a stream_dirty refusal shows the warning with the new paths and a second confirmation discards", async () => {
    removeStream = vi
      .fn<(args: Record<string, unknown>) => Promise<void>>()
      .mockRejectedValueOnce("stream_dirty: n1.txt, n2.txt")
      .mockResolvedValueOnce(undefined);
    await openBranches();
    const dialog = await openDialog();
    await userEvent.click(within(dialog).getByRole("button", { name: "Delete work stream" }));
    const warning = await within(dialog).findByTestId("git-delete-dirty-warning");
    expect(within(warning).getByText("n1.txt")).toBeInTheDocument();
    await userEvent.click(
      within(dialog).getByRole("button", { name: "Delete and discard changes" }),
    );
    await screen.findByTestId("git-delete-result");
    expect(calls("delete_work_stream")[1][1]).toMatchObject({ discardUncommitted: true });
  });

  it.each([
    ["stream_busy", "A run holds this work stream or waits in its queue."],
    ["stream_has_runs", "Runs of this work stream are still working."],
    ["stream_active", "The worktree of this work stream is the active one."],
  ])("GIT-FR-XOLE, GIT-FR-UDKY: the refusal %s shows in the confirmation", async (code, sentence) => {
    removeStream = async () => {
      throw code;
    };
    await openBranches();
    const dialog = await openDialog();
    await userEvent.click(within(dialog).getByRole("button", { name: "Delete work stream" }));
    expect(await within(dialog).findByTestId("git-delete-error")).toHaveTextContent(sentence);
    expect(screen.queryByTestId("git-delete-result")).toBeNull();
    expect(calls("delete_branch")).toHaveLength(0);
  });
});

describe("a work stream with unmerged commits (GIT-FR-XOLE)", () => {
  const AHEAD_PLAN = (aheadOfBase: number) =>
    deletionPlan({
      stream: { streamId: "s1", streamName: "Stream One", busyRunId: null, aheadOfBase },
    });

  it("GIT-FR-XOLE: warns with the count, and Delete anyway passes force", async () => {
    inspect = async () => AHEAD_PLAN(6);
    await openBranches();
    const dialog = await openDialog();

    const warning = await within(dialog).findByTestId("git-delete-unmerged-warning");
    expect(warning).toHaveTextContent("holds 6 commits that its base branch does not hold");
    await userEvent.click(within(dialog).getByRole("button", { name: "Delete anyway" }));

    await screen.findByTestId("git-delete-result");
    expect(calls("delete_work_stream")[0][1]).toEqual({
      streamId: "s1",
      force: true,
      discardUncommitted: false,
    });
    expect(calls("delete_branch")).toHaveLength(0);
  });

  it("GIT-FR-XOLE, GIT-FR-SIJB: the confirm action names the force and the discard together", async () => {
    inspect = async () => AHEAD_PLAN(1);
    streamPaths = async () => ["x1.txt"];
    await openBranches();
    const dialog = await openDialog();

    expect(await within(dialog).findByTestId("git-delete-unmerged-warning")).toHaveTextContent(
      "holds 1 commit that",
    );
    await within(dialog).findByTestId("git-delete-dirty-warning");
    await userEvent.click(
      within(dialog).getByRole("button", { name: "Delete anyway and discard changes" }),
    );
    await screen.findByTestId("git-delete-result");
    expect(calls("delete_work_stream")[0][1]).toEqual({
      streamId: "s1",
      force: true,
      discardUncommitted: true,
    });
  });

  it("GIT-FR-XOLE: a stream with no unmerged commit is deleted without force and shows no warning", async () => {
    inspect = async () => AHEAD_PLAN(0);
    await openBranches();
    const dialog = await openDialog();
    expect(within(dialog).queryByTestId("git-delete-unmerged-warning")).toBeNull();
    await userEvent.click(within(dialog).getByRole("button", { name: "Delete work stream" }));
    await screen.findByTestId("git-delete-result");
    expect(calls("delete_work_stream")[0][1]).toMatchObject({ force: false });
  });

  it("GIT-FR-XOLE: a stream_unmerged refusal turns into the warning, and a second confirmation forces it", async () => {
    inspect = async () => AHEAD_PLAN(0);
    removeStream = vi
      .fn<(args: Record<string, unknown>) => Promise<void>>()
      .mockRejectedValueOnce("stream_unmerged: 3")
      .mockResolvedValueOnce(undefined);
    await openBranches();
    const dialog = await openDialog();
    await userEvent.click(within(dialog).getByRole("button", { name: "Delete work stream" }));

    expect(await within(dialog).findByTestId("git-delete-unmerged-warning")).toHaveTextContent(
      "holds 3 commits",
    );
    expect(within(dialog).queryByTestId("git-delete-error")).toBeNull();
    await userEvent.click(within(dialog).getByRole("button", { name: "Delete anyway" }));
    await screen.findByTestId("git-delete-result");
    expect(calls("delete_work_stream")[0][1]).toMatchObject({ force: false });
    expect(calls("delete_work_stream")[1][1]).toMatchObject({ force: true });
  });

  it.each([
    ["stream_busy", "A run holds this work stream or waits in its queue."],
    ["stream_has_runs", "Runs of this work stream are still working."],
    ["stream_active", "The worktree of this work stream is the active one."],
  ])("GIT-FR-XOLE: %s stays a refusal even when deleting anyway", async (code, sentence) => {
    inspect = async () => AHEAD_PLAN(2);
    removeStream = async () => {
      throw code;
    };
    await openBranches();
    const dialog = await openDialog();
    await userEvent.click(within(dialog).getByRole("button", { name: "Delete anyway" }));
    expect(await within(dialog).findByTestId("git-delete-error")).toHaveTextContent(sentence);
    expect(screen.queryByTestId("git-delete-result")).toBeNull();
  });
});

