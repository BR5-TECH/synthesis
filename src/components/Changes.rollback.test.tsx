import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type {
  RollbackOutcome,
  RollbackResult,
} from "../types";
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
  onRequestRollback,
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
// Rollback (CHG-FR-55, CHG-FR-56, CHG-FR-33, CHG-FR-26 – CHG-FR-63, CHG-FR-64, CHG-FR-65)
// ---------------------------------------------------------------------------

/** The footer's rollback button (CHG-FR-56). */
function rollbackButton(): HTMLButtonElement {
  return screen.getByRole("button", {
    name: "Discard selected changes",
  }) as HTMLButtonElement;
}

/** One outcome entry, as `rollback_paths` reports it (GTC-FR-25). */
function outcomeEntry(
  path: string,
  over: Partial<RollbackOutcome["entries"][number]> = {},
): RollbackOutcome["entries"][number] {
  return {
    id: path,
    path,
    previousPath: null,
    outcome: "restored",
    restoredPaths: [path],
    removedPaths: [],
    failures: [],
    ...over,
  };
}

describe("the footer's count row and rollback button (CHG-FR-55, CHG-FR-56, CHG-FR-33, CHG-FR-26)", () => {
  it("states the count on its own row, with rollback leading the footer", async () => {
    backend({ uncommitted: changeSet([LIB, SKILL]) });
    const { container } = renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");

    // CHG-FR-55: rendered whether the number is zero or not, so the row does
    // not appear and disappear beneath the tree as ticks change.
    const countRow = container.querySelector(".changes-selected-count");
    expect(countRow).not.toBeNull();
    expect(countRow!.textContent).toBe("0 files selected");

    await userEvent.click(checkbox("Library.tsx"));
    expect(countRow!.textContent).toBe("1 file selected");
    await userEvent.click(checkbox("onboarding.md"));
    expect(countRow!.textContent).toBe("2 files selected");

    // CHG-FR-56 / CHG-FR-33: rollback at the leading edge, the split control at
    // the trailing one.
    const footer = container.querySelector(".changes-actions")!;
    const buttons = [...footer.querySelectorAll("button")];
    expect(buttons[0]).toHaveAttribute("aria-label", "Discard selected changes");
    expect(buttons[buttons.length - 1]).toHaveAttribute(
      "aria-label",
      "Choose action",
    );
  });

  it("renders neither the count row nor the rollback button in Branch mode", async () => {
    backend({ uncommitted: changeSet([LIB]) });
    const { container } = renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");
    expect(container.querySelector(".changes-selected-count")).not.toBeNull();

    await pickSelector("Comparison mode", "branch");
    await waitFor(() =>
      expect(container.querySelector(".changes-selected-count")).toBeNull(),
    );
    // CHG-FR-56: Branch mode has no rollback action at all.
    expect(
      screen.queryByRole("button", { name: "Discard selected changes" }),
    ).toBeNull();
    // …and the split control has not moved off the trailing edge.
    const footer = container.querySelector(".changes-actions")!;
    const buttons = [...footer.querySelectorAll("button")];
    expect(buttons[buttons.length - 1]).toHaveAttribute(
      "aria-label",
      "Choose action",
    );
  });
});

