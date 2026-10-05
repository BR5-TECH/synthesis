import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Mock } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Git } from "./Git";
import type { GitBranch, UpstreamSyncState, WorktreeContext } from "../types";
import {
  COMMIT_DIFF,
  COMMIT_FILES,
  history,
} from "../test/gitPanelFixtures";
import type { SwitchOutcome } from "./WorktreeSelector";

// The Git panel's branches section is the second route to a checkout
// (GIT-FR-06). It reaches the backend through `invoke` for the listing, and
// hands the checkout itself to the shell's worktree-switch transition — the
// same one the top-chrome selector uses.
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

// GIT-FR-11: the panel follows `"branches-changed"` to keep its branches section
// current. The mock records the channel name with each handler so a test fires
// exactly that one, and honours the unlisten contract.
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
const fireBranchesChanged = (repositoryRoot = "/dev/acme-platform") =>
  handlers
    .filter(([n]) => n === "branches-changed")
    .forEach(([, h]) => h({ payload: { repositoryRoot } }));

/** GTC-FR-04: one line of transfer output, wherever the transfer was started. */
const fireGitOutputLine = (line: string, operation = "push") =>
  handlers
    .filter(([n]) => n === "git-output-line")
    .forEach(([, h]) => h({ payload: { operation, line } }));

/** GTC-FR-04: a transfer's single terminal event. */
const fireGitOperationFinished = (
  payload: { operation?: string; ok: boolean; error?: string },
) =>
  handlers
    .filter(([n]) => n === "git-operation-finished")
    .forEach(([, h]) => h({ payload: { operation: "push", ...payload } }));

let branches: GitBranch[];
let listError: string | null;
/** GTC-FR-21: what `get_upstream_sync_state` answers; a test may change it. */
let sync: UpstreamSyncState;
let outcome: SwitchOutcome;
let onSwitchWorktree: Mock<
  (operation: () => Promise<WorktreeContext>) => Promise<SwitchOutcome>
>;
let handedOperation: (() => Promise<WorktreeContext>) | null;

beforeEach(() => {
  handlers = [];
  listError = null;
  // Two commits the remote does not have, so Push is offered by default and the
  // tests that are about authentication stay about authentication.
  sync = { hasRemote: true, hasUpstream: true, ahead: 2, behind: 0 };
  outcome = { ok: true };
  handedOperation = null;
  branches = [
    { name: "develop", kind: "local", isCurrent: false },
    { name: "main", kind: "local", isCurrent: true },
    { name: "origin/experiment", kind: "remote", isCurrent: false },
  ];
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "list_branches") {
      if (listError) throw listError;
      return branches;
    }
    if (cmd === "get_upstream_sync_state") return sync;
    // GIT-FR-KPTE: the Commits section is the one the panel opens on.
    if (cmd === "list_commit_history") return history(3);
    if (cmd === "list_commit_files") return COMMIT_FILES;
    if (cmd === "get_commit_file_diff") return COMMIT_DIFF;
    // A project already pointed at a token, so the tests that are not about
    // authentication are not stopped by its precondition (GTC-FR-10).
    if (cmd === "get_project_github_token_binding")
      return { tokenId: "t1", resolution: "bound" };
    return undefined;
  });
  onSwitchWorktree = vi.fn(async (op: () => Promise<WorktreeContext>) => {
    handedOperation = op;
    return outcome;
  });
});

afterEach(cleanup);

/** GIT-FR-DXNC / GIT-FR-JRYS: select the first commit and its first file. */
const openLogDiff = async () => {
  await userEvent.click(await screen.findByText("Subject of commit 0"));
  await userEvent.click(await screen.findByText("main.ts"));
  return screen.findByText("new line");
};

/** GIT-FR-ZEKI / GIT-FR-04: check a branch out through its row's menu. */
const checkOutFromMenu = async (section: HTMLElement, name: string) => {
  fireEvent.contextMenu(within(section).getByTitle(name));
  await userEvent.click(await screen.findByRole("menuitem", { name: "Check out" }));
};

const openBranches = async (canCheckOutBranches = true) => {
  render(
    <Git
      onSwitchWorktree={onSwitchWorktree}
      canCheckOutBranches={canCheckOutBranches}
    />,
  );
  await userEvent.click(screen.getByText("Branches"));
  return screen.findByTestId("git-branches");
};

