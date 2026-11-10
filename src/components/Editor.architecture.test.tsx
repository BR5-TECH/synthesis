import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";

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

import { renderHook } from "@testing-library/react";
import { Editor } from "./Editor";
import { useArchitectureView } from "./Editor/ArchitectureView";
import { EditSessionStore } from "../state/editSessions";
import * as logging from "../logging";
import { resetLogBufferForTest } from "../logging";
import { readCalm, resetCalmForTest } from "../calm/read";
import { CALM_TEMPLATES } from "../calm/templates";

const FILE = "system.calm.json";
const SOURCE = CALM_TEMPLATES.find((t) => t.key === "flow")!.source + "\n";

interface Backend {
  body: string;
  saves: string[];
}

function wireBackend(body: string): Backend {
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

function mount(body = SOURCE, props: { readOnly?: boolean; artifactType?: "architecture" } = {}) {
  const backend = wireBackend(body);
  const view = render(
    <Editor
      artifactId={FILE}
      artifactName={FILE}
      artifactType={props.artifactType ?? "architecture"}
      sessions={sessions}
      readOnly={props.readOnly}
    />,
  );
  return { backend, view };
}

const toggle = () => screen.getByTestId("architecture-view-toggle");
const diagram = () => screen.queryByTestId("architecture-diagram");
const sourceBox = () => document.querySelector(".editor__source-wrap") as HTMLElement;


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
  resetCalmForTest();
  sessions = new EditSessionStore();
});

afterEach(() => cleanup());

