import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { Mock } from "vitest";
import {
  act,
  cleanup,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import {
  WorktreeSelector,
  type SwitchOutcome,
} from "./WorktreeSelector";
import {
  GITHUB_TOKEN_ERRORS,
  type PullOutcome,
  type RefreshOutcome,
  type WorktreeContext,
  type WorktreeEntry,
} from "../types";
import {
  ACTIVE,
  REPO,
  defaultWorktreeContext,
} from "../test/worktreeFixtures";
import { NotificationToasts } from "./NotificationToasts";
import { clearToasts } from "../state/toasts";
import { resetLogBufferForTest } from "../logging";

// The selector talks to the backend only through `invoke` (`../api`), and hands
// every switch to `onSwitch` — the shell owns the transition itself, so these
// tests assert what the selector *asks for*, not what the shell then does.
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

// The selector subscribes to two channels: `"worktree-context-changed"`
// (WTS-FR-25) and `"branches-changed"` (WTS-FR-32). The mock records the channel
// name with each handler, so a test fires exactly one of them — firing both at
// once would let a component that listens on the wrong channel pass. It honours
// the unlisten contract too, so a leak shows up as a growing handler list.
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
const fireBranchesChanged = (repositoryRoot: string) =>
  fire("branches-changed", { repositoryRoot });

let context: WorktreeContext;
/** What the mocked `list_worktrees_and_branches` does — resolve or throw. */
let listError: string | null;
/**
 * The refresh the mocked `pull_current_branch` runs first (WTC-FR-PLHU). A
 * function so a test can vary the answer between calls — which is exactly what
 * the retry after a confirmed token picker needs (WTS-FR-35).
 */
let refresh: () => Promise<RefreshOutcome>;
/** What the pull makes of a refresh that reached the remote. */
let landed: Partial<PullOutcome>;
/** How many pulls were asked for, so a test can prove one was not. */
let refreshCalls: number;

/** The pull outcome a refresh leads to, as the backend composes it. */
function pullOf(r: RefreshOutcome): PullOutcome {
  const base = {
    ahead: 0,
    behind: 0,
    updatedPaths: [],
    refresh: r,
    branch: "main",
    upstream: "origin/main",
  };
  if (r.remoteState === "skipped") {
    return { ...base, state: "refused", error: "no remote configured" };
  }
  if (r.remoteState === "failed") {
    return { ...base, state: "fetch_failed", error: r.remoteError };
  }
  return { ...base, state: "up_to_date", ...landed };
}
let proposed: string;
/** The outcome the shell reports for a requested switch. */
let outcome: SwitchOutcome;
let onSwitch: Mock<
  (operation: () => Promise<WorktreeContext>) => Promise<SwitchOutcome>
>;
/** The operation the selector handed to `onSwitch`, still un-run. */
let handedOperation: (() => Promise<WorktreeContext>) | null;

beforeEach(() => {
  clearToasts();
  resetLogBufferForTest();
  handlers = [];
  listError = null;
  proposed = "/dev/acme-platform-fix-editor-scroll";
  outcome = { ok: true };
  handedOperation = null;
  refreshCalls = 0;
  landed = {};
  // The happy path: the remote was reached and the listing is the refreshed one.
  refresh = async () => ({ context, remoteState: "refreshed" });
  context = defaultWorktreeContext();
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "list_worktrees_and_branches") {
      if (listError) throw listError;
      return context;
    }
    if (cmd === "propose_worktree_path") return proposed;
    if (cmd === "browse_for_folder") return "cancelled";
    if (cmd === "pull_current_branch") {
      refreshCalls += 1;
      return pullOf(await refresh());
    }
    if (cmd === "append_log_records") return undefined;
    return undefined;
  });
  onSwitch = vi.fn(async (op: () => Promise<WorktreeContext>) => {
    handedOperation = op;
    return outcome;
  });
});

afterEach(cleanup);

function renderSelector(
  active: WorktreeEntry | null = ACTIVE,
  onOpen?: () => void,
  extra?: {
    onRequestGithubToken?: () => Promise<boolean>;
    onOpenGlobalSettings?: () => void;
    onPullDiverged?: (outcome: PullOutcome) => void;
  },
) {
  return render(
    <>
      <WorktreeSelector
        active={active}
        onSwitch={onSwitch}
        onOpen={onOpen}
        {...extra}
      />
      <NotificationToasts onActivate={vi.fn()} />
    </>,
  );
}

