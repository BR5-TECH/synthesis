import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { FlowCanvas } from "./Flow";
import { FlowSessionStore } from "../state/flowSessions";
import { AUTOSAVE_DELAY_MS } from "../state/writeSchedule";
import { renameNode } from "../state/flowDocument";
import {
  addFromMenu,
  canvas,
  connectNodes,
  doc,
  dragNode,
  ID,
  nodeEl,
  selectNode,
  SKILL_ID,
  TREE,
  TWO_NODES,
} from "../test/flowFixtures";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

let body = "";
let writes: string[] = [];
/**
 * FGV-FR-02: what `"validate flow document"` answers. Valid by default — the
 * rule set is the Rust suite's business (FGV-FR-05..14); what these tests care
 * about is that the canvas asks before it renders (FLO-FR-46) and what it shows
 * when the answer is `No` (FLO-FR-05).
 */
let validation: { valid: boolean; violations: Array<Record<string, string>> } = {
  valid: true,
  violations: [],
};

beforeEach(() => {
  writes = [];
  validation = { valid: true, violations: [] };
  invokeMock.mockReset();
  invokeMock.mockImplementation(
    async (cmd: string, args: Record<string, string>) => {
      switch (cmd) {
        case "load_project_tree":
          return TREE;
        case "validate_flow_document":
          return validation;
        case "load_artifact_contents_by_id":
          return { body, checksum: "ck1" };
        case "save_artifact_contents":
          writes.push(args.body);
          return { checksum: "ck2" };
        case "open_artifact_by_id":
          return { key: args.id, kind: "markdown" };
        default:
          throw new Error(`unexpected invoke ${cmd}`);
      }
    },
  );
});

afterEach(cleanup);

/** Load `content` as the Flow's body and render a canvas over it. */
async function renderCanvas(content: string) {
  body = content;
  const onOpenArtifact = vi.fn();
  const flows = new FlowSessionStore();
  await act(async () => {
    flows.openTab(ID);
    await flows.get(ID)?.pendingLoad;
  });
  const view = render(
    <FlowCanvas flowId={ID} flows={flows} onOpenArtifact={onOpenArtifact} />,
  );
  // The artifact list is fetched on mount; wait for it so reference resolution
  // is settled rather than in its `pending` state.
  await waitFor(() =>
    expect(
      invokeMock.mock.calls.some((c) => c[0] === "load_project_tree"),
    ).toBe(true),
  );
  // The call having been ISSUED is not the same as its result having been
  // applied. Until the tree lands, `treeLoaded` is false and every reference
  // renders in the `pending` shape — a bare id with no click-through, so a test
  // that clicks "Open <name>" finds nothing (FLO-FR-11). Flush the resolution
  // and the state update it causes, which is what "settled" above claims.
  await act(async () => {});
  return { flows, view, onOpenArtifact };
}

describe("opening a Flow (FLO-FR-01, FLO-FR-03 / FLO-FR-04, FLO-FR-05, FLO-FR-46)", () => {
  it("renders the nodes and edges the deserialized document describes", async () => {
    await renderCanvas(TWO_NODES);

    expect(invokeMock).toHaveBeenCalledWith("load_artifact_contents_by_id", {
      id: ID,
    });
    expect(nodeEl("n1")).toBeInTheDocument();
    expect(nodeEl("n2")).toBeInTheDocument();
    expect(screen.getByTestId("flow-edge-e1")).toHaveTextContent("ok");
    expect(screen.getByTestId("flow-status")).toHaveTextContent(
      "2 nodes · 1 edge",
    );
  });

  // FLO-FR-04, FLO-FR-05, FLO-FR-46: a zero-byte Flow opens ready to edit, and a malformed one shows
  // an error state that offers no editing and writes nothing.
  it("opens a zero-byte file as an empty graph ready for editing", async () => {
    await renderCanvas("");

    expect(canvas()).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(screen.getByTestId("flow-status")).toHaveTextContent(
      "0 nodes · 0 edges",
    );
    expect(screen.getByLabelText("Add to flow")).toBeInTheDocument();
  });

  it("renders an error state for a malformed body, with no editing and no write", async () => {
    const { flows } = await renderCanvas("{ not json");

    expect(screen.getByRole("alert")).toHaveTextContent(
      /could not be read as a Flow/,
    );
    expect(screen.getByRole("alert")).toHaveTextContent(/not valid JSON/);
    expect(canvas()).toBeNull();
    // The add affordance lives on the canvas, so a tab with no graph to edit
    // renders none at all.
    expect(screen.queryByLabelText("Add to flow")).not.toBeInTheDocument();

    await act(async () => {
      await flows.flush(ID, { force: true });
    });
    expect(writes).toEqual([]);
  });
});

