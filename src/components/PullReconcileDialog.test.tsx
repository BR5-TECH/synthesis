import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { PullReconcileDialog } from "./PullReconcileDialog";
import { resetLogBufferForTest } from "../logging";
import type { PullOutcome, PullReconcileOutcome } from "../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
type EventHandler = (ev: { payload: unknown }) => void;
let handlers: [string, EventHandler][] = [];
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: EventHandler) => {
    handlers.push([name, handler]);
    return Promise.resolve(() => {
      handlers = handlers.filter(([, h]) => h !== handler);
    });
  },
}));
const fire = (name: string, payload: unknown) =>
  handlers.filter(([n]) => n === name).forEach(([, h]) => h({ payload }));

const LOCAL = "1".repeat(40);
const REMOTE = "2".repeat(40);

const diverged: PullOutcome = {
  state: "diverged",
  branch: "main",
  upstream: "origin/main",
  localTip: LOCAL,
  remoteTip: REMOTE,
  ahead: 2,
  behind: 1,
  updatedPaths: [],
  refresh: {
    context: {
      repositoryRoot: "/repo",
      activeWorktreePath: "/repo",
      worktrees: [],
      branches: [],
    },
    remoteState: "refreshed",
  },
};

/** What `reconcile_pull` answers, or a promise a test settles by hand. */
let reconcile: () => Promise<PullReconcileOutcome>;

beforeEach(() => {
  resetLogBufferForTest();
  handlers = [];
  reconcile = async () => ({ state: "merged", commit: "c".repeat(40), updatedPaths: [] });
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "reconcile_pull") return reconcile();
    if (cmd === "get_upstream_sync_state")
      return Promise.resolve({ hasRemote: true, hasUpstream: true, ahead: 2, behind: 1 });
    if (cmd === "append_log_records") return Promise.resolve();
    return Promise.reject(new Error(`unexpected command ${cmd}`));
  });
});

afterEach(cleanup);

function renderDialog() {
  const onClose = vi.fn();
  const onOpenRun = vi.fn();
  render(<PullReconcileDialog outcome={diverged} onClose={onClose} onOpenRun={onOpenRun} />);
  return { onClose, onOpenRun };
}

const reconcileCalls = () => invokeMock.mock.calls.filter((c) => c[0] === "reconcile_pull");