describe("Git panel branches section (GIT-FR-06 / GTC-FR-07)", () => {
  // GTC-FR-07, GTC-FR-06 as the panel sees it: the active worktree's branch is the one
  // marked current.
  it("lists local and remote branches with the active worktree's branch current", async () => {
    const section = await openBranches();

    // GIT-FR-HLGO: the Local kind is shown first; Remote is one switch away.
    expect(within(section).getByText("develop")).toBeInTheDocument();
    expect(within(section).queryByText("origin/experiment")).toBeNull();
    expect(within(section).getAllByText("current")).toHaveLength(1);
    expect(
      within(section).getByTitle("main (current)"),
    ).toBeInTheDocument();
    await userEvent.click(within(section).getByRole("radio", { name: "Remote" }));
    expect(within(section).getByText("origin/experiment")).toBeInTheDocument();
    expect(within(section).queryByText("develop")).toBeNull();
  });

  // GIT-FR-04, GIT-FR-ZEKI: checking out from the menu is the same operation
  // the selector performs in place, so it goes through the one switch
  // transition.
  it("GIT-FR-04, GIT-FR-ZEKI: routes a menu checkout through the worktree-switch transition", async () => {
    const section = await openBranches();
    await checkOutFromMenu(section, "develop");

    expect(onSwitchWorktree).toHaveBeenCalledOnce();
    expect(handedOperation).not.toBeNull();

    // Running the handed operation is what reaches the backend — and it is the
    // *same* command the selector's "Check out here" chooses.
    invokeMock.mockClear();
    await handedOperation!();
    expect(invokeMock).toHaveBeenCalledWith(
      "check_out_branch_in_active_worktree",
      { branch: "develop" },
    );
  });

  it("GIT-FR-LNEI: invokes nothing when a branch row is clicked", async () => {
    const section = await openBranches();
    await userEvent.click(within(section).getByTitle("main (current)"));
    await userEvent.click(within(section).getByTitle("develop"));
    expect(onSwitchWorktree).not.toHaveBeenCalled();
  });

  it("GIT-FR-ZEKI: the current branch's menu offers no Check out", async () => {
    const section = await openBranches();
    fireEvent.contextMenu(within(section).getByTitle("main (current)"));
    await screen.findByRole("menuitem", { name: "Information" });
    expect(screen.queryByRole("menuitem", { name: "Check out" })).toBeNull();
  });

  // GIT-FR-06 shares WTS-FR-24's error handling: a typed refusal renders in the
  // surface that asked for the switch.
  it("renders a typed refusal inline in the panel", async () => {
    outcome = { ok: false, error: "checkout blocked by local changes" };
    const section = await openBranches();
    await checkOutFromMenu(section, "develop");

    expect(await screen.findByTestId("git-branch-error")).toHaveTextContent(
      "checkout blocked by local changes",
    );
  });

  // A cancelled flush has already focused the blocking tab; the panel must not
  // also report it as a failure.
  it("shows no error when the flush cancelled the switch", async () => {
    outcome = { ok: false, cancelled: true };
    const section = await openBranches();
    await checkOutFromMenu(section, "develop");

    await waitFor(() => expect(onSwitchWorktree).toHaveBeenCalled());
    expect(screen.queryByTestId("git-branch-error")).toBeNull();
  });

  it("surfaces a failed listing inline rather than crashing the panel", async () => {
    listError = "not a git repository";
    const section = await openBranches();

    expect(await screen.findByTestId("git-branch-error")).toHaveTextContent(
      "This project is not inside a Git repository.",
    );
    expect(section).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// A linked worktree offers no checkout (GIT-FR-06 / WTC-FR-21)
// ---------------------------------------------------------------------------

describe("Git panel branches section inside a linked worktree (GIT-FR-11)", () => {
  // GIT-FR-11: the listing is unconditional — describing a branch is not
  // offering to check it out — but selecting one does nothing.
  it("still lists the repository's branches, with the current one marked", async () => {
    const section = await openBranches(false);

    expect(within(section).getByText("develop")).toBeInTheDocument();
    expect(within(section).getAllByText("current")).toHaveLength(1);
    await userEvent.click(within(section).getByRole("radio", { name: "Remote" }));
    expect(within(section).getByText("origin/experiment")).toBeInTheDocument();
  });

  it("checks nothing out when a branch is selected", async () => {
    const section = await openBranches(false);
    await userEvent.click(within(section).getByText("develop"));

    expect(onSwitchWorktree).not.toHaveBeenCalled();
    expect(
      invokeMock.mock.calls.filter(
        (c) => c[0] === "check_out_branch_in_active_worktree",
      ),
    ).toHaveLength(0);
  });

  it("GIT-FR-04, GIT-FR-ZEKI: the row menu offers no Check out from a linked worktree", async () => {
    const section = await openBranches(false);
    const row = within(section).getByTitle("develop");

    // The row stays focusable, because its menu is still offered (GIT-FR-ZEKI),
    // but the menu offers no checkout.
    expect(row).toHaveAttribute("aria-haspopup", "menu");
    fireEvent.contextMenu(row);
    await screen.findByRole("menuitem", { name: "Information" });
    expect(screen.queryByRole("menuitem", { name: "Check out" })).toBeNull();
  });
});

describe("GitHub authentication for operations that reach a remote (GHA-FR-16)", () => {
  /** Answer the binding read with a given resolution. */
  const withBinding = (resolution: string) => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_branches") return branches;
      if (cmd === "get_upstream_sync_state") return sync;
      if (cmd === "get_project_github_token_binding")
        return { tokenId: resolution === "bound" ? "t1" : null, resolution };
      return undefined;
    });
  };

  /** Every `push_current_branch` invoke so far (GIT-FR-PZIE). */
  const pushCalls = () =>
    invokeMock.mock.calls.filter((c) => c[0] === "push_current_branch").length;

  /**
   * Mount the panel and open the Logs section, which is where the transfer
   * controls and the push/pull output area live (GIT-FR-PZIE).
   */
  const openTransfers = async (props: Partial<{
    onRequestGithubToken: () => Promise<boolean>;
  }> = {}) => {
    render(
      <Git
        onSwitchWorktree={onSwitchWorktree}
        canCheckOutBranches
        {...props}
      />,
    );
    await userEvent.click(screen.getByText("Logs"));
    await screen.findByTestId("git-transfer-output");
  };

  it("GIT-FR-PZIE, GTC-FR-22 opens the picker when the project has not been pointed at a token, and runs the operation once one is chosen", async () => {
    // GHA-FR-16: `selection_required` is the state that prompts, and the
    // operation that was waiting proceeds on confirm.
    withBinding("selection_required");
    const onRequestGithubToken = vi.fn(async () => true);
    await openTransfers({ onRequestGithubToken });

    await userEvent.click(screen.getByRole("button", { name: "Push" }));

    await waitFor(() => expect(onRequestGithubToken).toHaveBeenCalled());
    // GIT-FR-PZIE: the operation ran rather than being abandoned — the real push
    // was invoked once a token had been chosen.
    await waitFor(() => expect(pushCalls()).toBe(1));
  });

  it("GIT-FR-PZIE, GTC-FR-22 abandons the operation when the picker is cancelled", async () => {
    // GHA-FR-17: cancelling records nothing and the push does not run.
    withBinding("selection_required");
    const onRequestGithubToken = vi.fn(async () => false);
    await openTransfers({ onRequestGithubToken });

    await userEvent.click(screen.getByRole("button", { name: "Push" }));

    await waitFor(() =>
      expect(screen.getByTestId("git-auth-note")).toHaveTextContent(
        /Push cancelled — no GitHub token was selected/,
      ),
    );
    expect(pushCalls()).toBe(0);
  });

  it("GIT-FR-02 routes to Global settings instead of prompting when no token is stored", async () => {
    // GHA-FR-19: there is nothing to choose between, so no modal opens.
    withBinding("none_stored");
    const onRequestGithubToken = vi.fn(async () => true);
    await openTransfers({ onRequestGithubToken });

    await userEvent.click(screen.getByRole("button", { name: "Push" }));

    await waitFor(() =>
      expect(screen.getByTestId("git-auth-note")).toHaveTextContent(
        /Add one in Global settings → GitHub/,
      ),
    );
    expect(onRequestGithubToken).not.toHaveBeenCalled();
    expect(pushCalls()).toBe(0);
  });

  it("never prompts when the project already resolves a token", async () => {
    // GHA-FR-18: bound, or the single stored token used implicitly — the
    // author who keeps one token is never asked.
    for (const resolution of ["bound", "implicit"]) {
      cleanup();
      invokeMock.mockClear();
      withBinding(resolution);
      const onRequestGithubToken = vi.fn(async () => true);
      await openTransfers({ onRequestGithubToken });

      await userEvent.click(screen.getByRole("button", { name: "Push" }));

      await waitFor(() => expect(pushCalls()).toBe(1));
      expect(onRequestGithubToken).not.toHaveBeenCalled();
    }
  });

  it("renders a typed rejection as a sentence rather than as its wire slug", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_branches") return branches;
      if (cmd === "get_upstream_sync_state") return sync;
      if (cmd === "get_project_github_token_binding") throw "keychain_unavailable";
      return undefined;
    });
    await openTransfers();

    await userEvent.click(screen.getByRole("button", { name: "Push" }));

    await waitFor(() =>
      expect(screen.getByTestId("git-auth-note")).toHaveTextContent(
        /system keychain is unavailable/,
      ),
    );
    expect(screen.getByTestId("git-auth-note")).not.toHaveTextContent(
      /^keychain_unavailable$/,
    );
  });

  it("applies the same precondition to pulling and to creating a pull request", async () => {
    // GTC-FR-09: all three reach GitHub, so all three resolve a token first.
    withBinding("none_stored");
    await openTransfers();

    await userEvent.click(screen.getByRole("button", { name: /Pull/ }));
    await waitFor(() =>
      expect(screen.getByTestId("git-auth-note")).toHaveTextContent(
        /Pull needs a GitHub token/,
      ),
    );

    await userEvent.click(screen.getByText("PRs"));
    await userEvent.click(
      screen.getByRole("button", { name: /Create PR for current branch/ }),
    );
    await waitFor(() =>
      expect(screen.getByTestId("git-auth-note-prs")).toHaveTextContent(
        /Creating a pull request needs a GitHub token/,
      ),
    );
  });
});

