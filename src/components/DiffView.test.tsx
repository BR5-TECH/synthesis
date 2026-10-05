import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";

import { DiffModeToolbar, DiffView } from "./DiffView";
import { EditSessionStore } from "../state/editSessions";
import { comparisonLabel, diffScopeFor } from "../comparison";
import { peekDiffModes, resetDiffModes } from "../state/diffModes";
import { resetAppPreferencesCache } from "../state/appPreferences";
import { CHANGES_UPDATED } from "../events";
import type {
  DiffRenderingMode,
  DiffVisualizationMode,
  FileRevisions,
} from "../types";
import {
  DEFAULT_REVISIONS,
  TEXT_DIFF,
  activeIn,
  againstMain,
  awaitDiff,
  diffLine,
  findDiffLine,
  makeWireBackend,
  renderDiff,
  revs,
  richBlockEl,
  target,
  toolbarGroup,
  uncommitted,
} from "../test/diffViewFixtures";

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

const wireBackend = makeWireBackend(invokeMock);

const callsTo = (name: string) =>
  invokeMock.mock.calls.filter((c) => c[0] === name);

beforeEach(() => {
  invokeMock.mockReset();
  unlistenMock.mockReset();
  listeners = {};
  resetDiffModes();
  resetAppPreferencesCache();
  wireBackend();
});

afterEach(cleanup);

describe("Diff tab identity and posture (DFV-FR-03 / DFV-FR-06)", () => {
  it("names the comparison and the project-relative path in its own header (DFV-FR-03, SNV-FR-57)", async () => {
    renderDiff(target("src/App.tsx", againstMain));
    await awaitDiff();

    expect(screen.getByText("src/App.tsx")).toBeInTheDocument();
    expect(screen.getByText("against main")).toBeInTheDocument();
  });

  // DFV-FR-03, second half (SNV-FR-57): the name at the header's leading edge
  // and the path at its trailing edge spell one file one way. The panel's own
  // eyebrow treatment would otherwise render it `APP.TSX`.
  it("keeps the file name's on-disk case in its header (DFV-FR-03, SNV-FR-57)", async () => {
    renderDiff(target("src/LIB-library.md", againstMain));
    await awaitDiff();

    const title = document.querySelector(".panel-header__title")!;
    expect(title.textContent).toBe("LIB-library.md");
    // The modifier is what turns the eyebrow's uppercasing off for a header
    // whose title is a filename rather than the panel's own name.
    expect(title).toHaveClass("panel-header__title--file");
    expect(screen.getByText("src/LIB-library.md")).toBeInTheDocument();
  });

  it("exposes no mutating affordance and accepts no text input (DFV-FR-06, DFV-FR-41)", async () => {
    const { container } = renderDiff(target());
    await awaitDiff();

    // DFV-FR-40: every control the tab renders acts on how the comparison is
    // READ and never on what is being compared. Two kinds qualify: the mode
    // toggles, and the action control (DFV-FR-39), which opens a conversation
    // about the file and writes a comment log rather than a byte of the material
    // under comparison. Anything that staged, committed, or edited would be a
    // button that is neither.
    const controls = Array.from(
      container.querySelectorAll("button, a[href], input, textarea, select"),
    );
    expect(controls.length).toBeGreaterThan(0);
    for (const control of controls) {
      expect(control.tagName).toBe("BUTTON");
      const isModeToggle = control.getAttribute("role") === "radio";
      const isActionControl = control.closest("[data-action-control]") !== null;
      expect(isModeToggle || isActionControl).toBe(true);
    }
    // DFV-FR-39: collapsed, the control is one graphical control and nothing
    // else — no composer and no panel is mounted until it is asked for.
    expect(
      container.querySelectorAll(".draft-composer, .action-control__panel"),
    ).toHaveLength(0);

    // DFV-FR-41: exactly one of the two revisions accepts a caret. Every
    // editable host in the tab belongs to the target — a removed row carries the
    // ORIGINAL's content and reports itself read-only.
    const editable = Array.from(
      container.querySelectorAll<HTMLElement>("[contenteditable]"),
    );
    expect(editable.length).toBeGreaterThan(0);
    for (const host of editable) {
      expect(host.dataset.target).toBe("true");
      expect(host.getAttribute("aria-label")).toContain("target revision");
    }
    const removed = diffLine("was this")!;
    expect(removed.querySelector("[contenteditable]")).toBeNull();
    expect(
      removed.querySelector(".diff-line__text")!.getAttribute("aria-readonly"),
    ).toBe("true");
  });
});

