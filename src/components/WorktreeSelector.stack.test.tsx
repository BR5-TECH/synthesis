/**
 * The stack step of the New worktree dialog
 * (`../../specifications/ui/WTS-worktree-selector.md` WTS-FR-HUQW,
 * WTS-FR-PZEO, WTS-FR-GDKX, WTS-FR-XQOB, WTS-FR-BMQI, WTS-FR-KVPA,
 * WTS-FR-DXCE, WTS-FR-21; `NTF-notifications.md` NTF-FR-HCWM;
 * `../../specifications/core/WTC-worktree-context.md` WTC-FR-SBNV,
 * WTC-FR-NQZE).
 *
 * The backend is a mocked `invoke` that answers by command name and rejects
 * every command it does not expect. Each wait is for a state.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { WorktreeSelector, type SwitchOutcome } from "./WorktreeSelector";
import { finalBranchName, namesNewBranch } from "./WorktreeSelector/newWorktreeStack";
import type { BranchStack, GitBranch, WorktreeContext, WorktreeEntry } from "../types";
import { flushLogs, resetLogBufferForTest } from "../logging";
import { clearToasts, visibleToasts } from "../state/toasts";
import { stackOn, stackUnavailable } from "../test/createPullRequestFixtures";
import { ACTIVE, REPO, defaultWorktreeContext, wt } from "../test/worktreeFixtures";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
}));

const LOCAL: GitBranch[] = [
  { name: "main", kind: "local", isCurrent: false },
  { name: ACTIVE.branch!, kind: "local", isCurrent: true },
  { name: "Feature/Existing", kind: "local", isCurrent: false },
  { name: "origin/remote-only", kind: "remote", isCurrent: false },
];

/** The top of a stack with an open pull request. */
const TOP = stackOn(ACTIVE.branch!, "main", {
  source: "github",
  stackNumber: 4,
  pullRequest: { number: 21, title: "New window" },
});

let stack: () => Promise<BranchStack>;
let addToStack: (args: Record<string, unknown>) => Promise<BranchStack>;

/** A lower layer of a stack, with an open pull request, under `feature/docs`. */
const LOWER = { ...TOP, isTop: false, top: "feature/docs" };

/** The listing `create_worktree` answers: the new worktree is active. */
function createdContext(branch: string): WorktreeContext {
  const context = defaultWorktreeContext();
  context.worktrees = [
    { ...ACTIVE, isActive: false },
    wt({ path: `${REPO}-${branch}`, branch, isActive: true }),
  ];
  return context;
}

beforeEach(() => {
  resetLogBufferForTest();
  clearToasts();
  stack = async () => TOP;
  addToStack = async (args) => stackOn(args.branch as string, args.parent as string);
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown>) => {
    switch (cmd) {
      case "list_worktrees_and_branches":
        return defaultWorktreeContext();
      case "propose_worktree_path":
        return `${REPO}-${(args?.branch as string) ?? ""}`;
      case "get_branch_stack":
        return stack();
      case "list_branches":
        return LOCAL;
      case "get_default_branch":
        return "main";
      case "create_worktree":
        return createdContext(args.branch as string);
      case "add_branch_to_stack":
        return addToStack(args);
      case "append_log_records":
        return undefined;
      default:
        return Promise.reject(new Error(`unexpected command ${cmd}`));
    }
  });
});

afterEach(cleanup);

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

function renderSelector(active: WorktreeEntry = ACTIVE) {
  // The shell runs the operation it is handed (OVW-FR-12).
  const onSwitch = vi.fn(async (op: () => Promise<WorktreeContext>): Promise<SwitchOutcome> => {
    try {
      await op();
      return { ok: true };
    } catch (error: unknown) {
      return { ok: false, error: String(error) };
    }
  });
  render(<WorktreeSelector active={active} onSwitch={onSwitch} />);
  return onSwitch;
}

async function openDialog(active: WorktreeEntry = ACTIVE) {
  const onSwitch = renderSelector(active);
  await userEvent.click(screen.getByTestId("worktree-selector"));
  await screen.findByTestId("worktree-group");
  await userEvent.click(screen.getByTestId("worktree-new"));
  await screen.findByLabelText("Branch");
  return onSwitch;
}

/** The three reads of the opening have been answered. */
async function readsAnswered() {
  await waitFor(() => expect(calls("get_default_branch")).toHaveLength(1));
  await act(async () => {});
}