const trigger = () => screen.getByTestId("worktree-selector");
const openDropdown = async () => {
  await userEvent.click(trigger());
  await screen.findByTestId("worktree-group");
};

/**
 * Run the operation the selector handed over and return the command it invokes.
 * The shell is what actually calls it, so this is how a test sees which backend
 * operation a click chose.
 */
async function invokedByHandedOperation(): Promise<[string, unknown]> {
  expect(handedOperation).not.toBeNull();
  invokeMock.mockClear();
  await handedOperation!();
  const calls = invokeMock.mock.calls;
  const call = calls[calls.length - 1];
  return [call[0] as string, call[1]];
}

// ---------------------------------------------------------------------------
// The refresh control (WTS-FR-29 .. WTS-FR-36)
// ---------------------------------------------------------------------------

const refreshBtn = () => screen.getByTestId("worktree-refresh");
/** NTF-FR-PLQV: the toast a pull shows, or null. */
const note = () =>
  document.querySelector<HTMLElement>(
    '[data-testid="notification-toast"][data-key="pull-result"]',
  );
/** Wait until the pull toast shows. */
const findNote = () =>
  waitFor(() => {
    const shown = note();
    expect(shown).not.toBeNull();
    return shown!;
  });
/** NTF-FR-PLQV: the link a missing-token failure carries. */
const settingsLink = () =>
  note() ? within(note()!).queryByTestId("notification-toast-link") : null;

/** A promise a test resolves by hand, for asserting an in-flight state. */
function deferred<T>() {
  let settle!: (value: T) => void;
  const promise = new Promise<T>((resolve) => {
    settle = resolve;
  });
  return { promise, settle };
}

describe("WorktreeSelector refresh control placement (WTS-FR-29, WTS-FR-02)", () => {
  it("renders immediately after the selector inside a repository", () => {
    renderSelector();
    const button = refreshBtn();
    expect(button).toBeInTheDocument();
    // "Immediately after" is the DOM order, not merely co-presence: the
    // selector's trigger is the element right before it (SNV-FR-32).
    expect(button.previousElementSibling).toBe(trigger());
  });

  it("renders neither the selector nor the refresh control outside a repository", () => {
    // WTS-FR-29: the two share one visibility rule — refreshing a branch list is
    // meaningless where there are no branches.
    const { container } = renderSelector(null);
    expect(container).toBeEmptyDOMElement();
    expect(screen.queryByTestId("worktree-refresh")).toBeNull();
    expect(screen.queryByTestId("worktree-selector")).toBeNull();
  });
});

describe("WorktreeSelector refresh invocation (WTS-FR-30)", () => {
  it("invokes the refresh without opening the dropdown", async () => {
    renderSelector();
    await userEvent.click(refreshBtn());

    await waitFor(() => expect(refreshCalls).toBe(1));
    expect(invokeMock).toHaveBeenCalledWith("pull_current_branch");
    // WTS-FR-30: it belongs to the chrome, not to the dropdown.
    expect(screen.queryByTestId("worktree-selector-menu")).toBeNull();
  });

  it("invokes the same operation from an open dropdown and leaves it open", async () => {
    renderSelector();
    await openDropdown();
    await userEvent.click(refreshBtn());

    await waitFor(() => expect(refreshCalls).toBe(1));
    expect(screen.getByTestId("worktree-selector-menu")).toBeInTheDocument();
  });

  it("is not a worktree switch: nothing is flushed and no tab closes", async () => {
    // WTS-FR-33 (WTS-FR-33). The transition is `onSwitch`'s entirely — flush,
    // close every tab, reopen the Dashboard — so a refresh that reached it would
    // wipe the viewport. Never calling it is the whole claim, and it is also
    // what makes a refresh available while an Editor tab holds unsaved changes.
    renderSelector();
    await userEvent.click(refreshBtn());

    await waitFor(() => expect(refreshCalls).toBe(1));
    expect(onSwitch).not.toHaveBeenCalled();
  });
});

