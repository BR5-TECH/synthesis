/**
 * The Library filter row: how it is arranged, which lenses it offers, and what
 * it keeps at a narrow panel width
 * (`../../specifications/ui/LIB-library.md`, LIB-FR-19, SNV-FR-58 … SNV-FR-63).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  screen,
  waitFor,
} from "@testing-library/react";

const invokeMock = vi.fn();
const unlistenMock = vi.fn();
// Captured event subscribers, keyed by event name, so a test can fire the
// backend `"project tree changed"` event by hand.
let listeners: Record<string, (event: { payload: TreeChangedPayload }) => void> =
  {};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    async (
      event: string,
      cb: (event: { payload: TreeChangedPayload }) => void,
    ) => {
      listeners[event] = cb;
      return unlistenMock;
    },
  ),
}));

import {
  baseTree,
  expandedAll,
  file,
  folder,
  makeTreeStubs,
  panelState,
  renderLibrary,
} from "../test/libraryFixtures";
import type { TreeChangedPayload } from "../types";
import { PROJECT_TREE_CHANGED } from "../events";
import {
  pickSelector,
  selectorButton,
  selectorValue,
  selectorValues,
} from "../test/selectors";
import { resetPanelReveals } from "../state/panelReveal";

const { mockInvoke, setLoadTree } = makeTreeStubs(invokeMock);

beforeEach(() => {
  // Module-level, like the preferences cache: reveal nonces are minted from
  // one counter for the app's life, so a suite that does not reset finds its
  // first request already consumed by the previous one.
  resetPanelReveals();
  invokeMock.mockReset();
  unlistenMock.mockReset();
  listeners = {};
});

afterEach(() => {
  cleanup();
});

// ---------------------------------------------------------------------------
// SNV-FR-58: the order every vertical panel arranges its pinned
// controls in — the text filter first, directly beneath the panel header, and
// the selectors beneath it, the narrowest directly above the list it narrows.
// ---------------------------------------------------------------------------

describe("filter-row arrangement (SNV-FR-58)", () => {
  it("puts the text filter above the type lens, both above the tree", async () => {
    setLoadTree(baseTree());
    renderLibrary();
    await screen.findByText("specifications");

    const controls = document.querySelector(".panel-controls")!;
    const lens = screen.getByRole("radiogroup", { name: "Filter by type" });
    const filter = screen.getByLabelText("Filter tree");

    // Both are pinned in the shared control stack rather than laid out ad hoc.
    expect(controls).toContainElement(lens);
    expect(controls).toContainElement(filter);

    // `compareDocumentPosition` reads render order, which is what decides the
    // visual order inside a column — jsdom lays nothing out, so geometry is not
    // available to assert on.
    expect(
      filter.compareDocumentPosition(lens) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();

    // And the stack precedes the scrollable body it narrows.
    const body = document.querySelector(".vpanel__body")!;
    expect(
      controls.compareDocumentPosition(body) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });
});

// ---------------------------------------------------------------------------
// LIB-FR-19 / LIB-FR-04, LIB-FR-10, SNV-FR-62 / SNV-FR-61, SNV-FR-63: the lens offers the types the project has
// rather than the eight it could have — and never takes away the one the
// author is standing on.
// ---------------------------------------------------------------------------

describe("the lens's present-type rule (LIB-FR-19)", () => {
  it("offers only the types the tree holds, bounded by the two sentinels (LIB-FR-19, LIB-FR-04, LIB-FR-10, SNV-FR-62)", async () => {
    // Skills, Specs, and a folder carrying a `scenario` assignment — and no
    // Prompt, Agent, Flow, Instructions, or Scratchpad anywhere.
    const drafts = folder("drafts", true, []);
    drafts.artifactType = "scenario";
    drafts.typeSource = "assigned";
    const tree = folder("", true, [
      folder(".claude", true, [
        file(".claude/onboarding.md", "skill", "inferred"),
      ]),
      folder("specifications", true, [
        file("specifications/LIB-library.md", "spec", "inferred"),
      ]),
      drafts,
    ]);
    setLoadTree(tree);
    renderLibrary();
    await screen.findByText("drafts");

    expect(selectorValues("Filter by type")).toEqual([
      "artifacts",
      "skill",
      "spec",
      "scenario",
      "files",
    ]);
    expect(selectorValue("Filter by type")).toBe("artifacts");

    // Each button discloses its type's full name, whatever the tag reads.
    expect(selectorButton("Filter by type", "scenario")).toHaveAttribute(
      "aria-label",
      "Scenario",
    );
    expect(
      selectorButton("Filter by type", "scenario").querySelector(
        ".selector-row__tip",
      ),
    ).toHaveTextContent("Scenario");
  });

  it("gains a type's button when the project's first file of it appears (LIB-FR-19, LIB-FR-04, LIB-FR-10, SNV-FR-62)", async () => {
    const before = folder("", true, [file("a.md", "skill", "inferred")]);
    const after = folder("", true, [
      file("a.md", "skill", "inferred"),
      file("release.flow", "flow", "inferred"),
    ]);
    let tree = before;
    mockInvoke(
      { load_project_tree: () => structuredClone(tree) },
      expandedAll(before),
    );
    renderLibrary();
    await screen.findByText("a.md");
    expect(selectorValues("Filter by type")).toEqual([
      "artifacts",
      "skill",
      "files",
    ]);

    tree = after;
    listeners[PROJECT_TREE_CHANGED]?.({ payload: { changeCount: 1 } });

    await waitFor(() =>
      expect(selectorValues("Filter by type")).toEqual([
        "artifacts",
        "skill",
        "flow",
        "files",
      ]),
    );
    // …without the active lens changing on the author's behalf.
    expect(selectorValue("Filter by type")).toBe("artifacts");
  });

  it("keeps a lens whose last file left, and narrows to nothing instead (LIB-FR-19, SNV-FR-61, SNV-FR-63)", async () => {
    const before = folder("", true, [
      file("a.md", "skill", "inferred"),
      file("s.md", "spec", "inferred"),
    ]);
    const after = folder("", true, [file("s.md", "spec", "inferred")]);
    let tree = before;
    mockInvoke(
      { load_project_tree: () => structuredClone(tree) },
      expandedAll(before),
    );
    renderLibrary();
    await screen.findByText("a.md");

    await pickSelector("Filter by type", "skill");
    expect(screen.getByText("a.md")).toBeInTheDocument();

    // The project's only Skill is deleted on disk.
    tree = after;
    listeners[PROJECT_TREE_CHANGED]?.({ payload: { changeCount: 1 } });
    await waitFor(() =>
      expect(screen.queryByText("a.md")).not.toBeInTheDocument(),
    );

    // SNV-FR-63: the active button is exempt, so it is still rendered and still
    // active — the lens was not changed on the author's behalf.
    expect(selectorValues("Filter by type")).toEqual([
      "artifacts",
      "skill",
      "spec",
      "files",
    ]);
    expect(selectorValue("Filter by type")).toBe("skill");

    // SNV-FR-61: the narrowed-to-nothing state, inside the tree's own region,
    // with both filter controls still present — not the centred empty block.
    expect(screen.getByText("No skill artifacts found.")).toBeInTheDocument();
    expect(document.querySelector(".panel-empty")).toBeNull();
    expect(screen.getByLabelText("Filter tree")).toBeInTheDocument();

    // Only once the author moves off it does the button go.
    await pickSelector("Filter by type", "artifacts");
    expect(selectorValues("Filter by type")).toEqual([
      "artifacts",
      "spec",
      "files",
    ]);
  });
});

// ---------------------------------------------------------------------------
// SNV-FR-63 / SNV-FR-33 at the panel, where the exempt positions have meaning:
// the leading one is All artifacts and the trailing one is All files, so what a
// narrow panel keeps is the two lenses that bound what it can admit.
//
// jsdom lays nothing out, so the widths are stubbed the way
// `SelectorRow.test.tsx` does — otherwise the row measures zero-wide and the
// clip never engages.
// ---------------------------------------------------------------------------

describe("the lens row at a narrow panel width (SNV-FR-63, SNV-FR-33)", () => {
  const BUTTON_W = 40;
  const originalOffsetWidth = Object.getOwnPropertyDescriptor(
    HTMLElement.prototype,
    "offsetWidth",
  );
  const originalClientWidth = Object.getOwnPropertyDescriptor(
    Element.prototype,
    "clientWidth",
  );
  let rowWidth = 0;
  let observers: (() => void)[] = [];

  beforeEach(() => {
    rowWidth = 1000;
    observers = [];
    Object.defineProperty(HTMLElement.prototype, "offsetWidth", {
      configurable: true,
      get(this: HTMLElement) {
        return this.dataset.value ? BUTTON_W : 0;
      },
    });
    Object.defineProperty(Element.prototype, "clientWidth", {
      configurable: true,
      get(this: Element) {
        return this.classList.contains("selector-row") ? rowWidth : 0;
      },
    });
    vi.stubGlobal(
      "ResizeObserver",
      class {
        constructor(cb: () => void) {
          observers.push(cb);
        }
        observe() {}
        disconnect() {}
      },
    );
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    if (originalOffsetWidth) {
      Object.defineProperty(
        HTMLElement.prototype,
        "offsetWidth",
        originalOffsetWidth,
      );
    }
    if (originalClientWidth) {
      Object.defineProperty(Element.prototype, "clientWidth", originalClientWidth);
    }
  });

  const resizeTo = async (width: number) => {
    await act(async () => {
      rowWidth = width;
      observers.forEach((cb) => cb());
    });
  };

  it("keeps All artifacts, All files, and the active lens at every width", async () => {
    // Four present types, so the row is All + 4 + Files = six positions.
    setLoadTree(
      folder("", true, [
        file("a.md", "skill", "inferred"),
        file("b.md", "agent", "inferred"),
        file("c.md", "spec", "inferred"),
        file("d.flow", "flow", "inferred"),
      ]),
    );
    renderLibrary();
    await screen.findByText("a.md");
    expect(selectorValues("Filter by type")).toHaveLength(6);

    // Stand on a lens in the middle of the row, then narrow past what fits.
    await pickSelector("Filter by type", "spec");
    await resizeTo(2 * BUTTON_W + 4);

    // The two sentinels bound what the lens can admit and the active one names
    // what it admits right now — those are what a narrow panel must not take.
    expect(selectorValues("Filter by type")).toEqual([
      "artifacts",
      "spec",
      "files",
    ]);
    expect(selectorValue("Filter by type")).toBe("spec");

    // Widening the panel is what reaches the rest (SNV-FR-33).
    await resizeTo(1000);
    expect(selectorValues("Filter by type")).toEqual([
      "artifacts",
      "skill",
      "agent",
      "spec",
      "flow",
      "files",
    ]);
  });
});

// ---------------------------------------------------------------------------
// LIB-FR-19's source: "the loaded tree", not "the part of it on screen".
// ---------------------------------------------------------------------------

describe("what the present-type rule reads (LIB-FR-19)", () => {
  it("counts types inside collapsed folders too", async () => {
    // A lens row computed from the rendered rows would offer no Skill button
    // until the author expanded `.claude`, and buttons would come and go as
    // they browsed.
    const tree = folder("", true, [
      folder(".claude", true, [file(".claude/a.md", "skill", "inferred")]),
      file("AGENTS.md", "agent", "inferred"),
    ]);
    setLoadTree(tree, panelState({ expandedPaths: [] }));
    renderLibrary();
    await screen.findByText("AGENTS.md");

    // The Skill file itself is not rendered — its folder is collapsed.
    expect(screen.queryByText("a.md")).not.toBeInTheDocument();
    expect(selectorValues("Filter by type")).toEqual([
      "artifacts",
      "skill",
      "agent",
      "files",
    ]);
  });

  it("offers the two sentinels alone when nothing is classified", async () => {
    setLoadTree(folder("", false, [file("notes.txt"), file("package.json")]));
    renderLibrary();
    await screen.findByText("No artifacts found.");

    // No type button can narrow this tree to anything, so the row offers the
    // default and the escape hatch and nothing between them.
    expect(selectorValues("Filter by type")).toEqual(["artifacts", "files"]);
  });
});
