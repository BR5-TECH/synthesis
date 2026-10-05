import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type {
  ChangeSet,
  DiffTarget,
} from "../types";
import { CHANGES_UPDATED } from "../events";
import { resetAppPreferencesCache } from "../state/appPreferences";
import { resetPanelReveals } from "../state/panelReveal";
import {
  pickSelector,
  selectorValue,
} from "../test/selectors";
import {
  HOOK,
  LIB,
  SKILL,
  SPEC,
  change,
  changeSet,
  makeBackend,
  onOpenDiff,
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
    listeners[event] = cb;
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
// Opening diffs (CHG-FR-13, CHG-FR-18)
// ---------------------------------------------------------------------------

describe("opening a Diff tab (CHG-FR-18)", () => {
  it("hands the shell the file and the active comparison", async () => {
    backend({ uncommitted: changeSet([LIB]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");

    await userEvent.click(row("Library.tsx"));
    expect(onOpenDiff).toHaveBeenCalledTimes(1);
    const target = onOpenDiff.mock.calls[0][0] as DiffTarget;
    expect(target.path).toBe("src/components/Library.tsx");
    expect(target.name).toBe("Library.tsx");
    expect(target.scope).toEqual({
      kind: "path",
      path: "src/components/Library.tsx",
    });
    // Unclassified: the field is absent rather than invented.
    expect(target.artifactType).toBeUndefined();
  });

  // CHG-FR-18: the panel supplies the file's RESOLVED type, which is what lets
  // the Diff tab read a Flow as a Flow (DFV-FR-17). The resolution is the
  // backend's — including a user's assignment overriding the extension — so the
  // panel's job is to hand over what the entry carries, whichever way it got it.
  it("hands over the file's resolved artifact type", async () => {
    backend({
      uncommitted: changeSet([
        change("workflows/review.flow", {
          artifactType: "flow",
          typeSource: "inferred",
        }),
        // Named nothing in particular; a Flow because the user said so.
        change("workflows/pipeline", {
          artifactType: "flow",
          typeSource: "assigned",
        }),
        // A `.flow` file the user assigned another type is not a Flow.
        change("workflows/notes.flow", {
          artifactType: "spec",
          typeSource: "assigned",
        }),
      ]),
    });
    renderPanel();
    await showAllFiles();
    await screen.findByText("review.flow");

    for (const [name, type] of [
      ["review.flow", "flow"],
      ["pipeline", "flow"],
      ["notes.flow", "spec"],
    ]) {
      onOpenDiff.mockClear();
      await userEvent.click(row(name));
      expect((onOpenDiff.mock.calls[0][0] as DiffTarget).artifactType).toBe(type);
    }
  });

  it("uses the branch scope while in Branch mode, so the comparison differs", async () => {
    backend({
      panelState: { mode: "branch", targetBranch: "develop" },
      branchChanges: changeSet([LIB], "develop"),
    });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");

    await userEvent.click(row("Library.tsx"));
    const target = onOpenDiff.mock.calls[0][0] as DiffTarget;
    expect(target.scope).toEqual({
      kind: "branch",
      path: "src/components/Library.tsx",
      targetBranch: "develop",
    });
    expect(target.comparisonLabel).toContain("develop");
  });

  it("does not open a diff for a folder row", async () => {
    backend({ uncommitted: changeSet([LIB]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");

    await userEvent.click(row("src"));
    expect(onOpenDiff).not.toHaveBeenCalled();
  });
});

// ---------------------------------------------------------------------------
// Live reload + refresh (CHG-FR-21, DFV-FR-06 / CHG-FR-23)
// ---------------------------------------------------------------------------

describe("reloading", () => {
  it("reloads the active mode when the backend reports a change (CHG-FR-21, DFV-FR-06)", async () => {
    let entries = [LIB];
    backend({ uncommitted: () => changeSet(entries) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");

    entries = [LIB, HOOK];
    listeners[CHANGES_UPDATED]?.({ payload: { changeCount: 2 } });
    expect(await screen.findByText("useEditHistory.ts")).toBeInTheDocument();
  });

  it("reloads the branch mode's list, not the uncommitted one", async () => {
    backend({ panelState: { mode: "branch", targetBranch: "develop" } });
    renderPanel();
    await waitFor(() => expect(calls("list_branch_changes")).toHaveLength(1));

    listeners[CHANGES_UPDATED]?.({ payload: { changeCount: 1 } });
    await waitFor(() => expect(calls("list_branch_changes")).toHaveLength(2));
    expect(calls("list_uncommitted_changes")).toHaveLength(0);
  });

  it("re-invokes the active mode's list on manual refresh (CHG-FR-23)", async () => {
    let entries = [LIB];
    backend({ uncommitted: () => changeSet(entries) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");

    entries = [LIB, HOOK];
    await userEvent.click(screen.getByLabelText("Refresh changes"));
    expect(await screen.findByText("useEditHistory.ts")).toBeInTheDocument();
    expect(calls("list_uncommitted_changes")).toHaveLength(2);
  });

  it("unsubscribes on unmount so a remount cannot double-reload", async () => {
    renderPanel();
    await waitFor(() => expect(calls("list_uncommitted_changes")).toHaveLength(1));
    cleanup();
    await waitFor(() => expect(unlistenMock).toHaveBeenCalled());
  });
});

// ---------------------------------------------------------------------------
// Bounded reach over the repository (CHG-FR-21, DFV-FR-06)
// ---------------------------------------------------------------------------

describe("bounded reach over the repository (CHG-FR-21)", () => {
  it("offers commit, push, and rollback and nothing else that touches the repository", async () => {
    backend({ uncommitted: changeSet([LIB, SKILL]) });
    const { container } = renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");
    await userEvent.click(row("Library.tsx"));

    // Word boundaries matter: the mode toggle legitimately reads
    // "Uncommitted", which contains "commit" without offering one.
    //
    // "discard" is deliberately absent from this list: CHG-FR-21 now names
    // three operations rather than two, and the rollback button says so in
    // words (CHG-FR-58). What stays forbidden is everything that belongs to the
    // Git panel instead.
    const forbidden = /\b(stage|unstage|pull|checkout|revert)\b/i;
    const controls = container.querySelectorAll<HTMLElement>(
      "button, select, input, [role='radio']",
    );
    expect(controls.length).toBeGreaterThan(0);
    for (const control of controls) {
      const label = `${control.getAttribute("aria-label") ?? ""} ${control.textContent ?? ""}`;
      expect(label).not.toMatch(forbidden);
    }
    expect(container.textContent).not.toMatch(forbidden);
    expect(container.textContent).not.toMatch(/new branch|create branch/i);

    // CHG-FR-21: the three operations are the whole of the panel's reach, and
    // none of them runs until the author activates a footer control.
    const invoked = invokeMock.mock.calls.map((c) => c[0] as string);
    expect(invoked.length).toBeGreaterThan(0);
    for (const cmd of invoked) {
      expect(cmd).not.toMatch(
        /^(stage_|unstage_|commit_paths|rollback_paths|push_|pull_|create_branch|checkout_|delete_|rename_|save_artifact)/,
      );
    }
  });
});

// ---------------------------------------------------------------------------
// The panel's non-list states (CHG-FR-22, CHG-FR-47, CHG-FR-56, SNV-FR-60 / CHG-FR-23 / CHG-FR-24)
// ---------------------------------------------------------------------------

describe("inline states", () => {
  it("explains a project that is not a Git repository, without failing (CHG-FR-22, CHG-FR-47, CHG-FR-56, SNV-FR-60)", async () => {
    backend({ fail: { list_uncommitted_changes: "not a git repository" } });
    renderPanel();

    // CHG-FR-22 / SNV-FR-60: the shared centred block, carrying no action —
    // there is nothing here to commit to (CHG-FR-47).
    const line = await screen.findByText("Not a Git repository.");
    const block = line.closest(".panel-empty");
    expect(block).not.toBeNull();
    expect(block?.parentElement).toHaveClass("vpanel__body--empty");
    // No action at all rather than a disabled one (SNV-FR-60), which a
    // `querySelector("button")` alone would not establish.
    expect(
      block?.querySelectorAll("button, :disabled, [aria-disabled]"),
    ).toHaveLength(0);
    // The sentence, not the stated line alone.
    expect(screen.getByText(/isn’t tracked by Git/)).toBeInTheDocument();
    // SNV-FR-60: outside a repository there is no comparison to select and
    // nothing to narrow, so the control stack is not rendered either.
    expect(screen.queryByRole("radio", { name: "Uncommitted" })).toBeNull();
    expect(screen.queryByLabelText("Filter changes")).toBeNull();
    // The panel itself still rendered rather than failing.
    expect(screen.getByText("Changes")).toBeInTheDocument();
  });

  it("composes the empty change set like the not-a-repository state but says a different thing (CHG-FR-23)", async () => {
    backend({ uncommitted: changeSet([]) });
    renderPanel();

    // CHG-FR-23 / SNV-FR-60: same block, same placement, different statement —
    // the two are told apart by what they say, not by where they sit.
    const line = await screen.findByText("No changes.");
    const block = line.closest(".panel-empty");
    expect(block).not.toBeNull();
    expect(block?.parentElement).toHaveClass("vpanel__body--empty");
    expect(
      block?.querySelectorAll("button, :disabled, [aria-disabled]"),
    ).toHaveLength(0);
    // SNV-FR-60: the sentence naming what the absent thing is, not the line
    // alone — in Uncommitted mode it names the last commit.
    expect(
      screen.getByText(/This worktree matches its last commit/),
    ).toBeInTheDocument();
    expect(screen.queryByText("Not a Git repository.")).not.toBeInTheDocument();
    // The mode toggle survives: it selects *which* comparison is computed
    // rather than narrowing the one on screen, and a clean working tree is
    // exactly when the author reaches for Branch mode.
    expect(screen.getByRole("radio", { name: "Branch" })).toBeInTheDocument();
    // The two narrowing controls go — nothing for them to narrow (SNV-FR-60).
    expect(screen.queryByLabelText("Filter changes")).toBeNull();
    expect(screen.queryByLabelText("Filter by type")).toBeNull();
    // A group with nothing in it renders nothing at all (CHG-FR-16).
    expect(rowNames()).toEqual([]);
  });

  it("names the target branch in the empty block in Branch mode (CHG-FR-23)", async () => {
    backend({
      panelState: { mode: "branch", targetBranch: "main" },
      branchChanges: changeSet([], "main"),
    });
    renderPanel();

    await screen.findByText("No changes.");
    // The statement is mode-specific: the same "No changes." line over a
    // different sentence, because what yields nothing differs between the two
    // comparisons and the author needs to know which one they are looking at.
    expect(
      screen.getByText("Nothing differs between this worktree and main."),
    ).toBeInTheDocument();
    expect(
      screen.queryByText(/This worktree matches its last commit/),
    ).not.toBeInTheDocument();
  });

  it("distinguishes an empty result from one the lens emptied (SNV-FR-61)", async () => {
    // The common case, because CHG-FR-15 opens on All artifacts and hides every
    // unclassified changed file: the tree is empty on first render without the
    // author having typed or picked anything, and that is *not* the same
    // statement as "nothing changed" (CHG-FR-23).
    backend({
      uncommitted: changeSet([change("package.json"), change("pnpm-lock.yaml")]),
    });
    renderPanel();

    const message = await screen.findByText(/No changes match/);
    // SNV-FR-61: the shared narrowed-to-nothing state, which every vertical
    // panel renders the same way — and not the first-class empty block, which
    // says a different thing and takes the filters away with it.
    expect(message.closest(".panel-filtered")).not.toBeNull();
    expect(document.querySelector(".panel-empty")).toBeNull();
    // It sits where the tree would have been rather than in its top-left
    // corner, which is what the body class carries.
    expect(document.querySelector(".vpanel__body--filtered")).not.toBeNull();
  });

  it("leaves the lens and text the author set standing (SNV-FR-61)", async () => {
    backend({
      uncommitted: changeSet([
        change("package.json"),
        change("pnpm-lock.yaml"),
        // The one classified entry, so the Skill lens has a button to exist
        // under (CHG-FR-14 / LIB-FR-19). The text below filters it out.
        SKILL,
      ]),
    });
    renderPanel();
    await screen.findByText("onboarding.md");

    // Every control stays present *and still holds what was picked or typed*.
    // Asserting mere presence would pass against a panel that silently reset
    // the author's lens and text, which is the half of SNV-FR-61 that makes the
    // state recoverable.
    await userEvent.type(screen.getByLabelText("Filter changes"), "package");
    await pickSelector("Filter by type", "skill");

    expect(await screen.findByText(/No changes match/)).toBeInTheDocument();
    expect(document.querySelector(".panel-empty")).toBeNull();
    expect(selectorValue("Filter by type")).toBe("skill");
    expect(screen.getByLabelText("Filter changes")).toHaveValue("package");
  });

  it("names an unresolvable target branch and keeps the picker usable (CHG-FR-05)", async () => {
    backend({
      panelState: { mode: "branch", targetBranch: "feature/old" },
      fail: { list_branch_changes: "unknown branch" },
    });
    renderPanel();

    const state = await waitFor(() => {
      const el = document.querySelector<HTMLElement>(
        '.changes-state[data-state="unknown-branch"]',
      );
      if (!el) throw new Error("no unknown-branch state rendered");
      return el;
    });
    expect(state.textContent).toContain("feature/old");
    // The picker is still there, still on the branch the user configured — the
    // panel has not silently substituted a different target.
    const picker = screen.getByLabelText<HTMLSelectElement>("Target branch");
    expect(picker).toBeEnabled();
    expect(picker).toHaveValue("feature/old");
    expect(calls("list_branch_changes")).toHaveLength(1);
  });

  it("lets the user recover from an unresolvable branch by picking another", async () => {
    let target = "feature/old";
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      switch (cmd) {
        case "load_changes_panel_state":
          return { mode: "branch", targetBranch: "feature/old" };
        case "list_comparison_branches":
          return [{ name: "main", isCurrent: false, isDefault: true }];
          // The panel's own reads beyond the change set (CHG-FR-34 / CHG-FR-38).
        case "get_upstream_sync_state":
          return { hasRemote: true, hasUpstream: true, ahead: 0, behind: 0 };
        case "load_app_preferences":
          return { theme: "system" };
      case "list_branch_changes":
          target = String(args?.targetBranch);
          if (target === "feature/old") throw "unknown branch";
          return changeSet([SPEC], target);
        default:
          return undefined;
      }
    });
    renderPanel();
    await waitFor(() =>
      expect(
        document.querySelector('.changes-state[data-state="unknown-branch"]'),
      ).not.toBeNull(),
    );

    await userEvent.selectOptions(screen.getByLabelText("Target branch"), "main");
    expect(await screen.findByText("CHG-changes.md")).toBeInTheDocument();
    expect(screen.queryByText(/no longer exists/)).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// Request sequencing and persistence hygiene
// ---------------------------------------------------------------------------

describe("request sequencing", () => {
  /** A promise plus the resolver that settles it. */
  function deferred<T>() {
    let resolve!: (value: T) => void;
    let reject!: (reason?: unknown) => void;
    const promise = new Promise<T>((res, rej) => {
      resolve = res;
      reject = rej;
    });
    return { promise, resolve, reject };
  }

  it("discards a slow branch response that lands after a newer uncommitted one", async () => {
    // Without sequencing the branch entries would paint under an "Uncommitted"
    // toggle, and a row clicked afterwards would open a Diff tab carrying the
    // branch comparison — the wrong identity under CHG-FR-19.
    const slowBranch = deferred<ChangeSet>();
    backend({
      panelState: { mode: "branch", targetBranch: "main" },
      branchChanges: (() => slowBranch.promise) as never,
      uncommitted: changeSet([SKILL]),
    });
    invokeMock.mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case "load_changes_panel_state":
          return { mode: "branch", targetBranch: "main" };
        case "list_comparison_branches":
          return [{ name: "main", isCurrent: false, isDefault: true }];
          // The panel's own reads beyond the change set (CHG-FR-34 / CHG-FR-38).
        case "get_upstream_sync_state":
          return { hasRemote: true, hasUpstream: true, ahead: 0, behind: 0 };
        case "load_app_preferences":
          return { theme: "system" };
      case "list_branch_changes":
          return slowBranch.promise;
        case "list_uncommitted_changes":
          return changeSet([SKILL]);
        default:
          return undefined;
      }
    });

    renderPanel();
    await waitFor(() => expect(calls("list_branch_changes")).toHaveLength(1));

    // Switch modes before the branch call settles; the fast call renders.
    await userEvent.click(screen.getByRole("radio", { name: "Uncommitted" }));
    expect(await screen.findByText("onboarding.md")).toBeInTheDocument();

    // The superseded branch response now arrives with different entries.
    await act(async () => {
      slowBranch.resolve(changeSet([SPEC], "main"));
    });
    expect(screen.queryByText("CHG-changes.md")).not.toBeInTheDocument();
    expect(screen.getByText("onboarding.md")).toBeInTheDocument();
  });

  it("discards a superseded rejection instead of painting a stale error", async () => {
    const slowBranch = deferred<ChangeSet>();
    invokeMock.mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case "load_changes_panel_state":
          return { mode: "branch", targetBranch: "feature/old" };
        case "list_comparison_branches":
          return [{ name: "main", isCurrent: false, isDefault: true }];
          // The panel's own reads beyond the change set (CHG-FR-34 / CHG-FR-38).
        case "get_upstream_sync_state":
          return { hasRemote: true, hasUpstream: true, ahead: 0, behind: 0 };
        case "load_app_preferences":
          return { theme: "system" };
      case "list_branch_changes":
          return slowBranch.promise;
        case "list_uncommitted_changes":
          return changeSet([SKILL]);
        default:
          return undefined;
      }
    });

    renderPanel();
    await waitFor(() => expect(calls("list_branch_changes")).toHaveLength(1));
    await userEvent.click(screen.getByRole("radio", { name: "Uncommitted" }));
    await screen.findByText("onboarding.md");

    await act(async () => {
      slowBranch.reject("unknown branch");
    });
    expect(
      document.querySelector('.changes-state[data-state="unknown-branch"]'),
    ).toBeNull();
    expect(screen.getByText("onboarding.md")).toBeInTheDocument();
  });
});

describe("persistence hygiene (CHG-FR-07)", () => {
  it("writes nothing when the restore produced what was already stored", async () => {
    backend({ panelState: { mode: "branch", targetBranch: "develop" } });
    renderPanel();
    await waitFor(() => expect(calls("list_branch_changes")).toHaveLength(1));
    // A restore that echoed itself back to disk would be a pointless write the
    // backend watcher then reports as a change.
    expect(calls("save_changes_panel_state")).toHaveLength(0);
  });

  it("writes nothing when the target merely came from the repository default", async () => {
    backend({ panelState: { mode: "uncommitted" }, defaultBranch: "trunk" });
    renderPanel();
    await waitFor(() => expect(calls("list_uncommitted_changes")).toHaveLength(1));
    expect(calls("save_changes_panel_state")).toHaveLength(0);
  });
});

describe("renamed entries open a paired diff (CHG-FR-12)", () => {
  it("carries the previous path into the Diff scope", async () => {
    const renamed = change("src-tauri/lib.rs", {
      changeStatus: "renamed",
      previousPath: "src-tauri/main.rs",
      addedLines: 1,
      removedLines: 1,
    });
    backend({ uncommitted: changeSet([renamed]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("src-tauri");

    await userEvent.click(row("lib.rs (was src-tauri/main.rs)"));
    const target = onOpenDiff.mock.calls[0][0] as DiffTarget;
    expect(target.scope).toEqual({
      kind: "path",
      path: "src-tauri/lib.rs",
      previousPath: "src-tauri/main.rs",
    });
  });
});
