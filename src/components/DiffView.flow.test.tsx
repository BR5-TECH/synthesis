import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";

import { resetDiffModes } from "../state/diffModes";
import { resetAppPreferencesCache } from "../state/appPreferences";
import type {
  FileRevisions,
} from "../types";
import {
  activeIn,
  awaitDiff,
  makeWireBackend,
  renderDiff,
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

// ---------------------------------------------------------------------------
// Rich rendering of a Flow (DFV-FR-33, DFV-FR-17 – DFV-FR-18)
// ---------------------------------------------------------------------------

describe("a Flow read as its plan (DFV-FR-33 – DFV-FR-36)", () => {
  const FLOW_PATH = "plans/payments.flow";

  /** A Flow body from the parts a test cares about. */
  const flow = (doc: {
    name?: string;
    items?: Array<Record<string, unknown>>;
    edges?: Array<Record<string, unknown>>;
  }) =>
    `${JSON.stringify(
      {
        version: 2,
        name: doc.name ?? "Payments",
        items: doc.items ?? [],
        edges: doc.edges ?? [],
      },
      null,
      2,
    )}\n`;

  /** A top-level milestone, so a test needs no hierarchy around it. */
  const node = (id: string, name: string, extra: Record<string, unknown> = {}) => ({
    id,
    kind: "milestone",
    name,
    position: { x: 0, y: 0 },
    size: { width: 640, height: 380 },
    ...extra,
  });

  /** Render a Flow diff in `mode`, waiting for the graph to land. */
  async function renderFlow(
    revisions: FileRevisions,
    mode: "unified" | "side_by_side" | "final" = "unified",
  ) {
    wireBackend({
      revisions,
      prefs: { diffRenderingMode: "rich", diffVisualizationMode: mode },
    });
    renderDiff(target(FLOW_PATH, uncommitted, "flow"));
    await waitFor(() =>
      expect(
        document.querySelector(
          ".diff-flow, [data-state='empty'], [data-state='error'], " +
            "[data-state='deleted'], [data-state='unchanged-graph']",
        ),
      ).toBeTruthy(),
    );
  }

  /** One rendered entry, by the title it shows. */
  const entryEl = (title: string): HTMLElement | undefined =>
    Array.from(document.querySelectorAll<HTMLElement>(".diff-flow__entry")).find(
      (n) => n.querySelector(".diff-flow__name")?.textContent === title,
    );

  const factsOf = (title: string) =>
    Array.from(
      entryEl(title)?.querySelectorAll<HTMLElement>(".diff-flow__fact") ?? [],
    ).map((n) => `${n.dataset.mark}:${n.textContent}`);

  // DFV-FR-33, DFV-FR-RGBK, DFV-FR-17: the whole point — a plan read as a plan.
  it("DFV-FR-33, DFV-FR-RGBK: renders added, removed and changed items and edges", async () => {
    await renderFlow({
      isBinary: false,
      old: flow({
        items: [node("n1", "Collect"), node("n2", "Draft"), node("n3", "Old item")],
        edges: [{ id: "e1", from: "n1", to: "n2", type: "blocks" }],
      }),
      new: flow({
        items: [
          node("n1", "Collect", { artifactIds: ["specs/gather.md"] }),
          node("n2", "Draft"),
          node("n4", "New item"),
        ],
        edges: [{ id: "e1", from: "n1", to: "n2", type: "relates" }],
      }),
    });

    expect(entryEl("New item")).toHaveAttribute("data-mark", "added");
    expect(entryEl("Old item")).toHaveAttribute("data-mark", "removed");
    expect(entryEl("Collect")).toHaveAttribute("data-mark", "changed");
    expect(factsOf("Collect")).toEqual(["added:+Referencespecs/gather.md"]);
    // An item entry names its own kind.
    expect(entryEl("Collect")?.querySelector(".diff-flow__kind")?.textContent).toBe(
      "milestone",
    );
    // The edge is named by its endpoints and states both types.
    expect(entryEl("Collect — Draft")).toHaveAttribute("data-mark", "changed");
    expect(factsOf("Collect — Draft")).toEqual(["changed:~Typeblocks → relates"]);
    // An item nothing happened to is not part of a unified rendering at all.
    expect(entryEl("Draft")).toBeUndefined();
  });

  // DFV-FR-RGBK, DFV-FR-35: each side names the kind its own revision holds.
  it("DFV-FR-RGBK, DFV-FR-35: a changed kind shows the old kind left and the new kind right", async () => {
    const epic = { id: "ep1", kind: "epic", name: "Checkout", parentId: "n1", position: { x: 0, y: 0 }, size: { width: 400, height: 300 } };
    await renderFlow(
      {
        isBinary: false,
        old: flow({
          items: [node("n1", "Payments"), epic, { id: "x1", kind: "defect", name: "Bug", parentId: "ep1", position: { x: 0, y: 0 } }],
        }),
        new: flow({
          items: [node("n1", "Payments"), epic, { id: "x1", kind: "feature", name: "Bug", parentId: "ep1", position: { x: 0, y: 0 }, size: { width: 400, height: 220 } }],
        }),
      },
      "side_by_side",
    );
    const row = Array.from(document.querySelectorAll<HTMLElement>(".diff-sbs__row")).find(
      (r) => r.textContent?.includes("Bug"),
    )!;
    const [left, right] = Array.from(row.querySelectorAll<HTMLElement>(".diff-flow__entry"));
    expect(left.querySelector(".diff-flow__kind")?.textContent).toBe("defect");
    expect(right.querySelector(".diff-flow__kind")?.textContent).toBe("feature");
  });

  // DFV-FR-17: the resolved artifact type decides, not the extension — a user's
  // assignment overrides the path convention here as it does everywhere else.
  it("DFV-FR-17: reads a Flow by its resolved type, whatever the file is called", async () => {
    const revisions: FileRevisions = {
      isBinary: false,
      old: flow({ items: [] }),
      new: flow({ items: [node("n1", "Step")] }),
    };
    wireBackend({ revisions, prefs: { diffRenderingMode: "rich" } });

    // Assigned `flow` under a name no convention would infer it from.
    renderDiff(target("plans/payments", uncommitted, "flow"));
    await waitFor(() => expect(entryEl("Step")).toBeTruthy());
    expect(screen.getByTestId("diff-flow-unified")).toBeInTheDocument();

    cleanup();
    resetDiffModes();
    resetAppPreferencesCache();
    // …and a `.flow` file the user assigned another type is not a Flow. It is
    // not Markdown either, so rich does not apply to it at all.
    wireBackend({ revisions, prefs: { diffRenderingMode: "rich" } });
    renderDiff(target("plans/payments.flow", uncommitted, "spec"));
    await awaitDiff();

    expect(document.querySelector(".diff-flow")).toBeNull();
    for (const toggle of within(toolbarGroup("Diff rendering")).getAllByRole("radio")) {
      expect(toggle).toBeDisabled();
    }
  });

  it("DFV-FR-17: renders source for a .flow the target carries no resolved type for", async () => {
    wireBackend({
      revisions: { isBinary: false, old: flow({}), new: flow({ name: "R2" }) },
      prefs: { diffRenderingMode: "rich" },
    });
    renderDiff(target(FLOW_PATH));
    await awaitDiff();

    expect(document.querySelector(".diff-flow")).toBeNull();
    for (const toggle of within(toolbarGroup("Diff rendering")).getAllByRole("radio")) {
      expect(toggle).toBeDisabled();
    }
  });

  // DFV-FR-18, DFV-FR-17: rich is offered for a Flow and refused for a file with no
  // reading beyond its text.
  it("DFV-FR-18, DFV-FR-17: enables the rendering toggles for a Flow and disables them for source code", async () => {
    await renderFlow({ isBinary: false, old: flow({}), new: flow({ name: "R2" }) });

    for (const toggle of within(toolbarGroup("Diff rendering")).getAllByRole("radio")) {
      expect(toggle).not.toBeDisabled();
    }
    expect(activeIn("Diff rendering")).toEqual(["Rich"]);

    cleanup();
    wireBackend({ prefs: { diffRenderingMode: "rich" } });
    renderDiff(target("src/App.tsx"));
    await awaitDiff();

    for (const toggle of within(toolbarGroup("Diff rendering")).getAllByRole("radio")) {
      expect(toggle).toBeDisabled();
    }
    expect(activeIn("Diff rendering")).toEqual(["Source"]);
  });

  // DFV-FR-34, DFV-FR-31: what each visualization mode shows of the same comparison.
  it("DFV-FR-34: shows only changes in unified, the whole Flow in final", async () => {
    const revisions: FileRevisions = {
      isBinary: false,
      old: flow({ items: [node("n1", "Kept")] }),
      new: flow({ items: [node("n1", "Kept"), node("n2", "Added")] }),
    };

    await renderFlow(revisions, "unified");
    expect(screen.getByTestId("diff-flow-unified")).toBeInTheDocument();
    expect(entryEl("Added")).toHaveAttribute("data-mark", "added");
    expect(entryEl("Kept")).toBeUndefined();

    cleanup();
    resetDiffModes();
    resetAppPreferencesCache();
    await renderFlow(revisions, "final");

    expect(screen.getByTestId("diff-flow-final")).toBeInTheDocument();
    expect(entryEl("Added")).toHaveAttribute("data-mark", "added");
    expect(entryEl("Kept")).toHaveAttribute("data-mark", "none");
  });

  // DFV-FR-34, last clause / DFV-FR-31: a Flow the comparison added reads as
  // the Flow it is rather than as a wall of marking.
  it("DFV-FR-34, DFV-FR-31: marks nothing in final when the comparison added the Flow", async () => {
    await renderFlow(
      {
        isBinary: false,
        old: null,
        new: flow({ items: [node("n1", "First"), node("n2", "Second")] }),
      },
      "final",
    );

    expect(entryEl("First")).toHaveAttribute("data-mark", "none");
    expect(entryEl("Second")).toHaveAttribute("data-mark", "none");
  });

  // DFV-FR-35, DFV-FR-13: each side states what its own revision holds.
  it("DFV-FR-35, DFV-FR-13: puts each revision's references on its own side in side-by-side", async () => {
    await renderFlow(
      {
        isBinary: false,
        old: flow({ items: [node("n1", "Step", { artifactIds: ["old.md"] })] }),
        new: flow({
          items: [node("n1", "Step", { artifactIds: ["new.md"] }), node("n2", "Fresh")],
        }),
      },
      "side_by_side",
    );

    const rows = document.querySelectorAll(".diff-sbs__row");
    expect(screen.getByTestId("diff-flow-side-by-side")).toBeInTheDocument();
    const stepRow = Array.from(rows).find((r) =>
      r.textContent?.includes("old.md"),
    ) as HTMLElement;
    const [left, right] = Array.from(
      stepRow.querySelectorAll<HTMLElement>(".diff-flow__entry"),
    );
    expect(left.textContent).toContain("old.md");
    expect(left.textContent).not.toContain("new.md");
    expect(right.textContent).toContain("new.md");
    expect(right.textContent).not.toContain("old.md");

    // An item only the new revision has faces inert filler on the left.
    const freshRow = Array.from(rows).find((r) =>
      r.textContent?.includes("Fresh"),
    ) as HTMLElement;
    expect(
      freshRow.querySelector('.diff-flow__entry[data-mark="filler"]'),
    ).toBeTruthy();
  });

  // DFV-FR-34: Final is the mode that has to name what an item lost, since the
  // outcome cannot otherwise show it.
  it("DFV-FR-34: names in final the reference an item dropped and the item that went", async () => {
    await renderFlow(
      {
        isBinary: false,
        old: flow({
          items: [
            node("n1", "Step", { artifactIds: ["dropped.md"], description: "was" }),
            node("n2", "Gone", { description: "its description" }),
          ],
        }),
        new: flow({ items: [node("n1", "Step")] }),
      },
      "final",
    );

    expect(entryEl("Step")).toHaveAttribute("data-mark", "changed");
    // The value is named, not left as a signed blank.
    expect(factsOf("Step")).toEqual([
      "removed:−Descriptionwas",
      "removed:−Referencedropped.md",
    ]);
    // …and the item that went is shown in the position it held, with its own.
    expect(entryEl("Gone")).toHaveAttribute("data-mark", "removed");
    expect(factsOf("Gone")).toEqual(["unchanged: Descriptionits description"]);
  });

  // DFV-FR-15 / DFV-FR-27, DFV-FR-18 for a Flow: the two states that are not a graph.
  it("DFV-FR-15, DFV-FR-27, DFV-FR-18: renders the deleted and binary states rather than a graph", async () => {
    // Deleted: Final says so outright, and the other modes render the removal.
    await renderFlow(
      { isBinary: false, old: flow({ items: [node("n1", "Step")] }), new: null },
      "final",
    );
    expect(
      document.querySelector('[data-state="deleted"]')?.textContent,
    ).toMatch(/does not exist in the new revision/);

    cleanup();
    resetDiffModes();
    resetAppPreferencesCache();
    await renderFlow(
      { isBinary: false, old: flow({ items: [node("n1", "Step")] }), new: null },
      "side_by_side",
    );
    expect(entryEl("Step")).toHaveAttribute("data-mark", "removed");
    // The Flow's own entry goes with the file rather than facing a ghost.
    const rows = Array.from(document.querySelectorAll(".diff-sbs__row"));
    for (const row of rows) {
      const right = row.querySelectorAll(".diff-flow__entry")[1] as HTMLElement;
      expect(right.dataset.mark).toBe("filler");
    }

    cleanup();
    resetDiffModes();
    resetAppPreferencesCache();
    wireBackend({
      revisions: { old: null, new: null, isBinary: true },
      prefs: { diffRenderingMode: "rich" },
    });
    renderDiff(target(FLOW_PATH, uncommitted, "flow"));

    await waitFor(() =>
      expect(document.querySelector('[data-state="binary"]')).toBeTruthy(),
    );
    expect(document.querySelector(".diff-flow")).toBeNull();
    for (const toggle of within(toolbarGroup("Diff rendering")).getAllByRole("radio")) {
      expect(toggle).toBeDisabled();
    }
  });

  // DFV-FR-28, DFV-FR-33, DFV-FR-36: the two states that are not a comparison.
  it("DFV-FR-28, DFV-FR-33, DFV-FR-36: names a revision it cannot read, and says when only the file moved", async () => {
    await renderFlow({ isBinary: false, old: "{ not json", new: flow({}) });

    expect(
      document.querySelector('[data-state="error"]')?.textContent,
    ).toMatch(/could not be read as a Flow: old revision.*not valid JSON/);

    cleanup();
    resetDiffModes();
    resetAppPreferencesCache();
    await renderFlow({ isBinary: false, old: flow({}), new: "{ not json" });

    expect(
      document.querySelector('[data-state="error"]')?.textContent,
    ).toMatch(/could not be read as a Flow: new revision.*not valid JSON/);
    expect(document.querySelector(".diff-flow")).toBeNull();

    cleanup();
    resetDiffModes();
    resetAppPreferencesCache();
    // DFV-FR-33, FGV-FR-07: a version 1 revision is no plan to compare.
    await renderFlow({
      isBinary: false,
      old: '{"version":1,"name":"R","nodes":[],"edges":[]}',
      new: flow({ name: "R" }),
    });
    expect(
      document.querySelector('[data-state="error"]')?.textContent,
    ).toMatch(/could not be read as a Flow: old revision: unknown Flow version 1/);

    cleanup();
    resetDiffModes();
    resetAppPreferencesCache();
    // Same plan, different bytes: the file changed and the Flow did not.
    await renderFlow({
      isBinary: false,
      old: '{"version":2,"name":"R","items":[],"edges":[]}',
      new: flow({ name: "R" }),
    });

    // Its own state, not the no-change state of DFV-FR-28: the comparison did
    // report a change to this file.
    expect(
      document.querySelector('[data-state="unchanged-graph"]')?.textContent,
    ).toMatch(/file changed, but the Flow it describes did not/);
    expect(document.querySelector('[data-state="empty"]')).toBeNull();
  });
});
