import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { Editor } from "@tiptap/react";

vi.mock("../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

/** Whether the next import of the library fails, as a missing chunk would. */
const library = { fail: true };

type RenderModule = typeof import("./markdownMermaidRender");
type FidelityModule = typeof import("./markdownFidelity");

const SRC = "graph TD\nA-->B";
let editors: Editor[] = [];

let render: RenderModule;
let fidelity: FidelityModule;

// A successful import is cached, so each test loads the modules anew and its
// first import of the library fails again.
beforeEach(async () => {
  library.fail = true;
  vi.resetModules();
  // The registry keeps a factory's result, so each test registers it anew.
  vi.doMock("mermaid", async () => {
    if (library.fail) throw new Error("chunk missing");
    return (await import("../test/mermaidMock")).mermaidModule;
  });
  render = await import("./markdownMermaidRender");
  fidelity = await import("./markdownFidelity");
});

afterEach(() => {
  for (const ed of editors) ed.destroy();
  editors = [];
  vi.doUnmock("mermaid");
});

describe("a library that does not load (EDT-FR-XTEB)", () => {
  it("EDT-FR-XTEB: a failed load shows an error, and the load is tried again once the failure is forgotten", async () => {
    expect(await render.renderDiagram(SRC, "light")).toEqual({
      state: "error",
      message: "The diagram library could not be loaded.",
      transient: true,
    });
    library.fail = false;
    // Still known as failed until the next edit forgets it.
    expect(render.cachedDiagram(SRC, "light")?.state).toBe("error");
    render.forgetTransientFailures();
    expect(render.cachedDiagram(SRC, "light")).toBeUndefined();
    expect((await render.renderDiagram(SRC, "light"))?.state).toBe("rendered");
  });

  it("EDT-FR-XTEB, EDT-FR-MIQE: a fence that could not load the library renders after the next edit", async () => {
    const ed = new Editor({
      extensions: fidelity.markdownExtensions(),
      content: "```mermaid\ngraph TD\nA-->B\n```\n\nAfter.",
    });
    editors.push(ed);
    const pre = () => ed.view.dom.querySelector("pre")!;
    await vi.waitFor(() => expect(pre().getAttribute("data-diagram-view")).toBe("error"));
    expect(pre().querySelector(".mermaid-error")?.textContent).toBe(
      "Diagram error: The diagram library could not be loaded.",
    );
    library.fail = false;
    // An edit elsewhere in the document is enough to try again.
    ed.view.dispatch(ed.state.tr.insertText("!", ed.state.doc.content.size - 1));
    await vi.waitFor(() => expect(pre().getAttribute("data-diagram-view")).toBe("diagram"));
  });
});
