import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Git } from "./Git";
import type { GitBranch } from "../types";

// GIT-FR-FZMS: the Git panel selects the branch a status bar row named, opened
// from a `git_push` activation (STB-FR-RWPD).

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
}));

let branches: GitBranch[];
let resolveBranches: ((value: GitBranch[]) => void) | null;

beforeEach(() => {
  resolveBranches = null;
  branches = [
    { name: "develop", kind: "local", isCurrent: false },
    { name: "main", kind: "local", isCurrent: true },
    { name: "feature/x", kind: "local", isCurrent: false },
    { name: "feature/x", kind: "remote", isCurrent: false },
    { name: "origin/only-remote", kind: "remote", isCurrent: false },
  ];
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "list_branches") return branches;
    if (cmd === "get_upstream_sync_state")
      return { hasRemote: true, hasUpstream: true, ahead: 1, behind: 0 };
    return undefined;
  });
});

afterEach(cleanup);

const onSwitchWorktree = vi.fn(async () => ({ ok: true as const }));

function panel(request: { branch: string; nonce: number } | null, taken = vi.fn()) {
  const ui = (r: typeof request) => (
    <Git
      onSwitchWorktree={onSwitchWorktree}
      canCheckOutBranches
      selectBranch={r}
      onBranchSelected={taken}
    />
  );
  const view = render(ui(request));
  return { taken, rerender: (r: typeof request) => view.rerender(ui(r)) };
}

/** Open the branches section, where the request's selection shows. */
const openBranchesTab = async () => {
  await userEvent.click(screen.getByRole("tab", { name: "Branches" }));
  return screen.findByTestId("git-branches");
};

