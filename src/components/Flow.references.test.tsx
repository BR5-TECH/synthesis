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
  canvas,
  doc,
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

/**
 * The same, with `load_project_tree` rejecting — the state a canvas is in
 * before any tree has been read (FLO-FR-11's `pending` branch).
 */
async function renderCanvasWithNoTree(content: string) {
  body = content;
  invokeMock.mockImplementation(
    async (cmd: string, args: Record<string, string>) => {
      if (cmd === "load_project_tree") throw new Error("scan failed");
      if (cmd === "validate_flow_document") return { valid: true, violations: [] };
      if (cmd === "load_artifact_contents_by_id") return { body, checksum: "ck1" };
      if (cmd === "open_artifact_by_id") return { key: args.id, kind: "markdown" };
      throw new Error(`unexpected invoke ${cmd}`);
    },
  );
  const flows = new FlowSessionStore();
  await act(async () => {
    flows.openTab(ID);
    await flows.get(ID)?.pendingLoad;
  });
  render(
    <FlowCanvas flowId={ID} flows={flows} />,
  );
  await waitFor(() =>
    expect(invokeMock.mock.calls.some((c) => c[0] === "load_project_tree")).toBe(
      true,
    ),
  );
  // As in `renderCanvas`: let the rejection land before the test reads the
  // canvas. Here that leaves `treeLoaded` false, which is the `pending` branch
  // this helper exists to produce.
  await act(async () => {});
  return { flows };
}

