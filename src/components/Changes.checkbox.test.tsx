import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type {
  UpstreamSyncState,
} from "../types";
import { CHANGES_UPDATED, GIT_OPERATION_FINISHED } from "../events";
import { resetAppPreferencesCache } from "../state/appPreferences";
import { resetPanelReveals } from "../state/panelReveal";
import {
  pickSelector,
} from "../test/selectors";
import {
  HOOK,
  LIB,
  SKILL,
  change,
  changeSet,
  checkbox,
  makeBackend,
  onOpenDiff,
  onRequestCommitMessage,
  onRequestGithubToken,
  primary,
  renderPanel,
  resetHandlers,
  row,
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
// The checkbox column (CHG-FR-26, CHG-FR-09 .. CHG-FR-32)
// ---------------------------------------------------------------------------

describe("the checkbox column", () => {
  it("CHG-FR-26, CHG-FR-09 carries a checkbox on every row and group in Uncommitted mode and none in Branch mode", async () => {
    const fresh = change("docs/draft.md", { changeStatus: "untracked" });
    backend({ uncommitted: changeSet([LIB, HOOK, fresh]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");

    // Every row, without exception — both groups, every folder, every file.
    expect(document.querySelectorAll(".tree-row input[type=checkbox]")).toHaveLength(
      document.querySelectorAll(".tree-row").length,
    );
    for (const name of ["Revisioned", "Unrevisioned"]) {
      expect(
        row(name).querySelector("input[type=checkbox]"),
      ).toBeInTheDocument();
    }

    await userEvent.click(screen.getByRole("radio", { name: "Branch" }));
    await screen.findByText("CHG-changes.md");
    expect(
      document.querySelectorAll(".tree-row input[type=checkbox]"),
    ).toHaveLength(0);
    // The grouping is structural, so it stands in Branch mode too — labelling
    // the entries nested beneath it, with no checkbox on anything
    // (CHG-FR-09 / CHG-FR-26).
    expect(rowNames()).toEqual([
      "Revisioned",
      "specifications",
      "ui",
      "CHG-changes.md",
    ]);
  });

  it("CHG-FR-27, CHG-FR-28 commits only what the filters leave visible under a checked folder", async () => {
    // The default lens hides `Library.tsx`; checking `src` must not drag it in.
    backend({ uncommitted: changeSet([SKILL, LIB]) });
    renderPanel();
    await screen.findByText("onboarding.md");
    // Only the classified file and its folders are on screen.
    expect(rowNames()).not.toContain("Library.tsx");

    await userEvent.click(checkbox(".claude"));
    await userEvent.click(primary());

    await waitFor(() => expect(onRequestCommitMessage).toHaveBeenCalled());
    expect(onRequestCommitMessage.mock.calls[0][0]).toEqual([
      { path: SKILL.path, untracked: false },
    ]);
  });

  it("CHG-FR-28 cascades a folder over its visible descendants and renders indeterminate between", async () => {
    backend({ uncommitted: changeSet([LIB, change("src/components/Shell.tsx"), HOOK]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");

    await userEvent.click(checkbox("src"));
    expect(checkbox("Library.tsx").checked).toBe(true);
    expect(checkbox("Shell.tsx").checked).toBe(true);
    expect(checkbox("useEditHistory.ts").checked).toBe(true);
    expect(checkbox("src").checked).toBe(true);

    await userEvent.click(checkbox("Library.tsx"));
    expect(checkbox("src").indeterminate).toBe(true);

    await userEvent.click(checkbox("Shell.tsx"));
    await userEvent.click(checkbox("useEditHistory.ts"));
    expect(checkbox("src").checked).toBe(false);
    expect(checkbox("src").indeterminate).toBe(false);
  });

  it("CHG-FR-27, CHG-FR-29 retains a check while the filters hide the row, and restores it", async () => {
    backend({ uncommitted: changeSet([SKILL, LIB]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");
    await userEvent.click(checkbox("Library.tsx"));

    // Hidden by the lens: out of the commit set.
    await pickSelector("Filter by type", "artifacts");
    await waitFor(() => expect(rowNames()).not.toContain("Library.tsx"));
    expect(screen.getByText(/0 files selected/)).toBeInTheDocument();

    // Revealed again: still ticked, and back in the commit set.
    await showAllFiles();
    await waitFor(() => expect(checkbox("Library.tsx").checked).toBe(true));
    expect(screen.getByText(/1 file selected/)).toBeInTheDocument();
  });

  it("CHG-FR-30, CHG-FR-31, CHG-FR-36 starts with nothing ticked, and a new entry arrives unticked", async () => {
    let entries = [LIB, HOOK];
    backend({ uncommitted: () => changeSet(entries) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");

    expect(checkbox("Library.tsx").checked).toBe(false);
    expect(checkbox("useEditHistory.ts").checked).toBe(false);
    expect(primary()).toBeDisabled();

    await userEvent.click(checkbox("Library.tsx"));
    expect(primary()).toBeEnabled();

    const fresh = change("src/new.ts");
    entries = [LIB, HOOK, fresh];
    await act(async () => {
      listeners[CHANGES_UPDATED]?.({ payload: { changeCount: 3 } });
    });
    await screen.findByText("new.ts");
    expect(checkbox("new.ts").checked).toBe(false);
    expect(checkbox("Library.tsx").checked).toBe(true);
  });

  it("CHG-FR-31 drops the check of an entry that leaves the change set", async () => {
    let entries = [LIB, HOOK];
    backend({ uncommitted: () => changeSet(entries) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");
    await userEvent.click(checkbox("Library.tsx"));
    await userEvent.click(checkbox("useEditHistory.ts"));

    // `useEditHistory.ts` is reverted on disk, so it leaves the set.
    entries = [LIB];
    await act(async () => {
      listeners[CHANGES_UPDATED]?.({ payload: { changeCount: 1 } });
    });
    await waitFor(() => expect(rowNames()).not.toContain("useEditHistory.ts"));
    expect(checkbox("Library.tsx").checked).toBe(true);

    // Edited again: it comes back unchecked rather than remembering.
    entries = [LIB, HOOK];
    await act(async () => {
      listeners[CHANGES_UPDATED]?.({ payload: { changeCount: 2 } });
    });
    await screen.findByText("useEditHistory.ts");
    expect(checkbox("useEditHistory.ts").checked).toBe(false);
  });

  it("CHG-FR-32 starts a remount with nothing ticked", async () => {
    // CHG-FR-32: check state is in memory and discarded when the project closes
    // or the active worktree changes — both of which remount the panel, since
    // it lives inside the shell subtree keyed on those.
    backend({ uncommitted: changeSet([LIB]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");
    await userEvent.click(checkbox("Library.tsx"));
    expect(checkbox("Library.tsx").checked).toBe(true);

    cleanup();
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");
    expect(checkbox("Library.tsx").checked).toBe(false);
    // And nothing about what was ticked was ever written anywhere.
    expect(calls("save_changes_panel_state").map((c) => c[1])).not.toContainEqual(
      expect.objectContaining({ checked: expect.anything() }),
    );
  });
});

// ---------------------------------------------------------------------------
// Gaps the reviews found: the checkbox is not the row, the Unrevisioned group
// cascades, the retry is bounded, and neither action can be double-submitted.
// ---------------------------------------------------------------------------

describe("further checkbox and action behaviour", () => {
  it("CHG-FR-13, CHG-FR-18 toggling a checkbox opens nothing and collapses nothing", async () => {
    // CHG-FR-18: a checkbox is not a click on the row. One `stopPropagation`
    // guards both the Diff tab and the folder's expand state.
    backend({ uncommitted: changeSet([LIB, HOOK]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");

    await userEvent.click(checkbox("Library.tsx"));
    expect(checkbox("Library.tsx").checked).toBe(true);
    expect(onOpenDiff).not.toHaveBeenCalled();

    await userEvent.click(checkbox("src"));
    expect(onOpenDiff).not.toHaveBeenCalled();
    // The folder is still expanded: its children are still on screen.
    expect(rowNames()).toContain("Library.tsx");

    // The row itself still opens a Diff tab, so the guard is not blanket.
    await userEvent.click(row("Library.tsx"));
    expect(onOpenDiff).toHaveBeenCalledOnce();
  });

  it("CHG-FR-28 cascades a group over its own files and leaves the other alone", async () => {
    const fresh = change("src/new.ts", { changeStatus: "untracked" });
    const alsoFresh = change("docs/draft.md", { changeStatus: "untracked" });
    backend({ uncommitted: changeSet([LIB, fresh, alsoFresh]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Unrevisioned");

    await userEvent.click(checkbox("Unrevisioned"));
    expect(checkbox("new.ts").checked).toBe(true);
    expect(checkbox("draft.md").checked).toBe(true);
    // The tracked file in the other group is untouched.
    expect(checkbox("Library.tsx").checked).toBe(false);
    expect(checkbox("Revisioned").checked).toBe(false);
    expect(checkbox("Unrevisioned").checked).toBe(true);

    await userEvent.click(checkbox("new.ts"));
    expect(checkbox("Unrevisioned").indeterminate).toBe(true);

    // CHG-FR-40 / CMW-FR-06: both arrive at the window marked untracked.
    await userEvent.click(checkbox("new.ts"));
    await userEvent.click(primary());
    await waitFor(() => expect(onRequestCommitMessage).toHaveBeenCalled());
    expect(onRequestCommitMessage.mock.calls[0][0]).toEqual(
      expect.arrayContaining([
        { path: fresh.path, untracked: true },
        { path: alsoFresh.path, untracked: true },
      ]),
    );
  });

  it("CHG-FR-28 takes every visible tracked change with one tick on Revisioned", async () => {
    // The headline of the group: what one click on **Unrevisioned** does for new
    // files, one click on **Revisioned** does for the whole of a session's edits
    // — without walking down every folder holding one (CHG-FR-28).
    const shell = change("src/components/Shell.tsx");
    const cargo = change("src-tauri/Cargo.toml");
    const fresh = change("docs/draft.md", { changeStatus: "untracked" });
    backend({ uncommitted: changeSet([LIB, HOOK, shell, cargo, fresh]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Revisioned");

    await userEvent.click(checkbox("Revisioned"));
    for (const name of ["Library.tsx", "useEditHistory.ts", "Shell.tsx", "Cargo.toml"]) {
      expect(checkbox(name).checked).toBe(true);
    }
    expect(checkbox("Revisioned").checked).toBe(true);
    expect(checkbox("Revisioned").indeterminate).toBe(false);
    // The new file is a separate decision and stays unticked.
    expect(checkbox("draft.md").checked).toBe(false);
    expect(screen.getByText(/4 files selected/)).toBeInTheDocument();

    // Unticking one of them leaves the group between the two states.
    await userEvent.click(checkbox("Shell.tsx"));
    expect(checkbox("Revisioned").indeterminate).toBe(true);

    // And one more tick clears the whole group again.
    await userEvent.click(checkbox("Shell.tsx"));
    await userEvent.click(checkbox("Revisioned"));
    expect(checkbox("Library.tsx").checked).toBe(false);
    expect(screen.getByText(/0 files selected/)).toBeInTheDocument();
  });

  it("keeps a group's tick when an entry moves between the groups on reload (CHG-FR-31)", async () => {
    // The author runs `git add` on a file they had already ticked. Checks are
    // keyed by path, so the entry keeps its tick and its place in the commit
    // set as it crosses from Unrevisioned to Revisioned — and is handed to the
    // window as tracked, because that is what it now is.
    const path = "docs/draft.md";
    let entries = [LIB, change(path, { changeStatus: "untracked" })];
    backend({ uncommitted: () => changeSet(entries) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Unrevisioned");

    await userEvent.click(checkbox("draft.md"));
    expect(checkbox("Unrevisioned").checked).toBe(true);

    entries = [LIB, change(path, { changeStatus: "added" })];
    await act(async () => {
      listeners[CHANGES_UPDATED]?.({ payload: { changeCount: 2 } });
    });
    await waitFor(() =>
      expect(screen.queryByText("Unrevisioned")).not.toBeInTheDocument(),
    );
    expect(checkbox("draft.md").checked).toBe(true);
    // It now sits under Revisioned, which is partly ticked as a result.
    expect(checkbox("Revisioned").indeterminate).toBe(true);

    await userEvent.click(primary());
    await waitFor(() => expect(onRequestCommitMessage).toHaveBeenCalled());
    expect(onRequestCommitMessage.mock.calls[0][0]).toEqual([
      { path, untracked: false },
    ]);
  });

  it("re-reads a group's tri-state when a reload adds an entry (CHG-FR-28 / CHG-FR-30)", async () => {
    let entries = [LIB, HOOK];
    backend({ uncommitted: () => changeSet(entries) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");

    await userEvent.click(checkbox("Revisioned"));
    expect(checkbox("Revisioned").checked).toBe(true);
    expect(checkbox("Revisioned").indeterminate).toBe(false);

    // A new entry arrives unticked (CHG-FR-30), so the group falls back to
    // indeterminate rather than claiming to hold everything beneath it.
    entries = [LIB, HOOK, change("src/new.ts")];
    await act(async () => {
      listeners[CHANGES_UPDATED]?.({ payload: { changeCount: 3 } });
    });
    await screen.findByText("new.ts");
    expect(checkbox("Revisioned").indeterminate).toBe(true);
    expect(checkbox("Revisioned").checked).toBe(false);
  });

  it("keeps a group collapsed across a filter change and a reload (CHG-FR-17)", async () => {
    // The group row is rendered by its own code path, so the folder-level
    // guarantee of CHG-FR-17 does not carry to it for free.
    let entries = [SKILL, LIB];
    backend({ uncommitted: () => changeSet(entries) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");

    await userEvent.click(row("Revisioned"));
    expect(rowNames()).toEqual(["Revisioned"]);

    await pickSelector("Filter by type", "artifacts");
    entries = [SKILL, LIB, change("src/new.ts")];
    await act(async () => {
      listeners[CHANGES_UPDATED]?.({ payload: { changeCount: 3 } });
    });
    expect(rowNames()).toEqual(["Revisioned"]);
  });

  it("takes the whole filter-visible subtree of a collapsed group (CHG-FR-28)", async () => {
    // "Visible" in the cascade means visible under the filters, not on screen:
    // a collapsed group still hands over everything the filters leave in it.
    backend({ uncommitted: changeSet([LIB, HOOK]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");

    await userEvent.click(row("Revisioned"));
    expect(rowNames()).toEqual(["Revisioned"]);
    await userEvent.click(checkbox("Revisioned"));
    expect(screen.getByText(/2 files selected/)).toBeInTheDocument();

    await userEvent.click(row("Revisioned"));
    expect(checkbox("Library.tsx").checked).toBe(true);
    expect(checkbox("useEditHistory.ts").checked).toBe(true);
  });

  it("stays checked when the lens hides part of what it took (CHG-FR-27 / CHG-FR-29)", async () => {
    backend({ uncommitted: changeSet([SKILL, LIB]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");

    await userEvent.click(checkbox("Revisioned"));
    expect(screen.getByText(/2 files selected/)).toBeInTheDocument();

    // Narrowing the lens drops the hidden row from the commit set, and the
    // group reads as checked — not indeterminate — because the hidden file is
    // no longer among its visible descendants.
    await pickSelector("Filter by type", "artifacts");
    await waitFor(() => expect(rowNames()).not.toContain("Library.tsx"));
    expect(screen.getByText(/1 file selected/)).toBeInTheDocument();
    expect(checkbox("Revisioned").checked).toBe(true);
    expect(checkbox("Revisioned").indeterminate).toBe(false);

    // The hidden row kept its check and comes back with it.
    await showAllFiles();
    await screen.findByText("Library.tsx");
    expect(checkbox("Library.tsx").checked).toBe(true);
  });

  it("CHG-FR-27, CHG-FR-28 leaves a filter-hidden file out of what Revisioned takes", async () => {
    // The group cascades over its *currently visible* descendants only, exactly
    // as a folder does (CHG-FR-27 / CHG-FR-28) — the default lens hides
    // `Library.tsx`, so ticking the group must not drag it into the commit.
    backend({ uncommitted: changeSet([SKILL, LIB]) });
    renderPanel();
    await screen.findByText("onboarding.md");
    expect(rowNames()).not.toContain("Library.tsx");

    await userEvent.click(checkbox("Revisioned"));
    await userEvent.click(primary());
    await waitFor(() => expect(onRequestCommitMessage).toHaveBeenCalled());
    expect(onRequestCommitMessage.mock.calls[0][0]).toEqual([
      { path: SKILL.path, untracked: false },
    ]);
  });

  it("CHG-FR-44 cannot submit a second push while one is in flight", async () => {
    let release!: () => void;
    backend({
      uncommitted: changeSet([LIB]),
      sync: { hasRemote: true, hasUpstream: true, ahead: 2, behind: 0 },
      prefs: { changesCommitAction: "push" },
    });
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      if (cmd === "push_current_branch") {
        return new Promise<void>((resolve) => (release = () => resolve()));
      }
      return base(cmd, args);
    });
    renderPanel();
    await waitFor(() => expect(primary()).toBeEnabled());

    await userEvent.click(primary());
    await waitFor(() => expect(primary()).toBeDisabled());
    await userEvent.click(primary());
    expect(calls("push_current_branch")).toHaveLength(1);

    await act(async () => {
      release();
    });
    await waitFor(() => expect(primary()).toBeEnabled());
  });

  it("CHG-FR-44 keeps the button disabled while the token picker is open", async () => {
    let choose!: (chosen: boolean) => void;
    onRequestGithubToken.mockImplementation(
      () => new Promise<boolean>((resolve) => (choose = resolve)),
    );
    backend({
      uncommitted: changeSet([LIB]),
      sync: { hasRemote: true, hasUpstream: false, ahead: null, behind: null },
      prefs: { changesCommitAction: "push" },
      fail: { push_current_branch: "github_token_selection_required" },
    });
    renderPanel();
    await waitFor(() => expect(primary()).toBeEnabled());

    await userEvent.click(primary());
    await waitFor(() => expect(onRequestGithubToken).toHaveBeenCalled());
    expect(primary()).toBeDisabled();

    await act(async () => {
      choose(false);
    });
    await waitFor(() => expect(primary()).toBeEnabled());
  });

  it("CHG-FR-45 prompts for a token once and does not loop", async () => {
    // The retry is bounded: a binding still unresolved after the picker
    // confirmed is a failure to report, not a reason to prompt again.
    backend({
      uncommitted: changeSet([LIB]),
      sync: { hasRemote: true, hasUpstream: false, ahead: null, behind: null },
      prefs: { changesCommitAction: "push" },
      fail: { push_current_branch: "github_token_selection_required" },
    });
    renderPanel();
    await waitFor(() => expect(primary()).toBeEnabled());

    await userEvent.click(primary());

    await waitFor(() => expect(calls("push_current_branch")).toHaveLength(2));
    expect(onRequestGithubToken).toHaveBeenCalledOnce();
    expect(await screen.findByText(/Push failed/)).toBeInTheDocument();
  });

  it("CHG-FR-38 re-reads the sync state when a push finishes", async () => {
    let sync: UpstreamSyncState = {
      hasRemote: true,
      hasUpstream: true,
      ahead: 2,
      behind: 0,
    };
    backend({
      uncommitted: changeSet([LIB]),
      sync: () => sync,
      prefs: { changesCommitAction: "push" },
    });
    renderPanel();
    await waitFor(() => expect(primary()).toBeEnabled());
    await userEvent.click(primary());
    await waitFor(() => expect(calls("push_current_branch")).toHaveLength(1));

    // The push published the two commits, so there is nothing left to push.
    sync = { hasRemote: true, hasUpstream: true, ahead: 0, behind: 0 };
    await act(async () => {
      listeners[GIT_OPERATION_FINISHED]?.({
        payload: { operation: "push", ok: true },
      });
    });
    await waitFor(() => expect(primary()).toBeDisabled());
  });

  it("CHG-FR-45 leaves a token cause to the routing rather than rendering it as a failure", async () => {
    // The terminal event fires before the invoke rejects, so a panel that
    // rendered every terminal cause would flash "Push failed:
    // github_token_selection_required" over the picker it is about to open.
    backend({
      uncommitted: changeSet([LIB]),
      sync: { hasRemote: true, hasUpstream: false, ahead: null, behind: null },
      prefs: { changesCommitAction: "push" },
    });
    renderPanel();
    await waitFor(() => expect(primary()).toBeEnabled());

    await act(async () => {
      listeners[GIT_OPERATION_FINISHED]?.({
        payload: {
          operation: "push",
          ok: false,
          error: "github_token_selection_required",
        },
      });
    });
    expect(screen.queryByText(/Push failed/)).toBeNull();

    // Any other cause does render.
    await act(async () => {
      listeners[GIT_OPERATION_FINISHED]?.({
        payload: { operation: "push", ok: false, error: "remote unreachable" },
      });
    });
    expect(await screen.findByText(/Push failed/)).toBeInTheDocument();
  });
});
