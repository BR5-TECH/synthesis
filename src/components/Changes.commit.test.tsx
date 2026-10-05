import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type {
  CommitOutcome,
  UpstreamSyncState,
} from "../types";
import { CHANGES_UPDATED, GIT_OPERATION_FINISHED } from "../events";
import { resetAppPreferencesCache } from "../state/appPreferences";
import { resetPanelReveals } from "../state/panelReveal";
import {
  selectorButton,
} from "../test/selectors";
import {
  HOOK,
  LIB,
  change,
  changeSet,
  checkbox,
  chooseAction,
  makeBackend,
  onOpenGlobalSettings,
  onRequestCommitMessage,
  onRequestGithubToken,
  primary,
  renderPanel,
  resetHandlers,
  rowNames,
  showAllFiles,
} from "../test/changesFixtures";

const invokeMock = vi.fn();
const unlistenMock = vi.fn();
let listeners: Record<string, (event: { payload: unknown }) => void> = {};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, cb: (event: { payload: unknown }) => void) => {
    // The panel and the window's push state (GIT-FR-QMYB) both follow these
    // events, so a delivery reaches every handler registered for the name.
    const previous = listeners[event];
    listeners[event] = previous
      ? (ev) => {
          previous(ev);
          cb(ev);
        }
      : cb;
    return unlistenMock;
  }),
}));

const backend = makeBackend(invokeMock);

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

beforeEach(() => {
  // Module-level, like the preferences cache: reveal nonces are minted from
  // one counter for the app's life, so a suite that does not reset finds its
  // first request already consumed by the previous one.
  resetPanelReveals();
  invokeMock.mockReset();
  unlistenMock.mockReset();
  // The preferences record is a module-level cache shared across the process;
  // a test that leaves a stored action behind would seed the next one.
  resetAppPreferencesCache();
  resetHandlers();
  listeners = {};
  backend();
});

afterEach(cleanup);

// ---------------------------------------------------------------------------
// The footer action control (CHG-FR-33, CHG-FR-34 .. CHG-FR-38)
// ---------------------------------------------------------------------------

