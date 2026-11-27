// The links of an Editor tab (`EDT-editor.md` EDT-FR-GEHE, EDT-FR-YXUD,
// EDT-FR-HYVP, EDT-FR-QBIE; `NAW-new-artifact.md` NAW-FR-RUPR).
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import type { Editor as TiptapEditor } from "@tiptap/react";

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
import { LinkOpenerContext, type LinkOpener, type LinkPlace } from "./linkFollow";
import { resetToasts, visibleToasts } from "../state/toasts";

const PATH = "docs/guide.md";
const BODY = [
  "# Intro",
  "",
  "See [other](other.md#Setup), [self](guide.md#details), and [root](/README.md).",
  "",
  "Text.",
  "",
  "## Details",
  "",
  "End.",
  "",
].join("\n");

function wireBackend(body = BODY) {
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "load_artifact_contents_by_id") return { body, checksum: "ck1" };
    if (cmd === "save_artifact_contents") return { checksum: "ck2" };
    if (cmd === "list_comment_threads") return [];
    if (cmd === "append_log_records") return undefined;
    return Promise.reject(new Error(`unexpected invoke ${cmd}`));
  });
}

let sessions: EditSessionStore;
let scrollIntoView: ReturnType<typeof vi.fn>;
let scrollTo: ReturnType<typeof vi.fn>;
const original = {
  scrollIntoView: Object.getOwnPropertyDescriptor(HTMLElement.prototype, "scrollIntoView"),
  scrollTo: Object.getOwnPropertyDescriptor(HTMLElement.prototype, "scrollTo"),
};
function restore(name: keyof typeof original) {
  const descriptor = original[name];
  if (descriptor) Object.defineProperty(HTMLElement.prototype, name, descriptor);
  else delete (HTMLElement.prototype as unknown as Record<string, unknown>)[name];
}

const pmEl = () => document.querySelector(".ProseMirror") as HTMLElement;
const tiptap = () => (pmEl() as unknown as { editor: TiptapEditor }).editor;
const link = (text: string) =>
  Array.from(pmEl().querySelectorAll("a")).find((a) => a.textContent === text)!;
const ctrlClick = (text: string) =>
  fireEvent.click(link(text), { ctrlKey: true, button: 0 });

function opener(pending: Record<string, string> = {}): LinkOpener {
  const waiting = { ...pending };
  return {
    roots: ["/Users/me/project"],
    openFile: vi.fn(),
    activeFileChanged: vi.fn(),
    takePendingFragment: vi.fn((path: string) => {
      const fragment = waiting[path] ?? null;
      delete waiting[path];
      return fragment;
    }),
  };
}

function editorFor(linkOpener: LinkOpener | null, place?: LinkPlace | null, artifactId = PATH) {
  const editor = (
    <Editor
      artifactId={artifactId}
      artifactName={artifactId}
      sessions={sessions}
      toc
      linkPlace={place}
    />
  );
  return linkOpener ? (
    <LinkOpenerContext.Provider value={linkOpener}>{editor}</LinkOpenerContext.Provider>
  ) : (
    editor
  );
}

async function mount(linkOpener: LinkOpener | null, place?: LinkPlace | null, artifactId = PATH) {
  const view = render(editorFor(linkOpener, place, artifactId));
  await screen.findByRole("heading", { name: "Intro" });
  return view;
}

beforeEach(() => {
  invokeMock.mockReset();
  resetLogBufferForTest();
  resetMermaidForTest();
  resetMermaidMock();
  resetToasts();
  sessions = new EditSessionStore();
  scrollIntoView = vi.fn();
  scrollTo = vi.fn();
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
    configurable: true,
    writable: true,
    value: scrollIntoView,
  });
  Object.defineProperty(HTMLElement.prototype, "scrollTo", {
    configurable: true,
    writable: true,
    value: scrollTo,
  });
});

afterEach(() => {
  cleanup();
  restore("scrollIntoView");
  restore("scrollTo");
});