// ---------------------------------------------------------------------------
// The branches section follows the branch set (GIT-FR-11)
// ---------------------------------------------------------------------------

describe("Git panel branches section follows branches-changed (GIT-FR-01, GIT-FR-03, GIT-FR-06)", () => {
  it("picks up a fetched branch and drops a pruned one without a reopen", async () => {
    const section = await openBranches();
    await userEvent.click(within(section).getByRole("radio", { name: "Remote" }));
    expect(within(section).getByText("origin/experiment")).toBeInTheDocument();
    expect(within(section).queryByText("feature/new")).toBeNull();
    await userEvent.click(within(section).getByRole("radio", { name: "Local" }));
    const listCalls = () =>
      invokeMock.mock.calls.filter((c) => c[0] === "list_branches").length;
    expect(listCalls()).toBe(1);

    // The top-chrome refresh control fetched `feature/new` and pruned the
    // remote-tracking entry for `spike` — here `origin/experiment`.
    branches = [
      { name: "feature/new", kind: "local", isCurrent: false },
      { name: "main", kind: "local", isCurrent: true },
    ];
    await act(async () => {
      fireBranchesChanged();
    });

    await waitFor(() =>
      expect(within(section).getByText("feature/new")).toBeInTheDocument(),
    );
    await userEvent.click(within(section).getByRole("radio", { name: "Remote" }));
    expect(within(section).queryByText("origin/experiment")).toBeNull();
    expect(listCalls()).toBe(2);
    // The panel was never closed and reopened — the same section element is
    // still mounted, which is what "without a reopen" means.
    expect(section).toBe(screen.getByTestId("git-branches"));
  });

  it("reloads only the branch listing — no other section and no inline diff", async () => {
    // GIT-FR-11: reloading a branch listing is not a worktree change. Nothing
    // about the active worktree's content has moved, so the commit log keeps
    // its selection and the inline diff it opened stays put — unlike a switch
    // (GIT-FR-09), which discards both.
    render(
      <Git onSwitchWorktree={onSwitchWorktree} canCheckOutBranches />,
    );

    // GIT-FR-JRYS: open a diff by selecting a commit and one of its files.
    const before = await openLogDiff();
    expect(before).toBeInTheDocument();

    await act(async () => {
      fireBranchesChanged();
    });

    // The diff is still rendered, and the same element at that.
    expect(screen.getByText("new line")).toBe(before);
    // And nothing was asked of the backend, because the branches section is not
    // the one on show.
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "list_branches"),
    ).toHaveLength(0);
  });

  it("serves a fresh listing when the section is opened after the event", async () => {
    // The event may arrive while another section is active. The listing must
    // still be the refreshed one when the branches section is next opened,
    // rather than a stale set cached from before it.
    render(
      <Git onSwitchWorktree={onSwitchWorktree} canCheckOutBranches />,
    );
    await userEvent.click(screen.getByText("Branches"));
    await screen.findByTestId("git-branches");

    // Leave the section, then let the branch set change while it is away.
    await userEvent.click(screen.getByText("Commits"));
    branches = [{ name: "feature/new", kind: "local", isCurrent: true }];
    await act(async () => {
      fireBranchesChanged();
    });

    await userEvent.click(screen.getByText("Branches"));
    const section = await screen.findByTestId("git-branches");
    await waitFor(() =>
      expect(within(section).getByText("feature/new")).toBeInTheDocument(),
    );
  });

  it("unsubscribes when it unmounts", async () => {
    const { unmount } = render(
      <Git onSwitchWorktree={onSwitchWorktree} canCheckOutBranches />,
    );
    // Four channels: the branch set (GIT-FR-11), the change set that decides
    // whether Push is offered (GTC-FR-21), and the two transfer channels the
    // push/pull output area follows (GIT-FR-PZIE / GTC-FR-04).
    await waitFor(() => expect(handlers.length).toBe(4));
    unmount();
    await waitFor(() => expect(handlers.length).toBe(0));
  });
});

