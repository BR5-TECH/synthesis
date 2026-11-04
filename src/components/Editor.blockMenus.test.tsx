/**
 * The block group of the formatting toolbar — the Hx, Vx, and Ax dropdowns —
 * and the markup they write, on a mounted Editor tab (EDT-FR-UPNB and the
 * requirements around it). The commands themselves are in
 * `markdownMarkup.test.ts`.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import type { Editor as TiptapEditor } from "@tiptap/react";
import { NodeSelection, TextSelection } from "@tiptap/pm/state";

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
import { resetMermaidForTest } from "./markdownMermaidRender";
import { getMarkdown } from "./markdownFidelity";
import { blockMenuTrigger, chooseToolbarItem } from "../test/toolbarMenus";
import { letWriteLand } from "../test/autosave";

interface Backend {
  saves: string[];
}

/** A strict backend: it answers what the Editor asks for and rejects the rest. */
function wireBackend(body: string): Backend {
  const backend: Backend = { saves: [] };
  invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
    if (cmd === "load_artifact_contents_by_id") return { body, checksum: "ck1" };
    if (cmd === "save_artifact_contents") {
      backend.saves.push((args as { body: string }).body);
      return { checksum: `ck-saved-${backend.saves.length}` };
    }
    if (cmd === "append_log_records") return undefined;
    throw new Error(`unexpected invoke ${cmd}`);
  });
  return backend;
}

let sessions: EditSessionStore;

const pmEl = () => document.querySelector(".ProseMirror") as HTMLElement;
const tiptap = () => (pmEl() as unknown as { editor: TiptapEditor }).editor;
const pressUndo = () => fireEvent.keyDown(pmEl(), { key: "z", metaKey: true });
const popup = (menu: string) => screen.queryByRole("group", { name: menu });

async function mount(body: string, props: { readOnly?: boolean } = {}) {
  const backend = wireBackend(body);
  render(<Editor artifactId="a.md" artifactName="a.md" sessions={sessions} {...props} />);
  await screen.findByText("Body.");
  return backend;
}

async function saveNow() {
  await act(async () => {
    await sessions.flush("a.md", { force: true });
  });
}

function caretIn(text: string) {
  const ed = tiptap();
  let at = -1;
  ed.state.doc.descendants((node, pos) => {
    if (at === -1 && node.isTextblock && node.textContent === text) at = pos + 1;
  });
  act(() => {
    ed.view.dispatch(ed.state.tr.setSelection(TextSelection.create(ed.state.doc, at)));
  });
}

function selectSpacer() {
  const ed = tiptap();
  let at = -1;
  ed.state.doc.descendants((node, pos) => {
    if (at === -1 && node.type.name === "verticalBreak") at = pos;
  });
  act(() => {
    ed.view.dispatch(ed.state.tr.setSelection(NodeSelection.create(ed.state.doc, at)));
  });
}

beforeEach(() => {
  invokeMock.mockReset();
  resetLogBufferForTest();
  resetMermaidForTest();
  resetMermaidMock();
  sessions = new EditSessionStore();
});

afterEach(() => {
  cleanup();
});

