import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Git } from "./Git";
import { formatCommitInstant } from "./Git/format";
import type { BranchInformation, GitBranch } from "../types";
import { branchInformation, commit, deferred } from "../test/gitPanelFixtures";

// The Branches rail: the Local / Remote switch, the row context menu and the
// Information overlay (GIT-FR-HLGO, GIT-FR-ZEKI, GIT-FR-NUCX, GIT-FR-VCDG).

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
}));

let branches: GitBranch[];
let listError: string | null;
let informationRead: (name: string, kind: string) => Promise<BranchInformation>;
const onSwitchWorktree = vi.fn(async () => ({ ok: true as const }));

beforeEach(() => {
  listError = null;
  branches = [
    { name: "develop", kind: "local", isCurrent: false },
    { name: "main", kind: "local", isCurrent: true },
    { name: "feature/x", kind: "local", isCurrent: false },
    { name: "origin/main", kind: "remote", isCurrent: false },
    { name: "origin/feature/x", kind: "remote", isCurrent: false },
  ];
  informationRead = async (name, kind) =>
    branchInformation({ name, kind: kind as "local" | "remote" });
  onSwitchWorktree.mockClear();
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, string>) => {
    if (cmd === "list_branches") {
      if (listError) throw listError;
      return branches;
    }
    if (cmd === "get_branch_information") return informationRead(args.name, args.kind);
    if (cmd === "get_upstream_sync_state")
      return { hasRemote: true, hasUpstream: true, ahead: 0, behind: 0 };
    return undefined;
  });
});

afterEach(cleanup);

const openBranches = async () => {
  render(<Git onSwitchWorktree={onSwitchWorktree} canCheckOutBranches />);
  await userEvent.click(screen.getByText("Branches"));
  const section = await screen.findByTestId("git-branches");
  await within(section).findByText("develop");
  return section;
};

const row = (name: string) => screen.getByText(name).closest<HTMLElement>('[role="button"]')!;
const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

describe("the Local / Remote switch (GIT-FR-HLGO)", () => {
  it("GIT-FR-HLGO: opens on Local and lists only the local branches, with the current one marked", async () => {
    const section = await openBranches();
    expect(within(section).getByRole("radiogroup", { name: "Branch kind" })).toBeInTheDocument();
    expect(within(section).getByRole("radio", { name: "Local" })).toBeChecked();
    expect(within(section).getByRole("radio", { name: "Remote" })).not.toBeChecked();
    for (const name of ["develop", "main", "feature/x"])
      expect(within(section).getByText(name)).toBeInTheDocument();
    expect(within(section).queryByText("origin/main")).toBeNull();
    expect(within(section).getAllByText("current")).toHaveLength(1);
    expect(row("main")).toHaveAttribute("title", "main (current)");
  });

  it("GIT-FR-HLGO: Remote lists only the remote-tracking branches, and the arrow keys move the switch", async () => {
    const section = await openBranches();
    await userEvent.click(within(section).getByRole("radio", { name: "Remote" }));
    expect(within(section).getByText("origin/main")).toBeInTheDocument();
    expect(within(section).getByText("origin/feature/x")).toBeInTheDocument();
    expect(within(section).queryByText("develop")).toBeNull();

    within(section).getByRole("radio", { name: "Remote" }).focus();
    await userEvent.keyboard("{ArrowLeft}");
    expect(within(section).getByRole("radio", { name: "Local" })).toBeChecked();
    expect(within(section).getByText("develop")).toBeInTheDocument();
  });

  it("GIT-FR-HLGO: an empty kind and a failed read show in words", async () => {
    branches = [{ name: "main", kind: "local", isCurrent: true }];
    render(<Git onSwitchWorktree={onSwitchWorktree} canCheckOutBranches />);
    await userEvent.click(screen.getByText("Branches"));
    await userEvent.click(await screen.findByRole("radio", { name: "Remote" }));
    expect(await screen.findByTestId("git-branches-empty")).toHaveTextContent(
      "No remote branches.",
    );
    cleanup();

    listError = "not a git repository";
    render(<Git onSwitchWorktree={onSwitchWorktree} canCheckOutBranches />);
    await userEvent.click(screen.getByText("Branches"));
    expect(await screen.findByTestId("git-branch-error")).toHaveTextContent(
      "This project is not inside a Git repository.",
    );
  });

  it("GIT-FR-LNEI: a click on a row selects it and checks nothing out", async () => {
    const section = await openBranches();
    await userEvent.click(within(section).getByText("develop"));
    expect(await within(section).findByTestId("git-branch-selected")).toHaveTextContent(
      "develop",
    );
    expect(onSwitchWorktree).not.toHaveBeenCalled();
  });
});