describe("Git panel branches section reload robustness (GIT-FR-11)", () => {
  it("retries the listing when the section was left while it was still in flight", async () => {
    // The epoch is claimed before the listing settles, so a section left mid-load
    // must release it again — otherwise re-entering finds the epoch already
    // marked loaded with nothing loaded, and sits on LOADING… until a worktree
    // switch remounts the whole panel.
    let settle!: (rows: GitBranch[]) => void;
    const pending = new Promise<GitBranch[]>((resolve) => {
      settle = resolve;
    });
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_branches") return pending;
      if (cmd === "get_upstream_sync_state") return sync;
      return undefined;
    });

    render(<Git onSwitchWorktree={onSwitchWorktree} canCheckOutBranches />);
    await userEvent.click(screen.getByText("Branches"));
    expect(await screen.findByText("LOADING…")).toBeInTheDocument();

    // Leave while the listing is still outstanding, then come back.
    await userEvent.click(screen.getByText("Commits"));
    await act(async () => {
      settle(branches);
    });
    await userEvent.click(screen.getByText("Branches"));

    const section = await screen.findByTestId("git-branches");
    await waitFor(() =>
      expect(within(section).getByText("develop")).toBeInTheDocument(),
    );
    expect(within(section).queryByText("LOADING…")).toBeNull();
  });

  it("keeps an open inline diff while reloading a loaded branch listing", async () => {
    // GIT-FR-11's second clause, with BOTH halves true at once: the branches
    // section has actually loaded (so the event really does reload it) and an
    // inline diff is open in the commit log (so there is something a
    // wrongly-scoped reload could discard).
    render(<Git onSwitchWorktree={onSwitchWorktree} canCheckOutBranches />);
    await userEvent.click(screen.getByText("Branches"));
    await screen.findByTestId("git-branches");
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "list_branches"),
    ).toHaveLength(1);

    // Open an inline diff in the commit log.
    await userEvent.click(screen.getByText("Commits"));
    const diff = await openLogDiff();

    branches = [{ name: "feature/new", kind: "local", isCurrent: true }];
    await act(async () => {
      fireBranchesChanged();
    });

    // The diff is the same element — nothing about the active worktree moved.
    expect(screen.getByText("new line")).toBe(diff);

    // And the branch listing genuinely reloaded when its section came back.
    await userEvent.click(screen.getByText("Branches"));
    const section = await screen.findByTestId("git-branches");
    await waitFor(() =>
      expect(within(section).getByText("feature/new")).toBeInTheDocument(),
    );
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "list_branches"),
    ).toHaveLength(2);
  });
});