describe("rollback enablement and its stated reason (CHG-FR-58)", () => {
  it("names the reason it cannot be activated, and each reason as it applies", async () => {
    backend({ uncommitted: changeSet([LIB, SKILL]) });
    renderPanel();
    await screen.findByText("onboarding.md");

    // CHG-FR-58: under the default All artifacts lens with nothing ticked, both
    // conditions fail and both are named.
    let button = rollbackButton();
    expect(button).toBeDisabled();
    expect(button.getAttribute("title")).toMatch(/no files are selected/i);
    expect(button.getAttribute("title")).toMatch(/All files/i);

    await showAllFiles();
    await screen.findByText("Library.tsx");
    button = rollbackButton();
    expect(button).toBeDisabled();
    // Only the remaining reason now.
    expect(button.getAttribute("title")).toMatch(/no files are selected/i);
    expect(button.getAttribute("title")).not.toMatch(/All files/i);

    await userEvent.click(checkbox("Library.tsx"));
    button = rollbackButton();
    expect(button).toBeEnabled();
    expect(button.getAttribute("title")).toMatch(/returning these files to HEAD/i);

    // CHG-FR-58: a narrowing lens disables it again, because the author cannot
    // review the whole of what they have ticked.
    await pickSelector("Filter by type", "artifacts");
    button = rollbackButton();
    expect(button).toBeDisabled();
    expect(button.getAttribute("title")).toMatch(/All files/i);
  });

  it("says the action in words, never by icon alone (CHG-FR-58)", async () => {
    backend({ uncommitted: changeSet([LIB]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");

    const button = rollbackButton();
    // The accessible name is what a screen reader announces; the tooltip is
    // what a pointer user reads. Neither is the icon.
    expect(button).toHaveAttribute("aria-label", "Discard selected changes");
    expect(button.getAttribute("title")).toBeTruthy();
  });
});

describe("the rollback set handed over (CHG-FR-59)", () => {
  it("carries the visible checked rows with their rename and deletion facts", async () => {
    const RENAMED = change("src-tauri/lib.rs", {
      changeStatus: "renamed",
      previousPath: "src-tauri/main.rs",
    });
    const UNTRACKED = change("src/components/Changes.tsx", {
      changeStatus: "untracked",
    });
    const DELETED = change("old-notes.md", { changeStatus: "deleted" });
    backend({
      uncommitted: changeSet([SKILL, RENAMED, UNTRACKED, DELETED]),
    });
    renderPanel();
    await showAllFiles();
    await screen.findByText("lib.rs");

    await userEvent.click(checkbox("onboarding.md"));
    // A renamed row labels itself with both paths (CHG-FR-12), so it is found
    // by the label it actually renders rather than by its basename alone.
    await userEvent.click(checkbox("lib.rs (was src-tauri/main.rs)"));
    await userEvent.click(checkbox("Changes.tsx"));
    await userEvent.click(checkbox("old-notes.md"));
    await userEvent.click(rollbackButton());

    await waitFor(() => expect(onRequestRollback).toHaveBeenCalled());
    const files = onRequestRollback.mock.calls[0][0];
    const byPath = new Map(files.map((f) => [f.path, f]));
    expect(byPath.get(".claude/skills/onboarding.md")).toMatchObject({
      untracked: false,
      previousPath: null,
    });
    // GTC-FR-26: both identities travel, so the confirmation can name both.
    expect(byPath.get("src-tauri/lib.rs")).toMatchObject({
      previousPath: "src-tauri/main.rs",
    });
    expect(byPath.get("src/components/Changes.tsx")).toMatchObject({
      untracked: true,
    });
    expect(byPath.get("old-notes.md")).toMatchObject({ deleted: true });
  });

  it("leaves every check and buffer alone when the confirmation is dismissed", async () => {
    backend({ uncommitted: changeSet([LIB, SKILL]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");
    await userEvent.click(checkbox("Library.tsx"));
    await userEvent.click(checkbox("onboarding.md"));

    // The default mock dismisses (resolves null), which is the dismissal path.
    await userEvent.click(rollbackButton());
    await waitFor(() => expect(onRequestRollback).toHaveBeenCalled());

    // CHG-FR-59: nothing was performed and both checks stand. The panel asked
    // once and got null back, and no failure report or success note appeared —
    // asserting on `invoke` here would be vacuous, the prop being mocked.
    await waitFor(() => expect(checkbox("Library.tsx").checked).toBe(true));
    expect(checkbox("onboarding.md").checked).toBe(true);
    expect(onRequestRollback).toHaveBeenCalledTimes(1);
    expect(screen.queryByText(/could not be rolled back/i)).toBeNull();
    expect(screen.queryByText(/^Discarded /)).toBeNull();
  });
});

describe("applying a rollback outcome per path (CHG-FR-63, EXC-FR-JAWT, EXC-FR-ZXWI, TAB-FR-41, TAB-FR-42 / CHG-FR-65)", () => {
  it("clears only the checks the backend confirmed, and reports the rest", async () => {
    backend({ uncommitted: changeSet([LIB, SKILL, HOOK]) });
    onRequestRollback.mockImplementation(async () => ({
      outcome: {
        entries: [
          outcomeEntry(".claude/skills/onboarding.md"),
          outcomeEntry("src/components/Library.tsx", {
            outcome: "removed",
            restoredPaths: [],
            removedPaths: ["src/components/Library.tsx"],
          }),
          outcomeEntry("src/hooks/useEditHistory.ts", {
            outcome: "failed",
            restoredPaths: [],
            failures: [
              { path: "src/hooks/useEditHistory.ts", kind: "permission_denied" },
            ],
          }),
        ],
      },
      saveFailures: [],
    }));
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");
    await userEvent.click(checkbox("Library.tsx"));
    await userEvent.click(checkbox("onboarding.md"));
    await userEvent.click(checkbox("useEditHistory.ts"));

    await userEvent.click(rollbackButton());
    await waitFor(() => expect(onRequestRollback).toHaveBeenCalled());

    // CHG-FR-63: the two confirmed paths lose their checks; the failed one
    // keeps its own, so the author can see what is still there.
    await waitFor(() => expect(checkbox("Library.tsx").checked).toBe(false));
    expect(checkbox("onboarding.md").checked).toBe(false);
    expect(checkbox("useEditHistory.ts").checked).toBe(true);

    // CHG-FR-65: no success state, and the failure is named with its cause.
    const report = await screen.findByText(/could not be rolled back/i);
    expect(report.textContent).toMatch(/1 file/);
    expect(screen.getByText("permission denied")).toBeInTheDocument();
    expect(screen.queryByText(/^Discarded /)).toBeNull();
  });

  it("reports success and reloads when every path succeeded (CHG-FR-63, CHG-FR-64, CHG-FR-65)", async () => {
    backend({ uncommitted: changeSet([LIB, SKILL]) });
    onRequestRollback.mockImplementation(async () => ({
      outcome: { entries: [outcomeEntry("src/components/Library.tsx")] },
      saveFailures: [],
    }));
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");
    await userEvent.click(checkbox("Library.tsx"));

    const before = calls("list_uncommitted_changes").length;
    await userEvent.click(rollbackButton());

    await screen.findByText(/^Discarded 1 file\.$/);
    expect(screen.queryByText(/could not be rolled back/i)).toBeNull();
    // CHG-FR-64: the active change set is reloaded.
    await waitFor(() =>
      expect(calls("list_uncommitted_changes").length).toBeGreaterThan(before),
    );
  });

  it("preserves unaffected checks and tree state across the reload (CHG-FR-64, CHG-FR-17, CHG-FR-31)", async () => {
    backend({ uncommitted: changeSet([LIB, SKILL, HOOK]) });
    onRequestRollback.mockImplementation(async () => ({
      outcome: { entries: [outcomeEntry(".claude/skills/onboarding.md")] },
      saveFailures: [],
    }));
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");

    // Two checks the rollback will not name, and a selection.
    await userEvent.click(checkbox("Library.tsx"));
    await userEvent.click(checkbox("useEditHistory.ts"));
    await userEvent.click(row("Library.tsx"));
    await userEvent.click(checkbox("onboarding.md"));

    await userEvent.click(rollbackButton());
    await waitFor(() => expect(checkbox("onboarding.md").checked).toBe(false));

    // CHG-FR-64: everything the rollback did not touch survives.
    expect(checkbox("Library.tsx").checked).toBe(true);
    expect(checkbox("useEditHistory.ts").checked).toBe(true);
    expect(row("Library.tsx")).toHaveAttribute("data-selected", "true");
  });
});

describe("the panel while a rollback runs (CHG-FR-60, EDT-FR-81, EDT-FR-72)", () => {
  it("is inert and cannot be submitted twice", async () => {
    backend({ uncommitted: changeSet([LIB, SKILL]) });
    let release!: (result: RollbackResult) => void;
    onRequestRollback.mockImplementation(
      (_files, onConfirmed) =>
        new Promise<RollbackResult>((resolve) => {
          // The shell signals this the moment the author confirms, which is
          // what starts the visible in-progress state (CHG-FR-60).
          onConfirmed();
          release = resolve;
        }),
    );
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");
    await userEvent.click(checkbox("Library.tsx"));

    await userEvent.click(rollbackButton());
    await waitFor(() => expect(onRequestRollback).toHaveBeenCalledTimes(1));

    // CHG-FR-60: the in-progress state is visible, so the wait on somebody
    // else's in-flight save is not mistaken for nothing happening.
    expect(await screen.findByText("Discarding…")).toBeInTheDocument();
    expect(rollbackButton()).toBeDisabled();

    // The rows are inert: a click on a checkbox changes nothing.
    await userEvent.click(checkbox("onboarding.md"));
    expect(checkbox("onboarding.md").checked).toBe(false);
    // And a second activation submits nothing.
    await userEvent.click(rollbackButton());
    expect(onRequestRollback).toHaveBeenCalledTimes(1);

    await act(async () => {
      release({
        outcome: { entries: [outcomeEntry("src/components/Library.tsx")] },
        saveFailures: [],
      });
    });
    await waitFor(() => expect(screen.queryByText("Discarding…")).toBeNull());
  });
});

describe("regressions the browser pass caught", () => {
  it("shows no in-progress label while the confirmation is still open", async () => {
    // The label announces an operation Escape can still cancel without
    // touching anything, so it waits for the confirmation signal (CHG-FR-60).
    backend({ uncommitted: changeSet([LIB]) });
    let confirm!: () => void;
    let release!: (r: RollbackResult) => void;
    onRequestRollback.mockImplementation(
      (_files, onConfirmed) =>
        new Promise<RollbackResult>((resolve) => {
          confirm = onConfirmed;
          release = resolve;
        }),
    );
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");
    await userEvent.click(checkbox("Library.tsx"));
    await userEvent.click(rollbackButton());
    await waitFor(() => expect(onRequestRollback).toHaveBeenCalled());

    // Confirmation is up: inert, but not yet announcing work.
    expect(rollbackButton()).toBeDisabled();
    expect(screen.queryByText("Discarding…")).toBeNull();

    await act(async () => {
      confirm();
    });
    expect(await screen.findByText("Discarding…")).toBeInTheDocument();

    await act(async () => {
      release({
        outcome: { entries: [outcomeEntry("src/components/Library.tsx")] },
        saveFailures: [],
      });
    });
  });

  it("keeps the split action control inert during a rollback (CHG-FR-60)", async () => {
    backend({ uncommitted: changeSet([LIB]) });
    let release!: (r: RollbackResult) => void;
    onRequestRollback.mockImplementation(
      (_files, onConfirmed) =>
        new Promise<RollbackResult>((resolve) => {
          onConfirmed();
          release = resolve;
        }),
    );
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");
    await userEvent.click(checkbox("Library.tsx"));
    await userEvent.click(rollbackButton());
    await screen.findByText("Discarding…");

    // The dropdown is deliberately operable when the selected action merely
    // cannot be performed (CHG-FR-49); a running rollback is the one condition
    // that closes it, the commit set being mid-change.
    expect(screen.getByRole("button", { name: "Choose action" })).toBeDisabled();

    await act(async () => {
      release({
        outcome: { entries: [outcomeEntry("src/components/Library.tsx")] },
        saveFailures: [],
      });
    });
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Choose action" }),
      ).toBeEnabled(),
    );
  });

  it("reports a save that failed while preparing (CHG-FR-61 / CHG-FR-65)", async () => {
    backend({ uncommitted: changeSet([LIB]) });
    onRequestRollback.mockImplementation(async (_files, onConfirmed) => {
      onConfirmed();
      return {
        outcome: { entries: [outcomeEntry("src/components/Library.tsx")] },
        saveFailures: [
          { artifactId: "src/components/Library.tsx", reason: "disk full" },
        ],
      };
    });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");
    await userEvent.click(checkbox("Library.tsx"));
    await userEvent.click(rollbackButton());

    // The author sees it as well as the log: a rollback whose only failure was
    // a save still shows no overall success state.
    const report = await screen.findByText(/could not be rolled back/i);
    expect(report).toBeInTheDocument();
    expect(
      screen.getByText(/an unsaved edit could not be written/i),
    ).toBeInTheDocument();
    expect(screen.queryByText(/^Discarded /)).toBeNull();
  });

  it("clears the failure report on a manual refresh (CHG-FR-65)", async () => {
    backend({ uncommitted: changeSet([LIB]) });
    onRequestRollback.mockImplementation(async (_files, onConfirmed) => {
      onConfirmed();
      return {
        outcome: {
          entries: [
            outcomeEntry("src/components/Library.tsx", {
              outcome: "failed",
              restoredPaths: [],
              failures: [
                { path: "src/components/Library.tsx", kind: "permission_denied" },
              ],
            }),
          ],
        },
        saveFailures: [],
      };
    });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");
    await userEvent.click(checkbox("Library.tsx"));
    await userEvent.click(rollbackButton());
    await screen.findByText(/could not be rolled back/i);

    // A stale report outliving the state it described would tell the author a
    // file they have since fixed by hand still could not be rolled back.
    await userEvent.click(
      screen.getByRole("button", { name: "Refresh changes" }),
    );
    await waitFor(() =>
      expect(screen.queryByText(/could not be rolled back/i)).toBeNull(),
    );
  });

  it("excludes a checked row the text filter is hiding (CHG-FR-57)", async () => {
    // The one place a rollback diverges from a commit: hidden paths are never
    // offered back (CHG-FR-48 is a commit's concern). Destroying a file the
    // author cannot see is the failure this prevents.
    backend({ uncommitted: changeSet([LIB, SKILL]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");
    await userEvent.click(checkbox("Library.tsx"));
    await userEvent.click(checkbox("onboarding.md"));

    // Narrow the text filter so only one of the two checked rows is visible.
    const filter = screen.getByPlaceholderText(/filter changes/i);
    await userEvent.type(filter, "Library");
    await waitFor(() => expect(rowNames()).not.toContain("onboarding.md"));

    await userEvent.click(rollbackButton());
    await waitFor(() => expect(onRequestRollback).toHaveBeenCalled());
    const files = onRequestRollback.mock.calls[0][0].map((f) => f.path);
    expect(files).toEqual(["src/components/Library.tsx"]);
    expect(files).not.toContain(".claude/skills/onboarding.md");
  });
});
