import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { CHANGES_UPDATED } from "../events";
import { resetAppPreferencesCache } from "../state/appPreferences";
import { resetPanelReveals } from "../state/panelReveal";
import {
  pickSelector,
  selectorValue,
  selectorValues,
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
  onRequestCommitMessage,
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
// Filters (CHG-FR-14, CHG-FR-15 .. CHG-FR-16, CHG-FR-09)
// ---------------------------------------------------------------------------

describe("filters (CHG-FR-14..FR-16)", () => {
  it("defaults to All artifacts and reveals unclassified files under All files (CHG-FR-14, CHG-FR-15)", async () => {
    const plain = change("package.json");
    backend({ uncommitted: changeSet([SKILL, plain]) });
    renderPanel();

    await screen.findByText("onboarding.md");
    expect(screen.queryByText("package.json")).not.toBeInTheDocument();
    expect(selectorValue("Filter by type")).toBe("artifacts");

    await pickSelector("Filter by type", "files");
    expect(await screen.findByText("package.json")).toBeInTheDocument();
    // Its folder appears with it — a folder shows only when it holds a visible
    // changed file (CHG-FR-08).
    expect(screen.getByText("onboarding.md")).toBeInTheDocument();
  });

  it("AND-combines the type and text filters (CHG-FR-14)", async () => {
    backend({ uncommitted: changeSet([SKILL, SPEC]) });
    renderPanel();
    await screen.findByText("onboarding.md");

    await pickSelector("Filter by type", "skill");
    expect(screen.queryByText("CHG-changes.md")).not.toBeInTheDocument();
    expect(screen.getByText("onboarding.md")).toBeInTheDocument();

    await userEvent.type(screen.getByLabelText("Filter changes"), "zzz");
    await waitFor(() =>
      expect(screen.queryByText("onboarding.md")).not.toBeInTheDocument(),
    );
    expect(document.querySelectorAll(".tree-row")).toHaveLength(0);
  });

  it("hides a group when nothing inside it is visible (CHG-FR-16, CHG-FR-09)", async () => {
    const untypedNew = change("scripts/build.sh", { changeStatus: "untracked" });
    backend({ uncommitted: changeSet([SKILL, untypedNew]) });
    renderPanel();

    await screen.findByText("onboarding.md");
    expect(screen.queryByText("Unrevisioned")).not.toBeInTheDocument();
    // The surviving group still renders its own label around what is left
    // rather than flattening it to the top level (CHG-FR-09).
    expect(screen.getByText("Revisioned")).toBeInTheDocument();

    await pickSelector("Filter by type", "files");
    expect(await screen.findByText("Unrevisioned")).toBeInTheDocument();
    expect(screen.getByText("build.sh")).toBeInTheDocument();
    expect(screen.getByText("scripts")).toBeInTheDocument();
  });

  it("hides the Revisioned group when only untracked files are visible (CHG-FR-16, CHG-FR-09)", async () => {
    const fresh = change("docs/draft.md", { changeStatus: "untracked" });
    backend({ uncommitted: changeSet([LIB, fresh]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Revisioned");

    await userEvent.type(screen.getByLabelText("Filter changes"), "draft");
    await waitFor(() =>
      expect(screen.queryByText("Revisioned")).not.toBeInTheDocument(),
    );
    expect(rowNames()).toEqual(["Unrevisioned", "docs", "draft.md"]);
  });

  it("keeps collapse and selection across a filter change and a reload (CHG-FR-17)", async () => {
    backend({ uncommitted: changeSet([SKILL, LIB, change("package.json")]) });
    renderPanel();
    await screen.findByText("onboarding.md");

    // Collapse `.claude` and select a file row.
    await userEvent.click(row(".claude"));
    expect(screen.queryByText("onboarding.md")).not.toBeInTheDocument();
    await pickSelector("Filter by type", "files");
    await screen.findByText("package.json");
    await userEvent.click(row("Library.tsx"));
    expect(row("Library.tsx")).toHaveAttribute("data-selected", "true");

    // A filter change keeps both.
    await pickSelector("Filter by type", "artifacts");
    await waitFor(() =>
      expect(screen.queryByText("package.json")).not.toBeInTheDocument(),
    );
    expect(row(".claude")).toBeInTheDocument();
    expect(screen.queryByText("onboarding.md")).not.toBeInTheDocument();

    // And so does a backend-driven reload.
    await pickSelector("Filter by type", "files");
    listeners[CHANGES_UPDATED]?.({ payload: { changeCount: 1 } });
    await waitFor(() => expect(calls("list_uncommitted_changes")).toHaveLength(2));
    expect(screen.queryByText("onboarding.md")).not.toBeInTheDocument();
    expect(row("Library.tsx")).toHaveAttribute("data-selected", "true");
  });
});

// ---------------------------------------------------------------------------
// The hidden-changes hand-over (CHG-FR-48)
// ---------------------------------------------------------------------------

describe("handing over what the filters hide (CHG-FR-48)", () => {
  const handedHidden = () => onRequestCommitMessage.mock.calls[0][1];

  it("CHG-FR-48 hands over the files the lens is hiding behind the ticked Spec", async () => {
    // The default All-artifacts lens hides exactly the work the Spec produced.
    const source = change("src/components/Changes.tsx");
    const test = change("src/components/Changes.test.tsx", {
      changeStatus: "untracked",
    });
    backend({ uncommitted: changeSet([SPEC, source, test]) });
    renderPanel();
    await screen.findByText("CHG-changes.md");
    expect(rowNames()).not.toContain("Changes.tsx");

    await userEvent.click(checkbox("CHG-changes.md"));
    await userEvent.click(primary());

    await waitFor(() => expect(onRequestCommitMessage).toHaveBeenCalled());
    expect(onRequestCommitMessage.mock.calls[0][0]).toEqual([
      { path: SPEC.path, untracked: false },
    ]);
    expect(handedHidden()).toEqual([
      { path: source.path, untracked: false },
      { path: test.path, untracked: true },
    ]);
  });

  it("CHG-FR-48 hands over nothing once the author can see everything", async () => {
    // A row the author can see and left unticked is a decision, not an
    // oversight, so it is never offered back.
    const source = change("src/components/Changes.tsx");
    backend({ uncommitted: changeSet([SPEC, source]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Changes.tsx");

    await userEvent.click(checkbox("CHG-changes.md"));
    await userEvent.click(primary());

    await waitFor(() => expect(onRequestCommitMessage).toHaveBeenCalled());
    expect(handedHidden()).toEqual([]);
  });

  it("CHG-FR-48 counts the text filter as hiding too", async () => {
    const source = change("src/components/Changes.tsx");
    backend({ uncommitted: changeSet([SPEC, source]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Changes.tsx");
    await userEvent.click(checkbox("CHG-changes.md"));

    await userEvent.type(screen.getByLabelText("Filter changes"), "CHG");
    await waitFor(() => expect(rowNames()).not.toContain("Changes.tsx"));

    await userEvent.click(primary());
    await waitFor(() => expect(onRequestCommitMessage).toHaveBeenCalled());
    expect(handedHidden()).toEqual([{ path: source.path, untracked: false }]);
  });
});

// ---------------------------------------------------------------------------
// SNV-FR-58: the Changes panel is the one with three narrowing
// controls, so it is where their order is worth pinning down.
// ---------------------------------------------------------------------------

describe("filter-row arrangement (SNV-FR-58)", () => {
  /** True when `a` precedes `b` in document order. */
  const precedes = (a: Element, b: Element) =>
    !!(a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING);

  it("descends text filter → mode toggle → type lens, above the tree", async () => {
    backend({ uncommitted: changeSet([LIB]) });
    renderPanel();
    await showAllFiles();
    await screen.findByText("Library.tsx");

    const controls = document.querySelector(".panel-controls")!;
    const text = screen.getByLabelText("Filter changes");
    const mode = screen.getByRole("radiogroup", { name: "Comparison mode" });
    const lens = screen.getByRole("radiogroup", { name: "Filter by type" });
    const body = document.querySelector(".vpanel__body")!;

    // All three are pinned in the shared control stack.
    [text, mode, lens].forEach((el) => expect(controls).toContainElement(el));

    // The text filter first, directly beneath the header, then the selectors
    // broadest first — the lens last and directly above what it narrows.
    expect(precedes(text, mode)).toBe(true);
    expect(precedes(mode, lens)).toBe(true);
    expect(precedes(controls, body)).toBe(true);
  });
});

// ---------------------------------------------------------------------------
// CHG-FR-14 / LIB-FR-19, SNV-FR-58: the lens is the toggle row of SNV-FR-62, offering the
// types this comparison actually touched (LIB-FR-19).
// ---------------------------------------------------------------------------

describe("the artifact-type lens (CHG-FR-14 / LIB-FR-19, SNV-FR-58, SNV-FR-62)", () => {
  /** True when `a` precedes `b` in document order. */
  const precedes = (a: Element, b: Element) =>
    !!(a.compareDocumentPosition(b) & Node.DOCUMENT_POSITION_FOLLOWING);

  it("offers only the types the change set touched, beneath the text filter", async () => {
    // One Skill, two Specs, and four files resolving to no type.
    backend({
      uncommitted: changeSet([
        SKILL,
        SPEC,
        change("specifications/ui/LIB-library.md", {
          artifactType: "spec",
          typeSource: "inferred",
        }),
        LIB,
        HOOK,
        change("package.json"),
        change("pnpm-lock.yaml"),
      ]),
    });
    renderPanel();
    await screen.findByText("onboarding.md");

    expect(selectorValues("Filter by type")).toEqual([
      "artifacts",
      "skill",
      "spec",
      "files",
    ]);

    // …and it sits beneath the text filter with the mode toggle between them.
    const text = screen.getByLabelText("Filter changes");
    const mode = screen.getByRole("radiogroup", { name: "Comparison mode" });
    const lens = screen.getByRole("radiogroup", { name: "Filter by type" });
    expect(precedes(text, mode)).toBe(true);
    expect(precedes(mode, lens)).toBe(true);

    // Activating a position narrows the tree, with no dropdown having opened.
    await pickSelector("Filter by type", "spec");
    expect(screen.getByText("CHG-changes.md")).toBeInTheDocument();
    expect(screen.getByText("LIB-library.md")).toBeInTheDocument();
    expect(screen.queryByText("onboarding.md")).not.toBeInTheDocument();
    expect(document.querySelector(".panel-controls select")).toBeNull();

    // …and the row still offers everything the *comparison* touched, not what
    // survived the narrowing. Computing it from the filtered entries would
    // leave Spec as the only type button, so the author's way back to Skill
    // would be through All artifacts (CHG-FR-14, per LIB-FR-19).
    expect(selectorValues("Filter by type")).toEqual([
      "artifacts",
      "skill",
      "spec",
      "files",
    ]);
  });

  it("gains a button when the comparison starts touching that type", async () => {
    let entries = [SKILL];
    backend({ uncommitted: () => changeSet(entries) });
    renderPanel();
    await screen.findByText("onboarding.md");
    expect(selectorValues("Filter by type")).toEqual([
      "artifacts",
      "skill",
      "files",
    ]);

    entries = [
      SKILL,
      change("flows/release.flow", {
        artifactType: "flow",
        typeSource: "inferred",
      }),
    ];
    act(() => {
      listeners[CHANGES_UPDATED]?.({ payload: undefined });
    });

    await waitFor(() =>
      expect(selectorValues("Filter by type")).toEqual([
        "artifacts",
        "skill",
        "flow",
        "files",
      ]),
    );
  });
});