describe("WorktreeSelector refresh busy state (WTS-FR-31)", () => {
  it("renders busy while in flight and stacks no second refresh on the first", async () => {
    const d = deferred<RefreshOutcome>();
    refresh = () => d.promise;
    renderSelector();

    await userEvent.click(refreshBtn());
    await waitFor(() => expect(refreshBtn()).toHaveAttribute("aria-busy", "true"));
    expect(refreshBtn()).toBeDisabled();

    // A second activation while busy invokes nothing.
    await userEvent.click(refreshBtn());
    expect(refreshCalls).toBe(1);

    await act(async () => {
      d.settle({ context, remoteState: "refreshed" });
    });
    await waitFor(() =>
      expect(refreshBtn()).toHaveAttribute("aria-busy", "false"),
    );
    expect(refreshBtn()).not.toBeDisabled();

    // And it is activatable again.
    await userEvent.click(refreshBtn());
    await waitFor(() => expect(refreshCalls).toBe(2));
  });
});

describe("WorktreeSelector refreshed rows (WTS-FR-32)", () => {
  /** The context as it looks once the remote's new branch has been fetched. */
  const withFetchedBranch = (): WorktreeContext => ({
    ...context,
    branches: [
      ...context.branches,
      { name: "feature/new", kind: "local", headShortHash: "ddd4444" },
    ],
  });

  it("re-renders an open dropdown's rows while keeping its filter query", async () => {
    // WTS-FR-32: the query survives, and the refreshed rows are filtered by it —
    // a refresh that reset the filter would drop the user back to the full list
    // mid-search.
    renderSelector();
    await openDropdown();
    await userEvent.type(screen.getByTestId("worktree-filter"), "feat");

    refresh = async () => ({
      context: withFetchedBranch(),
      remoteState: "refreshed",
    });
    await userEvent.click(refreshBtn());

    const group = await screen.findByTestId("branch-group");
    await waitFor(() =>
      expect(within(group).getByText("feature/new")).toBeInTheDocument(),
    );
    expect(screen.getByTestId("worktree-filter")).toHaveValue("feat");
    // Only the rows matching the query are rendered.
    expect(within(group).queryByText("chore/deps")).toBeNull();
    expect(within(group).queryByText("origin/experiment")).toBeNull();
  });

  it("reflects a refresh that returned while the dropdown was closed", async () => {
    // WTS-FR-32: the next open already shows the refreshed set, without a second
    // refresh being asked for.
    renderSelector();
    context = withFetchedBranch();
    refresh = async () => ({ context, remoteState: "refreshed" });
    await userEvent.click(refreshBtn());
    await waitFor(() => expect(refreshCalls).toBe(1));

    await openDropdown();
    const group = screen.getByTestId("branch-group");
    expect(within(group).getByText("feature/new")).toBeInTheDocument();
    expect(refreshCalls).toBe(1);
  });

  it("re-renders on a branches-changed event whatever raised it", async () => {
    // WTS-FR-32's last clause: the event is followed on its own, so a branch set
    // re-read anywhere reaches an open dropdown — and an undelivered outcome
    // cannot leave the rows stale.
    renderSelector();
    await openDropdown();
    expect(screen.queryByText("feature/new")).toBeNull();

    context = withFetchedBranch();
    await act(async () => {
      fireBranchesChanged(REPO);
    });

    await waitFor(() =>
      expect(
        within(screen.getByTestId("branch-group")).getByText("feature/new"),
      ).toBeInTheDocument(),
    );
    expect(refreshCalls).toBe(0);
  });
});