describe("the branch context menu (GIT-FR-ZEKI)", () => {
  it("GIT-FR-ZEKI: a right-click opens a menu with Information, Check out and Delete on a local row, and the menu takes focus", async () => {
    await openBranches();
    await userEvent.pointer({ keys: "[MouseRight]", target: row("develop") });

    const menu = await screen.findByRole("menu", { name: "Actions for develop" });
    const items = within(menu).getAllByRole("menuitem");
    expect(items.map((i) => i.textContent)).toEqual(["Information", "Check out", "Delete"]);
    // LCM-FR-07: every entry carries a leading icon.
    for (const item of items) expect(item.querySelector("svg")).not.toBeNull();
    expect(items[0]).toHaveFocus();
    // Opening the menu checks nothing out.
    expect(onSwitchWorktree).not.toHaveBeenCalled();
  });

  it("GIT-FR-ZEKI: the context-menu key opens the same menu", async () => {
    await openBranches();
    row("feature/x").focus();
    await userEvent.keyboard("{ContextMenu}");
    const menu = await screen.findByRole("menu");
    expect(within(menu).getAllByRole("menuitem")).toHaveLength(3);
    expect(onSwitchWorktree).not.toHaveBeenCalled();
  });

  it("GIT-FR-ZEKI: Shift+F10 opens the same menu", async () => {
    await openBranches();
    row("feature/x").focus();
    await userEvent.keyboard("{Shift>}{F10}{/Shift}");
    const menu = await screen.findByRole("menu");
    expect(within(menu).getAllByRole("menuitem")).toHaveLength(3);
    expect(onSwitchWorktree).not.toHaveBeenCalled();
  });

  it("GIT-FR-ZEKI: a remote row offers Information and Check out, and no Delete", async () => {
    const section = await openBranches();
    await userEvent.click(within(section).getByRole("radio", { name: "Remote" }));
    fireEvent.contextMenu(row("origin/main"));
    const menu = await screen.findByRole("menu");
    expect(within(menu).getAllByRole("menuitem").map((i) => i.textContent)).toEqual([
      "Information",
      "Check out",
    ]);
  });

  it("GIT-FR-ZEKI, GIT-FR-04: Check out in the menu checks the branch out", async () => {
    await openBranches();
    fireEvent.contextMenu(row("develop"));
    await userEvent.click(await screen.findByRole("menuitem", { name: "Check out" }));
    expect(onSwitchWorktree).toHaveBeenCalledOnce();
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("GIT-FR-ZEKI: the arrow keys move between entries and wrap", async () => {
    await openBranches();
    fireEvent.contextMenu(row("develop"));
    const menu = await screen.findByRole("menu");
    const [info, checkout, del] = within(menu).getAllByRole("menuitem");
    expect(info).toHaveFocus();
    await userEvent.keyboard("{ArrowDown}");
    expect(checkout).toHaveFocus();
    await userEvent.keyboard("{ArrowDown}");
    expect(del).toHaveFocus();
    await userEvent.keyboard("{ArrowDown}");
    expect(info).toHaveFocus();
    await userEvent.keyboard("{ArrowUp}");
    expect(del).toHaveFocus();
    await userEvent.keyboard("{Home}");
    expect(info).toHaveFocus();
    await userEvent.keyboard("{End}");
    expect(del).toHaveFocus();
  });

  it("GIT-FR-ZEKI: Escape closes the menu and returns focus to the row", async () => {
    await openBranches();
    row("develop").focus();
    await userEvent.keyboard("{Shift>}{F10}{/Shift}");
    await screen.findByRole("menu");
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBeNull();
    expect(row("develop")).toHaveFocus();
    expect(calls("get_branch_information")).toHaveLength(0);
    expect(calls("inspect_branch_deletion")).toHaveLength(0);
  });

  it("GIT-FR-ZEKI: a press outside the menu closes it and runs nothing", async () => {
    await openBranches();
    fireEvent.contextMenu(row("develop"));
    await screen.findByRole("menu");
    await userEvent.click(screen.getByRole("tab", { name: "Commits" }));
    expect(screen.queryByRole("menu")).toBeNull();
    expect(calls("get_branch_information")).toHaveLength(0);
  });
});

describe("the Information overlay (GIT-FR-NUCX, GIT-FR-VCDG)", () => {
  const openInformation = async (name: string) => {
    fireEvent.contextMenu(row(name));
    await userEvent.click(await screen.findByRole("menuitem", { name: "Information" }));
    return screen.findByRole("dialog");
  };

  it("GIT-FR-NUCX: reads the branch information and shows name, type, upstream, worktree, work stream, tip and commits newest first", async () => {
    informationRead = async (name, kind) =>
      branchInformation({
        name,
        kind: kind as "local",
        stream: { streamId: "s1", streamName: "Stream One" },
        commits: [commit(1), commit(2), commit(3)],
      });
    await openBranches();
    const dialog = await openInformation("feature/x");

    expect(calls("get_branch_information")[0][1]).toEqual({
      name: "feature/x",
      kind: "local",
    });
    expect(await within(dialog).findByTestId("git-info-name")).toHaveTextContent("feature/x");
    expect(within(dialog).getByText("Local branch")).toBeInTheDocument();
    expect(within(dialog).getByText("origin/feature/x")).toBeInTheDocument();
    expect(within(dialog).getByText(/Worktree feature-x at \/work\/feature-x/)).toBeInTheDocument();
    expect(within(dialog).getByText(/Belongs to work stream Stream One/)).toBeInTheDocument();
    expect(within(dialog).getByTestId("git-info-tip")).toHaveTextContent(
      `${commit(1).shortId} Subject of commit 1`,
    );

    const commits = within(dialog).getAllByTestId("git-info-commit");
    expect(commits).toHaveLength(3);
    expect(commits[0]).toHaveTextContent(/Author 1 <author1@example.com>/);
    expect(commits[0]).toHaveTextContent(formatCommitInstant(commit(1).authoredAt));
    expect(commits[0]).toHaveTextContent("Body of commit 1.");
    expect(commits[2]).toHaveTextContent("Author 3");
  });

  it("GIT-FR-NUCX: a remote branch is described as a remote branch", async () => {
    informationRead = async (name, kind) =>
      branchInformation({ name, kind: kind as "remote", upstream: undefined, worktree: undefined });
    const section = await openBranches();
    await userEvent.click(within(section).getByRole("radio", { name: "Remote" }));
    const dialog = await openInformation("origin/main");
    expect(await within(dialog).findByText("Remote branch")).toBeInTheDocument();
    expect(within(dialog).getByText("Not checked out in any worktree")).toBeInTheDocument();
    expect(calls("get_branch_information")[0][1]).toEqual({ name: "origin/main", kind: "remote" });
  });

  it("GIT-FR-NUCX: shows a loading state, and then the failure with its typed cause and a retry", async () => {
    const pending = deferred<BranchInformation>();
    informationRead = () => pending.promise;
    await openBranches();
    const dialog = await openInformation("develop");
    expect(within(dialog).getByText("Loading branch information…")).toBeInTheDocument();

    pending.reject("unknown branch");
    expect(await within(dialog).findByRole("alert")).toHaveTextContent(
      "Branch develop does not exist any more.",
    );

    informationRead = async (name) => branchInformation({ name });
    await userEvent.click(within(dialog).getByRole("button", { name: "Retry" }));
    expect(await within(dialog).findByTestId("git-info-name")).toBeInTheDocument();
  });

  it("GIT-FR-NUCX, GIT-FR-VCDG: takes focus when it opens and Escape closes it and returns focus to the row", async () => {
    await openBranches();
    row("develop").focus();
    await userEvent.keyboard("{ContextMenu}");
    await userEvent.click(await screen.findByRole("menuitem", { name: "Information" }));
    const dialog = await screen.findByRole("dialog");
    await within(dialog).findByTestId("git-info-name");
    expect(dialog.contains(document.activeElement)).toBe(true);

    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(row("develop")).toHaveFocus();
  });

  it("GIT-FR-NUCX, GIT-FR-VCDG: the close control and the backdrop close it and return focus to the row", async () => {
    await openBranches();
    row("develop").focus();
    await userEvent.keyboard("{ContextMenu}");
    await userEvent.click(await screen.findByRole("menuitem", { name: "Information" }));
    const dialog = await screen.findByRole("dialog");
    await userEvent.click(within(dialog).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(row("develop")).toHaveFocus();

    row("main").focus();
    await userEvent.keyboard("{ContextMenu}");
    await userEvent.click(await screen.findByRole("menuitem", { name: "Information" }));
    await screen.findByRole("dialog");
    fireEvent.mouseDown(screen.getByTestId("git-overlay-backdrop"));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(row("main")).toHaveFocus();
  });

  it("GIT-FR-VCDG: keeps focus inside the window when Tab or Shift+Tab goes past its ends", async () => {
    await openBranches();
    const dialog = await openInformation("develop");
    await within(dialog).findByTestId("git-info-name");
    const close = within(dialog).getByRole("button", { name: "Close" });
    close.focus();
    await userEvent.tab();
    expect(dialog.contains(document.activeElement)).toBe(true);
    await userEvent.tab({ shift: true });
    expect(dialog.contains(document.activeElement)).toBe(true);
    await userEvent.tab({ shift: true });
    expect(dialog.contains(document.activeElement)).toBe(true);
  });

  it("GIT-FR-VCDG: only one overlay is open at a time", async () => {
    await openBranches();
    await openInformation("develop");
    expect(screen.getAllByRole("dialog")).toHaveLength(1);
  });
});