describe("the Flow's name and description (FLO-FR-25, FLO-FR-26, FLO-FR-28)", () => {
  it("edits both in place at the top of the tab and marks the Flow unsaved", async () => {
    const { flows } = await renderCanvas(TWO_NODES);

    fireEvent.change(screen.getByLabelText("Flow name"), {
      target: { value: "Onboarding review" },
    });
    expect(doc(flows).name).toBe("Onboarding review");

    fireEvent.change(screen.getByLabelText("Flow description"), {
      target: { value: "Reviews a new joiner's first PR." },
    });
    expect(doc(flows).description).toBe("Reviews a new joiner's first PR.");
    expect(flows.get(ID)?.dirty).toBe(true);

    // A save writes both.
    await act(async () => {
      await flows.flush(ID);
    });
    expect(JSON.parse(writes[0])).toMatchObject({
      name: "Onboarding review",
      description: "Reviews a new joiner's first PR.",
    });

    // The description is optional: emptying it removes the field rather than
    // storing "", so a cleared description writes what one never set does.
    fireEvent.change(screen.getByLabelText("Flow description"), {
      target: { value: "" },
    });
    expect("description" in doc(flows)).toBe(false);

    await act(async () => {
      await flows.flush(ID);
    });
    const written = JSON.parse(writes[1]);
    expect(written.name).toBe("Onboarding review");
    expect("description" in written).toBe(false);
  });

  // Required means marked, never enforced: a Flow that refused to save until it
  // was named would cost the author the graph they just drew.
  it("marks a missing name without blocking anything", async () => {
    const { flows } = await renderCanvas(TWO_NODES);

    expect(screen.getByTestId("flow-name-required")).toBeInTheDocument();
    expect(screen.getByLabelText("Flow name")).toHaveAttribute(
      "aria-invalid",
      "true",
    );

    await act(async () => {
      await flows.flush(ID, { force: true });
    });
    expect(writes).toHaveLength(1);
    expect(JSON.parse(writes[0]).name).toBe("");

    fireEvent.change(screen.getByLabelText("Flow name"), {
      target: { value: "Named" },
    });
    expect(screen.queryByTestId("flow-name-required")).not.toBeInTheDocument();
    expect(screen.getByLabelText("Flow name")).toHaveAttribute(
      "aria-invalid",
      "false",
    );
    // Whitespace is not a name.
    fireEvent.change(screen.getByLabelText("Flow name"), {
      target: { value: "   " },
    });
    expect(screen.getByTestId("flow-name-required")).toBeInTheDocument();
  });

  it("round-trips the name and description it was opened with", async () => {
    const { flows } = await renderCanvas(
      JSON.stringify({
        version: 1,
        name: "Onboarding review",
        description: "Reviews a new joiner's first PR.",
        nodes: [],
        edges: [],
      }),
    );

    expect(screen.getByLabelText<HTMLInputElement>("Flow name").value).toBe(
      "Onboarding review",
    );
    expect(
      screen.getByLabelText<HTMLInputElement>("Flow description").value,
    ).toBe("Reviews a new joiner's first PR.");
    expect(screen.queryByTestId("flow-name-required")).not.toBeInTheDocument();
    expect(flows.get(ID)?.dirty).toBe(false);
  });
});