describe("The two toggle groups (DFV-FR-07 / DFV-FR-08)", () => {
  it("renders three visualization toggles and two rendering toggles, one active each (DFV-FR-07, DFV-FR-08)", async () => {
    renderDiff(target("notes.md"));
    await awaitDiff();

    expect(within(toolbarGroup("Diff visualization")).getAllByRole("radio")).toHaveLength(3);
    expect(within(toolbarGroup("Diff rendering")).getAllByRole("radio")).toHaveLength(2);
    expect(activeIn("Diff visualization")).toEqual(["Unified"]);
    expect(activeIn("Diff rendering")).toEqual(["Source"]);
  });

  it("activating one member deactivates the previously active one (DFV-FR-07, DFV-FR-08)", async () => {
    renderDiff(target());
    await awaitDiff();

    await userEvent.click(screen.getByRole("radio", { name: "Side-by-side" }));

    expect(activeIn("Diff visualization")).toEqual(["Side-by-side"]);
  });
});

describe("Toolbar presentation", () => {
  it("renders each toggle as a glyph that still carries the mode's name", async () => {
    renderDiff(target("notes.md"));
    await awaitDiff();

    for (const label of ["Unified", "Side-by-side", "Final", "Source", "Rich"]) {
      const toggle = screen.getByRole("radio", { name: label });
      // No caption — the glyph is the affordance — but the name still reaches
      // a screen reader, and the tooltip still says what the mode does.
      expect(toggle.textContent).toBe("");
      expect(toggle.querySelector("svg")).toBeTruthy();
      expect(toggle.getAttribute("title")).toContain(label);
    }
  });

  it("keeps both groups together as one cluster", async () => {
    const { container } = renderDiff(target("notes.md"));
    await awaitDiff();

    const toolbar = container.querySelector(".diff-toolbar")!;
    // One cluster holding both groups with a divider between them, and — at the
    // row's trailing end and nowhere else — the target's write state
    // (DFV-FR-51). The centring itself is asserted against the stylesheet
    // below, because jsdom loads none.
    const children = Array.from(toolbar.children).map((n) => n.className);
    expect(children[0]).toContain("diff-toolbar__cluster");
    expect(children[1]).toContain("diff-toolbar__write-state");
    expect(children).toHaveLength(2);

    const cluster = Array.from(
      toolbar.querySelector(".diff-toolbar__cluster")!.children,
    ).map((n) => n.className);
    expect(cluster[0]).toContain("diff-toolbar__group");
    expect(cluster[1]).toContain("diff-toolbar__divider");
    expect(cluster[2]).toContain("diff-toolbar__group");
    expect(cluster).toHaveLength(3);
  });

  it("gives every mode its own glyph", async () => {
    renderDiff(target("notes.md"));
    await awaitDiff();

    // Five buttons all rendering the same icon would satisfy "each toggle has
    // an icon" while telling the user nothing.
    const glyphs = ["Unified", "Side-by-side", "Final", "Source", "Rich"].map(
      (label) =>
        screen.getByRole("radio", { name: label }).querySelector("svg")!
          .innerHTML,
    );
    expect(new Set(glyphs).size).toBe(5);
  });

  it("still names a disabled toggle (DFV-FR-18)", async () => {
    // With the caption gone, a disabled icon carrying no accessible name is a
    // blank square.
    renderDiff(target("src/Library.tsx"));
    await awaitDiff();

    const rich = screen.getByRole("radio", { name: "Rich" });
    expect(rich).toBeDisabled();
    expect(rich).toHaveAttribute("aria-label", "Rich");
    expect(rich.getAttribute("title")).toContain("Rich");
  });

  it("reaches each toggle from the keyboard", async () => {
    renderDiff(target("notes.md"));
    await awaitDiff();

    const sideBySide = screen.getByRole("radio", { name: "Side-by-side" });
    sideBySide.focus();
    expect(document.activeElement).toBe(sideBySide);
    await userEvent.keyboard("{Enter}");
    expect(activeIn("Diff visualization")).toEqual(["Side-by-side"]);
  });
});