// ---------------------------------------------------------------------------
// The push/pull output area (GIT-FR-PZIE, GTC-FR-22) and what the panel does NOT offer
// (GIT-FR-02 / GIT-FR-08), plus the commit log (GIT-FR-01, GIT-FR-03, GIT-FR-06)
// ---------------------------------------------------------------------------

describe("the push/pull output area (GIT-FR-PZIE)", () => {
  const openOutput = async () => {
    render(<Git onSwitchWorktree={onSwitchWorktree} canCheckOutBranches />);
    await userEvent.click(screen.getByText("Logs"));
    return screen.findByTestId("git-transfer-output");
  };

  it("GIT-FR-PZIE, GTC-FR-22 records a push started from the Changes panel, indistinguishably", async () => {
    // GTC-FR-22: both channels are surface-agnostic, so a push the author began
    // in the Changes panel streams into this area exactly as one begun here.
    const area = await openOutput();
    expect(area).toHaveTextContent("No transfers yet.");

    await act(async () => {
      fireGitOutputLine("Pushing feature/x to origin");
      fireGitOutputLine("remote: Resolving deltas: 100%");
      fireGitOperationFinished({ ok: true });
    });

    expect(area).toHaveTextContent("Pushing feature/x to origin");
    expect(area).toHaveTextContent("remote: Resolving deltas: 100%");
    expect(area).toHaveTextContent("push finished");
  });

  it("GIT-FR-PZIE, GTC-FR-22 records a failed transfer's terminal status too", async () => {
    const area = await openOutput();
    await act(async () => {
      fireGitOperationFinished({ ok: false, error: "remote rejected the push" });
    });
    expect(area).toHaveTextContent(/push failed: remote rejected the push/);
  });

  it("keeps lines that arrive while another section is showing", async () => {
    // The event may land while the commit log is on screen; the transcript must
    // still hold it when the branches section is next opened.
    render(<Git onSwitchWorktree={onSwitchWorktree} canCheckOutBranches />);
    await act(async () => {
      fireGitOutputLine("Pushing main to origin");
    });

    await userEvent.click(screen.getByText("Logs"));
    expect(await screen.findByTestId("git-transfer-output")).toHaveTextContent(
      "Pushing main to origin",
    );
  });
});