describe("artifact references (FLO-FR-10, FLO-FR-11 / FLO-FR-12, TAB-FR-07 / FLO-FR-12)", () => {
  it("accumulates references on a node and removes them one at a time", async () => {
    const { flows } = await renderCanvas(TWO_NODES);
    selectNode("n1");
    const picker = () => within(nodeEl("n1")).getByLabelText("Add artifact");

    await userEvent.selectOptions(picker(), "prompts/gather-repo.md");

    expect(doc(flows).nodes[0].artifactIds).toEqual(["prompts/gather-repo.md"]);
    const ref = within(nodeEl("n1")).getByLabelText("Open gather-repo.md");
    expect(ref).toHaveTextContent("gather-repo.md");
    expect(ref.querySelector(".chip-type")).toHaveTextContent("PRO");

    // FLO-FR-10: a second reference is added alongside the first, not over it.
    await userEvent.selectOptions(picker(), SKILL_ID);

    expect(doc(flows).nodes[0].artifactIds).toEqual([
      "prompts/gather-repo.md",
      SKILL_ID,
    ]);
    expect(
      within(nodeEl("n1")).getByLabelText("Open gather-repo.md"),
    ).toBeInTheDocument();
    // Neither is offered again while the node already carries it.
    expect(
      [...picker().querySelectorAll("option")].map((o) => o.value),
    ).toEqual([""]);

    await userEvent.click(
      within(nodeEl("n1")).getByLabelText("Remove reference gather-repo.md"),
    );

    // The other reference and the node's edges are untouched.
    expect(doc(flows).nodes[0].artifactIds).toEqual([SKILL_ID]);
    expect(
      within(nodeEl("n1")).queryByLabelText("Open gather-repo.md"),
    ).not.toBeInTheDocument();
    expect(doc(flows).edges).toHaveLength(1);
  });

  // FLO-FR-10, FLO-FR-11: what the picker offers, and what it calls it.
  it("offers only skills, prompts and instructions, a skill by its declared name", async () => {
    await renderCanvas(TWO_NODES);
    selectNode("n1");
    const picker = () => within(nodeEl("n1")).getByLabelText("Add artifact");

    // A skill is offered under the name it declares, not as one more `SKILL.md`.
    await waitFor(() =>
      expect(
        [...picker().querySelectorAll("option")].map((o) => o.textContent),
      ).toEqual(["Add artifact…", "Code review", "prompts/gather-repo.md"]),
    );

    const values = [...picker().querySelectorAll("option")].map((o) => o.value);
    // The skill's own supporting file, the spec, and the unclassified file are
    // all absent: none of them is something a workflow step is made of.
    expect(values).toEqual(["", SKILL_ID, "prompts/gather-repo.md"]);
    expect(picker().querySelectorAll("optgroup")).toHaveLength(2);

    await userEvent.selectOptions(picker(), SKILL_ID);

    const ref = within(nodeEl("n1")).getByLabelText("Open Code review");
    expect(ref).toHaveTextContent("Code review");
    expect(ref).not.toHaveTextContent("SKILL.md");
    expect(ref.querySelector(".chip-type")).toHaveTextContent("SKL");
  });

  // FLO-FR-11: the artifact was deleted, moved, or renamed. Nothing in the graph
  // is removed on the user's behalf.
  it("marks a reference that no longer resolves and keeps the node and its edges", async () => {
    const { flows } = await renderCanvas(
      JSON.stringify({
        version: 1,
        nodes: [
          {
            id: "n1",
            name: "Summarise",
            artifactIds: ["prompts/gone.md"],
            position: { x: 0, y: 0 },
          },
          { id: "n2", name: "B", position: { x: 200, y: 0 } },
        ],
        edges: [{ id: "e1", from: "n1", to: "n2" }],
      }),
    );

    await waitFor(() =>
      expect(nodeEl("n1")).toHaveTextContent("unresolved: prompts/gone.md"),
    );
    expect(nodeEl("n1")).toBeInTheDocument();
    expect(doc(flows).edges).toHaveLength(1);
    // Nothing was rewritten just because the reference broke.
    expect(flows.get(ID)?.dirty).toBe(false);
  });

  it("opens a resolved reference through open artifact by id, and an unresolved one opens nothing", async () => {
    const { onOpenArtifact } = await renderCanvas(
      JSON.stringify({
        version: 1,
        nodes: [
          {
            id: "n1",
            name: "A",
            artifactIds: ["prompts/gather-repo.md"],
            position: { x: 0, y: 0 },
          },
          {
            id: "n2",
            name: "B",
            artifactIds: ["prompts/gone.md"],
            position: { x: 200, y: 0 },
          },
        ],
        edges: [],
      }),
    );

    await userEvent.click(
      await within(nodeEl("n1")).findByLabelText("Open gather-repo.md"),
    );
    expect(invokeMock).toHaveBeenCalledWith("open_artifact_by_id", {
      id: "prompts/gather-repo.md",
    });
    // The resolved stable key is what the shell opens the tab on, which is what
    // the single-tab rule compares (TAB-FR-04 / TAB-FR-07).
    await waitFor(() =>
      expect(onOpenArtifact).toHaveBeenCalledWith({
        id: "prompts/gather-repo.md",
        name: "gather-repo.md",
        artifactType: "prompt",
      }),
    );

    // FLO-FR-12: the unresolved reference is not an affordance at all — the
    // click has to land on the unresolved row itself, or a regression that gave
    // it a handler would go unnoticed.
    const opensBefore = invokeMock.mock.calls.filter(
      (c) => c[0] === "open_artifact_by_id",
    ).length;
    await userEvent.click(
      within(nodeEl("n2")).getByText(/unresolved: prompts\/gone\.md/),
    );
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "open_artifact_by_id"),
    ).toHaveLength(opensBefore);
    expect(onOpenArtifact).toHaveBeenCalledTimes(1);
  });


  // FLO-FR-12: a nested Flow opens on the Flow canvas, not in the Editor — the
  // routing follows `open artifact by id`'s kind hint, which is the only thing
  // that distinguishes the two.
  it("routes a referenced Flow to a Flow tab and a vanished artifact to nothing", async () => {
    const { onOpenArtifact } = await renderCanvas(
      JSON.stringify({
        version: 1,
        nodes: [
          {
            id: "n1",
            name: "A",
            artifactIds: ["prompts/gather-repo.md"],
            position: { x: 0, y: 0 },
          },
        ],
        edges: [],
      }),
    );

    invokeMock.mockImplementation(async (cmd: string, args: Record<string, string>) => {
      if (cmd === "load_project_tree") return TREE;
      if (cmd === "open_artifact_by_id") return { key: args.id, kind: "flow" };
      throw new Error(`unexpected invoke ${cmd}`);
    });
    await userEvent.click(
      within(nodeEl("n1")).getByLabelText("Open gather-repo.md"),
    );

    await waitFor(() =>
      expect(onOpenArtifact).toHaveBeenCalledWith({
        id: "prompts/gather-repo.md",
        name: "gather-repo.md",
        artifactType: "flow",
      }),
    );

    // And an artifact that has gone since the tree was read opens nothing,
    // exactly as an unresolved reference does.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_project_tree") return TREE;
      if (cmd === "open_artifact_by_id") throw new Error("no such file");
      throw new Error(`unexpected invoke ${cmd}`);
    });
    await userEvent.click(
      within(nodeEl("n1")).getByLabelText("Open gather-repo.md"),
    );

    expect(onOpenArtifact).toHaveBeenCalledTimes(1);
  });

  // FLO-FR-11: a stored reference to a skill renders under the name the scan
  // read out of the file, so a Flow of skills is not a column of `SKILL.md`.
  it("renders a stored skill reference by its declared name, and opens it by it", async () => {
    const { onOpenArtifact } = await renderCanvas(
      JSON.stringify({
        version: 1,
        nodes: [
          {
            id: "n1",
            name: "Review",
            artifactIds: [SKILL_ID],
            position: { x: 0, y: 0 },
          },
        ],
        edges: [],
      }),
    );

    const ref = await within(nodeEl("n1")).findByLabelText("Open Code review");
    expect(ref).toHaveTextContent("Code review");
    expect(nodeEl("n1")).not.toHaveTextContent("SKILL.md");

    await userEvent.click(ref);

    // The tab it opens is labelled by what the node shows, not by the filename
    // every skill shares.
    await waitFor(() =>
      expect(onOpenArtifact).toHaveBeenCalledWith({
        id: SKILL_ID,
        name: "Code review",
        artifactType: "skill",
      }),
    );
  });

  it("keeps an unresolved reference on the node rather than dropping it", async () => {
    // The regression this guards: a reference whose artifact is merely missing
    // must survive every edit to the rest of the node. It is dropped from its
    // own row and nowhere else, so nothing can clear it as a side effect.
    const { flows } = await renderCanvas(
      JSON.stringify({
        version: 1,
        nodes: [
          {
            id: "n1",
            name: "A",
            artifactIds: ["prompts/gone.md"],
            position: { x: 0, y: 0 },
          },
        ],
        edges: [],
      }),
    );
    selectNode("n1");

    fireEvent.change(within(nodeEl("n1")).getByLabelText("Node name"), {
      target: { value: "Renamed" },
    });
    await userEvent.selectOptions(
      within(nodeEl("n1")).getByLabelText("Add artifact"),
      "prompts/gather-repo.md",
    );

    expect(doc(flows).nodes[0].artifactIds).toEqual([
      "prompts/gone.md",
      "prompts/gather-repo.md",
    ]);
    expect(nodeEl("n1")).toHaveTextContent("unresolved: prompts/gone.md");

    // …and it is removable on its own terms, by the id it was stored under.
    await userEvent.click(
      within(nodeEl("n1")).getByLabelText("Remove reference prompts/gone.md"),
    );
    expect(doc(flows).nodes[0].artifactIds).toEqual(["prompts/gather-repo.md"]);
  });
});