describe("the block group (EDT-FR-UPNB, EDT-FR-HDCZ, EDT-FR-EUUI)", () => {
  it("EDT-FR-UPNB: Hx, Vx, and Ax stand in that order, each named, with a tooltip", async () => {
    await mount("Body.\n");
    const triggers = ["Heading", "Vertical break", "Alignment"].map((name) => {
      const trigger = blockMenuTrigger(name);
      expect(trigger).toHaveAttribute("title", name);
      expect(trigger).toHaveAttribute("aria-expanded", "false");
      return trigger;
    });
    expect(triggers.map((t) => t.textContent?.replace("▾", "").trim())).toEqual(["Hx", "Vx", "Ax"]);
    for (let i = 1; i < triggers.length; i++) {
      expect(
        triggers[i - 1].compareDocumentPosition(triggers[i]) & Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();
    }
    // The block group comes before the insert group's diagram button.
    expect(
      triggers[2].compareDocumentPosition(screen.getByRole("button", { name: "Insert diagram" })) &
        Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("EDT-FR-HDCZ: Vx holds V1 to V4, named Vertical break Vn", async () => {
    await mount("Body.\n");
    fireEvent.click(blockMenuTrigger("Vertical break"));
    const items = within(popup("Vertical break")!).getAllByRole("button");
    expect(items.map((item) => item.textContent)).toEqual(["V1", "V2", "V3", "V4"]);
    expect(items.map((item) => item.getAttribute("aria-label"))).toEqual([
      "Vertical break V1",
      "Vertical break V2",
      "Vertical break V3",
      "Vertical break V4",
    ]);
    expect(items.map((item) => item.getAttribute("title"))).toEqual(
      items.map((item) => item.getAttribute("aria-label")),
    );
  });

  it("EDT-FR-EUUI: Ax holds Left, Center, and Right as icons with no text", async () => {
    await mount("Body.\n");
    fireEvent.click(blockMenuTrigger("Alignment"));
    const items = within(popup("Alignment")!).getAllByRole("button");
    expect(items.map((item) => item.getAttribute("aria-label"))).toEqual([
      "Align left",
      "Align center",
      "Align right",
    ]);
    for (const item of items) {
      expect(item).toHaveAttribute("title", item.getAttribute("aria-label"));
      expect(item.textContent).toBe("");
      expect(item.querySelector("svg")).not.toBeNull();
    }
  });
});

describe("keyboard and closing (EDT-FR-VSUO, EDT-FR-IRNA)", () => {
  it("EDT-FR-VSUO: Enter opens on the first item, the arrows move, a selection closes the popup", async () => {
    await mount("Body.\n");
    caretIn("Body.");
    const trigger = blockMenuTrigger("Heading");
    trigger.focus();
    fireEvent.keyDown(trigger, { key: "Enter" });
    const items = within(popup("Heading")!).getAllByRole("button");
    expect(document.activeElement).toBe(items[0]);
    fireEvent.keyDown(items[0], { key: "ArrowRight" });
    expect(document.activeElement).toBe(items[1]);
    fireEvent.keyDown(items[1], { key: "ArrowLeft" });
    fireEvent.keyDown(items[0], { key: "ArrowLeft" });
    expect(document.activeElement).toBe(items[3]);
    // Enter on a button is its click.
    fireEvent.click(items[3]);
    expect(popup("Heading")).toBeNull();
    expect(pmEl().querySelector("h4")).toHaveTextContent("Body.");
    // The focus command of the editor runs in the next frame.
    await waitFor(() => expect(document.activeElement).toBe(pmEl()));
  });

  it("EDT-FR-VSUO: Space and Down Arrow open on the active item", async () => {
    await mount("## Body.\n");
    caretIn("Body.");
    const trigger = blockMenuTrigger("Heading");
    fireEvent.keyDown(trigger, { key: " " });
    expect(document.activeElement).toBe(
      within(popup("Heading")!).getByRole("button", { name: "Heading 2" }),
    );
    fireEvent.keyDown(window, { key: "Escape" });
    expect(popup("Heading")).toBeNull();
    fireEvent.keyDown(trigger, { key: "ArrowDown" });
    expect(document.activeElement).toBe(
      within(popup("Heading")!).getByRole("button", { name: "Heading 2" }),
    );
  });

  it("EDT-FR-VSUO: Escape closes the popup and focuses the trigger, also with the focus in the editor", async () => {
    await mount("Body.\n");
    const trigger = blockMenuTrigger("Alignment");
    act(() => pmEl().focus());
    fireEvent.mouseDown(trigger);
    fireEvent.click(trigger);
    expect(trigger).toHaveAttribute("aria-expanded", "true");
    expect(document.activeElement).toBe(pmEl());
    fireEvent.keyDown(pmEl(), { key: "Escape" });
    expect(popup("Alignment")).toBeNull();
    expect(document.activeElement).toBe(trigger);
  });

  it("EDT-FR-VSUO: a click outside closes the popup", async () => {
    await mount("Body.\n");
    fireEvent.click(blockMenuTrigger("Vertical break"));
    fireEvent.mouseDown(document.body);
    expect(popup("Vertical break")).toBeNull();
  });

  it("EDT-FR-IRNA: at most one dropdown is open, the diagram type menu included", async () => {
    await mount("Body.\n");
    fireEvent.click(blockMenuTrigger("Heading"));
    fireEvent.mouseDown(blockMenuTrigger("Vertical break"));
    fireEvent.click(blockMenuTrigger("Vertical break"));
    expect(popup("Heading")).toBeNull();
    expect(popup("Vertical break")).not.toBeNull();

    // A keyboard move to the diagram button closes the open dropdown.
    const diagram = screen.getByRole("button", { name: "Insert diagram" });
    act(() => diagram.focus());
    expect(popup("Vertical break")).toBeNull();
    fireEvent.click(diagram);
    expect(screen.getByRole("listbox", { name: "Insert diagram" })).toBeInTheDocument();

    // A keyboard opening of a dropdown closes the diagram type menu.
    fireEvent.keyDown(blockMenuTrigger("Alignment"), { key: "Enter" });
    expect(popup("Alignment")).not.toBeNull();
    expect(screen.queryByRole("listbox", { name: "Insert diagram" })).toBeNull();
    expect(screen.getAllByRole("group")).toHaveLength(1);
  });

  it("EDT-FR-IRNA: the triggers are disabled while the surface accepts no input", async () => {
    await mount("Body.\n", { readOnly: true });
    for (const name of ["Heading", "Vertical break", "Alignment"]) {
      expect(blockMenuTrigger(name)).toBeDisabled();
    }
  });
});

describe("the active choice (EDT-FR-GELS)", () => {
  it("EDT-FR-GELS: Ax shows the alignment of the caret block, and Left is pressed on a plain block", async () => {
    await mount("<!-- center -->\nCentered.\n\n<!-- right -->\nRight.\n\nBody.\n");
    const trigger = blockMenuTrigger("Alignment");
    const value = () => trigger.querySelector(".block-menu__value")!;

    const itemIcon = (name: string) => {
      fireEvent.click(trigger);
      const markup = within(popup("Alignment")!).getByRole("button", { name }).innerHTML;
      fireEvent.mouseDown(document.body);
      return markup;
    };
    caretIn("Centered.");
    expect(value()).toHaveAttribute("data-choice", "center");
    expect(value().innerHTML).toBe(itemIcon("Align center"));
    fireEvent.click(trigger);
    expect(within(popup("Alignment")!).getByRole("button", { name: "Align center" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    fireEvent.mouseDown(document.body);
    caretIn("Right.");
    expect(value()).toHaveAttribute("data-choice", "right");
    expect(value().innerHTML).toBe(itemIcon("Align right"));
    caretIn("Body.");
    expect(value()).toHaveAttribute("data-choice", "");
    expect(trigger).toHaveTextContent("Ax");
    fireEvent.click(trigger);
    expect(within(popup("Alignment")!).getByRole("button", { name: "Align left" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
  });

  it("EDT-FR-GELS: Vx shows the level of a selected spacer, and its item is pressed", async () => {
    await mount("<!-- %%% -->\nBody.\n");
    const trigger = blockMenuTrigger("Vertical break");
    caretIn("Body.");
    expect(trigger).toHaveTextContent("Vx");
    selectSpacer();
    expect(trigger).toHaveTextContent("V3");
    fireEvent.click(trigger);
    expect(
      within(popup("Vertical break")!).getByRole("button", { name: "Vertical break V3" }),
    ).toHaveAttribute("aria-pressed", "true");
  });
});

describe("markup edits (EDT-FR-RZTS, EDT-FR-UXCK, EDT-FR-MAVJ)", () => {
  it("EDT-FR-RZTS, EDT-FR-UXCK: a Vx choice writes the break, marks the file dirty, and is one undo step", async () => {
    const backend = await mount("Title\n\nBody.\n");
    const now = vi.spyOn(Date, "now").mockReturnValue(1_000_000);
    try {
      caretIn("Title");
      chooseToolbarItem("Heading", "Heading 1");
      chooseToolbarItem("Vertical break", "Vertical break V1");
    } finally {
      now.mockRestore();
    }
    expect(sessions.get("a.md")?.dirty).toBe(true);
    // EDT-FR-70: the edit schedules its own write.
    await letWriteLand();
    expect(backend.saves[backend.saves.length - 1]).toBe("<!-- % -->\n# Title\n\nBody.");

    pressUndo();
    expect(getMarkdown(tiptap())).toBe("# Title\n\nBody.");
    pressUndo();
    expect(getMarkdown(tiptap())).toBe("Title\n\nBody.");
  });

  it("EDT-FR-RZTS, EDT-FR-MAVJ: an Ax choice writes the marker and is one undo step", async () => {
    const backend = await mount("Body.\n");
    caretIn("Body.");
    const now = vi.spyOn(Date, "now").mockReturnValue(1_000_000);
    try {
      chooseToolbarItem("Alignment", "Align right");
      expect(sessions.get("a.md")?.dirty).toBe(true);
      chooseToolbarItem("Alignment", "Align left");
    } finally {
      now.mockRestore();
    }
    pressUndo();
    expect(getMarkdown(tiptap())).toBe("<!-- right -->\nBody.");
    pressUndo();
    expect(getMarkdown(tiptap())).toBe("Body.");
    chooseToolbarItem("Alignment", "Align right");
    await saveNow();
    expect(backend.saves[backend.saves.length - 1]).toBe("<!-- right -->\nBody.");
  });

  it("EDT-FR-RZTS, EDT-FR-UXCK: a Vx choice on a selected spacer resizes it as one undo step", async () => {
    const backend = await mount("<!-- %% -->\nBody.\n");
    selectSpacer();
    chooseToolbarItem("Vertical break", "Vertical break V4");
    expect(sessions.get("a.md")?.dirty).toBe(true);
    await saveNow();
    expect(backend.saves[backend.saves.length - 1]).toBe("<!-- %%%% -->\nBody.");
    pressUndo();
    expect(getMarkdown(tiptap())).toBe("<!-- %% -->\nBody.");
  });

  it("EDT-FR-RZTS, EDT-FR-FXAG: removing a chip is an ordinary edit that undo reverses", async () => {
    const backend = await mount("<!-- note -->\n\nBody.\n");
    const ed = tiptap();
    act(() => {
      ed.view.dispatch(ed.state.tr.setSelection(NodeSelection.create(ed.state.doc, 0)));
    });
    fireEvent.keyDown(pmEl(), { key: "Delete" });
    expect(pmEl().querySelector("[data-md-chip]")).toBeNull();
    expect(sessions.get("a.md")?.dirty).toBe(true);
    await saveNow();
    expect(backend.saves[backend.saves.length - 1]).toBe("Body.");
    pressUndo();
    expect(pmEl().querySelector("[data-md-chip]")).not.toBeNull();
  });

  it("EDT-FR-RZTS, EDT-FR-XKTS: removing a spacer is an ordinary edit that undo reverses", async () => {
    const backend = await mount("<!-- %% -->\nBody.\n");
    selectSpacer();
    fireEvent.keyDown(pmEl(), { key: "Backspace" });
    expect(pmEl().querySelector(".md-break")).toBeNull();
    await saveNow();
    expect(backend.saves[backend.saves.length - 1]).toBe("Body.");
    pressUndo();
    expect(pmEl().querySelector(".md-break")).not.toBeNull();
  });
});

describe("the file and the raw-text surface (EDT-FR-66, EDT-FR-THHF)", () => {
  const BODY = "<!--   %%   -->\n<!-- center -->\n# Title\n\nBody. <!-- note -->\n";

  it("EDT-FR-66, EDT-FR-THHF: rendering leaves the bytes, and raw text shows the markers as text", async () => {
    await mount(BODY);
    expect(pmEl().querySelector(".md-break")).not.toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Edit as Markdown source" }));
    const source = (await screen.findByLabelText("Markdown source")) as HTMLTextAreaElement;
    // The raw-text surface is a text area: the markers are its plain text.
    expect(source.value).toBe(BODY);
    expect(source.tagName).toBe("TEXTAREA");
    expect(sessions.get("a.md")?.dirty ?? false).toBe(false);
  });
});