describe("a branch selection request (GIT-FR-FZMS)", () => {
  it("GIT-FR-FZMS: shows the Logs section with the push/pull output area", async () => {
    const { taken } = panel({ branch: "feature/x", nonce: 1 });
    expect(await screen.findByTestId("git-transfer-output")).toBeInTheDocument();
    expect(screen.getByRole("tab", { name: "Logs" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.queryByTestId("git-branches")).not.toBeInTheDocument();
    expect(taken).toHaveBeenCalledTimes(1);
  });

  it("GIT-FR-FZMS, GIT-FR-LNEI: marks the named local branch as selected in the branches section", async () => {
    panel({ branch: "feature/x", nonce: 1 });
    await screen.findByTestId("git-transfer-output");
    const section = await openBranchesTab();
    const selected = await within(section).findByTestId("git-branch-selected");
    expect(selected).toHaveTextContent("feature/x");
    expect(selected).toHaveAttribute("aria-current", "true");
    // A visible form that does not depend on colour alone.
    expect(within(selected).getByText("selected")).toBeInTheDocument();
    // The local row is the one chosen where a remote-tracking row shares the name.
    expect(within(section).getAllByTestId("git-branch-selected")).toHaveLength(1);
    // GIT-FR-HLGO: the switch shows the kind of the row the request selected.
    expect(within(section).getByRole("radio", { name: "Local" })).toBeChecked();
  });

  it("GIT-FR-FZMS: names the remote-tracking row when no local branch has the name", async () => {
    panel({ branch: "origin/only-remote", nonce: 1 });
    await openBranchesTab();
    const selected = await screen.findByTestId("git-branch-selected");
    expect(selected).toHaveTextContent("origin/only-remote");
  });

  it("GIT-FR-HLGO, GIT-FR-FZMS: a request sets the switch to the kind of the row it selects", async () => {
    const { rerender } = panel({ branch: "origin/only-remote", nonce: 1 });
    let section = await openBranchesTab();
    await within(section).findByTestId("git-branch-selected");
    expect(within(section).getByRole("radio", { name: "Remote" })).toBeChecked();
    expect(within(section).getByRole("radio", { name: "Local" })).not.toBeChecked();

    // A later request for a local branch switches back.
    rerender({ branch: "develop", nonce: 2 });
    await screen.findByTestId("git-transfer-output");
    section = await openBranchesTab();
    await waitFor(() =>
      expect(within(section).getByRole("radio", { name: "Local" })).toBeChecked(),
    );
    expect(await within(section).findByTestId("git-branch-selected")).toHaveTextContent(
      "develop",
    );
  });

  it("GIT-FR-HLGO, GIT-FR-FZMS: the author's own switch choice is not undone by the request that came before it", async () => {
    panel({ branch: "develop", nonce: 1 });
    const section = await openBranchesTab();
    await within(section).findByTestId("git-branch-selected");
    await userEvent.click(within(section).getByRole("radio", { name: "Remote" }));
    expect(within(section).getByRole("radio", { name: "Remote" })).toBeChecked();
    expect(within(section).queryByTestId("git-branch-selected")).toBeNull();
  });

  it("GIT-FR-FZMS: selecting checks nothing out and starts nothing", async () => {
    panel({ branch: "feature/x", nonce: 1 });
    await openBranchesTab();
    await screen.findByTestId("git-branch-selected");
    expect(onSwitchWorktree).not.toHaveBeenCalled();
    const commands = invokeMock.mock.calls.map((call) => call[0]);
    expect(commands).not.toContain("push_current_branch");
    expect(commands).not.toContain("check_out_branch_in_active_worktree");
  });

  it("GIT-FR-FZMS: a request that arrives while the listing loads is applied when it arrives", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_branches")
        return new Promise<GitBranch[]>((resolve) => {
          resolveBranches = resolve;
        });
      return undefined;
    });
    panel({ branch: "develop", nonce: 1 });
    await openBranchesTab();
    expect(await screen.findByText("LOADING…")).toBeInTheDocument();
    expect(screen.queryByTestId("git-branch-selected")).not.toBeInTheDocument();
    resolveBranches!(branches);
    expect(await screen.findByTestId("git-branch-selected")).toHaveTextContent("develop");
  });

  it("GIT-FR-FZMS: a branch the listing does not hold leaves no row selected", async () => {
    panel({ branch: "deleted-meanwhile", nonce: 1 });
    const section = await openBranchesTab();
    await within(section).findByText("develop");
    expect(screen.queryByTestId("git-branch-selected")).not.toBeInTheDocument();
    expect(screen.getByTestId("git-compare-no-branch")).toBeInTheDocument();
  });

  it("GIT-FR-FZMS: a later request replaces the selection, including from another section", async () => {
    const { rerender } = panel({ branch: "develop", nonce: 1 });
    await openBranchesTab();
    expect(await screen.findByTestId("git-branch-selected")).toHaveTextContent("develop");
    await userEvent.click(screen.getByRole("tab", { name: "Commits" }));
    expect(screen.queryByTestId("git-branches")).not.toBeInTheDocument();

    rerender({ branch: "main", nonce: 2 });
    await screen.findByTestId("git-transfer-output");
    await openBranchesTab();
    await waitFor(() =>
      expect(screen.getByTestId("git-branch-selected")).toHaveTextContent("main"),
    );
  });

  it("GIT-FR-FZMS: the selected row is scrolled into view, and a repeated request scrolls it again", async () => {
    const scrolled = vi.fn();
    Object.defineProperty(Element.prototype, "scrollIntoView", {
      configurable: true,
      value: scrolled,
    });
    try {
      const { rerender } = panel({ branch: "develop", nonce: 1 });
      await openBranchesTab();
      await screen.findByTestId("git-branch-selected");
      await waitFor(() => expect(scrolled).toHaveBeenCalled());
      const before = scrolled.mock.calls.length;

      // Same branch, new request: the author may have scrolled away since.
      rerender({ branch: "develop", nonce: 2 });
      await screen.findByTestId("git-transfer-output");
      await openBranchesTab();
      await waitFor(() => expect(scrolled.mock.calls.length).toBeGreaterThan(before));
    } finally {
      delete (Element.prototype as { scrollIntoView?: unknown }).scrollIntoView;
    }
  });

  it("GIT-FR-FZMS: with no request the panel opens on its own section and selects nothing", async () => {
    panel(null);
    expect(screen.queryByTestId("git-branches")).not.toBeInTheDocument();
    expect(screen.queryByTestId("git-transfer-output")).not.toBeInTheDocument();
    await openBranchesTab();
    expect(screen.queryByTestId("git-branch-selected")).not.toBeInTheDocument();
  });
});
