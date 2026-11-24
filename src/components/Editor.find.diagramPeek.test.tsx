import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { Editor as TiptapEditor } from "@tiptap/react";
import { TextSelection } from "@tiptap/pm/state";

vi.mock("mermaid", async () => (await import("../test/mermaidMock")).mermaidModule);

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import { Editor } from "./Editor";
import { EditSessionStore } from "../state/editSessions";
import { resetLogBufferForTest } from "../logging";
import { resetMermaidMock } from "../test/mermaidMock";
import { DIAGRAM_LABEL } from "./markdownMermaid";
import { resetMermaidForTest } from "./markdownMermaidRender";

/**
 * `EFR-editor-find-replace.md`: the diagram peek (EFR-FR-DGBG) through the real
 * Editor. A find action moves the peek to the current match. An edit of the
 * author can make another match current, but the peek stays where it is
 * (EFR-FR-KQWT) until the next find action (EFR-FR-HZRM).
 */
const BODY = "a graph here\n\n```mermaid\ngraph TD\nA-->B\n```\n\nAfter.\n";

function wireBackend() {
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "load_artifact_contents_by_id") {
      return Promise.resolve({ body: BODY, checksum: "ck1" });
    }
    if (cmd === "save_artifact_contents") return Promise.resolve({ checksum: "ck2" });
    if (cmd === "append_log_records") return Promise.resolve();
    return Promise.reject(new Error(`unexpected invoke ${cmd}`));
  });
}

let sessions: EditSessionStore;

beforeEach(() => {
  invokeMock.mockReset();
  resetLogBufferForTest();
  resetMermaidForTest();
  resetMermaidMock();
  sessions = new EditSessionStore();
  wireBackend();
});

afterEach(() => {
  cleanup();
});

const pmEl = () => document.querySelector(".ProseMirror") as HTMLElement;
const view = () => (pmEl() as unknown as { editor: TiptapEditor }).editor.view;
const viewOf = () =>
  (pmEl().querySelector("pre") as HTMLElement).getAttribute("data-diagram-view");
const count = () => screen.getByTestId("find-count").textContent;
const next = () => fireEvent.click(screen.getByRole("button", { name: "Next match" }));

/** The positions of every "graph" in the document, in order. */
function graphs(): number[] {
  const found: number[] = [];
  view().state.doc.descendants((node, pos) => {
    if (!node.isText || !node.text) return;
    let at = node.text.indexOf("graph");
    while (at !== -1) {
      found.push(pos + at);
      at = node.text.indexOf("graph", at + 1);
    }
  });
  return found;
}

/** Select the `nth` "graph" and type `text` over it, as the author does. */
function typeOverGraph(nth: number, text: string) {
  act(() => {
    const v = view();
    const from = graphs()[nth];
    const { tr } = v.state;
    tr.setSelection(TextSelection.create(tr.doc, from, from + "graph".length));
    v.dispatch(tr.insertText(text));
  });
}

async function nextFrames() {
  for (let i = 0; i < 2; i++) {
    await act(
      () =>
        new Promise<void>((resolve) => {
          if (typeof requestAnimationFrame === "function") {
            requestAnimationFrame(() => resolve());
          } else setTimeout(resolve, 0);
        }),
    );
  }
}

async function findGraph(expected: string) {
  render(<Editor artifactId="a.md" artifactName="a.md" sessions={sessions} />);
  await screen.findByRole("button", { name: DIAGRAM_LABEL });
  act(() => sessions.setFind("a.md", { form: "find" }));
  fireEvent.change(screen.getByLabelText("Find"), { target: { value: "graph" } });
  await waitFor(() => expect(count()).toBe(expected));
}

describe("the diagram peek after an edit (EFR-FR-KQWT, EFR-FR-DGBG)", () => {
  it("EFR-FR-KQWT, EFR-FR-HZRM: an edit that makes the fence match current does not peek; next-match does", async () => {
    await findGraph("1/2");
    expect(viewOf()).toBe("diagram");

    typeOverGraph(0, "chart");

    await waitFor(() => expect(count()).toBe("1/1"));
    await nextFrames();
    expect(viewOf()).toBe("diagram");

    next();

    await waitFor(() => expect(viewOf()).toBe("source"));
    expect(count()).toBe("1/1");
  });

  it("EFR-FR-KQWT, EFR-FR-DGBG: an open peek stays open when an edit moves the current match out, and a find action ends it", async () => {
    await findGraph("1/2");
    next();
    await waitFor(() => expect(count()).toBe("2/2"));
    await waitFor(() => expect(viewOf()).toBe("source"));

    // The fence's own match goes; the current match falls back to the first.
    typeOverGraph(1, "flowchart");

    await waitFor(() => expect(count()).toBe("1/1"));
    await nextFrames();
    expect(viewOf()).toBe("source");

    // The first next-match reveals the match outside the fence, so the peek ends.
    next();

    await waitFor(() => expect(viewOf()).toBe("diagram"));
    expect(count()).toBe("1/1");
  });
});