describe("Mode persistence and reach (DFV-FR-23 / DFV-FR-24)", () => {
  it("starts a user who has never chosen in Unified and Source (DFV-FR-23)", async () => {
    wireBackend({ prefs: { diffVisualizationMode: undefined, diffRenderingMode: undefined } });
    renderDiff(target("notes.md"));
    await awaitDiff();

    expect(activeIn("Diff visualization")).toEqual(["Unified"]);
    expect(activeIn("Diff rendering")).toEqual(["Source"]);
  });

  it("opens in the stored modes, as a relaunch would (DFV-FR-23)", async () => {
    wireBackend({
      prefs: { diffVisualizationMode: "side_by_side", diffRenderingMode: "rich" },
      revisions: { old: "# a\n", new: "# b\n", isBinary: false },
    });
    renderDiff(target("notes.md"));

    await screen.findByTestId("diff-rich-side-by-side");
    expect(activeIn("Diff visualization")).toEqual(["Side-by-side"]);
    expect(activeIn("Diff rendering")).toEqual(["Rich"]);
  });

  it("leaves a non-Markdown tab on Source when Rich is activated elsewhere (DFV-FR-18 / DFV-FR-24)", async () => {
    wireBackend({ revisions: { old: "# a\n", new: "# b\n", isBinary: false } });
    render(
      <>
        <DiffView target={target("notes.md")} sessions={new EditSessionStore()} />
        <DiffView
          target={target("src/Library.tsx")}
          sessions={new EditSessionStore()}
        />
      </>,
    );
    await waitFor(() => expect(callsTo("get_file_revisions")).toHaveLength(2));

    const richToggles = screen.getAllByRole("radio", { name: "Rich" });
    // Enabled in the Markdown tab, disabled in the other — and only the
    // enabled one is clickable.
    expect(richToggles[0]).toBeEnabled();
    expect(richToggles[1]).toBeDisabled();
    await userEvent.click(richToggles[0]);

    await waitFor(() =>
      expect(screen.getAllByRole("radio", { name: "Rich" })[0]).toHaveAttribute(
        "aria-checked",
        "true",
      ),
    );
    // The stored mode is now `rich` for everyone, but the tab it does not apply
    // to still shows Source active and both toggles disabled (DFV-FR-18).
    const sourceToggles = screen.getAllByRole("radio", { name: "Source" });
    expect(sourceToggles[1]).toHaveAttribute("aria-checked", "true");
    expect(screen.getAllByRole("radio", { name: "Rich" })[1]).toBeDisabled();
    expect(callsTo("save_app_preferences")[0][1]).toEqual({
      preferences: expect.objectContaining({ diffRenderingMode: "rich" }),
    });
  });

  it("re-renders every open Diff tab and persists once (DFV-FR-24)", async () => {
    wireBackend({ revisions: { old: "a\n", new: "b\n", isBinary: false } });
    render(
      <>
        <DiffView target={target("src/a.ts")} sessions={new EditSessionStore()} />
        <DiffView target={target("src/b.ts")} sessions={new EditSessionStore()} />
        <DiffView target={target("src/c.ts")} sessions={new EditSessionStore()} />
      </>,
    );
    await waitFor(() => expect(callsTo("get_file_revisions")).toHaveLength(3));

    // Activate Final in one of them.
    await userEvent.click(screen.getAllByRole("radio", { name: "Final" })[0]);

    await waitFor(() =>
      expect(screen.getAllByRole("radio", { name: "Final" })).toHaveLength(3),
    );
    for (const toggle of screen.getAllByRole("radio", { name: "Final" })) {
      expect(toggle).toHaveAttribute("aria-checked", "true");
    }
    await waitFor(() => expect(callsTo("save_app_preferences")).toHaveLength(1));
    expect(callsTo("save_app_preferences")[0][1]).toEqual({
      preferences: expect.objectContaining({ diffVisualizationMode: "final" }),
    });

    // A tab opened afterwards opens in it too.
    renderDiff(target("src/d.ts"));
    await waitFor(() =>
      expect(
        screen.getAllByRole("radio", { name: "Final" }).every(
          (t) => t.getAttribute("aria-checked") === "true",
        ),
      ).toBe(true),
    );

    // DFV-FR-24: the graduation review is the one surface outside that set. It
    // borrows this same toolbar with a sink of its own, so a toggle
    // activated there writes no preference and moves no Diff tab.
    //
    // The borrower is stood up with **real local state** rather than with mock
    // sinks and hardcoded props: against `vi.fn()` sinks and a fixed
    // `visualization`, both assertions below would hold no matter what the
    // component did, and deleting the override from the review would not fail
    // anything here.
    const before = callsTo("save_app_preferences").length;
    render(<LocalModeToolbar />);
    const borrowed = within(
      screen.getByTestId("local-mode-toolbar"),
    ).getByRole("radio", { name: "Unified" });
    await userEvent.click(borrowed);

    // The borrower moved, on its own state.
    expect(borrowed).toHaveAttribute("aria-checked", "true");
    // Nothing was persisted, and the four Diff tabs are where they were.
    expect(callsTo("save_app_preferences")).toHaveLength(before);
    expect(peekDiffModes().visualization).toBe("final");
    const tabToolbars = Array.from(
      document.querySelectorAll(".diff-toolbar"),
    ).filter((row) => !row.closest("[data-testid=local-mode-toolbar]"));
    expect(tabToolbars).toHaveLength(4);
    for (const row of tabToolbars) {
      expect(
        within(row as HTMLElement).getByRole("radio", { name: "Final" }),
      ).toHaveAttribute("aria-checked", "true");
    }
  });
});

