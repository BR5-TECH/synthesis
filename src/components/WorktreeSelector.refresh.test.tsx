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
  type RefreshOutcome,
  type WorktreeContext,
  type WorktreeEntry,
} from "../types";
import {
  ACTIVE,
  REPO,
  defaultWorktreeContext,
} from "../test/worktreeFixtures";

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
 * What the mocked `refresh_worktrees_and_branches` answers with. A function so a
 * test can vary the answer between calls — which is exactly what the retry after
 * a confirmed token picker needs (WTS-FR-35).
 */
let refresh: () => Promise<RefreshOutcome>;
/** How many refreshes were asked for, so a test can prove one was not. */
let refreshCalls: number;
let proposed: string;
/** The outcome the shell reports for a requested switch. */
let outcome: SwitchOutcome;
let onSwitch: Mock<
  (operation: () => Promise<WorktreeContext>) => Promise<SwitchOutcome>
>;
/** The operation the selector handed to `onSwitch`, still un-run. */
let handedOperation: (() => Promise<WorktreeContext>) | null;

beforeEach(() => {
  handlers = [];
  listError = null;
  proposed = "/dev/acme-platform-fix-editor-scroll";
  outcome = { ok: true };
  handedOperation = null;
  refreshCalls = 0;
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
    if (cmd === "refresh_worktrees_and_branches") {
      refreshCalls += 1;
      return refresh();
    }
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
  },
) {
  return render(
    <WorktreeSelector
      active={active}
      onSwitch={onSwitch}
      onOpen={onOpen}
      {...extra}
    />,
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
const note = () => screen.queryByTestId("worktree-refresh-note");

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
    expect(invokeMock).toHaveBeenCalledWith("refresh_worktrees_and_branches");
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

    const shown = await screen.findByTestId("worktree-refresh-note");
    expect(shown).toHaveTextContent(/couldn't reach the remote/i);

    // The locally refreshed listing was applied anyway.
    await openDropdown();
    expect(
      within(screen.getByTestId("branch-group")).getByText("wip"),
    ).toBeInTheDocument();

    // And dismissing the note leaves those rows in place.
    await userEvent.click(screen.getByTestId("worktree-refresh-note-dismiss"));
    expect(note()).toBeNull();
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

    const shown = await screen.findByTestId("worktree-refresh-note");
    expect(shown).toHaveTextContent(/no remote is configured/i);
    expect(shown).not.toHaveTextContent(/reach/i);
    expect(shown).not.toHaveTextContent(/token/i);
  });

  it("says nothing at all when the remote was reached", async () => {
    renderSelector();
    await userEvent.click(refreshBtn());
    await waitFor(() => expect(refreshCalls).toBe(1));
    expect(note()).toBeNull();
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
    expect(note()).toBeNull();
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

    const shown = await screen.findByTestId("worktree-refresh-note");
    expect(shown).toHaveTextContent(/no github token was selected/i);
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
    const shown = await screen.findByTestId("worktree-refresh-note");
    expect(onRequestGithubToken).not.toHaveBeenCalled();
    expect(shown).toHaveTextContent(/needs a github token/i);

    // A route, not a sentence naming one.
    await userEvent.click(
      screen.getByTestId("worktree-refresh-note-settings"),
    );
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

    const shown = await screen.findByTestId("worktree-refresh-note");
    expect(shown).toHaveTextContent(/refused the credential/i);
    expect(shown).not.toHaveTextContent(/couldn't reach/i);
    expect(shown).not.toHaveTextContent(/no remote is configured/i);
    // Nothing to route to Global settings for — a token IS bound, it was refused.
    expect(screen.queryByTestId("worktree-refresh-note-settings")).toBeNull();
  });

  it("does not dress an unrecognised cause up as one of the three known ones", async () => {
    refresh = async () => ({
      context,
      remoteState: "failed",
      remoteError: "something_new_from_the_backend",
    });
    renderSelector();
    await userEvent.click(refreshBtn());

    const shown = await screen.findByTestId("worktree-refresh-note");
    expect(shown).toHaveTextContent(/could not be refreshed/i);
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

    await waitFor(() =>
      expect(screen.getByTestId("worktree-refresh-note")).toHaveTextContent(
        "not a git repository",
      ),
    );
    expect(refreshBtn()).not.toBeDisabled();
    expect(refreshBtn()).toHaveAttribute("aria-busy", "false");

    refresh = async () => ({ context, remoteState: "refreshed" });
    await userEvent.click(refreshBtn());
    await waitFor(() => expect(refreshCalls).toBe(2));
  });

  it("clears a previous note once a refresh succeeds", async () => {
    refresh = async () => ({ context, remoteState: "skipped" });
    renderSelector();
    await userEvent.click(refreshBtn());
    await screen.findByTestId("worktree-refresh-note");

    refresh = async () => ({ context, remoteState: "refreshed" });
    await userEvent.click(refreshBtn());
    await waitFor(() => expect(note()).toBeNull());
  });

  it("renders exactly one note, inside the dropdown while it is open", async () => {
    // WTS-FR-34: two positions, never both — a note duplicated across them would
    // read as two separate outcomes.
    refresh = async () => ({ context, remoteState: "skipped" });
    renderSelector();
    await userEvent.click(refreshBtn());
    await screen.findByTestId("worktree-refresh-note");
    expect(screen.getAllByTestId("worktree-refresh-note")).toHaveLength(1);
    expect(
      screen.getByTestId("worktree-refresh-note").closest(".wt-select__menu"),
    ).toBeNull();

    await openDropdown();
    const notes = screen.getAllByTestId("worktree-refresh-note");
    expect(notes).toHaveLength(1);
    expect(notes[0].closest(".wt-select__menu")).not.toBeNull();
  });
});

describe("WorktreeSelector refresh cannot be stacked across the picker (WTS-FR-31)", () => {
  it("starts no second refresh while the token picker is outstanding", async () => {
    // The refresh is still outstanding while the picker is up — the author has
    // not answered it yet. A control that reported itself idle there would let a
    // second refresh run alongside the first, with two notes racing to report
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
    const shown = await screen.findByTestId("worktree-refresh-note");

    expect(onRequestGithubToken).toHaveBeenCalledTimes(1);
    expect(shown).toHaveTextContent(/no github token was selected/i);
    expect(refreshBtn()).not.toBeDisabled();
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
    // the note beside the control is what carries the outcome.
    refresh = async () => ({
      context,
      remoteState: "failed" as const,
      remoteError: GITHUB_TOKEN_ERRORS.githubUnreachable,
    });
    renderSelector();
    expect(glyph()).toBe("pull-down");

    await userEvent.click(refreshBtn());
    await waitFor(() => expect(note()).toBeInTheDocument());
    expect(glyph()).toBe("pull-down");
  });
});