const create = () => screen.getByRole("button", { name: "Create" });
const typeBranch = (value: string) => userEvent.type(screen.getByLabelText("Branch"), value);
const stackToast = () => visibleToasts().find((t) => t.key === "worktree-stack");
/** The log records sent so far, after a flush of the buffer. */
function loggedRecords() {
  flushLogs();
  return calls("append_log_records").flatMap(
    (c) => (c[1] as { records: { message: string; fields: Record<string, unknown> }[] }).records,
  );
}
const logged = (message: string) => loggedRecords().filter((r) => r.message === message);
/** Create has ended its wait for the stack read. */
const createSettled = () => waitFor(() => expect(create()).not.toHaveAttribute("aria-busy"));

describe("the reads of the New worktree dialog (WTS-FR-HUQW)", () => {
  it("WTS-FR-HUQW: on a named branch it reads the stack of that branch, the branches and the default branch", async () => {
    await openDialog();
    await readsAnswered();
    expect(calls("get_branch_stack")[0][1]).toEqual({ branch: ACTIVE.branch });
    expect(calls("list_branches")).toHaveLength(1);
  });

  it("WTS-FR-HUQW: a detached HEAD reads no stack, and Create is enabled as before", async () => {
    const detached = wt({ path: REPO, isDetached: true, isActive: true, isPrimary: true });
    await openDialog(detached);
    await typeBranch("next");
    await waitFor(() => expect(create()).toBeEnabled());
    expect(calls("get_branch_stack")).toHaveLength(0);
  });
});

describe("a new branch on a stack that cannot take it (WTS-FR-GDKX, WTS-FR-BMQI)", () => {
  it("WTS-FR-GDKX, WTS-FR-BMQI: a branch in a stack with no open pull request blocks Create, and says why", async () => {
    stack = async () => stackOn(ACTIVE.branch!, "main", { pullRequest: undefined });
    await openDialog();
    await readsAnswered();
    await typeBranch("next");
    const reason = `${ACTIVE.branch} is in a stack and has no open pull request. Create the worktree from a branch that is not in a stack, for example main.`;
    await waitFor(() => expect(screen.getByLabelText("Location")).not.toHaveValue(""));
    await waitFor(() => expect(create()).toBeDisabled());
    expect(screen.getByTestId("worktree-stack-reason")).toHaveTextContent(reason);
    expect(create()).toHaveAccessibleDescription(reason);
  });

  it("WTS-FR-GDKX, WTS-FR-BMQI: a layer with a pull request under a top with none blocks Create, and says why", async () => {
    stack = async () => ({ ...LOWER, top: "feature/draft", topHasPullRequest: false });
    await openDialog();
    await readsAnswered();
    await typeBranch("next");
    const reason =
      "The top of the stack, feature/draft, has no open pull request. Create the worktree from a branch that is not in a stack, for example main.";
    await waitFor(() => expect(screen.getByLabelText("Location")).not.toHaveValue(""));
    await waitFor(() => expect(create()).toBeDisabled());
    expect(screen.getByTestId("worktree-stack-reason")).toHaveTextContent(reason);
    expect(create()).toHaveAccessibleDescription(reason);
    expect(calls("create_worktree")).toHaveLength(0);
    expect(calls("add_branch_to_stack")).toHaveLength(0);
  });

  it("WTS-FR-GDKX: when the branch and the top both have no pull request, the reason is the one of the branch", async () => {
    stack = async () =>
      stackOn(ACTIVE.branch!, "main", { pullRequest: undefined, top: "feature/draft" });
    await openDialog();
    await readsAnswered();
    await typeBranch("next");
    await waitFor(() =>
      expect(screen.getByTestId("worktree-stack-reason")).toHaveTextContent(
        `${ACTIVE.branch} is in a stack and has no open pull request.`,
      ),
    );
    expect(create()).toBeDisabled();
  });

  it("WTS-FR-PZEO: a top with no pull request does not block an existing local branch", async () => {
    stack = async () => ({ ...LOWER, top: "feature/draft", topHasPullRequest: false });
    await openDialog();
    await readsAnswered();
    await typeBranch("main");
    await waitFor(() => expect(create()).toBeEnabled());
    expect(screen.queryByTestId("worktree-stack-reason")).toBeNull();
  });

  it("WTS-FR-BMQI: the reason is text, not an input, and the dialog keeps its two inputs", async () => {
    stack = async () => stackOn(ACTIVE.branch!, "main", { pullRequest: undefined });
    await openDialog();
    await readsAnswered();
    await typeBranch("next");
    await screen.findByTestId("worktree-stack-reason");
    const dialog = screen.getByRole("dialog");
    expect(dialog.querySelectorAll("input, select, textarea")).toHaveLength(2);
  });

  it("WTS-FR-PZEO: an existing local branch is not a new branch, so nothing blocks it", async () => {
    stack = async () => stackOn(ACTIVE.branch!, "main", { pullRequest: undefined });
    await openDialog();
    await readsAnswered();
    await typeBranch("main");
    await waitFor(() => expect(create()).toBeEnabled());
    expect(screen.queryByTestId("worktree-stack-reason")).toBeNull();
  });

  it("WTS-FR-BMQI: an unavailable stack adds no block", async () => {
    stack = async () => stackUnavailable(ACTIVE.branch!);
    await openDialog();
    await readsAnswered();
    await typeBranch("next");
    await waitFor(() => expect(create()).toBeEnabled());
  });

  it("WTS-FR-BMQI: a stack read that fails adds no block", async () => {
    stack = () => Promise.reject("github_unreachable");
    await openDialog();
    await readsAnswered();
    await typeBranch("next");
    await waitFor(() => expect(create()).toBeEnabled());
  });

  it("WTS-FR-BMQI: a stack read that still runs adds no block", async () => {
    stack = () => new Promise(() => {});
    await openDialog();
    await typeBranch("next");
    await waitFor(() => expect(create()).toBeEnabled());
  });

  it("WTS-FR-BMQI: a branch that is not in a stack adds no block", async () => {
    stack = async () => ({
      branch: ACTIVE.branch!,
      available: true,
      inStack: false,
      isTop: false,
      topHasPullRequest: false,
    });
    await openDialog();
    await readsAnswered();
    await typeBranch("next");
    await waitFor(() => expect(create()).toBeEnabled());
  });
});

