import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";

vi.mock("@xyflow/react", async (importOriginal) =>
  (await import("../../test/calmMock")).xyflowModule(await importOriginal()),
);
vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

import { logDebug } from "../../logging";
import { readCalm, resetCalmForTest } from "../../calm/read";
import type { CalmLayout } from "../../calm/types";
import { CalmCanvas } from "./CalmCanvas";
import { CalmInline, inlineFit } from "./CalmInline";
import {
  CalmFlowSelect,
  CalmStage,
  CalmZoomControls,
  useCalmInteraction,
} from "./CalmInteractive";
import { flowHighlight } from "./model";

const SCHEMA = "https://calm.finos.org/release/1.3/meta/calm.json";

const DOC = {
  $schema: SCHEMA,
  metadata: { owner: "payments" },
  adrs: ["https://example.com/adr/1"],
  nodes: [
    { "unique-id": "user", "node-type": "actor", name: "User", description: "A person." },
    {
      "unique-id": "api",
      "node-type": "service",
      name: "API <b>bold</b>",
      description: "Serves the app.",
      interfaces: [
        { "unique-id": "i1", port: 1 },
        { "unique-id": "i2" },
        { "unique-id": "i3" },
        { "unique-id": "i4" },
        { "unique-id": "i5" },
      ],
      controls: {
        "tls-control": {
          description: "Encrypt in transit.",
          requirements: [{ "requirement-url": "https://example.com/req", config: { min: "1.2" } }],
        },
      },
      metadata: { tier: 1, tags: ["a", "b"] },
    },
    { "unique-id": "db", "node-type": "database", name: "DB", description: "Keeps data." },
    { "unique-id": "east", "node-type": "network", name: "East", description: "Zone." },
    { "unique-id": "west", "node-type": "network", name: "West", description: "Zone." },
    { "unique-id": "queue", "node-type": "message-bus", name: "Queue", description: "Async." },
  ],
  relationships: [
    { "unique-id": "uses", description: "Uses", "relationship-type": { interacts: { actor: "user", nodes: ["api"] } } },
    {
      "unique-id": "api-db",
      description: "Reads",
      protocol: "JDBC",
      "relationship-type": {
        connects: { source: { node: "api", interfaces: ["i1"] }, destination: { node: "db" } },
      },
    },
    { "unique-id": "d1", "relationship-type": { "deployed-in": { container: "east", nodes: ["api", "db"] } } },
    { "unique-id": "d2", "relationship-type": { "deployed-in": { container: "west", nodes: ["db"] } } },
    {
      "unique-id": "choice",
      "relationship-type": { options: [{ description: "Use a queue", nodes: ["queue"], relationships: [] }] },
    },
  ],
  flows: [
    {
      "unique-id": "read-flow",
      name: "Read data",
      description: "The user reads data.",
      transitions: [
        { "relationship-unique-id": "uses", "sequence-number": 1, description: "Ask" },
        { "relationship-unique-id": "api-db", "sequence-number": 2, description: "Query" },
        { "relationship-unique-id": "api-db", "sequence-number": 3, description: "Answer", direction: "destination-to-source" },
      ],
    },
  ],
};

let layout: CalmLayout;

beforeEach(async () => {
  resetCalmForTest();
  vi.mocked(logDebug).mockClear();
  const result = await readCalm(JSON.stringify(DOC));
  if (result.state !== "rendered") throw new Error(result.detail);
  layout = result.layout;
});

afterEach(() => cleanup());

const nodeEl = (id: string) => document.querySelector(`[data-calm-node="${id}"]`) as HTMLElement;
const edgeLabel = (id: string) => document.querySelector(`[data-calm-edge="${id}"]`) as HTMLElement;