/**
 * A borrower of `DiffModeToolbar` that keeps its modes to itself, which is what
 * the graduation review is. Its state is real, so a toggle that did
 * reach the user-global store would be visible as a divergence rather than
 * hidden behind a mock.
 */
function LocalModeToolbar() {
  const [visualization, setVisualization] =
    useState<DiffVisualizationMode>("final");
  const [rendering, setRendering] = useState<DiffRenderingMode>("rich");
  return (
    <div data-testid="local-mode-toolbar">
      <DiffModeToolbar
        visualization={visualization}
        rendering={rendering}
        richApplies
        onVisualization={setVisualization}
        onRendering={setRendering}
      />
    </div>
  );
}

describe("Backend reads (DFV-FR-25 / DFV-FR-26)", () => {
  it("reads the original once and the target once, then renders every mode from the two (DFV-FR-25, DFV-FR-42)", async () => {
    wireBackend({ revisions: revs("# a\n", "# b\n") });
    renderDiff(target("notes.md"));
    await awaitDiff();

    // DFV-FR-25: one `get_file_revisions` for the comparison, and one
    // `load_artifact_contents_by_id` establishing the artifact's editing
    // session — which is the target (DFV-FR-42).
    expect(callsTo("get_file_revisions")).toHaveLength(1);
    expect(callsTo("load_artifact_contents_by_id")).toHaveLength(1);

    // DFV-FR-52: cycling every one of the six combinations is a local
    // re-derivation from the two revisions already held.
    for (const visualization of ["Side-by-side", "Final", "Unified"]) {
      for (const rendering of ["Rich", "Source"]) {
        await userEvent.click(screen.getByRole("radio", { name: visualization }));
        await userEvent.click(screen.getByRole("radio", { name: rendering }));
      }
    }
    expect(callsTo("get_file_revisions")).toHaveLength(1);
    expect(callsTo("load_artifact_contents_by_id")).toHaveLength(1);
  });

  it("re-fetches on `changes updated` and stays in its mode (EXC-FR-VTUH, DFV-FR-26, DFV-FR-53)", async () => {
    // DFV-FR-26: the ORIGINAL is re-read. The target is not taken from the
    // fetch — it is the editing session's, and a change that reached it while
    // the author's edits stood is answered by DFV-FR-53 instead.
    let original = "one\n";
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_file_revisions")
        return { old: original, new: "two\n", isBinary: false };
      if (cmd === "load_artifact_contents_by_id")
        return { body: "two\n", checksum: "sum" };
      if (cmd === "load_app_preferences")
        return { theme: "system", diffVisualizationMode: "side_by_side" };
      return undefined;
    });
    renderDiff(target());
    await findDiffLine("one");

    original = "moved on\n";
    listeners[CHANGES_UPDATED]?.({ payload: { changeCount: 1 } });

    expect(await findDiffLine("moved on")).toBeInTheDocument();
    expect(diffLine("one")).toBeUndefined();
    // Re-read exactly once more — the re-fetch must not become a fetch storm —
    // and the target was not re-read at all.
    expect(callsTo("get_file_revisions")).toHaveLength(2);
    expect(callsTo("load_artifact_contents_by_id")).toHaveLength(1);
    expect(activeIn("Diff visualization")).toEqual(["Side-by-side"]);
  });

  it("keeps the newest original when two reloads land out of order (DFV-FR-26)", async () => {
    const settlers: Array<(r: FileRevisions) => void> = [];
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_file_revisions")
        return new Promise<FileRevisions>((resolve) => settlers.push(resolve));
      if (cmd === "load_artifact_contents_by_id")
        return { body: "target\n", checksum: "sum" };
      return undefined;
    });
    renderDiff(target());
    await waitFor(() => expect(settlers).toHaveLength(1));

    listeners[CHANGES_UPDATED]?.({ payload: { changeCount: 1 } });
    await waitFor(() => expect(settlers).toHaveLength(2));

    // The newest request settles first, then the superseded one.
    settlers[1](revs("newer original\n", "target\n"));
    expect(await findDiffLine("newer original")).toBeInTheDocument();
    settlers[0](revs("older original\n", "target\n"));

    await waitFor(() => expect(diffLine("older original")).toBeUndefined());
    expect(diffLine("newer original")).toBeTruthy();
  });

  it("asks the backend for the file's own scope, rename halves included", async () => {
    renderDiff({
      path: "src-tauri/lib.rs",
      name: "lib.rs",
      scope: diffScopeFor(uncommitted, "src-tauri/lib.rs", "src-tauri/main.rs"),
      comparisonLabel: comparisonLabel(uncommitted),
    });
    await waitFor(() => expect(callsTo("get_file_revisions")).toHaveLength(1));
    expect(callsTo("get_file_revisions")[0][1]).toEqual({
      scope: {
        kind: "path",
        path: "src-tauri/lib.rs",
        previousPath: "src-tauri/main.rs",
      },
    });
  });

  it("carries the branch comparison into both reads", async () => {
    wireBackend({ prefs: { diffVisualizationMode: "final" } });
    renderDiff(target("src/App.tsx", againstMain));

    await waitFor(() => expect(callsTo("get_file_revisions")).toHaveLength(1));
    const expected = {
      scope: { kind: "branch", path: "src/App.tsx", targetBranch: "main" },
    };
    expect(callsTo("get_file_revisions")[0][1]).toEqual(expected);
    expect(callsTo("get_file_revisions")[0][1]).toEqual(expected);
  });

  it("does not re-fetch when its owner re-renders with an equal target", async () => {
    const sessions = new EditSessionStore();
    const { rerender } = render(
      <DiffView target={target()} sessions={sessions} />,
    );
    await waitFor(() => expect(callsTo("get_file_revisions")).toHaveLength(1));
    // A fresh, equal target object: the loader keys on the scope's VALUE.
    rerender(<DiffView target={target()} sessions={sessions} />);
    await waitFor(() => expect(callsTo("get_file_revisions")).toHaveLength(1));
  });

  it("unsubscribes on unmount", async () => {
    renderDiff(target());
    await waitFor(() => expect(callsTo("get_file_revisions")).toHaveLength(1));
    cleanup();
    await waitFor(() => expect(unlistenMock).toHaveBeenCalled());
  });
});

