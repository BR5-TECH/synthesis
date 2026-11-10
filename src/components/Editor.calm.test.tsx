import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { Editor as TiptapEditor } from "@tiptap/react";

vi.mock("mermaid", async () => (await import("../test/mermaidMock")).mermaidModule);
vi.mock("@xyflow/react", async (importOriginal) =>
  (await import("../test/calmMock")).xyflowModule(await importOriginal()),
);

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import { Editor } from "./Editor";
import { DiagramEditorHost } from "./DiagramEditor";
import { resetDiagramEditorForTest } from "./DiagramEditor/store";
import { CALM_HINT_EXAMPLE } from "./DiagramEditor/hints/calmHints";
import { EditSessionStore } from "../state/editSessions";
import * as logging from "../logging";
import { resetLogBufferForTest } from "../logging";
import { mermaidKey } from "./markdownMermaid";
import { resetMermaidMock } from "../test/mermaidMock";
import { resetMermaidForTest } from "./markdownMermaidRender";
import { readCalm, resetCalmForTest } from "../calm/read";
import { CALM_TEMPLATES } from "../calm/templates";

const SCHEMA = "https://calm.finos.org/release/1.3/meta/calm.json";
const CALM = JSON.stringify(
  {
    $schema: SCHEMA,
    nodes: [
      { "unique-id": "a", "node-type": "service", name: "Alpha", description: "A." },
      { "unique-id": "b", "node-type": "database", name: "Beta", description: "B." },
    ],
    relationships: [
      {
        "unique-id": "ab",
        "relationship-type": { connects: { source: { node: "a" }, destination: { node: "b" } } },
      },
    ],
  },
  null,
  2,
);
const fenceOf = (source: string) => "```calm\n" + source + "\n```";
const BODY = `# Title\n\n${fenceOf(CALM)}\n\nAfter the diagram.\n`;

interface Backend {
  body: string;
  saves: string[];
}

/** A strict backend: it answers what the Editor asks for and rejects the rest. */
function wireBackend(body = BODY): Backend {
  const backend: Backend = { body, saves: [] };
  invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
    if (cmd === "load_artifact_contents_by_id") return { body: backend.body, checksum: "ck1" };
    if (cmd === "save_artifact_contents") {
      backend.saves.push((args as { body: string }).body);
      return { checksum: `ck-saved-${backend.saves.length}` };
    }
    if (cmd === "append_log_records") return undefined;
    return Promise.reject(new Error(`unexpected invoke ${cmd}`));
  });
  return backend;
}

let sessions: EditSessionStore;

const pmEl = () => document.querySelector(".ProseMirror") as HTMLElement;
const tiptap = () => (pmEl() as unknown as { editor: TiptapEditor }).editor;
const calmFence = () => pmEl().querySelector("pre") as HTMLElement;
const CANVAS_NAME = "CALM architecture diagram";

/** The inline canvas of the first CALM fence, once its read has drawn it. */
async function drawnCanvas(): Promise<HTMLElement> {
  let found: HTMLElement | null = null;
  await waitFor(() => {
    found = document.querySelector<HTMLElement>(".calm-figure");
    expect(found?.querySelector(".calm-inline")).toBeTruthy();
  });
  return found!;
}

async function mount(body = BODY, readOnly = false) {
  const backend = wireBackend(body);
  const view = render(
    <>
      <Editor artifactId="a.md" artifactName="a.md" sessions={sessions} readOnly={readOnly} />
      <DiagramEditorHost />
    </>,
  );
  await waitFor(() => expect(pmEl()).not.toBeNull());
  return { backend, view };
}


/**
 * The first read of a test file loads the schema validator and the layout
 * engine. That load happens here, under the hook limit, so that no test waits
 * for it under the shorter limit of a `findBy` on a slow runner.
 */
beforeAll(async () => {
  await readCalm(CALM_TEMPLATES[0].source);
});

beforeEach(() => {
  invokeMock.mockReset();
  resetLogBufferForTest();
  resetMermaidForTest();
  resetMermaidMock();
  resetCalmForTest();
  resetDiagramEditorForTest();
  sessions = new EditSessionStore();
});

afterEach(() => {
  cleanup();
  delete document.documentElement.dataset.theme;
});