describe("reference resolution before a tree is read (FLO-FR-11)", () => {
  // The `pending` branch. Without it, opening a Flow in a project whose scan is
  // still in flight — or whose scan failed — flashes "unresolved" across every
  // node that references anything.
  it("renders a stored reference as its bare id, not as a warning, until a tree is read", async () => {
    const { flows } = await renderCanvasWithNoTree(
      JSON.stringify({
        version: 1,
        nodes: [
          {
            id: "n1",
            name: "A",
            artifactIds: ["prompts/gather-repo.md"],
            position: { x: 0, y: 0 },
          },
        ],
        edges: [],
      }),
    );

    expect(nodeEl("n1")).toHaveTextContent("prompts/gather-repo.md");
    expect(nodeEl("n1")).not.toHaveTextContent("unresolved");
    // Nothing was rewritten on the strength of a tree that never arrived.
    expect(flows.get(ID)?.dirty).toBe(false);

    // …and the reference is still the node's, so an unrelated edit cannot clear
    // one the canvas simply could not check.
    selectNode("n1");
    fireEvent.change(within(nodeEl("n1")).getByLabelText("Node name"), {
      target: { value: "Renamed" },
    });
    expect(flows.get(ID)?.doc?.nodes[0].artifactIds).toEqual([
      "prompts/gather-repo.md",
    ]);
  });
});