describe("the Architecture tab", () => {
  it("CAD-FR-RBYK, CAD-FR-NQXR: an Architecture tab opens on the Diagram view of its buffer", async () => {
    mount();
    await screen.findByRole("group", { name: "CALM architecture diagram" });
    expect(diagram()).toBeInTheDocument();
    expect(sourceBox()).toHaveStyle({ display: "none" });
    expect(screen.getByRole("combobox", { name: "Flow" })).toBeInTheDocument();
    expect(screen.getByRole("complementary", { name: "Details" })).toBeInTheDocument();
  });

  it("CAD-FR-OJTD, CAD-FR-PTBB: the toggle switches the views and changes no byte", async () => {
    const { backend } = mount();
    await screen.findByRole("group", { name: "CALM architecture diagram" });
    expect(toggle()).toHaveAccessibleName("Show architecture source");
    expect(toggle()).toHaveAttribute("title", "Show architecture source");
    fireEvent.click(toggle());
    expect(diagram()).toBeNull();
    expect(sourceBox()).not.toHaveStyle({ display: "none" });
    expect(toggle()).toHaveAccessibleName("Show architecture diagram");
    fireEvent.click(toggle());
    expect(diagram()).toBeInTheDocument();
    expect(screen.queryByText("● unsaved")).toBeNull();
    await act(async () => {
      await sessions.flush(FILE, { force: true });
    });
    expect(backend.saves).toEqual([SOURCE]);
  });

  it("ESH-FR-LKNM: an Architecture tab has the Diagram / Source toggle and no editing-mode toggle", async () => {
    mount();
    await screen.findByRole("group", { name: "CALM architecture diagram" });
    expect(screen.queryByTestId("editor-mode-toggle")).toBeNull();
    expect(toggle()).toBeInTheDocument();
  });

  it("ESH-FR-SSDV: a JSON file of another type has no Diagram view and no toggle", async () => {
    wireBackend(SOURCE);
    render(<Editor artifactId="data.json" artifactName="data.json" sessions={sessions} />);
    await waitFor(() => expect(sourceBox()).toBeInTheDocument());
    expect(screen.queryByTestId("architecture-view-toggle")).toBeNull();
    expect(diagram()).toBeNull();
  });

  it("CAD-FR-RBYK, ESH-FR-FKZB: a Markdown file with the architecture type keeps the Markdown surfaces", async () => {
    wireBackend("# Notes\n");
    render(
      <Editor artifactId="notes.md" artifactName="notes.md" artifactType="architecture" sessions={sessions} />,
    );
    await screen.findByTestId("editor-mode-toggle");
    expect(screen.queryByTestId("architecture-view-toggle")).toBeNull();
    expect(diagram()).toBeNull();
  });

  it("CAD-FR-NQXR, CAD-FR-PDVE: an empty buffer opens on the Source view, and its Diagram view says so", async () => {
    mount("");
    await waitFor(() => expect(toggle()).toHaveAccessibleName("Show architecture diagram"));
    expect(diagram()).toBeNull();
    fireEvent.click(toggle());
    expect(within(diagram()!).getByText("No diagram to show")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Show source" }));
    expect(diagram()).toBeNull();
  });

  it("CAD-FR-NQXR: a tab whose type becomes architecture after it opened stays on the Source view", async () => {
    wireBackend(SOURCE);
    const view = render(<Editor artifactId={FILE} artifactName={FILE} sessions={sessions} />);
    await waitFor(() => expect(sourceBox()).toBeInTheDocument());
    view.rerender(
      <Editor artifactId={FILE} artifactName={FILE} artifactType="architecture" sessions={sessions} />,
    );
    await waitFor(() => expect(toggle()).toHaveAccessibleName("Show architecture diagram"));
    expect(diagram()).toBeNull();
  });

  it("CAD-FR-OPTU: the Diagram view reads the buffer again after an edit", async () => {
    mount();
    await screen.findByRole("group", { name: "CALM architecture diagram" });
    fireEvent.click(toggle());
    const textarea = sourceBox().querySelector("textarea") as HTMLTextAreaElement;
    fireEvent.change(textarea, { target: { value: SOURCE.replace('"Checkout"', '"Till"') } });
    fireEvent.click(toggle());
    await waitFor(() =>
      expect(document.querySelector('[data-calm-node="checkout"]')).toHaveTextContent("Till"),
    );
  });

  it("CAD-FR-OZST: an error shows its statement and reason, and Show source puts the caret on its line", async () => {
    const broken = '{\n  "nodes": [\n    nope\n  ]\n}\n';
    mount(broken);
    // A buffer with text opens on the Diagram view, where the error shows.
    await screen.findByText("Diagram error: The CALM source is not valid JSON.");
    expect(screen.getByText(/Line 3, column \d+\./)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Show source" }));
    expect(diagram()).toBeNull();
    const textarea = sourceBox().querySelector("textarea") as HTMLTextAreaElement;
    await waitFor(() => expect(textarea.selectionStart).toBe(broken.indexOf("    nope")));
    expect(textarea).toHaveFocus();
  });

  it("CAD-FR-PVCT: opening Find while the Diagram view shows switches to the Source view", async () => {
    mount();
    await screen.findByRole("group", { name: "CALM architecture diagram" });
    // The shell routes the Find shortcut to the find state of the artifact.
    act(() => {
      sessions.setFind(FILE, { form: "find" });
    });
    await waitFor(() => expect(diagram()).toBeNull());
  });

  it("CAD-FR-PVCT: while Find stays open, the author can switch back to the Diagram view", async () => {
    mount();
    await screen.findByRole("group", { name: "CALM architecture diagram" });
    act(() => {
      sessions.setFind(FILE, { form: "find" });
    });
    await waitFor(() => expect(diagram()).toBeNull());
    fireEvent.click(toggle());
    expect(diagram()).toBeInTheDocument();
  });

  it("CAD-FR-PVCT: activating a comment thread while the Diagram view shows switches to the Source view", () => {
    const sourceRef = { current: null };
    const props = {
      architecture: true,
      loaded: true,
      buffer: SOURCE,
      findActive: false,
      reviewing: false,
      sourceRef,
    };
    const { result, rerender } = renderHook(
      ({ focusedThread }: { focusedThread: string | null }) =>
        useArchitectureView({ ...props, focusedThread }),
      { initialProps: { focusedThread: null as string | null } },
    );
    expect(result.current.view).toBe("diagram");
    rerender({ focusedThread: "thread-1" });
    expect(result.current.view).toBe("source");
    act(() => result.current.toggle());
    expect(result.current.view).toBe("diagram");
  });

  it("CAD-FR-QADV: on a tab that accepts no input the toggle stays enabled", async () => {
    mount(SOURCE, { readOnly: true });
    await screen.findByRole("group", { name: "CALM architecture diagram" });
    expect(toggle()).not.toBeDisabled();
    fireEvent.click(toggle());
    expect(diagram()).toBeNull();
  });

  it("CAD-FR-QLQZ: a switch of the view logs the new view and no document text", async () => {
    const debug = vi.spyOn(logging, "logDebug");
    try {
      mount();
      await screen.findByRole("group", { name: "CALM architecture diagram" });
      fireEvent.click(toggle());
      fireEvent.click(toggle());
      const records = debug.mock.calls.filter(
        ([, message]) => message === "the Architecture view changed",
      );
      // One record for each switch; the view a tab opens on is no switch.
      expect(records).toEqual([
        [["frontend"], "the Architecture view changed", { view: "source" }],
        [["frontend"], "the Architecture view changed", { view: "diagram" }],
      ]);
      expect(JSON.stringify(records)).not.toContain("Checkout");
    } finally {
      debug.mockRestore();
    }
  });

  it("CAD-FR-PVCT: a proposed change under review shows on the Source view", () => {
    const sourceRef = { current: null };
    const { result, rerender } = renderHook(
      ({ reviewing }: { reviewing: boolean }) =>
        useArchitectureView({
          architecture: true,
          loaded: true,
          buffer: SOURCE,
          findActive: false,
          focusedThread: null,
          reviewing,
          sourceRef,
        }),
      { initialProps: { reviewing: false } },
    );
    expect(result.current.view).toBe("diagram");
    rerender({ reviewing: true });
    expect(result.current.view).toBe("source");
  });
});