describe("what the Git panel no longer offers", () => {
  it("GIT-FR-02 presents no working-tree listing, stage, unstage, or commit control", async () => {
    const { container } = render(
      <Git onSwitchWorktree={onSwitchWorktree} canCheckOutBranches />,
    );
    // Every section in turn (GIT-FR-02).
    for (const section of ["Commits", "Branches", "PRs", "Logs"]) {
      await userEvent.click(screen.getByText(section));
      const forbidden = /\b(stage|unstage|commit|staged)\b/i;
      const labels = Array.from(
        container.querySelectorAll<HTMLElement>("button, input, textarea"),
      ).map((el) =>
        `${el.getAttribute("aria-label") ?? ""} ${el.getAttribute("placeholder") ?? ""} ${el.textContent ?? ""}`,
      );
      for (const label of labels) expect(label).not.toMatch(forbidden);
      // And no message editor at all: authoring a commit belongs to the commit
      // message window the Changes panel opens (CMW-FR-03).
      expect(container.querySelector("textarea")).toBeNull();
    }
    // Nor does the panel reach for any of those commands.
    const invoked = invokeMock.mock.calls.map((c) => c[0] as string);
    for (const cmd of invoked) {
      expect(cmd).not.toMatch(/^(stage_|unstage_|commit_|get_working_tree)/);
    }
  });

  it("GIT-FR-08 presents no force-push, branch-deletion, worktree-removal, or history-rewrite affordance", async () => {
    const section = await openBranches();
    const { container } = { container: document.body };
    expect(section).toBeInTheDocument();
    expect(container.textContent).not.toMatch(
      /force[- ]push|delete branch|remove worktree|rewrite|rebase|reset --hard/i,
    );
    const labels = Array.from(
      container.querySelectorAll<HTMLElement>("button"),
    ).map((b) => `${b.getAttribute("aria-label") ?? ""} ${b.textContent ?? ""}`);
    for (const label of labels) {
      expect(label).not.toMatch(/force|delete|remove|discard/i);
    }
  });
});

// ---------------------------------------------------------------------------
// Push is offered only when there is something to publish (GTC-FR-21)
// ---------------------------------------------------------------------------