describe("the merge-or-rebase dialog (GIT-FR-MRBQ)", () => {
  it("GIT-FR-MRBQ: names the branch, its upstream, both counts and both short tips", () => {
    renderDialog();
    expect(screen.getByRole("dialog")).toHaveAccessibleName("main and origin/main have diverged");
    const sides = screen.getByTestId("pull-reconcile-sides");
    expect(sides).toHaveTextContent(
      "main at 1111111 has 2 commits that origin/main does not. origin/main at 2222222 has 1 commit that main does not.",
    );
    expect(screen.getByRole("button", { name: "Merge" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Rebase" })).toBeInTheDocument();
    // A reflexive Enter changes no history.
    expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus();
  });

  it("GIT-FR-MRBQ: Cancel, Escape and a press outside invoke nothing", async () => {
    const { onClose } = renderDialog();
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await userEvent.keyboard("{Escape}");
    await userEvent.pointer({ keys: "[MouseLeft>]", target: document.querySelector(".scrim")! });
    expect(onClose).toHaveBeenCalledTimes(3);
    expect(reconcileCalls()).toHaveLength(0);
  });
});

describe("the reconcile the dialog runs (GIT-FR-RCDL)", () => {
  it("GIT-FR-RCDL: Merge invokes the reconcile at the tips shown, and states the new tip", async () => {
    renderDialog();
    await userEvent.click(screen.getByTestId("pull-reconcile-merge"));
    await waitFor(() =>
      expect(screen.getByTestId("pull-reconcile-result")).toHaveTextContent(
        "Merged. The branch is at ccccccc.",
      ),
    );
    expect(reconcileCalls()).toEqual([
      ["reconcile_pull", { strategy: "merge", localTip: LOCAL, remoteTip: REMOTE }],
    ]);
    expect(screen.getByTestId("pull-reconcile-close")).toBeInTheDocument();
    expect(screen.queryByTestId("pull-reconcile-merge")).toBeNull();
  });

  it("GIT-FR-RCDL: Rebase invokes the rebase strategy", async () => {
    reconcile = async () => ({ state: "rebased", commit: "d".repeat(40), updatedPaths: [] });
    renderDialog();
    await userEvent.click(screen.getByTestId("pull-reconcile-rebase"));
    await waitFor(() =>
      expect(screen.getByTestId("pull-reconcile-result")).toHaveTextContent("Rebased."),
    );
    expect(reconcileCalls()[0][1]).toMatchObject({ strategy: "rebase" });
  });

  it("GIT-FR-RCDL: a run that took the integration is named, with a route to it", async () => {
    reconcile = async () => ({
      state: "handed_to_run",
      runId: "run-7",
      runTitle: "Pull origin/main into main",
      updatedPaths: [],
    });
    const { onClose, onOpenRun } = renderDialog();
    await userEvent.click(screen.getByTestId("pull-reconcile-merge"));
    await waitFor(() =>
      expect(screen.getByTestId("pull-reconcile-result")).toHaveTextContent(
        "Pull origin/main into main",
      ),
    );
    await userEvent.click(screen.getByTestId("pull-reconcile-open-run"));
    expect(onClose).toHaveBeenCalledTimes(1);
    expect(onOpenRun).toHaveBeenCalledWith("run-7");
  });

  it("GIT-FR-RCDL: moved tips say to pull again, and a refusal reads in words", async () => {
    reconcile = async () => ({ state: "refused", error: "pull_tips_moved", updatedPaths: [] });
    renderDialog();
    await userEvent.click(screen.getByTestId("pull-reconcile-merge"));
    const result = await screen.findByTestId("pull-reconcile-result");
    expect(result).toHaveTextContent(/moved since the pull\. Pull again\./);
    expect(result).toHaveAttribute("role", "alert");
    expect(result).not.toHaveTextContent("pull_tips_moved");
  });

  it("GIT-FR-RCDL, GIT-FR-QMYB: runs once, and holds the dialog open while it runs", async () => {
    let settle!: (value: PullReconcileOutcome) => void;
    reconcile = () =>
      new Promise((resolve) => {
        settle = resolve;
      });
    const { onClose } = renderDialog();
    await userEvent.click(screen.getByTestId("pull-reconcile-merge"));
    await screen.findByTestId("pull-reconcile-status");
    expect(screen.getByTestId("pull-reconcile-merge")).toBeDisabled();
    expect(screen.getByTestId("pull-reconcile-rebase")).toBeDisabled();
    await userEvent.keyboard("{Escape}");
    expect(onClose).not.toHaveBeenCalled();
    expect(reconcileCalls()).toHaveLength(1);

    await act(async () => {
      settle({ state: "merged", commit: "e".repeat(40), updatedPaths: [] });
    });
    await screen.findByTestId("pull-reconcile-result");
  });
});

describe("the dialog and the shared transfer (GIT-FR-QMYB, GIT-FR-RCDL)", () => {
  it("GIT-FR-QMYB: offers no strategy while another transfer runs", async () => {
    renderDialog();
    await act(async () => {
      fire("git-output-line", { operation: "push", line: "Pushing main to origin" });
    });
    await waitFor(() => expect(screen.getByTestId("pull-reconcile-merge")).toBeDisabled());
    expect(screen.getByTestId("pull-reconcile-rebase")).toBeDisabled();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeEnabled();
    await act(async () => {
      fire("git-operation-finished", { operation: "push", ok: true });
    });
    await waitFor(() => expect(screen.getByTestId("pull-reconcile-merge")).toBeEnabled());
    expect(reconcileCalls()).toHaveLength(0);
  });

  it("GIT-FR-RCDL: a rejected reconcile reads in words and moves focus to Close", async () => {
    reconcile = () => Promise.reject("not a git repository");
    renderDialog();
    await userEvent.click(screen.getByTestId("pull-reconcile-merge"));
    const result = await screen.findByTestId("pull-reconcile-result");
    expect(result).toHaveTextContent("The pull did not run: not a git repository");
    await waitFor(() => expect(screen.getByTestId("pull-reconcile-close")).toHaveFocus());
  });

  it("GIT-FR-RCDL: a second activation while a reconcile runs invokes nothing", async () => {
    let settle!: (value: PullReconcileOutcome) => void;
    reconcile = () =>
      new Promise((resolve) => {
        settle = resolve;
      });
    renderDialog();
    const merge = screen.getByTestId("pull-reconcile-merge");
    await userEvent.click(merge);
    await screen.findByTestId("pull-reconcile-status");
    await userEvent.click(merge);
    await userEvent.click(screen.getByTestId("pull-reconcile-rebase"));
    expect(reconcileCalls()).toHaveLength(1);
    await act(async () => {
      settle({ state: "merged", commit: "e".repeat(40), updatedPaths: [] });
    });
    await waitFor(() => expect(screen.getByTestId("pull-reconcile-close")).toHaveFocus());
  });
});