describe("the canvas's own controls (FLO-FR-07, FLO-FR-09, FLO-FR-15, FLO-FR-26 / FLO-FR-22, FLO-FR-23)", () => {
  // FLO-FR-07 / FLO-FR-22: every action lives on the canvas, so the tab's top
  // band carries the Flow's own fields and nothing else.
  it("puts add-node and the view controls on the canvas, not in the header", async () => {
    await renderCanvas(TWO_NODES);
    const header = document.querySelector(".flow-header") as HTMLElement;

    expect(within(header).getByLabelText("Flow name")).toBeInTheDocument();
    expect(within(header).queryByLabelText("Add to flow")).not.toBeInTheDocument();
    expect(within(header).queryByLabelText("Fit")).not.toBeInTheDocument();
    expect(within(header).queryByLabelText("Zoom in")).not.toBeInTheDocument();

    for (const label of ["Add to flow", "Zoom in", "Zoom out", "Fit"]) {
      const control = within(canvas()).getByLabelText(label);
      // Each carries its name as a tooltip too, the caption's absence costing
      // a pointer user nothing.
      expect(control).toHaveAttribute("title", label);
    }
    // The band carries no actions at all, whatever they might be called.
    expect(header.querySelectorAll("button")).toHaveLength(0);
  });

  // No surface stages an artifact to an agent, and a Flow tab is no exception:
  // it is a graph of the artifacts it references rather than something to hand
  // to one.
  it("offers no staging affordance", async () => {
    await renderCanvas(TWO_NODES);

    expect(screen.queryByRole("button", { name: /inject/i })).not.toBeInTheDocument();
    expect(document.body.textContent).not.toMatch(/inject/i);
  });

  // The controls sit on the canvas, so a press on one must not read as a press
  // on the canvas behind it: no pan, and no loss of the current selection.
  it("neither pans nor deselects when a canvas control is pressed", async () => {
    const { flows } = await renderCanvas(TWO_NODES);
    selectNode("n1");
    expect(screen.getByTestId("flow-status")).toHaveTextContent(
      "selected: Collect context",
    );
    const viewport = () =>
      document.querySelector(".flow-viewport") as HTMLElement;

    // Every control, and the readout between them — which is part of the
    // cluster and not the canvas behind it.
    const targets: HTMLElement[] = [
      ...["Zoom in", "Fit", "Add to flow"].map((l) =>
        within(canvas()).getByLabelText(l),
      ),
      screen.getByTestId("flow-zoom"),
    ];
    for (const control of targets) {
      fireEvent.pointerDown(control, {
        pointerId: 1,
        buttons: 1,
        clientX: 400,
        clientY: 400,
      });
      fireEvent.pointerMove(canvas(), {
        pointerId: 1,
        buttons: 1,
        clientX: 500,
        clientY: 460,
      });
      fireEvent.pointerUp(control, { pointerId: 1 });

      expect(viewport().style.transform).toContain("translate(0px, 0px)");
      expect(screen.getByTestId("flow-status")).toHaveTextContent(
        "selected: Collect context",
      );
    }
    // A press and a release are not a click, so none of them acted at all —
    // and in particular none of them moved a node or dirtied the Flow.
    expect(doc(flows).nodes.map((n) => n.position)).toEqual([
      { x: 0, y: 0 },
      { x: 300, y: 0 },
    ]);
    expect(flows.get(ID)?.dirty).toBe(false);
  });
});

describe("node editing (FLO-FR-07, FLO-FR-09, FLO-FR-15, FLO-FR-26 / FLO-FR-08)", () => {
  it("adds nodes, renames one, and marks the Flow unsaved", async () => {
    const { flows } = await renderCanvas("");
    expect(screen.queryByTestId("flow-dirty")).not.toBeInTheDocument();

    await addFromMenu("Node");
    await addFromMenu("Node");

    expect(doc(flows).nodes).toHaveLength(2);
    fireEvent.change(within(nodeEl("n1")).getByLabelText("Node name"), {
      target: { value: "Collect context" },
    });

    expect(doc(flows).nodes[0].name).toBe("Collect context");
    // FLO-FR-07: a new node lands unconnected and carries neither field.
    expect(doc(flows).edges).toEqual([]);
    expect(doc(flows).nodes[1]).toEqual({
      id: "n2",
      name: "New node",
      position: expect.any(Object),
    });
    // FLO-FR-26: the canvas's unsaved marker, alongside the tab's.
    expect(screen.getByTestId("flow-dirty")).toBeInTheDocument();
  });

  it("removes a node together with both of its incident edges", async () => {
    // A -> B -> C, so B carries one incoming and one outgoing edge.
    const { flows } = await renderCanvas(
      JSON.stringify({
        version: 1,
        nodes: [
          { id: "n1", name: "A", position: { x: 0, y: 0 } },
          { id: "n2", name: "B", position: { x: 200, y: 0 } },
          { id: "n3", name: "C", position: { x: 400, y: 0 } },
        ],
        edges: [
          { id: "e1", from: "n1", to: "n2" },
          { id: "e2", from: "n2", to: "n3" },
        ],
      }),
    );

    selectNode("n2");
    await userEvent.click(screen.getByRole("button", { name: "Remove node B" }));

    expect(doc(flows).nodes.map((n) => n.id)).toEqual(["n1", "n3"]);
    expect(doc(flows).edges).toEqual([]);
    expect(screen.queryByTestId("flow-node-n2")).not.toBeInTheDocument();
  });
});