describe("an Editor tab on a project file", () => {
  it("EDT-FR-QBIE, EDT-FR-RFCU, EDT-FR-GEHE: a file link resolves against the file and opens through the shell", async () => {
    wireBackend();
    const shell = opener();
    await mount(shell);
    ctrlClick("other");
    ctrlClick("root");
    expect(shell.openFile).toHaveBeenNthCalledWith(1, "docs/other.md", "Setup");
    expect(shell.openFile).toHaveBeenNthCalledWith(2, "README.md", null);
  });

  it("EDT-FR-GEHE, EDT-FR-ZGKO: a link to its own file jumps in place and leaves the file unchanged", async () => {
    wireBackend();
    const shell = opener();
    await mount(shell);
    ctrlClick("self");
    expect(shell.openFile).not.toHaveBeenCalled();
    expect(tiptap().state.selection.$from.parent.textContent).toBe("Details");
    expect(scrollIntoView).toHaveBeenCalled();
    expect(sessions.get(PATH)?.dirty).toBe(false);
  });

  it("EDT-FR-QBIE: with no shell opener, a file link is refused and opens nothing", async () => {
    wireBackend();
    await mount(null);
    ctrlClick("other");
    expect(visibleToasts()[0]).toMatchObject({ key: "link-refused" });
    expect(invokeMock).not.toHaveBeenCalledWith("open_artifact_by_id", expect.anything());
  });
});

describe("a fragment that waits for the tab (EDT-FR-YXUD)", () => {
  it("EDT-FR-YXUD: the rich surface jumps to the heading after the file loads, one time", async () => {
    wireBackend();
    const shell = opener({ [PATH]: "details" });
    await mount(shell);
    await waitFor(() => expect(tiptap().state.selection.$from.parent.textContent).toBe("Details"));
    expect(shell.takePendingFragment).toHaveBeenCalledWith(PATH);
    expect(scrollIntoView.mock.contexts.map((el) => (el as Element).textContent)).toEqual(["Details"]);
    expect(sessions.get(PATH)?.dirty).toBe(false);
  });

  it("EDT-FR-YXUD: in raw-text mode the jump goes to the source line of the heading", async () => {
    wireBackend();
    const first = await mount(opener());
    fireEvent.click(screen.getByRole("button", { name: "Edit as Markdown source" }));
    await screen.findByLabelText("Markdown source");
    first.unmount();
    // The tab opens again in the mode its record keeps.
    render(editorFor(opener({ [PATH]: "Details" })));
    const source = (await screen.findByLabelText("Markdown source")) as HTMLTextAreaElement;
    await waitFor(() => expect(source.selectionStart).toBe(BODY.indexOf("## Details")));
    expect(scrollTo).toHaveBeenCalled();
  });

  it("EDT-FR-HYVP, NTF-FR-WQRN: a waiting fragment with no heading shows the toast and does not move", async () => {
    wireBackend();
    await mount(opener({ [PATH]: "nowhere" }));
    await waitFor(() =>
      expect(visibleToasts()).toEqual([
        expect.objectContaining({ key: "link-target-missing", title: "Link target not found" }),
      ]),
    );
    expect(scrollIntoView).not.toHaveBeenCalled();
  });

  it("EDT-FR-YXUD: a fragment that waits for another file is left for that file", async () => {
    wireBackend();
    const shell = opener({ "docs/other.md": "details" });
    await mount(shell);
    await act(async () => {});
    expect(scrollIntoView).not.toHaveBeenCalled();
    expect(shell.takePendingFragment).not.toHaveBeenCalledWith("docs/other.md");
    expect(tiptap().state.selection.$from.parent.textContent).not.toBe("Details");
  });
});

describe("the live prompt of a New Artifact tab (NAW-FR-RUPR)", () => {
  const draftBody =
    "# Prompt\n\n[spec](docs/x.md) [rooted](/docs/x.md#a) [me](Untitled.md#later) [up](../y.md)\n\n## Later\n";

  it("NAW-FR-RUPR: a file link resolves against the project root, and the prompt's own name stays in the prompt", async () => {
    wireBackend(draftBody);
    const shell = opener();
    render(editorFor(shell, { fileDir: "", selfPath: "Untitled.md" }, "draft-1/Untitled.md"));
    await screen.findByRole("heading", { name: "Prompt" });
    ctrlClick("spec");
    expect(shell.openFile).toHaveBeenCalledWith("docs/x.md", null);
    ctrlClick("rooted");
    expect(shell.openFile).toHaveBeenLastCalledWith("docs/x.md", "a");
    ctrlClick("me");
    expect(shell.openFile).toHaveBeenCalledTimes(2);
    expect(tiptap().state.selection.$from.parent.textContent).toBe("Later");
    ctrlClick("up");
    expect(visibleToasts()[0]).toMatchObject({
      key: "link-refused",
      body: "The link goes outside the project.",
    });
  });
});