describe("WorktreeSelector remote outcome note (WTS-FR-34)", () => {
  it("applies the local listing and reports an unreachable remote", async () => {
    // WTS-FR-34: offline, with a branch created locally since the last look.
    context = {
      ...context,
      branches: [
        ...context.branches,
        { name: "wip", kind: "local", headShortHash: "eee5555" },
      ],
    };
    refresh = async () => ({
      context,
      remoteState: "failed",
      remoteError: GITHUB_TOKEN_ERRORS.githubUnreachable,
    });
    renderSelector();
    await userEvent.click(refreshBtn());

    const shown = await findNote();
    expect(shown).toHaveTextContent(/couldn't reach the remote/i);

    // Dismissing the toast removes it and nothing else.
    await userEvent.click(within(shown).getByRole("button", { name: "Dismiss" }));
    expect(note()).toBeNull();

    // The locally refreshed listing was applied anyway.
    await openDropdown();
    expect(
      within(screen.getByTestId("branch-group")).getByText("wip"),
    ).toBeInTheDocument();
  });

  it("names a missing remote as the cause rather than a network or token failure", async () => {
    // WTS-FR-34: the three causes are distinguished because the author would act
    // on each differently — and "no remote" is not something to retry.
    refresh = async () => ({ context, remoteState: "skipped" });
    renderSelector();
    await userEvent.click(refreshBtn());

    const shown = await findNote();
    expect(shown).toHaveTextContent(/no remote is configured/i);
    expect(shown).not.toHaveTextContent(/reach/i);
    expect(shown).not.toHaveTextContent(/token/i);
  });

  it("WTS-FR-34: names an up-to-date branch when the remote was reached", async () => {
    renderSelector();
    await userEvent.click(refreshBtn());
    await waitFor(() =>
      expect(note()).toHaveTextContent("main is up to date with origin/main."),
    );
    expect(note()).toHaveAttribute("data-level", "Info");
    expect(note()).toHaveTextContent("Branch up to date");
  });

  it("WTS-FR-34: names the commits a fast-forward brought", async () => {
    landed = { state: "fast_forwarded", behind: 3, updatedPaths: ["a.md"] };
    renderSelector();
    await userEvent.click(refreshBtn());
    await waitFor(() =>
      expect(note()).toHaveTextContent("Pulled 3 commits into main."),
    );
    expect(note()).toHaveAttribute("data-level", "Info");
    expect(note()).toHaveTextContent("Branch pulled");
  });

  it("WTS-FR-34: names the uncommitted paths a pull would overwrite", async () => {
    landed = { state: "refused", error: "pull_would_overwrite: a.md, b.md" };
    renderSelector();
    await userEvent.click(refreshBtn());
    await waitFor(() =>
      expect(note()).toHaveTextContent(
        "The pull would overwrite uncommitted changes in a.md, b.md.",
      ),
    );
  });

  it("WTS-FR-34: names a branch that tracks nothing", async () => {
    landed = { state: "refused", error: "no_upstream" };
    renderSelector();
    await userEvent.click(refreshBtn());
    await waitFor(() => expect(note()).toHaveTextContent(/tracks no remote branch/));
  });
});

describe("WorktreeSelector refresh token routing (WTS-FR-35 .. WTS-FR-36, WTS-FR-05)", () => {
  /**
   * A first refresh blocked on token selection, then one that succeeds.
   *
   * The successful attempt updates the shared `context` as well as returning it,
   * because a real backend's next `list_worktrees_and_branches` would also report
   * the fetched branch — a test where only the outcome carried it would pass on a
   * component that ignored the outcome, and vice versa.
   */
  function selectionRequiredThenFetched() {
    let attempt = 0;
    return async (): Promise<RefreshOutcome> => {
      attempt += 1;
      if (attempt === 1) {
        return {
          context,
          remoteState: "failed",
          remoteError: GITHUB_TOKEN_ERRORS.selectionRequired,
        };
      }
      context = {
        ...context,
        branches: [
          ...context.branches,
          { name: "origin/fetched", kind: "remote", headShortHash: "fff6666" },
        ],
      };
      return { context, remoteState: "refreshed" };
    };
  }

  it("opens the picker and re-invokes the refresh when a token is confirmed", async () => {
    // WTS-FR-35, first half.
    refresh = selectionRequiredThenFetched();
    const onRequestGithubToken = vi.fn(async () => true);
    renderSelector(ACTIVE, undefined, { onRequestGithubToken });

    await userEvent.click(refreshBtn());
    await waitFor(() => expect(onRequestGithubToken).toHaveBeenCalledTimes(1));
    // The whole refresh runs again, not just its remote leg.
    await waitFor(() => expect(refreshCalls).toBe(2));

    await openDropdown();
    expect(
      within(screen.getByTestId("branch-group")).getByText("origin/fetched"),
    ).toBeInTheDocument();
    // The retried pull reached the remote, so the toast is its outcome and
    // no token cause stands any more.
    expect(note()).toHaveTextContent(/is up to date/);
    expect(note()).not.toHaveTextContent(/token/i);
  });

  it("abandons the remote leg on cancel, keeping the local listing and saying so", async () => {
    // WTS-FR-35, second half: no remote branch is added, the locally refreshed
    // rows are still shown, and the cancellation is visible beside the control.
    refresh = selectionRequiredThenFetched();
    const onRequestGithubToken = vi.fn(async () => false);
    renderSelector(ACTIVE, undefined, { onRequestGithubToken });

    await userEvent.click(refreshBtn());
    await waitFor(() => expect(onRequestGithubToken).toHaveBeenCalledTimes(1));
    expect(refreshCalls).toBe(1);

    const shown = await findNote();
    expect(shown).toHaveTextContent(/no github token was selected/i);
    expect(shown).toHaveAttribute("data-level", "Error");
    expect(settingsLink()).toBeNull();
    await openDropdown();
    expect(
      within(screen.getByTestId("branch-group")).queryByText("origin/fetched"),
    ).toBeNull();
    // The listing the local half refreshed is still what is on show.
    expect(
      within(screen.getByTestId("branch-group")).getByText("chore/deps"),
    ).toBeInTheDocument();
  });

  it("opens no picker when no token is stored and routes to Global settings", async () => {
    // WTS-FR-35: nothing to choose between (GHA-FR-19).
    refresh = async () => ({
      context,
      remoteState: "failed",
      remoteError: GITHUB_TOKEN_ERRORS.tokenMissing,
    });
    const onRequestGithubToken = vi.fn(async () => true);
    const onOpenGlobalSettings = vi.fn();
    renderSelector(ACTIVE, undefined, {
      onRequestGithubToken,
      onOpenGlobalSettings,
    });

    await userEvent.click(refreshBtn());
    const shown = await findNote();
    expect(onRequestGithubToken).not.toHaveBeenCalled();
    expect(shown).toHaveTextContent(/needs a github token/i);

    // A route, not a sentence naming one.
    expect(shown).toHaveAttribute("data-level", "Error");
    expect(settingsLink()).toHaveTextContent("Open GitHub settings");
    await userEvent.click(settingsLink()!);
    expect(onOpenGlobalSettings).toHaveBeenCalledTimes(1);
  });

  it("closes the dropdown when the picker opens and does not reopen it after", async () => {
    // WTS-FR-36 (WTS-FR-36 / WTS-FR-05): the picker is a floating overlay, so it
    // is never presented over the list, and dismissing it returns to the chrome.
    refresh = selectionRequiredThenFetched();
    // Held open, so the dropdown's state can be inspected while the picker is
    // genuinely outstanding rather than after it has already been dismissed.
    const answer = deferred<boolean>();
    const onRequestGithubToken = vi.fn(() => answer.promise);
    renderSelector(ACTIVE, undefined, { onRequestGithubToken });

    await openDropdown();
    await userEvent.click(refreshBtn());
    await waitFor(() => expect(onRequestGithubToken).toHaveBeenCalled());

    // WTS-FR-36: closed while the picker is up, so the picker is never presented
    // over the list it was reached from.
    await waitFor(() =>
      expect(screen.queryByTestId("worktree-selector-menu")).toBeNull(),
    );

    // And dismissing it returns to the chrome rather than reopening the dropdown.
    await act(async () => {
      answer.settle(false);
    });
    await waitFor(() => expect(note()).not.toBeNull());
    expect(screen.queryByTestId("worktree-selector-menu")).toBeNull();
  });
});

describe("WorktreeSelector pruned remote entries (WTS-FR-27)", () => {
  it("stops offering a pruned remote entry while keeping the local branch", async () => {
    // WTS-FR-27: a refresh drops the remote-tracking refs of branches the remote
    // no longer has, which is why those stop being offered — and the local
    // branches that tracked them are left intact. The prune is the backend's
    // (GTC-FR-13); what is pinned here is that the UI shows the split rather
    // than offering anything that deletes either one.
    context = {
      ...context,
      branches: [
        { name: "spike", kind: "local", headShortHash: "aaa1111" },
        { name: "origin/spike", kind: "remote", headShortHash: "aaa1111" },
      ],
    };
    renderSelector();
    await openDropdown();
    expect(screen.getByText("origin/spike")).toBeInTheDocument();

    // The remote deleted `spike`; the refresh pruned its remote-tracking ref.
    const pruned: WorktreeContext = {
      ...context,
      branches: [{ name: "spike", kind: "local", headShortHash: "aaa1111" }],
    };
    refresh = async () => ({ context: pruned, remoteState: "refreshed" });
    await userEvent.click(refreshBtn());

    const group = await screen.findByTestId("branch-group");
    await waitFor(() =>
      expect(within(group).queryByText("origin/spike")).toBeNull(),
    );
    expect(within(group).getByText("spike")).toBeInTheDocument();
    // And no worktree was removed — the group still lists every one of them.
    expect(
      within(screen.getByTestId("worktree-group")).getAllByRole("menuitem"),
    ).toHaveLength(context.worktrees.length);
  });
});

describe("WorktreeSelector refresh failure and note vocabulary (WTS-FR-31 / WTS-FR-34)", () => {
  it("names a refused credential as such rather than as an unreachable remote", async () => {
    // WTS-FR-34's third cause. Without this the `invalid_token` arm could be
    // deleted and every other test would stay green, collapsing two causes the
    // author acts on differently into one message.
    refresh = async () => ({
      context,
      remoteState: "failed",
      remoteError: GITHUB_TOKEN_ERRORS.invalidToken,
    });
    renderSelector();
    await userEvent.click(refreshBtn());

    const shown = await findNote();
    expect(shown).toHaveTextContent(/refused the credential/i);
    expect(shown).not.toHaveTextContent(/couldn't reach/i);
    expect(shown).not.toHaveTextContent(/no remote is configured/i);
    // Nothing to route to Global settings for — a token IS bound, it was refused.
    expect(settingsLink()).toBeNull();
  });

  it("does not dress an unrecognised cause up as one of the three known ones", async () => {
    refresh = async () => ({
      context,
      remoteState: "failed",
      remoteError: "something_new_from_the_backend",
    });
    renderSelector();
    await userEvent.click(refreshBtn());

    const shown = await findNote();
    expect(shown).toHaveTextContent(/could not be fetched/i);
    expect(shown).not.toHaveTextContent(/couldn't reach/i);
    expect(shown).not.toHaveTextContent(/no remote is configured/i);
    expect(shown).not.toHaveTextContent(/credential/i);
    // And never the raw wire slug.
    expect(shown).not.toHaveTextContent("something_new_from_the_backend");
  });

  it("reports a rejected refresh and stays activatable afterwards", async () => {
    // WTS-FR-31's second half on the failure path — the one a project closed
    // mid-flight takes. A refresh that left the control disabled would need the
    // window reopening to recover.
    refresh = async () => {
      throw "not a git repository";
    };
    renderSelector();
    await userEvent.click(refreshBtn());

    await waitFor(() => expect(note()).toHaveTextContent("not a git repository"));
    // The toast renders from its store, so the control is idle again in a
    // later commit than the toast. Wait for that state, not for the order.
    await waitFor(() => expect(refreshBtn()).not.toBeDisabled());
    expect(refreshBtn()).toHaveAttribute("aria-busy", "false");

    refresh = async () => ({ context, remoteState: "refreshed" });
    await userEvent.click(refreshBtn());
    await waitFor(() => expect(refreshCalls).toBe(2));
  });

  it("NTF-FR-PLQV: a pull that succeeds replaces the toast of an earlier failure", async () => {
    refresh = async () => ({ context, remoteState: "skipped" });
    renderSelector();
    await userEvent.click(refreshBtn());
    await findNote();
    await waitFor(() => expect(refreshBtn()).not.toBeDisabled());

    refresh = async () => ({ context, remoteState: "refreshed" });
    await userEvent.click(refreshBtn());
    await waitFor(() => expect(note()).toHaveTextContent(/is up to date/));
    expect(note()).not.toHaveTextContent(/no remote/i);
    expect(screen.getAllByTestId("notification-toast")).toHaveLength(1);
  });

  it("WTS-FR-34, NTF-FR-TSRL: a failed pull shows an Error toast and no note of the control", async () => {
    refresh = async () => ({ context, remoteState: "skipped" });
    renderSelector();
    await userEvent.click(refreshBtn());
    const shown = await findNote();
    expect(shown).toHaveAttribute("data-level", "Error");
    expect(shown).toHaveTextContent("Pull failed");
    expect(document.querySelector(".wt-select__note")).toBeNull();

    // The open dropdown holds no copy of the outcome either.
    await openDropdown();
    expect(
      screen.getByTestId("worktree-selector-menu").textContent,
    ).not.toMatch(/no remote is configured/i);
    expect(screen.getAllByTestId("notification-toast")).toHaveLength(1);
  });
});

describe("WorktreeSelector refresh cannot be stacked across the picker (WTS-FR-31)", () => {
  it("starts no second refresh while the token picker is outstanding", async () => {
    // The refresh is still outstanding while the picker is up — the author has
    // not answered it yet. A control that reported itself idle there would let a
    // second refresh run alongside the first, with two toasts racing to report
    // different outcomes.
    let attempt = 0;
    refresh = async () => {
      attempt += 1;
      if (attempt === 1) {
        return {
          context,
          remoteState: "failed",
          remoteError: GITHUB_TOKEN_ERRORS.selectionRequired,
        };
      }
      return { context, remoteState: "refreshed" };
    };
    const answer = deferred<boolean>();
    const onRequestGithubToken = vi.fn(() => answer.promise);
    renderSelector(ACTIVE, undefined, { onRequestGithubToken });

    await userEvent.click(refreshBtn());
    await waitFor(() => expect(onRequestGithubToken).toHaveBeenCalled());

    // Busy, and a click while the picker is up starts nothing.
    expect(refreshBtn()).toHaveAttribute("aria-busy", "true");
    expect(refreshBtn()).toBeDisabled();
    await userEvent.click(refreshBtn());
    expect(refreshCalls).toBe(1);

    // Confirming runs the retry — and only the retry.
    await act(async () => {
      answer.settle(true);
    });
    await waitFor(() => expect(refreshCalls).toBe(2));
    await waitFor(() =>
      expect(refreshBtn()).toHaveAttribute("aria-busy", "false"),
    );
    expect(onRequestGithubToken).toHaveBeenCalledTimes(1);
  });

  it("does not reopen the picker when the retry is also blocked on selection", async () => {
    // The recursion guard. Without it a backend that keeps answering
    // `selection_required` — a binding written to a token that has since been
    // removed, say — would reopen the picker forever.
    refresh = async () => ({
      context,
      remoteState: "failed",
      remoteError: GITHUB_TOKEN_ERRORS.selectionRequired,
    });
    const onRequestGithubToken = vi.fn(async () => true);
    renderSelector(ACTIVE, undefined, { onRequestGithubToken });

    await userEvent.click(refreshBtn());
    await waitFor(() => expect(refreshCalls).toBe(2));
    const shown = await findNote();

    expect(onRequestGithubToken).toHaveBeenCalledTimes(1);
    expect(shown).toHaveTextContent(/no github token was selected/i);
    await waitFor(() => expect(refreshBtn()).not.toBeDisabled());
  });
});

describe("WorktreeSelector refreshed rows keep their scroll position (WTS-FR-32)", () => {
  it("re-renders the same group element without resetting its scroll", async () => {
    // WTS-FR-32's other clause. The mechanism is that the group element is
    // re-rendered rather than remounted — a remount would silently reset the
    // scroll and drop a user who was part-way down a long branch list.
    renderSelector();
    await openDropdown();
    const group = screen.getByTestId("branch-group");
    // jsdom reports zero heights, so scrollTop is set directly — what is being
    // pinned is that nothing resets it, not the browser's scrolling.
    group.scrollTop = 42;

    refresh = async () => ({
      context: {
        ...context,
        branches: [
          ...context.branches,
          { name: "feature/new", kind: "local", headShortHash: "ddd4444" },
        ],
      },
      remoteState: "refreshed",
    });
    await userEvent.click(refreshBtn());

    await waitFor(() =>
      expect(within(group).getByText("feature/new")).toBeInTheDocument(),
    );
    expect(screen.getByTestId("branch-group")).toBe(group);
    expect(group.scrollTop).toBe(42);
  });
});

// ---------------------------------------------------------------------------
// WTS-FR-37 / WTS-FR-31, WTS-FR-34: the refresh control's icon depicts material arriving
// from elsewhere, not the circular arrows of a generic reload.
// ---------------------------------------------------------------------------

describe("WorktreeSelector refresh icon (WTS-FR-31, WTS-FR-37, WTS-FR-34)", () => {
  /** Which glyph the control is rendering right now. */
  const glyph = () =>
    refreshBtn().querySelector("svg")?.getAttribute("data-icon");

  it("depicts something arriving rather than a circular reload", () => {
    renderSelector();
    expect(glyph()).toBe("pull-down");
  });

  it("shows the busy state in its place, and only while in flight", async () => {
    const d = deferred<RefreshOutcome>();
    refresh = () => d.promise;
    renderSelector();

    await userEvent.click(refreshBtn());
    // WTS-FR-31: the busy state is the one thing that replaces the icon.
    await waitFor(() => expect(refreshBtn()).toHaveAttribute("aria-busy", "true"));
    expect(glyph()).toBe("refresh");

    await act(async () => {
      d.settle({ context, remoteState: "refreshed" });
    });
    await waitFor(() => expect(glyph()).toBe("pull-down"));
  });

  it("does not vary with what a refresh reported (WTS-FR-34)", async () => {
    // A remote leg that could not run still leaves the resting icon alone —
    // the toast is what carries the outcome.
    refresh = async () => ({
      context,
      remoteState: "failed" as const,
      remoteError: GITHUB_TOKEN_ERRORS.githubUnreachable,
    });
    renderSelector();
    expect(glyph()).toBe("pull-down");

    await userEvent.click(refreshBtn());
    await waitFor(() => expect(note()).toBeInTheDocument());
    await waitFor(() => expect(glyph()).toBe("pull-down"));
  });
});

describe("WorktreeSelector diverged pull (WTS-FR-DVGQ)", () => {
  it("WTS-FR-DVGQ: hands a diverged outcome to the shell and closes the dropdown", async () => {
    landed = {
      state: "diverged",
      ahead: 1,
      behind: 2,
      localTip: "1111111aaaa",
      remoteTip: "2222222bbbb",
    };
    const onPullDiverged = vi.fn();
    renderSelector(ACTIVE, undefined, { onPullDiverged });
    await openDropdown();
    await userEvent.click(refreshBtn());

    await waitFor(() => expect(onPullDiverged).toHaveBeenCalledTimes(1));
    expect(onPullDiverged.mock.calls[0][0]).toMatchObject({
      state: "diverged",
      ahead: 1,
      behind: 2,
      localTip: "1111111aaaa",
      remoteTip: "2222222bbbb",
    });
    expect(screen.queryByTestId("worktree-selector-menu")).toBeNull();
    // The dialog says what happened, so no toast stands beside it.
    expect(note()).toBeNull();
  });
});

describe("WorktreeSelector pull and the shared transfer (WTS-FR-31, GIT-FR-QMYB)", () => {
  it("WTS-FR-31: is not activatable while a push runs, whichever control started it", async () => {
    renderSelector();
    expect(refreshBtn()).not.toBeDisabled();

    await act(async () => {
      fire("git-output-line", { operation: "push", line: "Pushing main to origin" });
    });
    await waitFor(() => expect(refreshBtn()).toBeDisabled());
    // Not busy itself: the busy state is the pull's own (WTS-FR-31).
    expect(refreshBtn()).toHaveAttribute("aria-busy", "false");
    await userEvent.click(refreshBtn());
    expect(refreshCalls).toBe(0);

    await act(async () => {
      fire("git-operation-finished", { operation: "push", ok: true });
    });
    await waitFor(() => expect(refreshBtn()).not.toBeDisabled());
  });
});