describe("the footer action control", () => {
  it("CHG-FR-33, CHG-FR-34 persists the selected action user-globally", async () => {
    backend({ uncommitted: changeSet([LIB]) });
    renderPanel();
    await screen.findByText(/files selected/);
    expect(primary()).toHaveTextContent("Commit");

    await chooseAction("Commit & Push");

    expect(primary()).toHaveTextContent("Commit & Push");
    await waitFor(() => expect(calls("save_app_preferences")).toHaveLength(1));
    expect(
      (calls("save_app_preferences")[0][1] as { preferences: { changesCommitAction: string } })
        .preferences.changesCommitAction,
    ).toBe("commit_and_push");
  });

  it("CHG-FR-33, CHG-FR-34 reads the stored action on mount, whichever project is open", async () => {
    backend({
      uncommitted: changeSet([LIB]),
      prefs: { changesCommitAction: "commit_and_push" },
    });
    renderPanel();
    await waitFor(() => expect(primary()).toHaveTextContent("Commit & Push"));
  });

  it("CHG-FR-35, CHG-FR-36, CHG-FR-37 keeps Commit selectable while nothing is ticked, and Push not while nothing is pushable", async () => {
    // A clean working tree with a branch level with its upstream: nothing can
    // be performed. Commit and Commit & Push stay selectable because ticking is
    // how the author makes them available; Push does not, because nothing the
    // author does here makes a level branch pushable.
    backend({ uncommitted: changeSet([]) });
    renderPanel();
    await screen.findByText("No changes.");
    expect(primary()).toBeDisabled();

    await userEvent.click(screen.getByRole("button", { name: "Choose action" }));
    const menu = screen.getByRole("listbox", { name: "Commit action" });
    const options = within(menu).getAllByRole("option");
    expect(options).toHaveLength(3);

    const push = options[2];
    expect(push).toHaveAttribute("aria-disabled", "true");
    expect(push).toHaveAttribute("tabindex", "-1");
    // The reason lives on hover and nowhere else: the entry is labelled with
    // the action's name alone, so the dropdown reads the same whatever the
    // repository can do at this moment.
    expect(push).toHaveTextContent(/^Push$/);
    expect(push).toHaveAttribute(
      "title",
      "Nothing to push — the branch is level with its remote.",
    );
    for (const selectable of [options[0], options[1]]) {
      expect(selectable).not.toHaveAttribute("aria-disabled");
    }

    // Activating it persists nothing and changes nothing.
    await userEvent.click(push);
    expect(calls("save_app_preferences")).toHaveLength(0);
    expect(primary()).toHaveTextContent("Commit");

    // CHG-FR-35: an unavailable Commit is still the author's to select, so they
    // can set their habit before ticking anything.
    await userEvent.click(within(menu).getByRole("option", { name: "Commit & Push" }));
    expect(primary()).toHaveTextContent("Commit & Push");
    await waitFor(() =>
      expect(
        (calls("save_app_preferences")[0][1] as {
          preferences: { changesCommitAction: string };
        }).preferences.changesCommitAction,
      ).toBe("commit_and_push"),
    );
  });

  it("CHG-FR-35, CHG-FR-36, CHG-FR-37 makes Push selectable once the branch has something to publish", async () => {
    backend({
      uncommitted: changeSet([]),
      sync: { hasRemote: true, hasUpstream: true, ahead: 2, behind: 0 },
    });
    renderPanel();
    await screen.findByText("No changes.");

    await chooseAction("Push");

    expect(primary()).toHaveTextContent("Push");
    await waitFor(() => expect(primary()).toBeEnabled());
    await waitFor(() =>
      expect(
        (calls("save_app_preferences")[0][1] as {
          preferences: { changesCommitAction: string };
        }).preferences.changesCommitAction,
      ).toBe("push"),
    );
  });

  it("CHG-FR-35 keeps a persisted Push selected while it is unavailable", async () => {
    // GSS-FR-25: the stored action is the author's habit and is not silently
    // replaced; only the primary button is disabled.
    backend({
      uncommitted: changeSet([]),
      sync: { hasRemote: true, hasUpstream: true, ahead: 0, behind: 0 },
      prefs: { changesCommitAction: "push" },
    });
    renderPanel();

    await waitFor(() => expect(primary()).toHaveTextContent("Push"));
    expect(primary()).toBeDisabled();
    // Nothing was written to replace it.
    expect(calls("save_app_preferences")).toHaveLength(0);

    await userEvent.click(screen.getByRole("button", { name: "Choose action" }));
    const menu = screen.getByRole("listbox", { name: "Commit action" });
    const push = within(menu).getAllByRole("option")[2];
    expect(push).toHaveAttribute("aria-disabled", "true");
    // Still marked as the selected one, disabled or not.
    expect(push).toHaveAttribute("aria-selected", "true");
  });

  it("CHG-FR-36 enables Commit only in Uncommitted mode with something ticked", async () => {
    backend({ uncommitted: changeSet([LIB]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");
    expect(primary()).toBeDisabled();

    await userEvent.click(checkbox("Library.tsx"));
    expect(primary()).toBeEnabled();

    await userEvent.click(screen.getByRole("radio", { name: "Branch" }));
    await screen.findByText("CHG-changes.md");
    expect(primary()).toBeDisabled();
  });

  it("CHG-FR-49, CHG-FR-35, CHG-FR-36 gives both halves of the split control one appearance", async () => {
    backend({ uncommitted: changeSet([LIB]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");

    const split = document.querySelector(".changes-actions__split")!;
    const caret = screen.getByRole("button", { name: "Choose action" });

    // Nothing ticked: Commit cannot be performed, so the whole control — the
    // dropdown half included — carries the unavailable treatment.
    expect(primary()).toBeDisabled();
    expect(split).toHaveAttribute("data-unavailable", "true");

    // …but the dropdown stays operable, because changing the action is exactly
    // what an author does when the current one cannot be performed (CHG-FR-35).
    expect(caret).toBeEnabled();
    await userEvent.click(caret);
    expect(
      screen.getByRole("listbox", { name: "Commit action" }),
    ).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");

    // Ticking a file returns both halves to the available treatment together.
    await userEvent.click(checkbox("Library.tsx"));
    expect(primary()).toBeEnabled();
    expect(split).toHaveAttribute("data-unavailable", "false");
  });

  // The arm that proves the treatment tracks the SELECTED ACTION rather than the
  // commit set: Push is available from the branch's standing against its
  // upstream (CHG-FR-37) and is indifferent to the checkboxes, so the control
  // reads available with nothing ticked at all.
  it("CHG-FR-49, CHG-FR-35, CHG-FR-36 follows the selected action, not the commit set", async () => {
    backend({
      uncommitted: changeSet([LIB]),
      sync: { hasRemote: true, hasUpstream: true, ahead: 2, behind: 0 },
      prefs: { changesCommitAction: "push" },
    });
    renderPanel();
    await showAllFiles();
    await waitFor(() => expect(primary()).toHaveTextContent("Push"));

    const split = document.querySelector(".changes-actions__split")!;
    // Nothing is ticked, yet Push can be performed — so the control is available.
    expect(checkbox("Library.tsx")).not.toBeChecked();
    await waitFor(() => expect(primary()).toBeEnabled());
    expect(split).toHaveAttribute("data-unavailable", "false");
  });

  // CHG-FR-36 names Branch mode explicitly: a commit needs Uncommitted, so the
  // whole control goes unavailable there however many files are ticked.
  it("CHG-FR-49, CHG-FR-35, CHG-FR-36 marks the control unavailable in Branch mode", async () => {
    backend({ uncommitted: changeSet([LIB]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");

    const split = document.querySelector(".changes-actions__split")!;
    await userEvent.click(checkbox("Library.tsx"));
    expect(split).toHaveAttribute("data-unavailable", "false");

    await userEvent.click(screen.getByRole("radio", { name: "Branch" }));
    await screen.findByText("CHG-changes.md");
    expect(primary()).toBeDisabled();
    expect(split).toHaveAttribute("data-unavailable", "true");
  });

  it("CHG-FR-37 enables Push from the branch's standing against its upstream", async () => {
    const cases: [Partial<UpstreamSyncState>, boolean][] = [
      [{ hasRemote: true, hasUpstream: true, ahead: 0, behind: 0 }, false],
      [{ hasRemote: true, hasUpstream: true, ahead: 2, behind: 0 }, true],
      [{ hasRemote: true, hasUpstream: false, ahead: null, behind: null }, true],
      [{ hasRemote: false, hasUpstream: false, ahead: null, behind: null }, false],
    ];
    for (const [sync, enabled] of cases) {
      cleanup();
      resetAppPreferencesCache();
      backend({
        uncommitted: changeSet([LIB]),
        sync: sync as UpstreamSyncState,
        prefs: { changesCommitAction: "push" },
      });
      renderPanel();
      await waitFor(() => expect(primary()).toHaveTextContent("Push"));
      await waitFor(() =>
        enabled
          ? expect(primary()).toBeEnabled()
          : expect(primary()).toBeDisabled(),
      );
    }
  });

  it("CHG-FR-38 re-reads the sync state when the change set updates", async () => {
    let sync: UpstreamSyncState = {
      hasRemote: true,
      hasUpstream: true,
      ahead: 0,
      behind: 0,
    };
    backend({
      uncommitted: changeSet([LIB]),
      sync: () => sync,
      prefs: { changesCommitAction: "push" },
    });
    renderPanel();
    await waitFor(() => expect(primary()).toBeDisabled());

    // A commit lands elsewhere; the branch is now ahead.
    sync = { hasRemote: true, hasUpstream: true, ahead: 1, behind: 0 };
    await act(async () => {
      listeners[CHANGES_UPDATED]?.({ payload: { changeCount: 1 } });
    });
    await waitFor(() => expect(primary()).toBeEnabled());
  });
});

// ---------------------------------------------------------------------------
// Committing and pushing (CHG-FR-39 .. CHG-FR-46)
// ---------------------------------------------------------------------------

describe("committing and pushing", () => {
  /** Tick `Library.tsx` under the All-files lens and return once it is ticked. */
  async function tickLibrary() {
    await showAllFiles();
    await screen.findByText("Library.tsx");
    await userEvent.click(checkbox("Library.tsx"));
  }

  it("CHG-FR-39 opens the commit window with the commit set and commits nothing until it is confirmed", async () => {
    backend({ uncommitted: changeSet([LIB, HOOK]) });
    onRequestCommitMessage.mockResolvedValue(null); // dismissed
    renderPanel();
    await tickLibrary();
    await userEvent.click(checkbox("useEditHistory.ts"));

    await userEvent.click(primary());

    await waitFor(() => expect(onRequestCommitMessage).toHaveBeenCalledOnce());
    expect(onRequestCommitMessage.mock.calls[0][0]).toEqual([
      { path: LIB.path, untracked: false },
      { path: HOOK.path, untracked: false },
    ]);
    // Dismissed: nothing was committed and both rows are still ticked.
    expect(calls("commit_paths")).toHaveLength(0);
    await waitFor(() => expect(checkbox("Library.tsx").checked).toBe(true));
    expect(checkbox("useEditHistory.ts").checked).toBe(true);
  });

  it("CHG-FR-40 hands the window an untracked entry marked as such", async () => {
    const fresh = change("src/new.ts", { changeStatus: "untracked" });
    backend({ uncommitted: changeSet([LIB, fresh]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("new.ts");
    await userEvent.click(checkbox("new.ts"));
    await userEvent.click(checkbox("Library.tsx"));

    await userEvent.click(primary());

    await waitFor(() => expect(onRequestCommitMessage).toHaveBeenCalled());
    expect(onRequestCommitMessage.mock.calls[0][0]).toEqual(
      expect.arrayContaining([
        { path: fresh.path, untracked: true },
        { path: LIB.path, untracked: false },
      ]),
    );
  });

  it("CHG-FR-41, TAB-FR-22 clears every check once the commit succeeded", async () => {
    // Both rows are ticked and BOTH survive the reload, so the clearing is the
    // only thing that can unticket them — pruning an entry that left the change
    // set (CHG-FR-31) would prove nothing here.
    backend({ uncommitted: changeSet([LIB, HOOK]) });
    renderPanel();
    await tickLibrary();
    await userEvent.click(checkbox("useEditHistory.ts"));
    expect(screen.getByText(/2 files selected/)).toBeInTheDocument();

    await userEvent.click(primary());
    await waitFor(() => expect(onRequestCommitMessage).toHaveBeenCalled());

    await waitFor(() => expect(checkbox("Library.tsx").checked).toBe(false));
    expect(checkbox("useEditHistory.ts").checked).toBe(false);
    expect(screen.getByText(/0 files selected/)).toBeInTheDocument();

    // And the entries the commit really did take are gone once the
    // `"changes updated"` it produced lands.
    backend({ uncommitted: changeSet([HOOK]) });
    await act(async () => {
      listeners[CHANGES_UPDATED]?.({ payload: { changeCount: 1 } });
    });
    await waitFor(() => expect(rowNames()).not.toContain("Library.tsx"));
  });

  it("CHG-FR-41, TAB-FR-22 leaves every check in place when the commit was rejected", async () => {
    backend({ uncommitted: changeSet([LIB]) });
    // The window stays open with its message on a rejection (CMW-FR-08), so it
    // never reports a commit; from the panel's side that is the same answer as
    // a dismissal — nothing was committed, so nothing changes here.
    onRequestCommitMessage.mockResolvedValue(null);
    renderPanel();
    await tickLibrary();

    await userEvent.click(primary());
    await waitFor(() => expect(onRequestCommitMessage).toHaveBeenCalled());
    await waitFor(() => expect(checkbox("Library.tsx").checked).toBe(true));
  });

  it("CHG-FR-42 pushes after a successful commit and not after a rejected one", async () => {
    backend({
      uncommitted: changeSet([LIB]),
      prefs: { changesCommitAction: "commit_and_push" },
    });
    renderPanel();
    await waitFor(() => expect(primary()).toHaveTextContent("Commit & Push"));
    await tickLibrary();

    await userEvent.click(primary());
    await waitFor(() => expect(calls("push_current_branch")).toHaveLength(1));

    // Now a commit that never happens: no push follows it.
    onRequestCommitMessage.mockResolvedValue(null);
    await userEvent.click(checkbox("Library.tsx"));
    await userEvent.click(primary());
    await waitFor(() => expect(onRequestCommitMessage).toHaveBeenCalledTimes(2));
    expect(calls("push_current_branch")).toHaveLength(1);
  });

  it("CHG-FR-43 invokes the push and renders only its outcome, with no output area", async () => {
    backend({
      uncommitted: changeSet([LIB]),
      sync: { hasRemote: true, hasUpstream: true, ahead: 2, behind: 0 },
      prefs: { changesCommitAction: "push" },
    });
    const { container } = renderPanel();
    await waitFor(() => expect(primary()).toBeEnabled());

    await userEvent.click(primary());
    await waitFor(() => expect(calls("push_current_branch")).toHaveLength(1));

    // The terminal status arrives on the event, not from the invoke's return.
    await act(async () => {
      listeners[GIT_OPERATION_FINISHED]?.({
        payload: { operation: "push", ok: true },
      });
    });
    expect(await screen.findByText("Push complete.")).toBeInTheDocument();
    // CHG-FR-43: the panel hosts no transcript of its own.
    expect(container.querySelector(".git__output")).toBeNull();
  });

  it("CHG-FR-44 disables the primary button while a commit or a push is in flight", async () => {
    backend({ uncommitted: changeSet([LIB]) });
    let release!: (outcome: CommitOutcome | null) => void;
    onRequestCommitMessage.mockImplementation(
      () => new Promise<CommitOutcome | null>((resolve) => (release = resolve)),
    );
    renderPanel();
    await tickLibrary();

    await userEvent.click(primary());
    await waitFor(() => expect(primary()).toBeDisabled());
    // A second activation while the window is open submits nothing.
    await userEvent.click(primary());
    expect(onRequestCommitMessage).toHaveBeenCalledOnce();

    await act(async () => {
      release(null);
    });
    await waitFor(() => expect(primary()).toBeEnabled());
  });

  it("CHG-FR-45 opens the token picker when the project has not been pointed at a token", async () => {
    let attempts = 0;
    backend({
      uncommitted: changeSet([LIB]),
      sync: { hasRemote: true, hasUpstream: false, ahead: null, behind: null },
      prefs: { changesCommitAction: "push" },
    });
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "push_current_branch") {
        attempts += 1;
        if (attempts === 1) throw "github_token_selection_required";
        return undefined;
      }
      if (cmd === "load_changes_panel_state") return { mode: "uncommitted" };
      if (cmd === "get_default_branch") return "main";
      if (cmd === "list_uncommitted_changes") return changeSet([LIB]);
      if (cmd === "get_upstream_sync_state")
        return { hasRemote: true, hasUpstream: false, ahead: null, behind: null };
      if (cmd === "load_app_preferences")
        return { theme: "system", changesCommitAction: "push" };
      return undefined;
    });
    renderPanel();
    await waitFor(() => expect(primary()).toBeEnabled());

    await userEvent.click(primary());

    await waitFor(() => expect(onRequestGithubToken).toHaveBeenCalledOnce());
    // Confirmed, so the push ran on the retry.
    await waitFor(() => expect(attempts).toBe(2));
  });

  it("CHG-FR-45 abandons the push when the picker is cancelled", async () => {
    onRequestGithubToken.mockResolvedValue(false);
    backend({
      uncommitted: changeSet([LIB]),
      sync: { hasRemote: true, hasUpstream: false, ahead: null, behind: null },
      prefs: { changesCommitAction: "push" },
      fail: { push_current_branch: "github_token_selection_required" },
    });
    renderPanel();
    await waitFor(() => expect(primary()).toBeEnabled());

    await userEvent.click(primary());

    expect(
      await screen.findByText(/no GitHub token was selected/),
    ).toBeInTheDocument();
    expect(calls("push_current_branch")).toHaveLength(1);
  });

  it("CHG-FR-45 routes to Global settings when no token is stored at all", async () => {
    backend({
      uncommitted: changeSet([LIB]),
      sync: { hasRemote: true, hasUpstream: false, ahead: null, behind: null },
      prefs: { changesCommitAction: "push" },
      fail: { push_current_branch: "github_token_missing" },
    });
    renderPanel();
    await waitFor(() => expect(primary()).toBeEnabled());

    await userEvent.click(primary());

    expect(await screen.findByText(/needs a GitHub token/)).toBeInTheDocument();
    expect(onRequestGithubToken).not.toHaveBeenCalled();
    await userEvent.click(
      screen.getByRole("button", { name: /Global settings/ }),
    );
    expect(onOpenGlobalSettings).toHaveBeenCalled();
  });

  it("CHG-FR-46 leaves a successful commit standing when the push that followed failed", async () => {
    let entries = [LIB];
    backend({
      uncommitted: () => changeSet(entries),
      prefs: { changesCommitAction: "commit_and_push" },
      fail: { push_current_branch: "remote rejected the push" },
      sync: { hasRemote: true, hasUpstream: true, ahead: 1, behind: 0 },
    });
    renderPanel();
    await waitFor(() => expect(primary()).toHaveTextContent("Commit & Push"));
    await tickLibrary();

    await userEvent.click(primary());
    await waitFor(() => expect(onRequestCommitMessage).toHaveBeenCalled());
    expect(await screen.findByText(/Push failed/)).toBeInTheDocument();

    // The commit stands: its entry is gone from the uncommitted set, and the
    // author retries with Push, which is now available.
    entries = [];
    await act(async () => {
      listeners[CHANGES_UPDATED]?.({ payload: { changeCount: 0 } });
    });
    await screen.findByText("No changes.");
    await chooseAction("Push");
    await waitFor(() => expect(primary()).toBeEnabled());
    await userEvent.click(primary());
    await waitFor(() => expect(calls("push_current_branch")).toHaveLength(2));
  });

  it("CHG-FR-22, CHG-FR-47, CHG-FR-56, SNV-FR-60 renders no checkbox and no action control outside a repository", async () => {
    backend({ fail: { list_uncommitted_changes: "not a git repository" } });
    const { container } = renderPanel();

    await screen.findByText("Not a Git repository.");
    expect(container.querySelectorAll("input[type=checkbox]")).toHaveLength(0);
    expect(container.querySelector(".changes-actions")).toBeNull();
    // Neither group labels an empty tree in place of the explanation.
    expect(rowNames()).toEqual([]);
  });
});

// ---------------------------------------------------------------------------
// The action dropdown dismisses like every other menu in the shell (CHG-FR-33)
// ---------------------------------------------------------------------------

describe("dismissing the action dropdown", () => {
  const menu = () => screen.queryByRole("listbox", { name: "Commit action" });

  async function openMenu() {
    await userEvent.click(screen.getByRole("button", { name: "Choose action" }));
    expect(menu()).toBeInTheDocument();
  }

  it("closes on a click outside it", async () => {
    backend({ uncommitted: changeSet([LIB]) });
    renderPanel();
    await screen.findByText(/files selected/);
    await openMenu();

    // Anywhere that is not the split control or the menu itself.
    await userEvent.click(screen.getByLabelText("Filter changes"));
    expect(menu()).toBeNull();
  });

  it("CHG-FR-33: an entry is chosen from the keyboard alone", async () => {
    // The entries are options rather than buttons, so Enter and Space are the
    // only keyboard route to changing the action.
    backend({ uncommitted: changeSet([LIB]) });
    renderPanel();
    await screen.findByText(/files selected/);
    await openMenu();

    const push = within(menu()!).getByRole("option", { name: "Commit & Push" });
    push.focus();
    await userEvent.keyboard("{Enter}");
    expect(menu()).toBeNull();
    await waitFor(() => expect(primary()).toHaveTextContent("Commit & Push"));
  });

  it("CHG-FR-35: an inactive entry is inert from the keyboard too", async () => {
    // Nothing done in this panel makes a branch level with its remote pushable,
    // so **Push** renders inactive with the reason — and stays inactive however
    // it is reached.
    backend({
      uncommitted: changeSet([LIB]),
      sync: { hasRemote: true, hasUpstream: true, ahead: 0, behind: 0 },
    });
    renderPanel();
    await screen.findByText(/files selected/);
    await openMenu();

    const push = within(menu()!).getByRole("option", { name: "Push" });
    expect(push).toHaveAttribute("aria-disabled", "true");
    push.focus();
    await userEvent.keyboard("{Enter}");
    await userEvent.keyboard(" ");
    expect(menu()).toBeInTheDocument();
    expect(primary()).toHaveTextContent("Commit");
  });

  it("closes when another selector in the panel is opened", async () => {
    // Two menus standing open at once is the thing the outside-click guard
    // exists to prevent.
    backend({ uncommitted: changeSet([LIB]) });
    renderPanel();
    await screen.findByText(/files selected/);
    await openMenu();

    await userEvent.click(selectorButton("Filter by type", "artifacts"));
    expect(menu()).toBeNull();
  });

  it("closes on Escape and when the window loses focus", async () => {
    backend({ uncommitted: changeSet([LIB]) });
    renderPanel();
    await screen.findByText(/files selected/);

    await openMenu();
    await userEvent.keyboard("{Escape}");
    expect(menu()).toBeNull();

    await openMenu();
    await act(async () => {
      window.dispatchEvent(new Event("blur"));
    });
    expect(menu()).toBeNull();
  });

  it("stays open for a click on its own trigger or inside the menu", async () => {
    backend({
      uncommitted: changeSet([LIB]),
      sync: { hasRemote: true, hasUpstream: true, ahead: 2, behind: 0 },
    });
    renderPanel();
    await screen.findByText(/files selected/);
    await openMenu();

    // The inactive entry is inside the menu: activating it changes nothing and
    // must not be mistaken for an outside click either.
    await userEvent.click(
      within(menu()!).getByRole("option", { name: "Commit & Push" }),
    );
    // Choosing an action closes it, which is the menu's own doing.
    expect(menu()).toBeNull();

    // And the trigger toggles rather than reopening under the outside-click
    // handler.
    await openMenu();
    await userEvent.click(screen.getByRole("button", { name: "Choose action" }));
    expect(menu()).toBeNull();
  });
});
