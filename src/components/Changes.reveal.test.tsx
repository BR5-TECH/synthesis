import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

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
  checkbox,
  makeBackend,
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

/**
 * CHG-FR-54 / CHG-FR-29, CHG-FR-27, CHG-FR-28 / CHG-FR-02, CHG-FR-05: revealing a changed file another surface
 * named.
 *
 * The one caller today is the shell following a Diff tab (SNV-FR-64), and the
 * requirement most of these are about is the one that is invisible in a passing
 * render: **a reveal touches no check**.
 */
describe("reveal a changed file (CHG-FR-54)", () => {
  it("CHG-FR-29, CHG-FR-54, CHG-FR-27, CHG-FR-28: expands ancestors, selects the file row, and leaves every check alone", async () => {
    backend({ uncommitted: changeSet([SKILL, LIB, HOOK]) });
    const { revealFile } = renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");

    // The author has ticked two of the three.
    await userEvent.click(checkbox("Library.tsx"));
    await userEvent.click(checkbox("onboarding.md"));
    expect(checkbox("Library.tsx").checked).toBe(true);
    expect(checkbox("onboarding.md").checked).toBe(true);
    expect(checkbox("useEditHistory.ts").checked).toBe(false);

    // Collapse the folder the target sits under, so the reveal has to open it.
    await userEvent.click(row("hooks"));
    await waitFor(() => expect(rowNames()).not.toContain("useEditHistory.ts"));

    act(() => revealFile("src/hooks/useEditHistory.ts"));

    const revealed = await waitFor(() => row("useEditHistory.ts"));
    expect(revealed).toHaveAttribute("data-selected", "true");
    // The row, never a folder or a group.
    expect(row("hooks")).toHaveAttribute("data-selected", "false");
    expect(row("Revisioned")).toHaveAttribute("data-selected", "false");
    // And not one check moved — neither the revealed row's nor anyone else's.
    expect(checkbox("useEditHistory.ts").checked).toBe(false);
    expect(checkbox("Library.tsx").checked).toBe(true);
    expect(checkbox("onboarding.md").checked).toBe(true);
  });

  it("CHG-FR-29, CHG-FR-54, CHG-FR-27, CHG-FR-28: relaxes the lens when it is hiding the row, preserving its check", async () => {
    backend({ uncommitted: changeSet([SKILL, LIB]) });
    const { revealFile } = renderPanel();
    // Tick `Library.tsx` while All files is showing it, then go back to the
    // default lens, which hides it (CHG-FR-29: the check is retained).
    await showAllFiles();
    await screen.findByText("Library.tsx");
    await userEvent.click(checkbox("Library.tsx"));
    await pickSelector("Filter by type", "artifacts");
    await waitFor(() => expect(rowNames()).not.toContain("Library.tsx"));

    act(() => revealFile("src/components/Library.tsx"));

    const revealed = await waitFor(() => row("Library.tsx"));
    expect(selectorValue("Filter by type")).toBe("files");
    expect(revealed).toHaveAttribute("data-selected", "true");
    // CHG-FR-29: it re-enters the commit set because it is visible again — the
    // reveal un-hid it, and did not tick it.
    expect(checkbox("Library.tsx").checked).toBe(true);
  });

  it("CHG-FR-54: leaves both filters alone when neither is hiding the row", async () => {
    backend({ uncommitted: changeSet([SKILL, LIB]) });
    const { revealFile } = renderPanel();
    await screen.findByText("onboarding.md");
    expect(selectorValue("Filter by type")).toBe("artifacts");

    act(() => revealFile(".claude/skills/onboarding.md"));

    await waitFor(() =>
      expect(row("onboarding.md")).toHaveAttribute("data-selected", "true"),
    );
    expect(selectorValue("Filter by type")).toBe("artifacts");
  });

  it("CHG-FR-54: places the row in the group the change set reports, not one it derives", async () => {
    const untracked = change("src/components/Changes.tsx", {
      changeStatus: "untracked",
      addedLines: 142,
    });
    backend({ uncommitted: changeSet([SKILL, untracked]) });
    const { revealFile } = renderPanel();
    await showAllFiles();
    await screen.findByText("Changes.tsx");

    act(() => revealFile("src/components/Changes.tsx"));

    await waitFor(() =>
      expect(row("Changes.tsx")).toHaveAttribute("data-selected", "true"),
    );
    // Under **Unrevisioned**, because that is where its `untracked` status puts
    // it — the reveal reclassified nothing.
    expect(row("Unrevisioned")).toBeInTheDocument();
    const groups = rowNames();
    expect(groups.indexOf("Changes.tsx")).toBeGreaterThan(
      groups.indexOf("Unrevisioned"),
    );
  });

  it("CHG-FR-54: scrolls the revealed row into view", async () => {
    // The defect a browser found and jsdom cannot: the panel selected a row that
    // sat below the fold, so the selection — the whole signal — was invisible.
    // jsdom implements no layout, so the call itself is what is observable here.
    const scrolled: HTMLElement[] = [];
    (HTMLElement.prototype as Partial<HTMLElement>).scrollIntoView =
      function (this: HTMLElement) {
        scrolled.push(this);
      };
    try {
      backend({ uncommitted: changeSet([SKILL, LIB, HOOK]) });
      const { revealFile } = renderPanel();
      await showAllFiles();
      await screen.findByText("useEditHistory.ts");

      act(() => revealFile("src/hooks/useEditHistory.ts"));

      await waitFor(() => expect(scrolled.length).toBeGreaterThan(0));
      // The file row itself, not an ancestor folder and not the group.
      const target = scrolled[scrolled.length - 1];
      expect(target.getAttribute("data-node-key")).toBe(
        "r:f:src/hooks/useEditHistory.ts",
      );
    } finally {
      delete (HTMLElement.prototype as Partial<HTMLElement>).scrollIntoView;
    }
  });

  it("CHG-FR-54: clears a text filter hiding the row, and leaves one that is not", async () => {
    backend({ uncommitted: changeSet([SKILL, LIB]) });
    const { revealFile } = renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");

    // A filter that admits the target must survive the reveal untouched.
    await userEvent.type(screen.getByLabelText("Filter changes"), "Library");
    await waitFor(() => expect(rowNames()).not.toContain("onboarding.md"));
    act(() => revealFile("src/components/Library.tsx"));
    await waitFor(() =>
      expect(row("Library.tsx")).toHaveAttribute("data-selected", "true"),
    );
    expect(screen.getByLabelText("Filter changes")).toHaveValue("Library");

    // One that hides it is cleared — and only then.
    act(() => revealFile(".claude/skills/onboarding.md"));
    await waitFor(() =>
      expect(row("onboarding.md")).toHaveAttribute("data-selected", "true"),
    );
    expect(screen.getByLabelText("Filter changes")).toHaveValue("");
  });

  it("SNV-FR-68: a finished reveal is not re-applied when the panel remounts", async () => {
    // `VPanel` unmounts this panel on every surface switch, so a guard held in
    // the component would let the reveal run again the moment the author came
    // back — over the row they had since selected.
    backend({ uncommitted: changeSet([SKILL, LIB]) });
    const { revealFile, unmount } = renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");

    act(() => revealFile("src/components/Library.tsx"));
    await waitFor(() =>
      expect(row("Library.tsx")).toHaveAttribute("data-selected", "true"),
    );

    // Away to another surface and back, with the request still standing.
    unmount();
    renderPanel();
    // A remounted panel starts on the default lens, so the unclassified file is
    // hidden until it is widened again — same as the author coming back to it.
    await showAllFiles();
    await screen.findByText("Library.tsx");
    await userEvent.click(row("onboarding.md"));
    await waitFor(() =>
      expect(row("onboarding.md")).toHaveAttribute("data-selected", "true"),
    );

    // Nothing dragged the selection back to the tab's own file.
    expect(row("Library.tsx")).toHaveAttribute("data-selected", "false");
  });

  it("CHG-FR-54, CHG-FR-02, CHG-FR-05: a file this comparison does not hold changes nothing at all", async () => {
    backend({ uncommitted: changeSet([SKILL, LIB]) });
    const { revealFile } = renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");
    await userEvent.click(checkbox("Library.tsx"));
    await userEvent.click(row("onboarding.md"));
    await waitFor(() =>
      expect(row("onboarding.md")).toHaveAttribute("data-selected", "true"),
    );
    const before = rowNames();

    act(() => revealFile("some/file/that/is/not/here.md"));

    // Mode, filters, checks, selection, tree — every one of them as it was, and
    // nothing rendered to explain it.
    expect(selectorValue("Comparison mode")).toBe("uncommitted");
    expect(selectorValue("Filter by type")).toBe("files");
    expect(checkbox("Library.tsx").checked).toBe(true);
    expect(row("onboarding.md")).toHaveAttribute("data-selected", "true");
    expect(rowNames()).toEqual(before);
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("CHG-FR-54, CHG-FR-02, CHG-FR-05: a reveal never switches mode or target branch", async () => {
    backend({
      panelState: { mode: "branch", targetBranch: "main" },
      branchChanges: changeSet([SPEC], "main"),
      uncommitted: changeSet([LIB]),
    });
    const { revealFile } = renderPanel();
    await screen.findByText("CHG-changes.md");
    expect(selectorValue("Comparison mode")).toBe("branch");

    // A Diff tab on a file the *uncommitted* comparison holds and this one does
    // not. Branch mode is the comparison the author chose; a reveal is not
    // entitled to change it.
    act(() => revealFile("src/components/Library.tsx"));

    expect(selectorValue("Comparison mode")).toBe("branch");
    expect(calls("list_uncommitted_changes")).toHaveLength(0);
    expect(rowNames()).toContain("CHG-changes.md");
  });

  it("SNV-FR-68: a second request for the same file re-selects it", async () => {
    backend({ uncommitted: changeSet([SKILL, LIB]) });
    const { revealFile } = renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");

    act(() => revealFile("src/components/Library.tsx"));
    await waitFor(() =>
      expect(row("Library.tsx")).toHaveAttribute("data-selected", "true"),
    );

    // The author selects another row, then returns to that Diff tab.
    await userEvent.click(row("onboarding.md"));
    await waitFor(() =>
      expect(row("onboarding.md")).toHaveAttribute("data-selected", "true"),
    );

    act(() => revealFile("src/components/Library.tsx"));

    await waitFor(() =>
      expect(row("Library.tsx")).toHaveAttribute("data-selected", "true"),
    );
  });

  it("CHG-FR-17: a change-set reload does not re-assert a finished reveal", async () => {
    backend({ uncommitted: changeSet([SKILL, LIB]) });
    const { revealFile } = renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");

    act(() => revealFile("src/components/Library.tsx"));
    await waitFor(() =>
      expect(row("Library.tsx")).toHaveAttribute("data-selected", "true"),
    );

    await userEvent.click(row("onboarding.md"));
    await waitFor(() =>
      expect(row("onboarding.md")).toHaveAttribute("data-selected", "true"),
    );

    // CHG-FR-19: `"changes updated"` reloads the set. The author's selection has
    // to survive it — the reveal is long finished.
    act(() => listeners[CHANGES_UPDATED]?.({ payload: {} }));
    await waitFor(() =>
      expect(calls("list_uncommitted_changes").length).toBeGreaterThan(1),
    );

    expect(row("onboarding.md")).toHaveAttribute("data-selected", "true");
  });
});