describe("the CALM canvas", () => {
  it("CLM-FR-DLUV, CLM-FR-DTYX: each node shows its name, its type, and its shape", () => {
    render(<CalmCanvas layout={layout} />);
    expect(nodeEl("user")).toHaveClass("calm-node--actor");
    expect(nodeEl("db")).toHaveClass("calm-node--database");
    expect(nodeEl("queue")).toHaveClass("calm-node--generic");
    expect(within(nodeEl("queue")).getByText("message-bus")).toBeInTheDocument();
    expect(within(nodeEl("user")).getByText("User")).toBeInTheDocument();
  });

  it("CLM-FR-IRGG: CALM text shows as plain text, never as HTML", () => {
    render(<CalmCanvas layout={layout} />);
    expect(within(nodeEl("api")).getByText("API <b>bold</b>")).toBeInTheDocument();
    expect(document.querySelector("b")).toBeNull();
  });

  it("CLM-FR-DWLY: a deployed-in container is a group with a dashed border kind", () => {
    render(<CalmCanvas layout={layout} />);
    expect(nodeEl("east")).toHaveClass("calm-group", "calm-group--deployed-in");
    expect(document.querySelector('[data-id="api"]')).toHaveAttribute("data-parent", "east");
  });

  it("CLM-FR-DYAW: a node in two groups names the other group", () => {
    render(<CalmCanvas layout={layout} />);
    expect(within(nodeEl("db")).getByText("Also in West")).toBeInTheDocument();
  });

  it("CLM-FR-ETEA: a node shows three interfaces, then a count, and a control marker", () => {
    render(<CalmCanvas layout={layout} />);
    const api = within(nodeEl("api"));
    expect(api.getByText("i1")).toBeInTheDocument();
    expect(api.getByText("i3")).toBeInTheDocument();
    expect(api.queryByText("i4")).toBeNull();
    expect(api.getByText("+2 more")).toBeInTheDocument();
    expect(api.getByTitle("1 control")).toHaveTextContent("1");
  });

  it("CLM-FR-ESMQ: a node that a decision names shows a decision marker", () => {
    render(<CalmCanvas layout={layout} />);
    expect(within(nodeEl("queue")).getByTitle("1 decision")).toBeInTheDocument();
  });

  it("CLM-FR-ERXV: a connects edge shows its label, its protocol, and its interface", () => {
    render(<CalmCanvas layout={layout} />);
    const label = edgeLabel("api-db");
    expect(label).toHaveTextContent("Reads");
    expect(within(label).getByText("JDBC")).toHaveClass("calm-chip");
    expect(screen.getByText("i1", { selector: ".calm-end-label" })).toBeInTheDocument();
  });

  it("CLM-FR-ESCS: an interacts edge is dashed", () => {
    render(<CalmCanvas layout={layout} />);
    expect(document.querySelector('[data-edge-path="uses"]')).toHaveClass("calm-edge--interacts");
  });

  it("CAD-FR-KSEJ: a chosen flow numbers its steps, sets their direction, and dims the rest", () => {
    const highlight = flowHighlight(layout.graph.flows[0]);
    render(<CalmCanvas layout={layout} highlight={highlight} />);
    const steps = within(edgeLabel("api-db")).getAllByText(/^[23]$/);
    expect(steps.map((s) => s.textContent)).toEqual(["2", "3"]);
    const edge = document.querySelector('[data-id="api-db"]')!;
    expect(edge).toHaveAttribute("data-marker-end", "true");
    expect(edge).toHaveAttribute("data-marker-start", "true");
    expect(nodeEl("queue")).toHaveClass("calm-dim");
    expect(nodeEl("api")).toHaveClass("calm-in-flow");
    expect(nodeEl("api")).not.toHaveClass("calm-dim");
  });

  it("CAD-FR-IRIT, CAD-FR-MBML: the canvas does not pan, zoom, drag, connect, or select by itself", () => {
    render(<CalmCanvas layout={layout} onSelect={() => {}} />);
    const flow = screen.getByTestId("calm-flow");
    for (const option of [
      "panondrag",
      "panonscroll",
      "zoomonscroll",
      "zoomonpinch",
      "zoomondoubleclick",
      "preventscrolling",
      "nodesdraggable",
      "nodesconnectable",
      "elementsselectable",
    ]) {
      expect(flow).toHaveAttribute(`data-${option}`, "false");
    }
    for (const node of document.querySelectorAll(".react-flow__node")) {
      expect(node).toHaveAttribute("data-draggable", "false");
      expect(node).toHaveAttribute("data-connectable", "false");
    }
  });

  it("CAD-FR-IRIT: the inline canvas has no selection handler, so a click selects nothing", () => {
    render(<CalmCanvas layout={layout} />);
    expect(screen.getByTestId("calm-canvas")).not.toHaveClass("calm-canvas--interactive");
    fireEvent.click(nodeEl("api"));
    expect(nodeEl("api")).not.toHaveClass("calm-selected");
  });
});

