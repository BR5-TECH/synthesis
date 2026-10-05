import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";

import { Editor } from "./Editor";
import { EditSessionStore } from "../state/editSessions";
import { hasEdits } from "../state/editHistory";
import type { ArtifactContents } from "../types";

// EDT-FR-38 — Tab in the raw-text surface inserts the artifact's
// indentation convention. Entirely client-side: no backend operation is
// involved, and choosing a convention rewrites no existing line.

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

function wireLoad(load: ArtifactContents) {
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "load_artifact_contents_by_id") return load;
    if (cmd === "save_artifact_contents") return { checksum: "ck-saved" };
    throw new Error(`unexpected invoke ${cmd}`);
  });
}

beforeEach(() => {
  invokeMock.mockReset();
});

/**
 * Resolve after the next animation frame.
 *
 * The caret restore is deliberately deferred to a frame, because React
 * re-renders the textarea from `sourceText` and that reset happens *after* the
 * keydown handler returns. Stubbing rAF to run inline would move the restore
 * back before that reset and make the assertion pass against an implementation
 * that does not work — so the real one is awaited instead.
 */
function nextFrame() {
  return new Promise<void>((resolve) =>
    requestAnimationFrame(() => resolve()),
  );
}

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

/** Mount the Editor on `body` and switch it to the raw-text surface. */
async function openSource(sessions: EditSessionStore, body: string) {
  wireLoad({ body, checksum: "ck1" });
  render(
    <Editor
      artifactId="a.md"
      artifactName="a.md"
      sessions={sessions}
    />,
  );
  await screen.findByLabelText("artifact body");
  fireEvent.click(
    screen.getByRole("button", { name: "Edit as Markdown source" }),
  );
  return screen.getByLabelText("Markdown source") as HTMLTextAreaElement;
}

describe("Tab in the raw-text surface (EDT-FR-38)", () => {
  it("inserts the detected space unit at the caret", async () => {
    const sessions = new EditSessionStore();
    const source = await openSource(sessions, "- one\n  - nested\n");
    expect(sessions.get("a.md")?.indentation).toEqual({
      kind: "spaces",
      width: 2,
    });

    source.setSelectionRange(0, 0);
    fireEvent.keyDown(source, { key: "Tab" });

    expect(sessions.get("a.md")?.buffer).toBe("  - one\n  - nested\n");
  });

  it("leaves the caret just after the inserted unit", async () => {
    // React re-renders the textarea from `sourceText`, which resets the caret to
    // the end of the document. Without the explicit restore, every Tab would
    // fling the cursor to the bottom of the file — green tests, unusable editor.
    const sessions = new EditSessionStore();
    const source = await openSource(sessions, "- one\n  - nested\n");

    source.setSelectionRange(2, 2);
    await act(async () => {
      fireEvent.keyDown(source, { key: "Tab" });
      await nextFrame();
    });

    expect(sessions.get("a.md")?.buffer).toBe("-   one\n  - nested\n");
    expect(source.selectionStart).toBe(4);
    expect(source.selectionEnd).toBe(4);
  });

  it("puts the caret after a tab unit too, not after a space run", async () => {
    const sessions = new EditSessionStore();
    const source = await openSource(sessions, "abc\n");
    sessions.setIndentation("a.md", { kind: "tabs" });

    source.setSelectionRange(1, 1);
    await act(async () => {
      fireEvent.keyDown(source, { key: "Tab" });
      await nextFrame();
    });

    expect(sessions.get("a.md")?.buffer).toBe("a\tbc\n");
    expect(source.selectionStart).toBe(2);
  });

  it("inserts a tab character once the convention is overridden to tabs", async () => {
    // EDT-FR-38: the previously space-indented lines are left exactly as they
    // were, and the selection itself raised no dirty state.
    const sessions = new EditSessionStore();
    const source = await openSource(sessions, "- one\n  - nested\n");

    sessions.setIndentation("a.md", { kind: "tabs" });
    expect(sessions.get("a.md")?.dirty).toBe(false);
    expect(hasEdits(sessions.get("a.md")!.history)).toBe(false);

    source.setSelectionRange(0, 0);
    fireEvent.keyDown(source, { key: "Tab" });

    const buffer = sessions.get("a.md")!.buffer;
    expect(buffer).toBe("\t- one\n  - nested\n");
    // Every previously indented line is unchanged — only the caret's line grew.
    expect(buffer.split("\n")[1]).toBe("  - nested");
  });

  it("makes the insertion one undoable step, and the convention change none", async () => {
    // EDT-FR-38's last clause: one undo reverses only the inserted whitespace,
    // not the convention change (EDT-FR-23).
    const sessions = new EditSessionStore();
    const source = await openSource(sessions, "- one\n  - nested\n");
    sessions.setIndentation("a.md", { kind: "tabs" });

    source.setSelectionRange(0, 0);
    fireEvent.keyDown(source, { key: "Tab" });

    const session = sessions.get("a.md")!;
    expect(hasEdits(session.history)).toBe(true);
    expect(session.dirty).toBe(true);

    fireEvent.keyDown(source, { key: "z", metaKey: true });
    expect(sessions.get("a.md")?.buffer).toBe("- one\n  - nested\n");
  });

  it("replaces the selection rather than appending to it", async () => {
    const sessions = new EditSessionStore();
    const source = await openSource(sessions, "abc\n");
    sessions.setIndentation("a.md", { kind: "tabs" });

    source.setSelectionRange(0, 2);
    fireEvent.keyDown(source, { key: "Tab" });

    expect(sessions.get("a.md")?.buffer).toBe("\tc\n");
  });

  it("leaves Shift+Tab and the modifier combinations to the browser", async () => {
    // Shift+Tab is a backwards focus move, and de-indentation is not something
    // EDT-FR-38 asks for; ⌘/Ctrl/Alt+Tab belong to the OS.
    const sessions = new EditSessionStore();
    const source = await openSource(sessions, "abc\n");
    source.setSelectionRange(0, 0);

    for (const modifier of [
      { shiftKey: true },
      { metaKey: true },
      { ctrlKey: true },
      { altKey: true },
    ]) {
      fireEvent.keyDown(source, { key: "Tab", ...modifier });
    }
    expect(sessions.get("a.md")?.buffer).toBe("abc\n");
  });

  it("writes no bytes of its own — nothing is saved by a Tab", async () => {
    const sessions = new EditSessionStore();
    const source = await openSource(sessions, "abc\n");
    source.setSelectionRange(0, 0);
    fireEvent.keyDown(source, { key: "Tab" });

    expect(
      invokeMock.mock.calls.map((c) => c[0]),
    ).not.toContain("save_artifact_contents");
  });
});