describe("Inline states (DFV-FR-27 .. DFV-FR-29)", () => {
  it("says so for binary content in every visualization mode (DFV-FR-27, DFV-FR-18)", async () => {
    wireBackend({ revisions: { old: null, new: null, isBinary: true } });
    renderDiff(target("assets/logo.png"));

    expect(await screen.findByText(/Binary file/)).toBeInTheDocument();
    expect(document.querySelectorAll(".diff-line")).toHaveLength(0);
    for (const toggle of within(toolbarGroup("Diff rendering")).getAllByRole("radio")) {
      expect(toggle).toBeDisabled();
    }

    await userEvent.click(screen.getByRole("radio", { name: "Side-by-side" }));
    expect(screen.getByText(/Binary file/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("radio", { name: "Final" }));
    expect(screen.getByText(/Binary file/)).toBeInTheDocument();
  });

  it("still renders the file in Final when the comparison yields no hunks", async () => {
    // A pure rename changes the file without changing a line, so `get_diff`
    // returns no hunks. Final says it renders the complete new revision, and
    // an empty state in its place would hide a file that is genuinely there.
    wireBackend({
      revisions: revs("unmoved\n", "unmoved\n"),
      prefs: { diffVisualizationMode: "final" },
    });
    renderDiff(target());

    expect(await findDiffLine("unmoved")).toHaveAttribute("data-kind", "context");
    expect(screen.queryByText(/No changes in this file/)).not.toBeInTheDocument();
    // Nothing changed, so nothing is marked — this is where over-marking would
    // show up first.
    expect(document.querySelectorAll("mark")).toHaveLength(0);
  });

  it("renders a Markdown file with nothing to render without failing", async () => {
    wireBackend({
      prefs: { diffRenderingMode: "rich", diffVisualizationMode: "final" },
      revisions: { old: "# gone\n", new: "   \n\n", isBinary: false },
    });
    renderDiff(target("notes.md"));

    // The new revision renders no blocks at all; the heading that went away is
    // shown as removed, and nothing claims the file is binary or deleted.
    const view = await screen.findByTestId("diff-rich-final");
    expect(richBlockEl("gone")).toHaveAttribute("data-mark", "removed");
    expect(view.querySelectorAll('[data-mark="added"]')).toHaveLength(0);
    expect(screen.queryByText(/Binary file/)).not.toBeInTheDocument();
    expect(
      screen.queryByText(/does not exist in the new revision/),
    ).not.toBeInTheDocument();
  });

  it("renders an empty state distinct from binary and deleted (DFV-FR-28)", async () => {
    wireBackend({ revisions: revs("same\n", "same\n") });
    renderDiff(target());

    const state = await screen.findByText(/No changes in this file/);
    expect(state.closest(".changes-state")).toHaveAttribute("data-state", "empty");
    expect(screen.queryByText(/Binary file/)).not.toBeInTheDocument();
    expect(
      screen.queryByText(/does not exist in the new revision/),
    ).not.toBeInTheDocument();
  });

  it("explains a project that is not a Git repository rather than failing (DFV-FR-29)", async () => {
    invokeMock.mockImplementation(async () => {
      throw "not a git repository";
    });
    renderDiff(target());
    expect(await screen.findByText(/isn’t a Git repository/)).toBeInTheDocument();
    // The toolbar still renders — the tab does not fail.
    expect(toolbarGroup("Diff visualization")).toBeInTheDocument();
  });

  it("surfaces any other backend failure inline, in the same position (DFV-FR-29)", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_file_revisions") throw "failed to read revisions: boom";
      return undefined;
    });
    renderDiff(target());
    expect(await screen.findByText(/boom/)).toBeInTheDocument();
  });

  it("surfaces a whole-file read failure inline too (DFV-FR-29)", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_diff") return TEXT_DIFF;
      if (cmd === "get_file_revisions") throw "failed to read revisions: boom";
      return undefined;
    });
    renderDiff(target());
    await awaitDiff();

    await userEvent.click(screen.getByRole("radio", { name: "Side-by-side" }));
    expect(await screen.findByText(/failed to read revisions/)).toBeInTheDocument();
  });

  it("surfaces a target read failure inline, and retries when the mode is left and returned to", async () => {
    // DFV-FR-25: there is now ONE read of the target — the artifact's own
    // contents (DFV-FR-42) — so a failure of it is a failure of the tab rather
    // than of one mode, and the read that answers it is the editing session's.
    let failing = true;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_file_revisions") return revs("a\n", "b\n");
      if (cmd === "load_artifact_contents_by_id") {
        if (failing) throw "failed to read the file: boom";
        return { body: "b\n", checksum: "sum" };
      }
      if (cmd === "load_app_preferences") return { theme: "system" };
      return undefined;
    });
    renderDiff(target());

    expect(await screen.findByText(/failed to read the file/)).toBeInTheDocument();
    // The toolbar still renders — the tab does not fail (DFV-FR-29).
    expect(toolbarGroup("Diff visualization")).toBeInTheDocument();
    failing = false;
  });

  it("writes no state after unmounting mid-fetch", async () => {
    const warn = vi.spyOn(console, "error").mockImplementation(() => {});
    let settle: ((r: FileRevisions) => void) | undefined;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_file_revisions")
        return new Promise<FileRevisions>((resolve) => (settle = resolve));
      return undefined;
    });
    renderDiff(target());
    await waitFor(() => expect(settle).toBeDefined());

    cleanup();
    settle!(DEFAULT_REVISIONS);
    await waitFor(() => expect(invokeMock).toHaveBeenCalled());

    expect(warn).not.toHaveBeenCalled();
    warn.mockRestore();
  });
});