describe("the inline canvas", () => {
  it("CAD-FR-IPRR: the drawing fits the column, and the frame is 160 to 480 px high", () => {
    expect(inlineFit(400, { width: 800, height: 200 })).toEqual({ scale: 0.5, height: 160 });
    expect(inlineFit(1000, { width: 500, height: 300 })).toEqual({ scale: 1, height: 300 });
    expect(inlineFit(1000, { width: 500, height: 1200 })).toEqual({ scale: 0.4, height: 480 });
  });

  it("CAD-FR-JBUT, CLM-FR-HSUH: a graph with no nodes shows the statement", async () => {
    const result = await readCalm(JSON.stringify({ $schema: SCHEMA, nodes: [] }));
    if (result.state !== "rendered") throw new Error("expected a graph");
    render(<CalmInline layout={result.layout} />);
    expect(screen.getByText("The architecture has no nodes.")).toBeInTheDocument();
  });
});

function Interactive({ of }: { of: CalmLayout }) {
  const ix = useCalmInteraction(of);
  return (
    <div>
      <CalmFlowSelect ix={ix} />
      <CalmZoomControls ix={ix} />
      <CalmStage ix={ix} />
    </div>
  );
}

const details = () => within(screen.getByRole("complementary", { name: "Details" }));

describe("the interactive canvas", () => {
  it("CAD-FR-LQLI, CAD-FR-MZSV: the flow selector starts on No flow, then lists each flow", () => {
    render(<Interactive of={layout} />);
    const select = screen.getByRole("combobox", { name: "Flow" }) as HTMLSelectElement;
    expect(select.value).toBe("");
    expect(Array.from(select.options).map((o) => o.text)).toEqual(["No flow", "Read data"]);
  });

  it("CAD-FR-LQLI: a document with no flows has no flow selector", async () => {
    const result = await readCalm(JSON.stringify({ ...DOC, flows: [] }));
    if (result.state !== "rendered") throw new Error("expected a graph");
    render(<Interactive of={result.layout} />);
    expect(screen.queryByRole("combobox", { name: "Flow" })).toBeNull();
  });

  it("CAD-FR-MUJL: with nothing selected and no flow, the panel shows the summary and the Nodes list", () => {
    render(<Interactive of={layout} />);
    expect(details().getByText("6 nodes, 5 relationships, 1 flow")).toBeInTheDocument();
    expect(details().getByText("payments")).toBeInTheDocument();
    expect(details().getByText("https://example.com/adr/1")).toBeInTheDocument();
    const nodes = within(details().getByRole("region", { name: "Nodes" }));
    expect(nodes.getAllByRole("button").map((b) => b.textContent)).toEqual([
      "User",
      "East",
      "West",
      "Queue",
      "API <b>bold</b>",
      "DB",
    ]);
  });

  it("CAD-FR-KSEJ, CAD-FR-MJWJ: choosing a flow highlights it and lists its steps", () => {
    render(<Interactive of={layout} />);
    fireEvent.change(screen.getByRole("combobox", { name: "Flow" }), {
      target: { value: "read-flow" },
    });
    expect(nodeEl("queue")).toHaveClass("calm-dim");
    expect(details().getByRole("heading", { name: "Read data" })).toBeInTheDocument();
    const steps = within(details().getByRole("region", { name: "Steps" })).getAllByRole("button");
    expect(steps.map((s) => s.textContent)).toEqual([
      "1User → API <b>bold</b>Ask",
      "2API <b>bold</b> → DBQuery",
      "3DB → API <b>bold</b>Answer",
    ]);
    fireEvent.change(screen.getByRole("combobox", { name: "Flow" }), { target: { value: "" } });
    expect(nodeEl("queue")).not.toHaveClass("calm-dim");
  });

  it("CAD-FR-KUBU, CAD-FR-LQZM, CAD-FR-MISQ: a click on a node shows its details", () => {
    render(<Interactive of={layout} />);
    fireEvent.click(document.querySelector('[data-id="api"]')!);
    expect(nodeEl("api")).toHaveClass("calm-selected");
    const panel = details();
    expect(panel.getByRole("heading", { name: "API <b>bold</b>" })).toBeInTheDocument();
    expect(panel.getByText("service")).toBeInTheDocument();
    expect(panel.getByText("Serves the app.")).toBeInTheDocument();
    const controls = within(panel.getByRole("region", { name: "Controls" }));
    expect(controls.getByText("tls-control")).toBeInTheDocument();
    expect(controls.getByText("https://example.com/req")).toBeInTheDocument();
    expect(controls.getByText("min")).toBeInTheDocument();
    expect(controls.getByText("Encrypt in transit.")).toBeInTheDocument();
    expect(panel.getByText("api", { selector: ".calm-details__id span" })).toBeInTheDocument();
    const interfaces = within(panel.getByRole("region", { name: "Interfaces" }));
    expect(interfaces.getByText("port")).toBeInTheDocument();
    const metadata = within(panel.getByRole("region", { name: "Metadata" }));
    expect(metadata.getByText("tier")).toBeInTheDocument();
    expect(metadata.getByText(/"a",\s*"b"/)).toHaveClass("calm-details__json");
    expect(panel.getByRole("region", { name: "Interfaces" })).toBeInTheDocument();
    expect(panel.queryByRole("region", { name: "Decisions" })).toBeNull();
  });

  it("CAD-FR-MYHZ: a URL in the details is text, not a link", () => {
    render(<Interactive of={layout} />);
    fireEvent.click(document.querySelector('[data-id="api"]')!);
    expect(details().getByText("https://example.com/req").closest("a")).toBeNull();
    expect(document.querySelector(".calm-details a")).toBeNull();
  });

  it("CAD-FR-MDTX: a click on an edge shows its relationship", () => {
    render(<Interactive of={layout} />);
    fireEvent.click(document.querySelector('[data-id="api-db"]')!);
    const panel = details();
    expect(panel.getByRole("heading", { name: "API <b>bold</b> → DB" })).toBeInTheDocument();
    expect(panel.getByText("connects")).toBeInTheDocument();
    expect(panel.getByText("Reads")).toBeInTheDocument();
    expect(within(panel.getByRole("region", { name: "Protocol" })).getByText("JDBC")).toBeInTheDocument();
    expect(within(panel.getByRole("region", { name: "Interfaces" })).getByText("i1")).toBeInTheDocument();
  });

  it("CAD-FR-KUBU: a click on an edge label selects its edge, and a click on the pane clears it", () => {
    render(<Interactive of={layout} />);
    fireEvent.click(edgeLabel("api-db"));
    expect(edgeLabel("api-db")).toHaveClass("calm-selected");
    fireEvent.click(screen.getByTestId("calm-flow"));
    expect(edgeLabel("api-db")).not.toHaveClass("calm-selected");
    expect(details().getByRole("heading", { name: "Architecture" })).toBeInTheDocument();
  });

  it("CAD-FR-MXHO: an entry of the Nodes list selects its node from the keyboard", () => {
    render(<Interactive of={layout} />);
    const entry = within(details().getByRole("region", { name: "Nodes" })).getByRole("button", {
      name: "DB",
    });
    entry.focus();
    fireEvent.click(entry);
    expect(nodeEl("db")).toHaveClass("calm-selected");
    expect(details().getByRole("heading", { name: "DB" })).toBeInTheDocument();
  });

  it("CAD-FR-NNHM: a new read keeps a selection that still exists and clears one that does not", async () => {
    const { rerender } = render(<Interactive of={layout} />);
    fireEvent.click(document.querySelector('[data-id="db"]')!);
    const renamed = await readCalm(
      JSON.stringify({
        ...DOC,
        nodes: DOC.nodes.map((n) => (n["unique-id"] === "db" ? { ...n, name: "Store" } : n)),
      }),
    );
    if (renamed.state !== "rendered") throw new Error("expected a graph");
    rerender(<Interactive of={renamed.layout} />);
    expect(details().getByRole("heading", { name: "Store" })).toBeInTheDocument();
    const gone = await readCalm(
      JSON.stringify({
        $schema: SCHEMA,
        nodes: [DOC.nodes[0]],
      }),
    );
    if (gone.state !== "rendered") throw new Error("expected a graph");
    await act(async () => rerender(<Interactive of={gone.layout} />));
    expect(details().getByRole("heading", { name: "Architecture" })).toBeInTheDocument();
  });

  it("CAD-FR-NNHM: a new read keeps a flow that still exists and drops one that does not", async () => {
    const { rerender } = render(<Interactive of={layout} />);
    fireEvent.change(screen.getByRole("combobox", { name: "Flow" }), { target: { value: "read-flow" } });
    const kept = await readCalm(JSON.stringify({ ...DOC, flows: [{ ...DOC.flows[0], name: "Read it" }] }));
    if (kept.state !== "rendered") throw new Error("expected a graph");
    rerender(<Interactive of={kept.layout} />);
    expect((screen.getByRole("combobox", { name: "Flow" }) as HTMLSelectElement).value).toBe("read-flow");
    expect(details().getByRole("heading", { name: "Read it" })).toBeInTheDocument();
    const renamed = await readCalm(
      JSON.stringify({ ...DOC, flows: [{ ...DOC.flows[0], "unique-id": "other-flow" }] }),
    );
    if (renamed.state !== "rendered") throw new Error("expected a graph");
    await act(async () => rerender(<Interactive of={renamed.layout} />));
    expect((screen.getByRole("combobox", { name: "Flow" }) as HTMLSelectElement).value).toBe("");
    expect(nodeEl("queue")).not.toHaveClass("calm-dim");
  });

  it("CAD-FR-KUBU: one item is selected at a time", () => {
    render(<Interactive of={layout} />);
    fireEvent.click(document.querySelector('[data-id="api"]')!);
    fireEvent.click(document.querySelector('[data-id="api-db"]')!);
    expect(nodeEl("api")).not.toHaveClass("calm-selected");
    expect(edgeLabel("api-db")).toHaveClass("calm-selected");
  });

  it("CAD-FR-LQZM, CLM-FR-ESMQ: a node that a decision names lists the decision", () => {
    render(<Interactive of={layout} />);
    fireEvent.click(document.querySelector('[data-id="queue"]')!);
    const decisions = within(details().getByRole("region", { name: "Decisions" }));
    expect(decisions.getByText("Use a queue")).toBeInTheDocument();
    expect(details().getByText("queue")).toBeInTheDocument();
  });

  it("CAD-FR-MXHO: a step of the flow list selects the edge of its step", () => {
    render(<Interactive of={layout} />);
    fireEvent.change(screen.getByRole("combobox", { name: "Flow" }), { target: { value: "read-flow" } });
    const steps = within(details().getByRole("region", { name: "Steps" })).getAllByRole("button");
    fireEvent.click(steps[1]);
    expect(edgeLabel("api-db")).toHaveClass("calm-selected");
    expect(details().getByRole("heading", { name: "API <b>bold</b> → DB" })).toBeInTheDocument();
  });

  it("CAD-FR-LLNQ, CAD-FR-MBML: the diagram area has its name, and the zoom readout and Fit show", () => {
    render(<Interactive of={layout} />);
    expect(screen.getByRole("group", { name: "CALM architecture diagram" })).toBeInTheDocument();
    expect(screen.getByTestId("calm-zoom")).toHaveTextContent("%");
    expect(screen.getByRole("button", { name: "Fit to window" })).toBeInTheDocument();
  });

  it("CAD-FR-MBML: the keys zoom the drawing, and the key 0 fits it again", () => {
    render(<Interactive of={layout} />);
    const area = screen.getByRole("group", { name: "CALM architecture diagram" });
    const before = screen.getByTestId("calm-zoom").textContent;
    fireEvent.keyDown(area, { key: "+" });
    expect(screen.getByTestId("calm-zoom").textContent).not.toBe(before);
    expect(area).not.toHaveAttribute("data-fitted");
    fireEvent.keyDown(area, { key: "0" });
    expect(area).toHaveAttribute("data-fitted", "true");
  });

  it("choosing a flow logs the number of steps and no document text", () => {
    render(<Interactive of={layout} />);
    fireEvent.change(screen.getByRole("combobox", { name: "Flow" }), {
      target: { value: "read-flow" },
    });
    expect(logDebug).toHaveBeenCalledWith(["frontend"], "a CALM flow was chosen", {
      chosen: true,
      steps: 3,
    });
  });
});