describe("inline prompts (FLO-FR-13, FLO-FR-10, SNV-FR-43, CMT-FR-09)", () => {
  it("edits the prompt in place with no embedded Editor", async () => {
    const { flows } = await renderCanvas(TWO_NODES);
    selectNode("n1");

    fireEvent.change(within(nodeEl("n1")).getByLabelText("Inline prompt"), {
      target: { value: "Read the repo and list every changed file." },
    });

    expect(doc(flows).nodes[0].prompt).toBe(
      "Read the repo and list every changed file.",
    );
    // A plain textarea, with no formatting surface and no embedded Editor. That
    // ⌘F and ⌘R do nothing on a Flow tab is decided by the shell's menu
    // enablement, not here — `useShellSession.test.tsx` covers it (SNV-FR-43).
    expect(within(nodeEl("n1")).getByLabelText("Inline prompt").tagName).toBe(
      "TEXTAREA",
    );
    expect(nodeEl("n1").querySelector(".editor-toolbar")).toBeNull();
  });

  // FLO-FR-14: a node may carry either field, both, or neither, and none of
  // them is required for it to be valid or connectable.
  it("renders a node carrying neither field as name-only until it is selected", async () => {
    await renderCanvas(TWO_NODES);

    expect(
      within(nodeEl("n2")).queryByLabelText("Inline prompt"),
    ).not.toBeInTheDocument();
    expect(
      within(nodeEl("n2")).queryByLabelText("Add artifact"),
    ).not.toBeInTheDocument();

    selectNode("n2");

    expect(
      within(nodeEl("n2")).getByLabelText("Inline prompt"),
    ).toBeInTheDocument();
  });
});

describe("a node's prompt is Markdown (FLO-FR-10, SNV-FR-43, CMT-FR-09 / FLO-FR-13)", () => {
  const WITH_PROMPT = JSON.stringify({
    version: 1,
    name: "F",
    nodes: [
      {
        id: "n1",
        name: "Summarise",
        prompt: "One paragraph, **no** lists.",
        position: { x: 0, y: 0 },
      },
      { id: "n2", name: "Other", position: { x: 400, y: 0 } },
    ],
    edges: [],
  });

  it("renders rich text when unselected and the source when selected", async () => {
    const { flows } = await renderCanvas(WITH_PROMPT);

    // Unselected: emphasis reads as emphasis, and no asterisks are shown.
    const rendered = screen.getByTestId("flow-node-prompt-n1");
    expect(rendered).toHaveTextContent("One paragraph, no lists.");
    expect(rendered.textContent).not.toContain("**");
    expect(within(rendered).getByText("no").tagName).toBe("STRONG");
    expect(
      within(nodeEl("n1")).queryByLabelText("Inline prompt"),
    ).not.toBeInTheDocument();

    // Selected: the same prompt as its Markdown source, alongside the picker.
    selectNode("n1");
    const field = within(nodeEl("n1")).getByLabelText("Inline prompt");
    expect(field).toHaveValue("One paragraph, **no** lists.");
    expect(within(nodeEl("n1")).getByLabelText("Add artifact")).toBeInTheDocument();
    expect(
      screen.queryByTestId("flow-node-prompt-n1"),
    ).not.toBeInTheDocument();

    // Typing edits the source, and deselecting renders what was typed.
    await userEvent.clear(field);
    await userEvent.type(field, "Use `code` here.");
    selectNode("n2");
    const again = screen.getByTestId("flow-node-prompt-n1");
    expect(within(again).getByText("code").tagName).toBe("CODE");
    expect(doc(flows).nodes[0].prompt).toBe("Use `code` here.");
  });

  // FLO-FR-14: a node carrying no prompt renders neither face of it.
  it("renders nothing for a node with no prompt until it is selected", async () => {
    await renderCanvas(WITH_PROMPT);
    expect(screen.queryByTestId("flow-node-prompt-n2")).not.toBeInTheDocument();
    selectNode("n2");
    expect(within(nodeEl("n2")).getByLabelText("Inline prompt")).toHaveValue("");
  });
});