describe("the new branch starts at the stack top and goes on top of it (WTS-FR-XQOB, WTS-FR-KVPA, WTS-FR-DXCE, NTF-FR-HCWM)", () => {
  it("WTS-FR-21, WTS-FR-XQOB, WTS-FR-KVPA, WTC-FR-SBNV: on the top, the new branch starts at it and is added on it", async () => {
    await openDialog();
    await readsAnswered();
    await typeBranch("next");
    await waitFor(() => expect(create()).toBeEnabled());
    await userEvent.click(create());
    await waitFor(() => expect(calls("add_branch_to_stack")).toHaveLength(1));
    expect(calls("create_worktree")[0][1]).toEqual({
      branch: "next",
      path: `${REPO}-next`,
      start: ACTIVE.branch,
    });
    expect(calls("add_branch_to_stack")[0][1]).toEqual({
      branch: "next",
      parent: ACTIVE.branch,
    });
    const order = invokeMock.mock.calls.map((c) => c[0]);
    expect(order.indexOf("create_worktree")).toBeLessThan(order.indexOf("add_branch_to_stack"));
    // WTS-FR-DXCE: a success is logged and shows no toast.
    await waitFor(() => expect(logged("new worktree branch added to stack")).toHaveLength(1));
    expect(logged("new worktree branch added to stack")[0].fields).toMatchObject({
      branch: "next",
      parent: ACTIVE.branch,
    });
    expect(stackToast()).toBeUndefined();
  });

  it("WTS-FR-KVPA: the branch added is the branch of the new active worktree, not the typed text", async () => {
    await openDialog();
    await readsAnswered();
    await typeBranch("Next Thing");
    await waitFor(() => expect(create()).toBeEnabled());
    const fallback = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown>) =>
      cmd === "create_worktree" ? createdContext("next-thing") : fallback(cmd, args),
    );
    await userEvent.click(create());
    await waitFor(() => expect(calls("add_branch_to_stack")).toHaveLength(1));
    expect(calls("add_branch_to_stack")[0][1]).toEqual({
      branch: "next-thing",
      parent: ACTIVE.branch,
    });
  });

  it("WTS-FR-XQOB: while Create waits, a second click starts nothing and the fields keep their values", async () => {
    let answer: (stack: BranchStack) => void = () => {};
    stack = () => new Promise((resolve) => (answer = resolve));
    await openDialog();
    await typeBranch("next");
    await waitFor(() => expect(create()).toBeEnabled());
    await userEvent.click(create());
    await waitFor(() => expect(create()).toHaveAttribute("aria-busy", "true"));
    await userEvent.click(create());
    await userEvent.type(screen.getByLabelText("Branch"), "x");
    expect(screen.getByLabelText("Branch")).toHaveValue("next");
    await act(async () => answer(TOP));
    await waitFor(() => expect(calls("add_branch_to_stack")).toHaveLength(1));
    expect(calls("create_worktree")).toHaveLength(1);
    expect(calls("create_worktree")[0][1]).toMatchObject({ branch: "next" });
  });

  it("WTS-FR-XQOB: a dialog closed while Create waits creates nothing", async () => {
    let answer: (stack: BranchStack) => void = () => {};
    stack = () => new Promise((resolve) => (answer = resolve));
    await openDialog();
    await typeBranch("next");
    await waitFor(() => expect(create()).toBeEnabled());
    await userEvent.click(create());
    await waitFor(() => expect(create()).toHaveAttribute("aria-busy", "true"));
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    await act(async () => answer(TOP));
    await waitFor(() =>
      expect(logged("new worktree creation not started")[0]?.fields).toEqual({
        reason: "dialog closed",
      }),
    );
    expect(calls("create_worktree")).toHaveLength(0);
  });

  it("WTS-FR-XQOB: a stack answer with no top gives no start and adds nothing", async () => {
    stack = async () => ({ ...TOP, isTop: false, top: undefined });
    await openDialog();
    await readsAnswered();
    await typeBranch("next");
    await waitFor(() => expect(create()).toBeEnabled());
    await userEvent.click(create());
    await waitFor(() => expect(calls("create_worktree")).toHaveLength(1));
    expect(calls("create_worktree")[0][1]).toEqual({ branch: "next", path: `${REPO}-next` });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(calls("add_branch_to_stack")).toHaveLength(0);
  });

  it("WTS-FR-XQOB, WTS-FR-KVPA, WTC-FR-NQZE: on a lower layer, Create is available, the branch starts at the top and stands on it", async () => {
    stack = async () => LOWER;
    await openDialog();
    await readsAnswered();
    await typeBranch("next");
    await waitFor(() => expect(create()).toBeEnabled());
    expect(screen.queryByTestId("worktree-stack-reason")).toBeNull();
    await userEvent.click(create());
    await waitFor(() => expect(calls("add_branch_to_stack")).toHaveLength(1));
    expect(calls("create_worktree")[0][1]).toMatchObject({ branch: "next", start: "feature/docs" });
    expect(calls("add_branch_to_stack")[0][1]).toEqual({
      branch: "next",
      parent: "feature/docs",
    });
  });

  it("WTS-FR-XQOB: Create waits for a stack answer that arrives after the click, and is busy while it waits", async () => {
    let answer: (stack: BranchStack) => void = () => {};
    stack = () => new Promise((resolve) => (answer = resolve));
    await openDialog();
    await typeBranch("next");
    await waitFor(() => expect(create()).toBeEnabled());
    await userEvent.click(create());
    await waitFor(() => expect(create()).toHaveAttribute("aria-busy", "true"));
    expect(create()).toBeDisabled();
    expect(calls("create_worktree")).toHaveLength(0);
    await act(async () => answer(LOWER));
    await waitFor(() => expect(calls("add_branch_to_stack")).toHaveLength(1));
    expect(calls("create_worktree")).toHaveLength(1);
    expect(calls("create_worktree")[0][1]).toMatchObject({ start: "feature/docs" });
    expect(calls("add_branch_to_stack")[0][1]).toMatchObject({ parent: "feature/docs" });
  });

  it("WTS-FR-GDKX: an answer after the click that blocks creates nothing and shows the reason", async () => {
    let answer: (stack: BranchStack) => void = () => {};
    stack = () => new Promise((resolve) => (answer = resolve));
    await openDialog();
    await typeBranch("next");
    await waitFor(() => expect(create()).toBeEnabled());
    await userEvent.click(create());
    await waitFor(() => expect(create()).toHaveAttribute("aria-busy", "true"));
    await act(async () => answer(stackOn(ACTIVE.branch!, "main", { pullRequest: undefined })));
    await screen.findByTestId("worktree-stack-reason");
    await createSettled();
    await waitFor(() =>
      expect(logged("new worktree creation not started")[0]?.fields).toEqual({
        reason: "stack block",
      }),
    );
    expect(create()).toBeDisabled();
    expect(calls("create_worktree")).toHaveLength(0);
    expect(calls("add_branch_to_stack")).toHaveLength(0);
  });

  it("WTS-FR-KVPA, WTS-FR-PZEO: an existing branch gets no start and is not added to the stack", async () => {
    await openDialog();
    await readsAnswered();
    await typeBranch("feature/existing");
    await waitFor(() => expect(create()).toBeEnabled());
    await userEvent.click(create());
    await waitFor(() => expect(calls("create_worktree")).toHaveLength(1));
    expect(calls("create_worktree")[0][1]).toEqual({
      branch: "feature/existing",
      path: `${REPO}-feature/existing`,
    });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(calls("add_branch_to_stack")).toHaveLength(0);
  });

  it("WTS-FR-XQOB, WTS-FR-KVPA: an unavailable stack gives no start and adds nothing", async () => {
    stack = async () => stackUnavailable(ACTIVE.branch!);
    await openDialog();
    await readsAnswered();
    await typeBranch("next");
    await waitFor(() => expect(create()).toBeEnabled());
    await userEvent.click(create());
    await waitFor(() => expect(calls("create_worktree")).toHaveLength(1));
    expect(calls("create_worktree")[0][1]).toEqual({ branch: "next", path: `${REPO}-next` });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(calls("add_branch_to_stack")).toHaveLength(0);
  });

  it("WTS-FR-XQOB, WTS-FR-KVPA: a stack read that failed gives no start and adds nothing", async () => {
    stack = () => Promise.reject("github_unreachable");
    await openDialog();
    await readsAnswered();
    await typeBranch("next");
    await waitFor(() => expect(create()).toBeEnabled());
    await userEvent.click(create());
    await waitFor(() => expect(calls("create_worktree")).toHaveLength(1));
    expect(calls("create_worktree")[0][1]).toEqual({ branch: "next", path: `${REPO}-next` });
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(calls("add_branch_to_stack")).toHaveLength(0);
  });

  it("WTS-FR-DXCE: a failed creation runs no stack step", async () => {
    const fallback = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown>) =>
      cmd === "create_worktree" ? Promise.reject("branch_missing") : fallback(cmd, args),
    );
    await openDialog();
    await readsAnswered();
    await typeBranch("next");
    await waitFor(() => expect(create()).toBeEnabled());
    await userEvent.click(create());
    await waitFor(() =>
      expect(screen.getByTestId("worktree-dialog-error")).toHaveTextContent("branch_missing"),
    );
    expect(calls("add_branch_to_stack")).toHaveLength(0);
    expect(stackToast()).toBeUndefined();
  });

  it("WTS-FR-DXCE, NTF-FR-HCWM: a failed step keeps the worktree and shows an Error toast with the cause in words", async () => {
    stack = async () => LOWER;
    addToStack = () => Promise.reject("stack_parent_has_no_pull_request");
    const onSwitch = await openDialog();
    await readsAnswered();
    await typeBranch("next");
    await waitFor(() => expect(create()).toBeEnabled());
    await userEvent.click(create());
    await waitFor(() => expect(stackToast()).toBeDefined());
    expect(stackToast()).toMatchObject({
      level: "Error",
      title: "Branch not added to the stack",
      body: "next is not added to the stack on feature/docs, because the parent branch has no open pull request. The worktree stays created.",
      address: null,
    });
    // WTS-FR-DXCE: the failure is logged with its typed cause.
    expect(logged("new worktree branch not added to stack")[0]?.fields).toMatchObject({
      branch: "next",
      parent: "feature/docs",
      error: "stack_parent_has_no_pull_request",
    });
    expect(onSwitch).toHaveBeenCalledTimes(1);
    expect(screen.queryByRole("dialog")).toBeNull();
  });
});

describe("the name of a new branch (WTS-FR-PZEO)", () => {
  it("WTS-FR-PZEO: a branch is new when no local branch has its value or its normalized value without regard to case", () => {
    const locals = ["main", "Feature/Existing"];
    expect(namesNewBranch("next", locals)).toBe(true);
    expect(namesNewBranch("main", locals)).toBe(false);
    expect(namesNewBranch(" main ", locals)).toBe(false);
    expect(namesNewBranch("feature/existing", locals)).toBe(false);
    expect(namesNewBranch("Feature/Existing", locals)).toBe(false);
    expect(namesNewBranch("next", null)).toBe(false);
    expect(namesNewBranch("   ", locals)).toBe(false);
  });

  it("WTS-FR-PZEO: the final name follows the rule of WTC-FR-XLXY", () => {
    expect(finalBranchName("More fixes-")).toBe("more-fixes");
    expect(finalBranchName("/team//Big  Spike/")).toBe("team/big-spike");
    expect(finalBranchName("--")).toBe("");
  });
});
