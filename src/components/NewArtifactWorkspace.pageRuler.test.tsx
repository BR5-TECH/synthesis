/**
 * The page width ruler in the New Artifact tab (`EPW-editor-page-width.md`
 * EPW-FR-ADPJ).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, screen } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));

import { makeStubs, renderWorkspace } from "../test/newArtifactFixtures";
import { resetDraftDiscussions } from "../state/draftDiscussion";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { resetDiscussionFocus } from "../state/discussionFocus";
import { resetToasts } from "../state/toasts";
import { resetLogBufferForTest } from "../logging";
import { resetEditorPageWidthForTest } from "../state/editorPageWidth";

const { stub } = makeStubs(invokeMock);

beforeEach(() => {
  invokeMock.mockReset();
  resetDraftDiscussions();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  resetToasts();
  resetLogBufferForTest();
  resetEditorPageWidthForTest();
});
afterEach(() => {
  cleanup();
  resetEditorPageWidthForTest();
});

describe("New Artifact workspace — page width ruler", () => {
  it("EPW-FR-ADPJ: the draft's document page has the ruler below it, out of the zoom region", async () => {
    stub();
    renderWorkspace();
    const ruler = await screen.findByTestId("page-width-ruler");
    expect(ruler.closest(".draft-editor__surface")).not.toBeNull();
    expect(ruler.closest(".editor-zoom")).toBeNull();
    expect(ruler.closest("[data-zoom-scroller]")).toBeNull();
    expect(screen.getByTestId("page-ruler-handle-left")).toHaveAttribute("aria-valuenow", "820");
  });
});
