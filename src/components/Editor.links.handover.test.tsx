// The hand-over of a link's fragment from the shell's opener to the Editor of
// the file it opened (`EDT-editor.md` EDT-FR-GEHE, EDT-FR-YXUD).
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { useState } from "react";
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
import { LinkOpenerContext } from "./linkFollow";
import { useViewportLinkOpener } from "../hooks/shell/useFileLinkOpener";
import type { OpenableArtifact } from "../types";

const FILES: Record<string, string> = {
  "docs/a.md": "# Start\n\n[guide](guide.md#details) [spec](spec.md#scope)\n",
  "docs/guide.md": "# Guide\n\nIntro.\n\n## Details\n\nEnd.\n",
  "docs/spec.md": "# Spec\n\n## Scope\n\nText.\n",
};

let sessions: EditSessionStore;
let show: (path: string | null) => void = () => {};
let scrollIntoView: ReturnType<typeof vi.fn>;
const original = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "scrollIntoView");

/**
 * The viewport in small: one Editor for the active file. `docs/spec.md` goes
 * to a surface that is not an Editor, as a Spec file goes to the Map tab.
 */
function Shell() {
  const [active, setActive] = useState<string | null>("docs/a.md");
  show = setActive;
  const open = (item: OpenableArtifact) => setActive(item.id === "docs/spec.md" ? null : item.id!);
  const opener = useViewportLinkOpener(["/p"], open, active, active ?? "map");
  return (
    <LinkOpenerContext.Provider value={opener}>
      {active ? (
        <Editor key={active} artifactId={active} artifactName={active} sessions={sessions} />
      ) : (
        <p>map</p>
      )}
    </LinkOpenerContext.Provider>
  );
}

const pmEl = () => document.querySelector(".ProseMirror") as HTMLElement;
const tiptap = () => (pmEl() as unknown as { editor: TiptapEditor }).editor;
const ctrlClick = (text: string) =>
  fireEvent.click(
    Array.from(pmEl().querySelectorAll("a")).find((a) => a.textContent === text)!,
    { ctrlKey: true, button: 0 },
  );

beforeEach(() => {
  invokeMock.mockReset();
  resetLogBufferForTest();
  resetMermaidForTest();
  resetMermaidMock();
  sessions = new EditSessionStore();
  scrollIntoView = vi.fn();
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
    configurable: true,
    writable: true,
    value: scrollIntoView,
  });
  invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
    const id = (args as { id?: string } | undefined)?.id ?? "";
    if (cmd === "append_log_records") return undefined;
    if (cmd === "list_comment_threads") return [];
    if (cmd === "open_artifact_by_id") {
      return id in FILES ? { key: id, kind: "file" } : Promise.reject(new Error("not a file"));
    }
    if (cmd === "load_artifact_contents_by_id") return { body: FILES[id], checksum: "ck" };
    return Promise.reject(new Error(`unexpected invoke ${cmd}`));
  });
});

afterEach(() => {
  cleanup();
  if (original) Object.defineProperty(HTMLElement.prototype, "scrollIntoView", original);
  else delete (HTMLElement.prototype as unknown as Record<string, unknown>).scrollIntoView;
});

describe("the hand-over of a fragment", () => {
  it("EDT-FR-GEHE, EDT-FR-YXUD: the link opens the file, and its tab jumps to the heading after the load", async () => {
    render(<Shell />);
    await screen.findByRole("heading", { name: "Start" });
    ctrlClick("guide");
    await screen.findByRole("heading", { name: "Guide" });
    await waitFor(() => expect(tiptap().state.selection.$from.parent.textContent).toBe("Details"));
    expect(scrollIntoView.mock.contexts.map((el) => (el as Element).textContent)).toEqual(["Details"]);
  });

  it("EDT-FR-YXUD: a file whose session is already loaded jumps too", async () => {
    render(<Shell />);
    await screen.findByRole("heading", { name: "Start" });
    act(() => show("docs/guide.md"));
    await screen.findByRole("heading", { name: "Guide" });
    act(() => show("docs/a.md"));
    await screen.findByRole("heading", { name: "Start" });
    ctrlClick("guide");
    await screen.findByRole("heading", { name: "Guide" });
    await waitFor(() => expect(tiptap().state.selection.$from.parent.textContent).toBe("Details"));
  });

  it("EDT-FR-YXUD: a file that opened in no Editor takes no jump when it opens later", async () => {
    render(<Shell />);
    await screen.findByRole("heading", { name: "Start" });
    ctrlClick("spec");
    await screen.findByText("map");
    act(() => show("docs/spec.md"));
    await screen.findByRole("heading", { name: "Spec" });
    await act(async () => {});
    expect(tiptap().state.selection.$from.parent.textContent).not.toBe("Scope");
    expect(scrollIntoView).not.toHaveBeenCalled();
  });
});
