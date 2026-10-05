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
import {
  addFromMenu,
  canvas,
  doc,
  dragNode,
  ID,
  loopEl,
  nodeEl,
  panCanvas,
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

describe("pointer gestures the canvas must not leave armed", () => {
  // A release the canvas never sees — the pointer left it, or left the window —
  // must still end the gesture. Otherwise the node goes on following the cursor
  // and the next unrelated click drops it somewhere the author never chose.
  it("ends a node drag released outside the canvas, and moves nothing afterwards", async () => {
    const { flows } = await renderCanvas(TWO_NODES);

    fireEvent.pointerDown(nodeEl("n1"), {
      pointerId: 1,
      buttons: 1,
      clientX: 500,
      clientY: 500,
    });
    fireEvent.pointerMove(canvas(), {
      pointerId: 1,
      buttons: 1,
      clientX: 560,
      clientY: 540,
    });
    // Released over the toolbar band above the canvas, not over `.flow`.
    fireEvent.pointerUp(window, { pointerId: 1 });

    expect(doc(flows).nodes[0].position).toEqual({ x: 60, y: 40 });

    // The gesture is over: further motion moves nothing, and the next click
    // commits nothing.
    fireEvent.pointerMove(canvas(), { pointerId: 1, clientX: 900, clientY: 900 });
    await act(async () => {
      await flows.flush(ID);
    });
    fireEvent.pointerUp(canvas(), { pointerId: 1 });

    expect(doc(flows).nodes[0].position).toEqual({ x: 60, y: 40 });
    expect(flows.get(ID)?.dirty).toBe(false);
  });

  // The same hazard on the connection drag, where it corrupts the graph: an
  // abandoned connection left armed would attach itself to the next node the
  // user clicks, minting an edge they never asked for.
  it("abandons a connection released away from a node, and arms nothing for the next click", async () => {
    const { flows } = await renderCanvas(
      JSON.stringify({
        version: 1,
        nodes: [
          { id: "n1", name: "A", position: { x: 0, y: 0 } },
          { id: "n2", name: "B", position: { x: 200, y: 0 } },
          { id: "n3", name: "C", position: { x: 400, y: 0 } },
        ],
        edges: [],
      }),
    );

    fireEvent.pointerDown(
      within(nodeEl("n1")).getByLabelText("Connect from A"),
      { pointerId: 1, buttons: 1 },
    );
    fireEvent.pointerUp(window, { pointerId: 1 });

    // Abandoned, not refused: the user let go of it.
    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(doc(flows).edges).toEqual([]);

    // A later, unrelated click on another node creates nothing.
    fireEvent.pointerDown(nodeEl("n3"));
    fireEvent.pointerUp(nodeEl("n3"));

    expect(doc(flows).edges).toEqual([]);
    expect(flows.get(ID)?.dirty).toBe(false);
  });

  // A release over empty canvas is the ordinary abandon path.
  it("abandons a connection released over empty canvas without refusing it", async () => {
    const { flows } = await renderCanvas(TWO_NODES);

    fireEvent.pointerDown(
      within(nodeEl("n1")).getByLabelText("Connect from Collect context"),
      { pointerId: 1, buttons: 1 },
    );
    fireEvent.pointerUp(canvas(), { pointerId: 1 });

    expect(screen.queryByRole("status")).not.toBeInTheDocument();
    expect(doc(flows).edges).toHaveLength(1);
    expect(flows.get(ID)?.dirty).toBe(false);
  });

  // A pointer-down inside a field must not arm a drag, or selecting text in a
  // prompt would drag the node out from under the cursor and dirty the Flow.
  it("arms no drag from a pointer press inside a node's own fields", async () => {
    const { flows } = await renderCanvas(TWO_NODES);
    selectNode("n1");
    const before = doc(flows).nodes[0].position;

    for (const label of ["Node name", "Inline prompt"]) {
      const field = within(nodeEl("n1")).getByLabelText(label);
      fireEvent.pointerDown(field, {
        pointerId: 1,
        buttons: 1,
        clientX: 500,
        clientY: 500,
      });
      fireEvent.pointerMove(canvas(), {
        pointerId: 1,
        buttons: 1,
        clientX: 600,
        clientY: 600,
      });
      fireEvent.pointerUp(canvas(), { pointerId: 1 });
    }

    expect(doc(flows).nodes[0].position).toEqual(before);
    expect(flows.get(ID)?.dirty).toBe(false);
  });
});

describe("layout and view state (FLO-FR-27, FLO-FR-21, FLO-FR-26 / FLO-FR-22, FLO-FR-23)", () => {
  it("commits a drag as one edit and leaves a drag that moved nothing clean", async () => {
    const { flows } = await renderCanvas(TWO_NODES);
    const applyEdit = vi.spyOn(flows, "applyEdit");

    dragNode("n1", 60, 40, 8);

    expect(applyEdit).toHaveBeenCalledTimes(1);
    expect(doc(flows).nodes[0].position).toEqual({ x: 60, y: 40 });
    expect(flows.get(ID)?.dirty).toBe(true);

    // A press that never moved the node writes nothing: selecting a node must
    // not raise a false unsaved state.
    await act(async () => {
      await flows.flush(ID);
    });
    dragNode("n2", 0, 0);
    expect(flows.get(ID)?.dirty).toBe(false);

    // …and neither does one that wandered off and came back: the node is where
    // it was, so the graph is what it was (FLO-FR-26).
    fireEvent.pointerDown(nodeEl("n2"), {
      pointerId: 1,
      buttons: 1,
      clientX: 500,
      clientY: 500,
    });
    for (const [x, y] of [
      [560, 540],
      [520, 505],
      [500, 500],
    ]) {
      fireEvent.pointerMove(canvas(), { pointerId: 1, buttons: 1, clientX: x, clientY: y });
    }
    fireEvent.pointerUp(canvas(), { pointerId: 1 });

    expect(flows.get(ID)?.dirty).toBe(false);
  });

  // FLO-FR-22: pan and zoom are view state. They are not part of the document,
  // so they neither mark the Flow unsaved nor survive a remount.
  it("zooming and fitting change no document state and no dirty flag", async () => {
    const { flows, view } = await renderCanvas(TWO_NODES);
    expect(screen.getByTestId("flow-zoom")).toHaveTextContent("100%");

    await userEvent.click(screen.getByRole("button", { name: "Zoom in" }));
    await userEvent.click(screen.getByRole("button", { name: "Zoom in" }));
    expect(screen.getByTestId("flow-zoom")).toHaveTextContent("120%");
    await userEvent.click(screen.getByRole("button", { name: "Fit" }));

    expect(flows.get(ID)?.dirty).toBe(false);

    // Closing and reopening presents the default view of the saved layout.
    view.unmount();
    render(
      <FlowCanvas flowId={ID} flows={flows} />,
    );
    expect(screen.getByTestId("flow-zoom")).toHaveTextContent("100%");
    // …and the saved layout is what it presents, unchanged by the pan/zoom.
    expect(doc(flows).nodes[1].position).toEqual({ x: 300, y: 0 });
  });

  // FLO-FR-22: panning is a view operation over the same saved layout.
  it("pans the view without moving anything in the document", async () => {
    const { flows, view } = await renderCanvas(TWO_NODES);
    const viewport = () =>
      document.querySelector(".flow-viewport") as HTMLElement;
    expect(viewport().style.transform).toContain("translate(0px, 0px)");

    panCanvas(120, -40);

    expect(viewport().style.transform).toContain("translate(120px, -40px)");
    // No node moved and nothing is unsaved.
    expect(doc(flows).nodes.map((n) => n.position)).toEqual([
      { x: 0, y: 0 },
      { x: 300, y: 0 },
    ]);
    expect(flows.get(ID)?.dirty).toBe(false);

    // …and the pan does not survive the tab, so a reopened Flow presents the
    // default view of the saved layout.
    view.unmount();
    render(
      <FlowCanvas flowId={ID} flows={flows} />,
    );
    expect(viewport().style.transform).toContain("translate(0px, 0px)");
  });

  it("clamps the zoom at both ends", async () => {
    await renderCanvas(TWO_NODES);
    const zoomOut = screen.getByRole("button", { name: "Zoom out" });
    const zoomIn = screen.getByRole("button", { name: "Zoom in" });

    for (let i = 0; i < 20; i += 1) await userEvent.click(zoomOut);
    expect(screen.getByTestId("flow-zoom")).toHaveTextContent("25%");

    for (let i = 0; i < 40; i += 1) await userEvent.click(zoomIn);
    expect(screen.getByTestId("flow-zoom")).toHaveTextContent("200%");
  });

  // FLO-FR-23: a Flow is variable-size and the UI imposes no cap.
  it("renders every node of a large graph with no cap and no truncation", async () => {
    const nodes = Array.from({ length: 300 }, (_, i) => ({
      id: `n${i + 1}`,
      name: `node-${i + 1}`,
      position: { x: (i % 20) * 60, y: Math.floor(i / 20) * 60 },
    }));
    const edges = nodes.slice(1).map((n, i) => ({
      id: `e${i + 1}`,
      from: nodes[i].id,
      to: n.id,
    }));
    await renderCanvas(JSON.stringify({ version: 1, nodes, edges }));

    expect(document.querySelectorAll(".flow-node")).toHaveLength(300);
    // The rendered edges, not just the readout — which is computed from the
    // document and would keep reading 299 even if edge rendering were capped.
    expect(document.querySelectorAll(".flow-edge-label")).toHaveLength(299);
    // Direct children only: the arrowhead definitions (FLO-FR-48) are paths of
    // their own inside `<defs>`, and they are two however many edges there are.
    expect(document.querySelectorAll(".flow-edges > path")).toHaveLength(299);
    expect(screen.getByTestId("flow-status")).toHaveTextContent(
      "300 nodes · 299 edges",
    );
  });
});

describe("the add menu (FLO-FR-07, FLO-FR-09, FLO-FR-15, FLO-FR-26 / FLO-FR-37, FLO-FR-38, FLO-FR-10)", () => {
  it("offers Node and Loop, and adds neither until one is chosen", async () => {
    const { flows } = await renderCanvas("");

    await userEvent.click(screen.getByLabelText("Add to flow"));
    const menu = screen.getByTestId("flow-add-menu");
    const items = within(menu).getAllByRole("menuitem");
    expect(items.map((b) => b.textContent?.trim())).toEqual(["Node", "Loop"]);
    // FLO-FR-07: the action control's own menu treatment, so the two menus a
    // Flow tab can open read as one kind of surface — a named entry carrying a
    // glyph, not a bare label.
    expect(menu).toHaveClass("draft-actions__menu");
    for (const item of items) {
      expect(item).toHaveClass("draft-actions__item");
      expect(item.querySelector("svg")).not.toBeNull();
    }
    expect(doc(flows).nodes).toHaveLength(0);
    expect(doc(flows).loops).toHaveLength(0);

    // FLO-FR-07: Escape closes it and adds nothing.
    fireEvent.keyDown(menu, { key: "Escape" });
    expect(screen.queryByTestId("flow-add-menu")).not.toBeInTheDocument();
    expect(flows.get(ID)?.dirty).toBe(false);
    expect(document.activeElement).toBe(screen.getByLabelText("Add to flow"));
  });

  // FLO-FR-07: choosing an entry closes the menu, and focus goes back to the
  // affordance that opened it rather than to the document body — a keyboard
  // author who added a node is still on the control, ready to add the next one.
  it("returns focus to the add affordance after adding", async () => {
    const { flows } = await renderCanvas("");

    await addFromMenu("Node");
    expect(doc(flows).nodes).toHaveLength(1);
    expect(screen.queryByTestId("flow-add-menu")).not.toBeInTheDocument();
    expect(document.activeElement).toBe(screen.getByLabelText("Add to flow"));

    await addFromMenu("Loop");
    expect(doc(flows).loops).toHaveLength(1);
    expect(document.activeElement).toBe(screen.getByLabelText("Add to flow"));
  });

  // FLO-FR-07, FLO-FR-37, FLO-FR-38, FLO-FR-10, FLO-FR-26: an empty container with a default name, no artifact reference,
  // no pass bound, and a default size.
  it("adds an empty loop and edits its own fields in place", async () => {
    const { flows } = await renderCanvas("");

    await addFromMenu("Loop");

    expect(doc(flows).loops).toHaveLength(1);
    const loop = doc(flows).loops[0];
    expect(loop.artifactIds).toBeUndefined();
    expect(loop.maxPasses).toBeUndefined();
    expect(loop.size.width).toBeGreaterThan(0);
    expect(screen.getByTestId("flow-dirty")).toBeInTheDocument();

    const box = loopEl(loop.id);
    await userEvent.clear(within(box).getByLabelText("Loop name"));
    await userEvent.type(within(box).getByLabelText("Loop name"), "Review cycle");
    await userEvent.type(within(box).getByLabelText("Loop max passes"), "5");

    expect(doc(flows).loops[0]).toMatchObject({
      name: "Review cycle",
      maxPasses: 5,
    });
    // FLO-FR-38: the loop's own fields are plain text, and it carries no inline
    // prompt at all — the rich-text rendering belongs to a node's prompt alone.
    expect(
      within(box).queryByTestId(/^flow-node-prompt-/),
    ).not.toBeInTheDocument();
    expect(within(box).queryByLabelText("Inline prompt")).not.toBeInTheDocument();
  });

  // FLO-FR-07, FLO-FR-37, FLO-FR-38, FLO-FR-26 / FLO-FR-10: a loop is built out of the project's artifacts on
  // exactly a node's terms — that prompt is where the author says what ends it.
  it("references artifacts from the loop's own picker", async () => {
    const { flows, onOpenArtifact } = await renderCanvas("");

    await addFromMenu("Loop");
    const loopId = doc(flows).loops[0].id;

    // FLO-FR-10: the picker and the remove affordances appear while the element
    // is selected, and not before.
    expect(
      within(loopEl(loopId)).queryByLabelText("Add artifact"),
    ).not.toBeInTheDocument();
    await userEvent.click(within(loopEl(loopId)).getByLabelText("Loop name"));

    const picker = within(loopEl(loopId)).getByLabelText("Add artifact");
    await userEvent.selectOptions(picker, "prompts/gather-repo.md");
    expect(doc(flows).loops[0].artifactIds).toEqual(["prompts/gather-repo.md"]);

    // Rendered as the artifact's display name with its type chip, and no longer
    // offered by the picker that added it (FLO-FR-11, FLO-FR-10).
    const row = within(loopEl(loopId)).getByLabelText("Open gather-repo.md");
    expect(row.querySelector(".chip-type")).not.toBeNull();
    expect(
      within(within(loopEl(loopId)).getByLabelText("Add artifact")).queryByRole(
        "option",
        { name: /gather-repo\.md/ },
      ),
    ).not.toBeInTheDocument();

    // FLO-FR-10: a second reference is added alongside the first, and removing
    // one leaves the loop's others in place.
    await userEvent.selectOptions(
      within(loopEl(loopId)).getByLabelText("Add artifact"),
      SKILL_ID,
    );
    expect(doc(flows).loops[0].artifactIds).toEqual([
      "prompts/gather-repo.md",
      SKILL_ID,
    ]);

    await userEvent.click(
      within(loopEl(loopId)).getByLabelText("Remove reference gather-repo.md"),
    );
    expect(doc(flows).loops[0].artifactIds).toEqual([SKILL_ID]);
    // FLO-FR-12: a resolved reference on a loop opens its artifact, on exactly
    // the terms a node's does.
    await userEvent.click(
      within(loopEl(loopId)).getByLabelText("Open Code review"),
    );
    await waitFor(() =>
      expect(onOpenArtifact).toHaveBeenCalledWith({
        id: SKILL_ID,
        name: "Code review",
        artifactType: "skill",
      }),
    );

    await userEvent.click(
      within(loopEl(loopId)).getByLabelText("Remove reference Code review"),
    );
    expect("artifactIds" in doc(flows).loops[0]).toBe(false);
  });

  // FLO-FR-07 / ACT-FR-12: at most one of the tab's transient surfaces is open.
  it("dismisses the action control when it opens, and goes when the control does", async () => {
    await renderCanvas(TWO_NODES);

    await userEvent.click(screen.getByLabelText("Add to flow"));
    expect(screen.getByTestId("flow-add-menu")).toBeInTheDocument();

    await userEvent.click(screen.getByLabelText("Actions"));
    expect(screen.queryByTestId("flow-add-menu")).not.toBeInTheDocument();

    await userEvent.click(screen.getByLabelText("Add to flow"));
    expect(screen.getByTestId("flow-add-menu")).toBeInTheDocument();
    expect(screen.queryByRole("menuitem", { name: "Discuss" })).not.toBeInTheDocument();
  });
});

describe("the add menu dismisses on the terms FLO-FR-07 sets", () => {
  it("closes on a real Escape from wherever activation left focus", async () => {
    await renderCanvas(TWO_NODES);

    await userEvent.click(screen.getByLabelText("Add to flow"));
    expect(screen.getByTestId("flow-add-menu")).toBeInTheDocument();

    // Not fired at the menu element — pressed wherever focus actually is, which
    // is what a user does.
    await userEvent.keyboard("{Escape}");

    expect(screen.queryByTestId("flow-add-menu")).not.toBeInTheDocument();
    // Focus goes back to the affordance rather than to the document body.
    expect(document.activeElement).toBe(screen.getByLabelText("Add to flow"));
  });

  // FLO-FR-07: "a pointer-down elsewhere in the tab" — the band of Flow fields
  // above the canvas is in the tab and outside the canvas.
  it("closes on a pointer-down in the tab's band, not only on the canvas", async () => {
    await renderCanvas(TWO_NODES);
    await userEvent.click(screen.getByLabelText("Add to flow"));

    fireEvent.pointerDown(screen.getByLabelText("Flow name"));

    expect(screen.queryByTestId("flow-add-menu")).not.toBeInTheDocument();
  });
});
