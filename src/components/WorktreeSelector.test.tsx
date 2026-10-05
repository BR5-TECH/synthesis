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
  WORKTREE_GROUP_MAX_ROWS,
  WORKTREE_ROW_PX,
  type SwitchOutcome,
} from "./WorktreeSelector";
import {
  type RefreshOutcome,
  type WorktreeContext,
  type WorktreeEntry,
} from "../types";
import {
  ACTIVE,
  LINKED,
  REPO,
  defaultWorktreeContext,
  wt,
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
const fireContextChanged = (payload: unknown) =>
  fire("worktree-context-changed", payload);

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
// Presence + resting label (WTS-FR-01..03)
// ---------------------------------------------------------------------------

describe("WorktreeSelector presence and label (WTS-FR-02 / WTS-FR-03)", () => {
  // WTS-FR-01, WTS-FR-03.
  it("labels itself with the active worktree's branch", () => {
    renderSelector();
    expect(trigger()).toHaveTextContent("feature/new-window");
  });

  // WTS-FR-02: not a Git repository -> no selector anywhere in the chrome.
  it("renders nothing at all when there is no active worktree", () => {
    const { container } = renderSelector(null);
    expect(container).toBeEmptyDOMElement();
    expect(screen.queryByTestId("worktree-selector")).toBeNull();
  });

  // WTS-FR-03.
  it("names the detached state with the abbreviated commit id and no branch", () => {
    renderSelector(
      wt({ path: REPO, isDetached: true, headShortHash: "4f2a10c", isActive: true }),
    );
    expect(trigger()).toHaveTextContent("detached at 4f2a10c");
    expect(trigger()).not.toHaveTextContent("feature");
  });
});

// ---------------------------------------------------------------------------
// The dropdown (WTS-FR-04..12)
// ---------------------------------------------------------------------------

describe("WorktreeSelector dropdown (WTS-FR-04..12)", () => {
  // WTS-FR-04, WTS-FR-08.
  it("loads its contents in one round-trip and shows Worktrees above Branches", async () => {
    renderSelector();
    await openDropdown();

    expect(
      invokeMock.mock.calls.filter(
        (c) => c[0] === "list_worktrees_and_branches",
      ),
    ).toHaveLength(1);

    const menu = screen.getByTestId("worktree-selector-menu");
    const heads = within(menu)
      .getAllByText(/^(Worktrees|Branches)$/)
      .map((el) => el.textContent);
    expect(heads).toEqual(["Worktrees", "Branches"]);
  });

  // WTS-FR-05: the single-overlay invariant is enforced at the opening site.
  it("tells the shell to close the other overlays when it opens", async () => {
    const onOpen = vi.fn();
    renderSelector(ACTIVE, onOpen);
    await openDropdown();
    expect(onOpen).toHaveBeenCalledOnce();
  });

  // WTS-FR-06.
  it("focuses the filter input so a query can be typed without a second click", async () => {
    renderSelector();
    await openDropdown();
    const filter = screen.getByTestId("worktree-filter");
    expect(filter).toHaveFocus();
    await userEvent.keyboard("main");
    expect(filter).toHaveValue("main");
  });

  // WTS-FR-07: the filter narrows both groups, and a worktree matches on path.
  it("filters both groups at once, matching a worktree on its path too", async () => {
    renderSelector();
    await openDropdown();
    const filter = screen.getByTestId("worktree-filter");

    await userEvent.type(filter, "main");
    expect(screen.getByText("main")).toBeInTheDocument();
    expect(screen.queryByText("chore/deps")).toBeNull();

    await userEvent.clear(filter);
    await userEvent.type(filter, "acme-platform-main");
    expect(screen.getByText("/dev/acme-platform-main")).toBeInTheDocument();
    expect(screen.queryByText("chore/deps")).toBeNull();

    // Filtering is client-side over the already-loaded payload.
    expect(
      invokeMock.mock.calls.filter(
        (c) => c[0] === "list_worktrees_and_branches",
      ),
    ).toHaveLength(1);
  });

  // WTS-FR-09, WTS-FR-11.
  it("renders every worktree, flagging the detached and the missing one", async () => {
    renderSelector();
    await openDropdown();
    const group = screen.getByTestId("worktree-group");

    expect(within(group).getAllByRole("menuitem")).toHaveLength(4);
    expect(within(group).getByText("detached at 4f2a10c")).toBeInTheDocument();
    expect(within(group).getByText("missing")).toBeInTheDocument();
    expect(within(group).getByText("current")).toBeInTheDocument();
  });

  // WTS-FR-12.
  it("renders only branches without a worktree, flagging the remote one", async () => {
    renderSelector();
    await openDropdown();
    const group = screen.getByTestId("branch-group");

    expect(within(group).getByText("chore/deps")).toBeInTheDocument();
    expect(within(group).getByText("origin/experiment")).toBeInTheDocument();
    expect(within(group).getAllByText("remote")).toHaveLength(1);
    // `main` has a worktree, so the backend never lists it as a branch — and
    // the group renders exactly what it reports.
    expect(within(group).queryByText("main")).toBeNull();
  });

  // WTS-FR-26: bounded height, vertical scroll, never horizontal.
  it("caps each group's height and scrolls it vertically, never horizontally", async () => {
    renderSelector();
    await openDropdown();
    for (const id of ["worktree-group", "branch-group"]) {
      const group = screen.getByTestId(id);
      expect(group.style.maxHeight).toBe(
        `${WORKTREE_GROUP_MAX_ROWS * WORKTREE_ROW_PX}px`,
      );
      expect(group.style.overflowY).toBe("auto");
      expect(group.style.overflowX).toBe("hidden");
    }
  });
});

// ---------------------------------------------------------------------------
// Choosing a worktree (WTS-FR-10, WTS-FR-11, WTS-FR-13)
// ---------------------------------------------------------------------------

describe("WorktreeSelector worktree rows (WTS-FR-10 / WTS-FR-11 / WTS-FR-13)", () => {
  // WTS-FR-13, WTS-FR-22.
  it("activates a worktree that is neither current nor missing", async () => {
    renderSelector();
    await openDropdown();
    await userEvent.click(screen.getByTitle("/dev/acme-platform-main"));

    expect(onSwitch).toHaveBeenCalledOnce();
    expect(await invokedByHandedOperation()).toEqual([
      "activate_worktree",
      { path: "/dev/acme-platform-main" },
    ]);
  });

  // WTS-FR-10: the current row is inert and the dropdown stays open.
  it("invokes nothing when the current row is clicked", async () => {
    renderSelector();
    await openDropdown();
    // Scoped to the group: the chrome trigger carries the same path as its
    // title, and the current row is the one under test.
    const group = screen.getByTestId("worktree-group");
    await userEvent.click(within(group).getByTitle(REPO));

    expect(onSwitch).not.toHaveBeenCalled();
    expect(screen.getByTestId("worktree-selector-menu")).toBeInTheDocument();
  });

  // WTS-FR-11.
  it("invokes nothing when a missing row is clicked", async () => {
    renderSelector();
    await openDropdown();
    await userEvent.click(screen.getByTitle("/dev/acme-platform-release"));

    expect(onSwitch).not.toHaveBeenCalled();
    expect(screen.getByTestId("worktree-selector-menu")).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Choosing a branch (WTS-FR-14..16)
// ---------------------------------------------------------------------------

describe("WorktreeSelector branch rows (WTS-FR-14..16)", () => {
  // WTS-FR-14: selecting a branch invokes nothing; it reveals a choice.
  it("reveals an inline two-way choice rather than switching", async () => {
    renderSelector();
    await openDropdown();
    await userEvent.click(screen.getByText("fix/editor-scroll"));

    expect(onSwitch).not.toHaveBeenCalled();
    const choice = screen
      .getByRole("button", { name: "Check out here" })
      .closest("div")!;
    // Both halves of the choice sit on the row itself.
    expect(
      within(choice).getByRole("button", { name: "New worktree…" }),
    ).toBeInTheDocument();
    // And the dropdown's own action, outside both groups, is still there.
    expect(screen.getByTestId("worktree-new")).toBeInTheDocument();
  });

  it("dismisses the choice and leaves the dropdown open", async () => {
    renderSelector();
    await openDropdown();
    await userEvent.click(screen.getByText("fix/editor-scroll"));
    await userEvent.click(screen.getByText("fix/editor-scroll"));

    expect(screen.queryByRole("button", { name: "Check out here" })).toBeNull();
    expect(screen.getByTestId("worktree-selector-menu")).toBeInTheDocument();
  });

  // WTS-FR-15.
  it("checks the branch out in place when that half of the choice is taken", async () => {
    renderSelector();
    await openDropdown();
    await userEvent.click(screen.getByText("fix/editor-scroll"));
    await userEvent.click(screen.getByRole("button", { name: "Check out here" }));

    expect(await invokedByHandedOperation()).toEqual([
      "check_out_branch_in_active_worktree",
      { branch: "fix/editor-scroll" },
    ]);
  });

  // WTS-FR-05, WTS-FR-16, WTS-FR-18.
  it("opens the dialog pre-filled with the branch and its proposed location", async () => {
    renderSelector();
    await openDropdown();
    await userEvent.click(screen.getByText("fix/editor-scroll"));
    const choice = screen
      .getByRole("button", { name: "Check out here" })
      .closest("div")!;
    await userEvent.click(
      within(choice).getByRole("button", { name: "New worktree…" }),
    );

    expect(await screen.findByLabelText("Branch")).toHaveValue(
      "fix/editor-scroll",
    );
    await waitFor(() =>
      expect(screen.getByLabelText("Location")).toHaveValue(proposed),
    );
    expect(invokeMock).toHaveBeenCalledWith("propose_worktree_path", {
      branch: "fix/editor-scroll",
    });
    // WTS-FR-05: the dialog is a floating overlay of its own, so the dropdown
    // it was reached from is no longer mounted behind it.
    expect(screen.queryByTestId("worktree-selector-menu")).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// The New worktree dialog (WTS-FR-17..21)
// ---------------------------------------------------------------------------

describe("WorktreeSelector New worktree dialog (WTS-FR-17..21)", () => {
  const openDialog = async () => {
    await openDropdown();
    await userEvent.click(screen.getByTestId("worktree-new"));
    return screen.findByLabelText("Branch");
  };

  // WTS-FR-05, WTS-FR-17.
  it("opens with an empty Branch field from the dropdown's own action", async () => {
    renderSelector();
    expect(await openDialog()).toHaveValue("");
    expect(screen.queryByTestId("worktree-selector-menu")).toBeNull();
  });

  // WTS-FR-05, WTS-FR-17, second half: cancelling leaves neither overlay mounted — it
  // returns to the chrome, not to the dropdown.
  it("leaves neither overlay mounted when cancelled", async () => {
    renderSelector();
    await openDialog();
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));

    await waitFor(() => expect(screen.queryByLabelText("Branch")).toBeNull());
    expect(screen.queryByTestId("worktree-selector-menu")).toBeNull();
    // …and the control is still there to reopen.
    expect(trigger()).toBeInTheDocument();
  });

  it("leaves neither overlay mounted once the worktree is created", async () => {
    renderSelector();
    const branch = await openDialog();
    proposed = "/dev/acme-platform-spike";
    await userEvent.type(branch, "spike");
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Create" })).toBeEnabled(),
    );
    await userEvent.click(screen.getByRole("button", { name: "Create" }));

    await waitFor(() => expect(screen.queryByLabelText("Branch")).toBeNull());
    expect(screen.queryByTestId("worktree-selector-menu")).toBeNull();
  });

  // WTS-FR-19: re-derivation, and the moment it stops.
  it("re-derives the location while the branch is edited, then stops once overridden", async () => {
    renderSelector();
    const branch = await openDialog();
    const location = screen.getByLabelText("Location");

    proposed = "/dev/acme-platform-b";
    await userEvent.type(branch, "b");
    await waitFor(() => expect(location).toHaveValue("/dev/acme-platform-b"));

    // The user takes the location over…
    await userEvent.clear(location);
    await userEvent.type(location, "/elsewhere/mine");
    // …and a further branch edit leaves it exactly as they typed it.
    proposed = "/dev/acme-platform-bc";
    await userEvent.type(branch, "c");
    await waitFor(() => expect(branch).toHaveValue("bc"));
    expect(location).toHaveValue("/elsewhere/mine");
  });

  // WTS-FR-20, WTS-FR-19.
  it("writes a browsed folder into Location and freezes re-derivation", async () => {
    renderSelector();
    const branch = await openDialog();
    const location = screen.getByLabelText("Location");

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "browse_for_folder")
        return { selected: { path: "/picked/here" } };
      if (cmd === "propose_worktree_path") return proposed;
      return context;
    });
    await userEvent.click(screen.getByRole("button", { name: "Browse…" }));
    await waitFor(() => expect(location).toHaveValue("/picked/here"));

    proposed = "/dev/acme-platform-z";
    await userEvent.type(branch, "z");
    await waitFor(() => expect(branch).toHaveValue("z"));
    expect(location).toHaveValue("/picked/here");
  });

  it("leaves Location untouched when a browse is cancelled", async () => {
    renderSelector();
    await openDialog();
    const location = screen.getByLabelText("Location");
    await waitFor(() => expect(location).toHaveValue(proposed));

    await userEvent.click(screen.getByRole("button", { name: "Browse…" }));
    expect(location).toHaveValue(proposed);
  });

  // WTS-FR-21.
  it("enables Create only with both fields filled, and creates with them", async () => {
    renderSelector();
    const branch = await openDialog();
    const create = screen.getByRole("button", { name: "Create" });
    expect(create).toBeDisabled();

    proposed = "/dev/acme-platform-spike";
    await userEvent.type(branch, "spike");
    await waitFor(() => expect(create).toBeEnabled());
    await userEvent.click(create);

    expect(await invokedByHandedOperation()).toEqual([
      "create_worktree",
      { branch: "spike", path: "/dev/acme-platform-spike" },
    ]);
  });

  it("keeps Create disabled when the location is emptied", async () => {
    renderSelector();
    const branch = await openDialog();
    await userEvent.type(branch, "spike");
    await userEvent.clear(screen.getByLabelText("Location"));
    expect(screen.getByRole("button", { name: "Create" })).toBeDisabled();
  });
});

// ---------------------------------------------------------------------------
// Outcomes (WTS-FR-23, WTS-FR-24)
// ---------------------------------------------------------------------------

describe("WorktreeSelector switch outcomes (WTS-FR-23 / WTS-FR-24)", () => {
  // WTS-FR-23: a blocked flush cancels the switch — nothing is invoked, and
  // the selector shows no error, because the blocking tab is where the user
  // resolves it.
  it("closes quietly when the shell reports the flush cancelled the switch", async () => {
    outcome = { ok: false, cancelled: true };
    renderSelector();
    await openDropdown();
    await userEvent.click(screen.getByTitle("/dev/acme-platform-main"));

    await waitFor(() =>
      expect(screen.queryByTestId("worktree-selector-menu")).toBeNull(),
    );
    expect(screen.queryByTestId("worktree-error")).toBeNull();
    // The operation was handed over but the shell never ran it.
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "activate_worktree"),
    ).toHaveLength(0);
  });

  // WTS-FR-24: a typed refusal renders inline and the dropdown stays open.
  it("renders a typed refusal inline in the dropdown", async () => {
    outcome = { ok: false, error: "branch already checked out" };
    renderSelector();
    await openDropdown();
    await userEvent.click(screen.getByText("fix/editor-scroll"));
    await userEvent.click(screen.getByRole("button", { name: "Check out here" }));

    expect(await screen.findByTestId("worktree-error")).toHaveTextContent(
      "branch already checked out",
    );
    expect(screen.getByTestId("worktree-selector-menu")).toBeInTheDocument();
    // The control still names the worktree the project is on.
    expect(trigger()).toHaveTextContent("feature/new-window");
  });

  // WTS-FR-24: a refusal from the dialog renders inside the dialog, which
  // stays open with both values intact.
  it("renders a typed refusal inside the dialog, keeping both values", async () => {
    outcome = { ok: false, error: "worktree path exists" };
    renderSelector();
    await openDropdown();
    await userEvent.click(screen.getByTestId("worktree-new"));
    const branch = await screen.findByLabelText("Branch");
    proposed = "/dev/taken";
    await userEvent.type(branch, "spike");
    await waitFor(() =>
      expect(screen.getByLabelText("Location")).toHaveValue("/dev/taken"),
    );
    await userEvent.click(screen.getByRole("button", { name: "Create" }));

    expect(await screen.findByTestId("worktree-dialog-error")).toHaveTextContent(
      "worktree path exists",
    );
    expect(screen.getByLabelText("Branch")).toHaveValue("spike");
    expect(screen.getByLabelText("Location")).toHaveValue("/dev/taken");
  });

  it("surfaces a failed listing inline rather than replacing the dropdown", async () => {
    listError = "not a git repository";
    renderSelector();
    await userEvent.click(trigger());

    expect(await screen.findByTestId("worktree-error")).toHaveTextContent(
      "not a git repository",
    );
    expect(screen.getByTestId("worktree-selector-menu")).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Nothing destructive (WTS-FR-27)
// ---------------------------------------------------------------------------

describe("WorktreeSelector offers nothing destructive (WTS-FR-27)", () => {
  // WTS-FR-27.
  it("offers no control that removes a worktree, deletes a branch, or discards content", async () => {
    renderSelector();
    await openDropdown();
    await userEvent.click(screen.getByText("fix/editor-scroll"));
    await userEvent.click(screen.getByTestId("worktree-new"));
    await screen.findByLabelText("Branch");

    const labels = screen
      .getAllByRole("button")
      .map((b) => `${b.textContent ?? ""} ${b.getAttribute("aria-label") ?? ""}`)
      .join(" | ")
      .toLowerCase();
    for (const forbidden of [
      "remove",
      "delete",
      "discard",
      "prune",
      "force",
      "reset",
      "revert",
    ]) {
      expect(labels).not.toContain(forbidden);
    }
  });
});

// ---------------------------------------------------------------------------
// Following a checkout made elsewhere (WTS-FR-25)
// ---------------------------------------------------------------------------

describe("WorktreeSelector follows the context-changed event (WTS-FR-25)", () => {
  // WTS-FR-25, dropdown half: a branch checked out from the Git panel while
  // this dropdown is open re-renders its rows with the new `current` flag,
  // without the user reopening it.
  it("re-renders an open dropdown when the active worktree changes elsewhere", async () => {
    renderSelector();
    await openDropdown();
    expect(
      within(screen.getByTestId("worktree-group")).getByText("feature/new-window"),
    ).toBeInTheDocument();

    // The checkout happened elsewhere: the backend now reports `develop`.
    context = {
      ...context,
      worktrees: [
        wt({ path: REPO, branch: "develop", isActive: true, isPrimary: true }),
      ],
      branches: [
        { name: "feature/new-window", kind: "local", headShortHash: "ddd4444" },
      ],
    };
    await act(async () => {
      fireContextChanged({
        activeWorktreePath: REPO,
        branch: "develop",
        isDetached: false,
      });
    });

    await waitFor(() =>
      expect(
        within(screen.getByTestId("worktree-group")).getByText("develop"),
      ).toBeInTheDocument(),
    );
    const group = screen.getByTestId("worktree-group");
    expect(within(group).getByText("current")).toBeInTheDocument();
    expect(
      invokeMock.mock.calls.filter(
        (c) => c[0] === "list_worktrees_and_branches",
      ).length,
    ).toBeGreaterThan(1);
  });

  it("unsubscribes every channel when it unmounts", async () => {
    const { unmount } = renderSelector();
    // Both channels the selector follows: the active worktree changing, and the
    // branch set being re-read.
    await waitFor(() => expect(handlers.length).toBe(2));
    expect(handlers.map(([name]) => name).sort()).toEqual([
      "branches-changed",
      "worktree-context-changed",
    ]);
    unmount();
    await waitFor(() => expect(handlers.length).toBe(0));
  });
});

// ---------------------------------------------------------------------------
// A linked worktree offers no branches (WTS-FR-12 / WTS-FR-28)
// ---------------------------------------------------------------------------

describe("WorktreeSelector inside a linked worktree (WTS-FR-12 / WTS-FR-28)", () => {
  // WTS-FR-12, WTS-FR-28: the group is absent entirely rather than rendered inert.
  /** The context as the backend reports it from a linked worktree. */
  const linkedContext = () => ({
    ...context,
    activeWorktreePath: LINKED.path,
    worktrees: context.worktrees.map((w) => ({
      ...w,
      isActive: w.path === LINKED.path,
    })),
    // WTC-FR-06: nothing can be checked out from here, so nothing is offered.
    branches: [],
  });

  it("shows worktrees and New worktree… with no Branches group at all", async () => {
    // The group being absent is the two sides agreeing, not the UI hiding data
    // it was given.
    context = linkedContext();
    renderSelector(LINKED);
    await openDropdown();

    expect(screen.getByTestId("worktree-group")).toBeInTheDocument();
    expect(screen.queryByTestId("branch-group")).toBeNull();
    const menu = screen.getByTestId("worktree-selector-menu");
    expect(within(menu).queryByText("Branches")).toBeNull();
    expect(within(menu).getByText("Worktrees")).toBeInTheDocument();
    // Creating a worktree stays available — it adds a checkout rather than
    // moving this one onto another branch.
    expect(screen.getByTestId("worktree-new")).toBeInTheDocument();
  });

  // WTS-FR-12, WTS-FR-28, second half: the filter names what is actually on offer.
  it("labels the filter for worktrees alone", async () => {
    context = linkedContext();
    renderSelector(LINKED);
    await openDropdown();

    const filter = screen.getByTestId("worktree-filter");
    expect(filter).toHaveAttribute("aria-label", "Filter worktrees");
    // Sentence case with a trailing ellipsis, as every other filter placeholder
    // in the window reads.
    expect(filter).toHaveAttribute("placeholder", "Filter worktrees…");
  });

  it("offers no control that checks a branch out", async () => {
    context = linkedContext();
    renderSelector(LINKED);
    await openDropdown();

    expect(screen.queryByRole("button", { name: "Check out here" })).toBeNull();
    const labels = screen
      .getAllByRole("menuitem")
      .map((b) => b.textContent ?? "")
      .join(" | ")
      .toLowerCase();
    expect(labels).not.toContain("check out");
  });

  // Even if the backend were to report branches from a linked worktree, the
  // group stays absent — the rule is the UI's to hold, not a filter over data.
  it("stays absent even if the backend reports branches anyway", async () => {
    context = { ...linkedContext(), branches: context.branches };
    renderSelector(LINKED);
    await openDropdown();

    expect(screen.queryByTestId("branch-group")).toBeNull();
    expect(screen.queryByText("chore/deps")).toBeNull();
  });

  // WTS-FR-28: what is lost is moving *this* checkout onto another branch.
  it("still activates another worktree and still creates one", async () => {
    context = linkedContext();
    renderSelector(LINKED);
    await openDropdown();

    // Moving to the primary worktree.
    await userEvent.click(
      within(screen.getByTestId("worktree-group")).getByTitle(REPO),
    );
    expect(await invokedByHandedOperation()).toEqual([
      "activate_worktree",
      { path: REPO },
    ]);
  });

  it("still opens the New worktree dialog from a linked worktree", async () => {
    context = linkedContext();
    renderSelector(LINKED);
    await openDropdown();
    await userEvent.click(screen.getByTestId("worktree-new"));

    expect(await screen.findByLabelText("Branch")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Create" })).toBeInTheDocument();
  });

  // WTS-FR-12: the group comes back in the repository's own checkout.
  it("presents the Branches group again once the primary worktree is active", async () => {
    context = linkedContext();
    const { rerender } = renderSelector(LINKED);
    await openDropdown();
    expect(screen.queryByTestId("branch-group")).toBeNull();

    // Select the primary worktree, exactly as the user would…
    await userEvent.click(
      within(screen.getByTestId("worktree-group")).getByTitle(REPO),
    );
    await waitFor(() => expect(onSwitch).toHaveBeenCalled());
    // …and the shell hands the new active worktree back down.
    context = {
      repositoryRoot: REPO,
      activeWorktreePath: REPO,
      worktrees: context.worktrees,
      branches: [
        { name: "chore/deps", kind: "local", headShortHash: "aaa1111" },
      ],
    };
    rerender(
      <WorktreeSelector active={ACTIVE} onSwitch={onSwitch} onOpen={undefined} />,
    );
    await openDropdown();
    await waitFor(() =>
      expect(screen.getByTestId("branch-group")).toBeInTheDocument(),
    );
    expect(
      within(screen.getByTestId("branch-group")).getByText("chore/deps"),
    ).toBeInTheDocument();
    expect(screen.getByTestId("worktree-filter")).toHaveAttribute(
      "aria-label",
      "Filter worktrees and branches",
    );
  });
});

describe("WorktreeSelector on a detached primary worktree", () => {
  // The rule is about which checkout you are in, not what its HEAD points at —
  // and from a detached primary, a checkout is the way back onto a branch.
  it("still offers branches while the primary worktree's HEAD is detached", async () => {
    const detachedPrimary = wt({
      path: REPO,
      isDetached: true,
      headShortHash: "4f2a10c",
      isActive: true,
      isPrimary: true,
    });
    renderSelector(detachedPrimary);
    expect(trigger()).toHaveTextContent("detached at 4f2a10c");

    await openDropdown();
    expect(screen.getByTestId("branch-group")).toBeInTheDocument();
    expect(
      within(screen.getByTestId("branch-group")).getByText("chore/deps"),
    ).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// The proposed location (WTS-FR-18 / WTS-FR-19, per WTC-FR-13)
// ---------------------------------------------------------------------------

describe("WorktreeSelector proposed location (WTS-FR-19, WTC-FR-13)", () => {
  // WTS-FR-19, WTC-FR-13: the branch's namespace is dropped from the directory name. The
  // derivation is the backend's (WTC-FR-13) — what is pinned here is that the
  // UI hands over the branch *whole* and shows what comes back, rather than
  // trimming or deriving anything itself.
  it("asks for the proposal with the full branch and shows what comes back", async () => {
    renderSelector();
    await openDropdown();
    await userEvent.click(screen.getByTestId("worktree-new"));
    const branch = await screen.findByLabelText("Branch");

    proposed = "/dev/acme-platform-editor-scroll";
    await userEvent.clear(branch);
    await userEvent.type(branch, "feature/PROJ-12345/editor-scroll");

    await waitFor(() =>
      expect(screen.getByLabelText("Location")).toHaveValue(
        "/dev/acme-platform-editor-scroll",
      ),
    );
    expect(invokeMock).toHaveBeenCalledWith("propose_worktree_path", {
      branch: "feature/PROJ-12345/editor-scroll",
    });
    // The namespace is gone from the location but untouched in the branch.
    expect(branch).toHaveValue("feature/PROJ-12345/editor-scroll");
  });
});

// ---------------------------------------------------------------------------
// Work stream worktrees (WTS-FR-ROMD) and direct graduation (WTS-FR-BQCI)
// ---------------------------------------------------------------------------

describe("WorktreeSelector and work streams (WTS-FR-ROMD / WTS-FR-BQCI)", () => {
  const streamWorktree = (over: Partial<WorktreeEntry> = {}) =>
    wt({
      path: "/short/w/s1",
      branch: "synthesis/stream/editor-work",
      stream: { streamId: "s1", streamName: "Editor work" },
      ...over,
    });

  // WTS-FR-ROMD: absent from the list, the filter results and the count.
  it("hides a work stream's working copy from the dropdown, its filter and its count", async () => {
    context = defaultWorktreeContext();
    context.worktrees = [...context.worktrees, streamWorktree()];
    renderSelector();
    await openDropdown();

    const group = screen.getByTestId("worktree-group");
    expect(within(group).queryByText("synthesis/stream/editor-work")).toBeNull();
    expect(within(group).queryByText("/short/w/s1")).toBeNull();
    const listed = within(group).getAllByRole("menuitem").length;
    expect(listed).toBe(defaultWorktreeContext().worktrees.length);

    // The filter finds nothing of a stream's working copy either.
    await userEvent.type(screen.getByTestId("worktree-filter"), "editor-work");
    expect(screen.queryByText("synthesis/stream/editor-work")).toBeNull();
    expect(screen.getByText("No worktrees.")).toBeInTheDocument();
  });

  // WTS-FR-ROMD: whether or not it is the active worktree.
  it("hides a stream's working copy even while it is the active worktree", async () => {
    const active = streamWorktree({ isActive: true });
    context = defaultWorktreeContext();
    context.worktrees = [
      ...context.worktrees.map((w) => ({ ...w, isActive: false })),
      active,
    ];
    context.activeWorktreePath = active.path;
    renderSelector(active);
    await openDropdown();

    expect(trigger()).toHaveTextContent("synthesis/stream/editor-work");
    const group = screen.getByTestId("worktree-group");
    expect(within(group).queryByText("current")).toBeNull();
    expect(within(group).queryByText("/short/w/s1")).toBeNull();
  });

  // WTS-FR-ROMD: ordinary worktrees keep their behaviour.
  it("keeps every other worktree and its behaviour", async () => {
    context = defaultWorktreeContext();
    context.worktrees = [...context.worktrees, streamWorktree()];
    renderSelector();
    await openDropdown();
    await userEvent.click(screen.getByText("main"));
    const [command, args] = await invokedByHandedOperation();
    expect(command).toBe("activate_worktree");
    expect(args).toEqual({ path: "/dev/acme-platform-main" });
  });

  // WTS-FR-BQCI: said in words, inline, and the dropdown stays open.
  it("renders a switch refused by a direct graduation run in words", async () => {
    outcome = { ok: false, error: "direct graduation active: g1" };
    renderSelector();
    await openDropdown();
    await userEvent.click(screen.getByText("main"));

    const error = await screen.findByTestId("worktree-error");
    expect(error).toHaveTextContent(/graduation run is working/);
    expect(error).toHaveTextContent(/waits until the run ends/);
    expect(error.textContent).not.toContain("direct graduation active");
    expect(screen.getByTestId("worktree-selector-menu")).toBeInTheDocument();
    expect(trigger()).toHaveTextContent("feature/new-window");
  });
});