// FLO-FR-26 enumerates the edits that raise the unsaved state. A canvas handler
// that forgot to route through the store would still change the document in the
// tests above — this is what pins the dirty transition itself.
describe("every edit raises the unsaved state (FLO-FR-26)", () => {
  const WITH_EVERYTHING = JSON.stringify({
    version: 1,
    nodes: [
      {
        id: "n1",
        name: "A",
        artifactIds: ["prompts/gather-repo.md"],
        prompt: "p",
        position: { x: 0, y: 0 },
      },
      { id: "n2", name: "B", position: { x: 200, y: 0 } },
    ],
    edges: [{ id: "e1", from: "n1", to: "n2" }],
  });

  const cases: Array<[string, () => Promise<void> | void]> = [
    ["adding a node", () => addFromMenu("Node")],
    [
      "renaming a node",
      () =>
        fireEvent.change(within(nodeEl("n1")).getByLabelText("Node name"), {
          target: { value: "renamed" },
        }),
    ],
    [
      "adding an artifact reference",
      () => {
        selectNode("n1");
        return userEvent.selectOptions(
          within(nodeEl("n1")).getByLabelText("Add artifact"),
          SKILL_ID,
        );
      },
    ],
    [
      "removing an artifact reference",
      () => {
        selectNode("n1");
        return userEvent.click(
          within(nodeEl("n1")).getByLabelText("Remove reference gather-repo.md"),
        );
      },
    ],
    [
      "editing an inline prompt",
      () => {
        selectNode("n1");
        fireEvent.change(within(nodeEl("n1")).getByLabelText("Inline prompt"), {
          target: { value: "changed" },
        });
      },
    ],
    [
      "renaming an edge",
      () => {
        fireEvent.pointerDown(screen.getByTestId("flow-edge-e1"));
        fireEvent.change(screen.getByLabelText("Edge label"), {
          target: { value: "retry" },
        });
      },
    ],
    [
      "removing an edge",
      () => {
        fireEvent.pointerDown(screen.getByTestId("flow-edge-e1"));
        return userEvent.click(screen.getByRole("button", { name: "Remove edge" }));
      },
    ],
    [
      "removing a node",
      () => {
        selectNode("n1");
        return userEvent.click(screen.getByRole("button", { name: "Remove node A" }));
      },
    ],
    ["adding an edge", () => connectNodes("n2", "n1")],
    ["moving a node", () => dragNode("n2", 30, 30)],
  ];

  for (const [what, act_] of cases) {
    it(`${what} marks the Flow unsaved`, async () => {
      const { flows } = await renderCanvas(WITH_EVERYTHING);
      expect(flows.get(ID)?.dirty).toBe(false);

      await act_();

      expect(flows.get(ID)?.dirty).toBe(true);
      expect(screen.getByTestId("flow-dirty")).toBeInTheDocument();
    });
  }
});

describe("state ownership (FLO-FR-30)", () => {
  // The regression this guards: a canvas that copied the graph into local state
  // at mount would pass every other test here, and lose the user's unsaved edits
  // the moment they switched tabs and back.
  it("resumes the same unsaved graph after an unmount and remount, with no reload", async () => {
    const { flows, view } = await renderCanvas(TWO_NODES);
    await addFromMenu("Node");
    const loadsBefore = invokeMock.mock.calls.filter(
      (c) => c[0] === "load_artifact_contents_by_id",
    ).length;

    // A tab switch away and back: `Viewport` unmounts the canvas entirely.
    view.unmount();
    render(
      <FlowCanvas flowId={ID} flows={flows} />,
    );

    expect(screen.getByTestId("flow-status")).toHaveTextContent(
      "3 nodes · 1 edge",
    );
    expect(screen.getByTestId("flow-dirty")).toBeInTheDocument();
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "load_artifact_contents_by_id"),
    ).toHaveLength(loadsBefore);
  });

  it("surfaces a failed write on the canvas", async () => {
    const { flows } = await renderCanvas(TWO_NODES);
    await addFromMenu("Node");
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_project_tree") return TREE;
      // The write passes validation and then fails on the disk itself, which is
      // the failure this test is about (FLO-FR-47 covers the other one).
      if (cmd === "validate_flow_document") return { valid: true, violations: [] };
      if (cmd === "save_artifact_contents") throw new Error("disk full");
      throw new Error(`unexpected invoke ${cmd}`);
    });

    await act(async () => {
      await flows.flush(ID);
    });

    expect(screen.getByRole("alert")).toHaveTextContent("disk full");
    // The edits survive the failed write, so the canvas still shows them unsaved.
    expect(screen.getByTestId("flow-dirty")).toBeInTheDocument();
  });

  it("renders no graph and no editing for a tab the backend cannot address", () => {
    render(
      <FlowCanvas flows={new FlowSessionStore()} />,
    );

    expect(screen.getByText("No Flow is open.")).toBeInTheDocument();
    expect(screen.queryByLabelText("Add to flow")).not.toBeInTheDocument();
    expect(document.querySelector(".flow-node")).toBeNull();
  });
});

