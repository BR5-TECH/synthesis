/**
 * The page width ruler in the Editor tab (`EPW-editor-page-width.md`
 * EPW-FR-ADPJ, EPW-FR-XOMR; `EZM-editor-zoom.md` EZM-FR-UJWE).
 *
 * The real Editor is mounted on a strict backend.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";

import { Editor } from "./Editor";
import { EditSessionStore } from "../state/editSessions";
import { resetLogBufferForTest } from "../logging";
import {
  PAGE_WIDTH_PROPERTY,
  previewEditorPageWidth,
  resetEditorPageWidthForTest,
} from "../state/editorPageWidth";
import { BODY, wireBackend } from "../test/editorCommentsFixtures";
import type { Backend } from "../test/editorCommentsFixtures";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));

beforeEach(() => {
  resetLogBufferForTest();
  resetEditorPageWidthForTest();
  invokeMock.mockReset();
});
afterEach(() => {
  cleanup();
  resetEditorPageWidthForTest();
});

async function mount(artifactId: string) {
  const sessions = new EditSessionStore();
  const backend: Backend = { load: { body: BODY, checksum: "ck1" }, threads: [], calls: [] };
  wireBackend(invokeMock, backend);
  render(
    <Editor artifactId={artifactId} artifactName={artifactId} sessions={sessions} />,
  );
  await screen.findByLabelText(artifactId.endsWith(".md") ? "artifact body" : "Source");
  return sessions;
}

/** The ruler stands at the foot of the column, out of the zoom region and the page. */
function expectRulerAtTheFoot() {
  const ruler = screen.getByTestId("page-width-ruler");
  expect(ruler.closest(".editor-zoom")).toBeNull();
  expect(ruler.closest("[data-zoom-scroller]")).toBeNull();
  expect(ruler.closest(".editor__document")).toBeNull();
  const region = screen.getByTestId("editor-zoom-region");
  expect(region.nextElementSibling).toBe(ruler);
  return ruler;
}

describe("page width ruler in the Editor tab", () => {
  it("EPW-FR-ADPJ, EZM-FR-UJWE: a Markdown file shows the ruler below its page on both surfaces", async () => {
    await mount("a.md");
    const ruler = expectRulerAtTheFoot();
    fireEvent.click(screen.getByRole("button", { name: "Edit as Markdown source" }));
    await screen.findByLabelText("Markdown source");
    expect(expectRulerAtTheFoot()).toBe(ruler);
  });

  it("EPW-FR-ADPJ: a source file shows the ruler below its page", async () => {
    await mount("src/main.rs");
    expectRulerAtTheFoot();
  });

  it("EPW-FR-XOMR: a change of the width remounts no page and does not make the file dirty", async () => {
    const sessions = await mount("a.md");
    const page = document.querySelector(".editor")!;
    const body = screen.getByLabelText("artifact body");
    const before = sessions.get("a.md");
    expect(before).toBeDefined();
    const buffer = before!.buffer;
    const mode = before!.mode;
    act(() => previewEditorPageWidth(1400));
    expect(document.documentElement.style.getPropertyValue(PAGE_WIDTH_PROPERTY)).toBe("1400px");
    expect(document.querySelector(".editor")).toBe(page);
    expect(screen.getByLabelText("artifact body")).toBe(body);
    expect(screen.getByTestId("page-ruler-handle-left")).toHaveAttribute("aria-valuenow", "1400");
    const after = sessions.get("a.md")!;
    expect(after.dirty).toBe(false);
    expect(after.buffer).toBe(buffer);
    expect(after.mode).toBe(mode);
    expect(screen.getByRole("button", { name: "Edit as Markdown source" })).toBeInTheDocument();
    await act(async () => {
      await new Promise<void>((res) => setTimeout(res, 0));
    });
    expect(invokeMock.mock.calls.some((c) => c[0] === "save_artifact_contents")).toBe(false);
  });
});
