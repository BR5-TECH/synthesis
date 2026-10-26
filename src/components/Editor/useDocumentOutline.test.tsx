import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";
import { createRef } from "react";

vi.mock("mermaid", async () => (await import("../../test/mermaidMock")).mermaidModule);
vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

import { resetMermaidMock } from "../../test/mermaidMock";
import { resetMermaidForTest } from "../markdownMermaidRender";
import { destroyRichEditor, mountRichEditor } from "../../test/richEditorFixtures";
import type { EditMode } from "../../state/editHistory";
import { useDocumentOutline } from "./useDocumentOutline";

beforeEach(() => {
  resetMermaidMock();
  resetMermaidForTest();
});

afterEach(() => {
  destroyRichEditor();
});

function setup(mode: EditMode, sourceText = "# Source heading\n") {
  const editor = mountRichEditor("# Rich heading\n\ntext\n");
  const sourceRef = createRef<HTMLTextAreaElement>();
  const hook = renderHook(
    ({ m, text }: { m: EditMode; text: string }) =>
      useDocumentOutline({ editor, mode: m, sourceText: text, sourceRef }),
    { initialProps: { m: mode, text: sourceText } },
  );
  const listener = vi.fn();
  const unsubscribe = hook.result.current.subscribe(listener);
  return { editor, hook, listener, unsubscribe };
}

describe("useDocumentOutline (EDT-FR-KJLQ)", () => {
  it("EDT-FR-KJLQ: a change of the rich document notifies, and a change of selection alone does not", () => {
    const { editor, listener } = setup("wysiwyg");
    act(() => {
      editor.commands.setTextSelection(3);
    });
    expect(listener).not.toHaveBeenCalled();

    act(() => {
      editor.commands.insertContentAt(editor.state.doc.content.size, "<p>more</p>");
    });
    expect(listener).toHaveBeenCalled();
  });

  it("EDT-FR-KJLQ: the snapshot is the same list while nothing changes, and a new one after a change", () => {
    const { editor, hook } = setup("wysiwyg");
    const first = hook.result.current.getSnapshot();
    expect(first.map((e) => e.text)).toEqual(["Rich heading"]);
    expect(hook.result.current.getSnapshot()).toBe(first);

    act(() => {
      editor.commands.setTextSelection(3);
    });
    expect(hook.result.current.getSnapshot()).toBe(first);

    act(() => {
      editor.chain().setTextSelection(editor.state.doc.content.size - 1).toggleHeading({ level: 2 }).run();
    });
    const next = hook.result.current.getSnapshot();
    expect(next).not.toBe(first);
    expect(next.map((e) => `${e.level}:${e.text}`)).toEqual(["1:Rich heading", "2:text"]);
  });

  it("EDT-FR-KJLQ: a switch of mode and a new source text notify, and the list follows the surface", () => {
    const { hook, listener } = setup("wysiwyg");
    listener.mockClear();

    hook.rerender({ m: "text", text: "# Source heading\n" });
    expect(listener).toHaveBeenCalled();
    expect(hook.result.current.getSnapshot().map((e) => e.text)).toEqual(["Source heading"]);

    listener.mockClear();
    hook.rerender({ m: "text", text: "# Source heading\n## Second\n" });
    expect(listener).toHaveBeenCalled();
    expect(hook.result.current.getSnapshot().map((e) => e.text)).toEqual([
      "Source heading",
      "Second",
    ]);
  });

  it("EDT-FR-KJLQ: after unsubscribing, a change notifies nothing", () => {
    const { editor, listener, unsubscribe } = setup("wysiwyg");
    unsubscribe();
    act(() => {
      editor.commands.insertContentAt(editor.state.doc.content.size, "<p>more</p>");
    });
    expect(listener).not.toHaveBeenCalled();
  });
});