describe("a body the backend does not call a Flow (FLO-FR-46, FLO-FR-05, FGV-FR-12)", () => {
  it("renders the error state listing every violation it named", async () => {
    validation = {
      valid: false,
      violations: [
        {
          code: "cross_container_edge",
          message: "edge `e1` joins `n1` in the top level to `n2` in loop `l1`",
          edgeId: "e1",
        },
        { code: "duplicate_id", message: "two elements carry `n1`", elementId: "n1" },
      ],
    };

    const { flows } = await renderCanvas(TWO_NODES);

    expect(screen.getByRole("alert")).toHaveTextContent(
      "This file could not be read as a Flow.",
    );
    const list = screen.getByTestId("flow-violations");
    const items = within(list).getAllByRole("listitem");
    expect(items).toHaveLength(2);
    // Each violation names the element or edge it sits on, and carries its code
    // so a reader can tell two similar messages apart.
    expect(items.map((li) => li.dataset.code)).toEqual([
      "cross_container_edge",
      "duplicate_id",
    ]);
    expect(items[0]).toHaveTextContent("e1");
    expect(items[1]).toHaveTextContent("two elements carry");
    // FLO-FR-05: no graph, no editing, and no write while it stands.
    expect(screen.queryByTestId("flow-node-n1")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Add to flow")).not.toBeInTheDocument();
    await act(async () => {
      await flows.flush(ID, { force: true });
    });
    expect(writes).toEqual([]);
  });
});

// FLO-FR-30 / FLO-FR-27: the tab's band carries no Save control, and neither
// does the canvas — a Flow writes itself, so a control to write it would either
// do nothing or duplicate what has already happened.
describe("the tab carries no Save control (FLO-FR-27, FLO-FR-30)", () => {
  it("renders no Save affordance in the band or on the canvas", async () => {
    await renderCanvas(TWO_NODES);

    expect(screen.queryByRole("button", { name: /^Save/i })).toBeNull();
    expect(screen.queryByTitle(/^Save/i)).toBeNull();
  });

  /**
   * FLO-FR-27 / FLO-FR-30: activating another tab unmounts the canvas, which is
   * why the rest lives in the store. A timer the canvas owned would be torn down
   * here and the edit would never reach disk.
   */
  it("FLO-FR-27, FLO-FR-30: writes while the Flow tab sits in the background", async () => {
    const { flows, view } = await renderCanvas(TWO_NODES);

    act(() => {
      flows.applyEdit(ID, (d) => renameNode(d, "n1", "Collect the context"));
    });
    view.unmount(); // the author activates a different tab

    await act(async () => {
      await new Promise((r) => setTimeout(r, AUTOSAVE_DELAY_MS + 200));
    });

    expect(writes).toHaveLength(1);
    expect(writes[0]).toContain("Collect the context");
    expect(flows.get(ID)?.dirty).toBe(false);
  });

  it("shows the unsaved marker while a write is outstanding and clears it", async () => {
    const { flows } = await renderCanvas(TWO_NODES);

    act(() => {
      flows.applyEdit(ID, (d) => renameNode(d, "n1", "Collect the context"));
    });
    expect(screen.getByTestId("flow-dirty")).toBeInTheDocument();

    // FLO-FR-27: nothing is activated — the rest elapses and the write lands.
    await act(async () => {
      await new Promise((r) => setTimeout(r, AUTOSAVE_DELAY_MS + 30));
    });

    expect(writes).toHaveLength(1);
    expect(screen.queryByTestId("flow-dirty")).toBeNull();
  });
});