describe("CALM fences in an Editor tab", () => {
  it("CAD-FR-PFNM, CAD-FR-IPRR, EDT-FR-MOYS: a calm fence shows the inline canvas and saves as the same fence", async () => {
    const { backend } = await mount();
    const canvas = await drawnCanvas();
    expect(canvas).toHaveAttribute("role", "button");
    expect(canvas).toHaveAccessibleName(CANVAS_NAME);
    expect(within(canvas).getByText("Alpha")).toBeInTheDocument();
    expect(calmFence().getAttribute("data-diagram-view")).toBe("diagram");
    await act(async () => {
      await sessions.flush("a.md", { force: true });
    });
    expect(backend.saves).toEqual([BODY]);
  });

  it("CAD-FR-IRPP: a click or Enter on the inline canvas shows the source with the caret in it", async () => {
    await mount();
    fireEvent.click(await drawnCanvas());
    expect(calmFence().getAttribute("data-diagram-view")).toBe("source");
    const { from } = tiptap().state.selection;
    expect(tiptap().state.doc.resolve(from).parent.type.name).toBe("codeBlock");

    fireEvent.click(screen.getByRole("button", { name: "Show diagram" }));
    const again = await screen.findByRole("button", { name: CANVAS_NAME });
    fireEvent.keyDown(again, { key: "Enter" });
    expect(calmFence().getAttribute("data-diagram-view")).toBe("source");
  });

  it("CAD-FR-ITUS, CAD-FR-IWGT: a schema error shows the source and the error, and no canvas", async () => {
    const bad = JSON.stringify({
      $schema: SCHEMA,
      nodes: [{ "unique-id": "a", "node-type": "service", name: "A" }],
    });
    await mount(`# Title\n\n${fenceOf(bad)}\n`);
    await screen.findByText(
      "Diagram error: The CALM source does not match the CALM 1.3 schema.",
    );
    expect(screen.getByText("/nodes/0: must have required property 'description'")).toBeInTheDocument();
    expect(calmFence().getAttribute("data-diagram-view")).toBe("error");
    expect(screen.queryByRole("button", { name: CANVAS_NAME })).toBeNull();
    expect(screen.queryByRole("button", { name: "View diagram" })).toBeNull();
    expect(screen.getByRole("button", { name: "Edit diagram" })).toBeInTheDocument();
  });

  it("CAD-FR-ITUS: a reference error and a parse error show their own statements", async () => {
    const dangling = JSON.stringify({
      $schema: SCHEMA,
      nodes: [{ "unique-id": "a", "node-type": "service", name: "A", description: "A." }],
      relationships: [
        { "unique-id": "r", "relationship-type": { connects: { source: { node: "a" }, destination: { node: "x" } } } },
      ],
    });
    await mount(`${fenceOf(dangling)}\n\n${fenceOf("{ oops")}\n`);
    await screen.findByText("Diagram error: The architecture refers to an item that does not exist.");
    await screen.findByText("Diagram error: The CALM source is not valid JSON.");
  });

  it("CAD-FR-JBUT: a graph with no nodes shows the statement and has the View button", async () => {
    await mount(fenceOf(JSON.stringify({ $schema: SCHEMA, nodes: [] })) + "\n");
    await screen.findByText("The architecture has no nodes.");
    expect(screen.getByRole("button", { name: "View diagram" })).toBeInTheDocument();
  });

  it("CAD-FR-IXAC: a change of theme keeps the drawn canvas and reads nothing again", async () => {
    await mount();
    const canvas = await drawnCanvas();
    const drawn = canvas.querySelector(".calm-inline");
    const debug = vi.spyOn(logging, "logDebug");
    try {
      act(() => {
        document.documentElement.dataset.theme = "dark";
      });
      // The surface has taken the new theme.
      await waitFor(() => expect(mermaidKey.getState(tiptap().state)?.theme).toBe("dark"));
      expect(document.querySelector(".calm-figure .calm-inline")).toBe(drawn);
      const reads = debug.mock.calls.filter(([, message]) => message === "a CALM read ended");
      expect(reads).toHaveLength(0);
    } finally {
      debug.mockRestore();
    }
  });

  it("EDT-FR-TQWB: a Mermaid fence and a CALM fence in one file each show their own diagram", async () => {
    await mount(`${fenceOf(CALM)}\n\n\`\`\`mermaid\ngraph TD\nA-->B\n\`\`\`\n`);
    await screen.findByRole("button", { name: CANVAS_NAME });
    await screen.findByRole("button", { name: "Mermaid diagram, edit source" });
    expect(screen.getAllByRole("button", { name: "Edit diagram" })).toHaveLength(2);
  });

  it("EDT-FR-TQWB, EDT-FR-LEUI: on a surface that accepts no input, a CALM fence is inert", async () => {
    await mount(BODY, true);
    const canvas = await screen.findByRole("img", { name: CANVAS_NAME });
    expect(canvas).not.toHaveAttribute("tabindex");
    expect(screen.queryByRole("button", { name: "Edit diagram" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Show diagram source" })).toBeNull();
  });
});

const calmButton = () => screen.getByRole("button", { name: "Insert CALM architecture" });
const templateOptions = () =>
  within(screen.getByRole("listbox", { name: "Insert CALM architecture" })).getAllByRole("option");

describe("Insert CALM architecture", () => {
  it("CAD-FR-HMXS, EDT-FR-VUGQ: the button sits right after Insert diagram, with an icon, its name, and its tooltip", async () => {
    await mount("Some text.\n");
    const button = calmButton();
    expect(button).toHaveAttribute("title", "Insert CALM architecture");
    expect(button.textContent).toBe("");
    expect(button.querySelector("svg")).not.toBeNull();
    const band = button.closest(".editor__toolbar") as HTMLElement;
    const buttons: HTMLElement[] = Array.from(band.querySelectorAll("button"));
    const diagram = screen.getByRole("button", { name: "Insert diagram" });
    expect(buttons.indexOf(button)).toBe(buttons.indexOf(diagram) + 1);
  });

  it("CAD-FR-HMXS: the button is disabled while the surface accepts no input", async () => {
    await mount("Some text.\n", true);
    expect(calmButton()).toBeDisabled();
  });

  it("CAD-FR-JNWR, CAD-FR-JWXC: the menu lists the templates and the filter matches a name", async () => {
    await mount("Some text.\n");
    fireEvent.click(calmButton());
    expect(templateOptions().map((o) => o.textContent)).toEqual(CALM_TEMPLATES.map((t) => t.name));
    fireEvent.change(screen.getByTestId("calm-template-filter"), { target: { value: "FLOW" } });
    expect(templateOptions().map((o) => o.textContent)).toEqual(["Business flow"]);
    fireEvent.keyDown(screen.getByTestId("calm-template-filter"), { key: "Escape" });
    expect(screen.queryByRole("listbox", { name: "Insert CALM architecture" })).toBeNull();
  });

  it("CAD-FR-KTWD: a template inserts a CALM fence that shows its source, as one undo step", async () => {
    await mount("Some text.\n");
    act(() => {
      tiptap().commands.setTextSelection(3);
    });
    fireEvent.click(calmButton());
    fireEvent.click(templateOptions()[0]);
    const fence = tiptap().state.doc.child(1);
    expect(fence.type.name).toBe("codeBlock");
    expect(fence.attrs.language).toBe("calm");
    expect(fence.textContent).toBe(CALM_TEMPLATES[0].source);
    await waitFor(() => expect(calmFence().getAttribute("data-diagram-view")).toBe("source"));
    expect(tiptap().state.doc.resolve(tiptap().state.selection.from).parent.type.name).toBe(
      "codeBlock",
    );
    fireEvent.keyDown(pmEl(), { key: "z", metaKey: true });
    expect(tiptap().state.doc.childCount).toBe(1);
    expect(tiptap().getText()).toBe("Some text.");
  });
});

describe("CALM in the diagram overlays", () => {
  it("CAD-FR-KUYJ, CAD-FR-KWIY: the editor overlay has the CALM names, the CALM diagram, and the CALM hints", async () => {
    await mount();
    await drawnCanvas();
    fireEvent.click(screen.getByRole("button", { name: "Edit diagram" }));
    const dialog = await screen.findByRole("dialog", { name: "Edit CALM architecture" });
    const preview = within(dialog).getByRole("region", { name: "Diagram preview" });
    await within(preview).findByRole("img", { name: CANVAS_NAME });
    const hints = within(dialog).getByRole("region", { name: "CALM syntax hints" });
    expect(within(hints).getByRole("heading", { name: "CALM 1.3" })).toBeInTheDocument();
    for (const group of ["Document", "Nodes", "Relationships", "Interfaces", "Flows", "Controls", "Example"]) {
      expect(within(hints).getByRole("region", { name: group })).toBeInTheDocument();
    }
  });

  it("CAD-FR-KUYJ: an error in the overlay source dims the last diagram under the error", async () => {
    await mount();
    await drawnCanvas();
    fireEvent.click(screen.getByRole("button", { name: "Edit diagram" }));
    const dialog = await screen.findByRole("dialog", { name: "Edit CALM architecture" });
    await within(dialog).findByRole("img", { name: CANVAS_NAME });
    fireEvent.change(within(dialog).getByRole("textbox", { name: "Diagram source" }), {
      target: { value: "{ broken" },
    });
    await within(dialog).findByText("Diagram error: The CALM source is not valid JSON.");
    expect(within(dialog).getByRole("img", { name: CANVAS_NAME })).toHaveAttribute("data-stale", "true");
  });

  it("CAD-FR-LEII: the complete example of the CALM hints reads as a diagram", async () => {
    const result = await readCalm(CALM_HINT_EXAMPLE);
    expect(result.state).toBe("rendered");
  });

  it("CAD-FR-LLNQ, CAD-FR-LQLI: the viewer has the CALM names, the canvas, and the flow selector", async () => {
    const flow = CALM_TEMPLATES.find((t) => t.key === "flow")!;
    await mount(fenceOf(flow.source) + "\n");
    await drawnCanvas();
    fireEvent.click(screen.getByRole("button", { name: "View diagram" }));
    const dialog = await screen.findByRole("dialog", { name: "View CALM architecture" });
    expect(await within(dialog).findByRole("group", { name: CANVAS_NAME })).toBeInTheDocument();
    const select = within(dialog).getByRole("combobox", { name: "Flow" }) as HTMLSelectElement;
    expect(Array.from(select.options).map((o) => o.text)).toEqual(["No flow", "Pay for an order"]);
    expect(within(dialog).getByRole("button", { name: "Fit to window" })).toBeInTheDocument();
    expect(within(dialog).getByRole("complementary", { name: "Details" })).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });
});