describe("the Logs section's Push control (GIT-FR-KLNK)", () => {
  const pushButton = () => screen.getByRole("button", { name: "Push" });

  const openBranchesWith = async (state: UpstreamSyncState) => {
    sync = state;
    render(<Git onSwitchWorktree={onSwitchWorktree} canCheckOutBranches />);
    await userEvent.click(screen.getByText("Logs"));
    await screen.findByTestId("git-transfer-output");
  };

  it("GIT-FR-KLNK: greys out Push when the branch is level with its remote", async () => {
    await openBranchesWith({
      hasRemote: true,
      hasUpstream: true,
      ahead: 0,
      behind: 0,
    });
    await waitFor(() => expect(pushButton()).toBeDisabled());
    expect(pushButton()).toHaveAttribute(
      "title",
      "Nothing to push — the branch is level with its remote.",
    );
    // Disabled means disabled: activating it reaches nothing.
    await userEvent.click(pushButton());
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "push_current_branch"),
    ).toHaveLength(0);
  });

  it("GIT-FR-KLNK: greys out Push when the branch is only behind its remote", async () => {
    await openBranchesWith({
      hasRemote: true,
      hasUpstream: true,
      ahead: 0,
      behind: 3,
    });
    await waitFor(() => expect(pushButton()).toBeDisabled());
  });

  it("GIT-FR-KLNK: greys out Push when the repository has no remote at all", async () => {
    await openBranchesWith({
      hasRemote: false,
      hasUpstream: false,
      ahead: null,
      behind: null,
    });
    await waitFor(() => expect(pushButton()).toBeDisabled());
    expect(pushButton()).toHaveAttribute(
      "title",
      "This repository has no remote configured.",
    );
  });

  it("GIT-FR-KLNK: offers Push when the branch is ahead, and when it has never been published", async () => {
    await openBranchesWith({
      hasRemote: true,
      hasUpstream: true,
      ahead: 2,
      behind: 0,
    });
    await waitFor(() => expect(pushButton()).toBeEnabled());
    expect(pushButton()).not.toHaveAttribute("title");

    cleanup();
    await openBranchesWith({
      hasRemote: true,
      hasUpstream: false,
      ahead: null,
      behind: null,
    });
    await waitFor(() => expect(pushButton()).toBeEnabled());
  });

  it("GIT-FR-KLNK: greys Push out again once the push it ran has completed", async () => {
    // GTC-FR-21 / CHG-FR-38: a completed push leaves the branch level with its
    // remote, and the control must follow without the panel being reopened.
    await openBranchesWith({
      hasRemote: true,
      hasUpstream: true,
      ahead: 1,
      behind: 0,
    });
    await waitFor(() => expect(pushButton()).toBeEnabled());

    await userEvent.click(pushButton());
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.filter((c) => c[0] === "push_current_branch"),
      ).toHaveLength(1),
    );

    sync = { hasRemote: true, hasUpstream: true, ahead: 0, behind: 0 };
    await act(async () => {
      fireGitOperationFinished({ ok: true });
    });
    await waitFor(() => expect(pushButton()).toBeDisabled());
  });

  it("GIT-FR-KLNK: offers Push as soon as a commit lands, without the panel being reopened", async () => {
    await openBranchesWith({
      hasRemote: true,
      hasUpstream: true,
      ahead: 0,
      behind: 0,
    });
    await waitFor(() => expect(pushButton()).toBeDisabled());

    // A commit made from the Changes panel announces itself on this channel.
    sync = { hasRemote: true, hasUpstream: true, ahead: 1, behind: 0 };
    await act(async () => {
      handlers
        .filter(([n]) => n === "changes-updated")
        .forEach(([, h]) => h({ payload: { changeCount: 1 } }));
    });
    await waitFor(() => expect(pushButton()).toBeEnabled());
  });

  it("GIT-FR-KLNK: does not offer a second push while one started elsewhere is running", async () => {
    // The output area records a push begun in the Changes panel; a concurrent
    // second push of the same branch must not be reachable from here.
    await openBranchesWith({
      hasRemote: true,
      hasUpstream: true,
      ahead: 1,
      behind: 0,
    });
    await waitFor(() => expect(pushButton()).toBeEnabled());

    await act(async () => {
      fireGitOutputLine("Pushing feature/x to origin");
    });
    expect(pushButton()).toBeDisabled();

    await act(async () => {
      fireGitOperationFinished({ ok: true });
    });
    await waitFor(() => expect(pushButton()).toBeEnabled());
  });

  it("GIT-FR-KLNK: leaves Push unavailable when the standing cannot be read", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_branches") return branches;
      if (cmd === "get_upstream_sync_state") throw "not a git repository";
      return undefined;
    });
    render(<Git onSwitchWorktree={onSwitchWorktree} canCheckOutBranches />);
    await userEvent.click(screen.getByText("Logs"));
    await screen.findByTestId("git-transfer-output");

    await waitFor(() => expect(pushButton()).toBeDisabled());
  });
});
