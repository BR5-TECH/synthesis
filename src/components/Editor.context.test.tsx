/**
 * The context indicator in the Editor tab's discussion column, driven through
 * the real Editor (`../../specifications/ui/DDS-draft-discussion.md`
 * DDS-FR-CTXI, DDS-FR-HJMF).
 *
 * The indicator's own logic is covered against the shared column in
 * `DraftDiscussion.context.test.tsx`. What is checked here is the wiring of the
 * tab: that its column carries the indicator for the file, and that the
 * edit session's dirty flag reaches the column, so a save reads the estimate
 * again.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";

import { Editor } from "./Editor";
import { EditSessionStore } from "../state/editSessions";
import { resetLogBufferForTest } from "../logging";
import { resetProjectIdentity } from "../state/projectIdentity";
import { resetLayoutPreferencesCache } from "../state/layoutPreferences";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { resetTurnContextsForTest } from "../state/turnContexts";
import { BODY, openColumn, wireBackend } from "../test/editorCommentsFixtures";
import type { Backend } from "../test/editorCommentsFixtures";
import type { DiscussionContext } from "../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

const ESTIMATE: DiscussionContext = {
  window: 200_000,
  estimatedTokens: 50_000,
  split: { instructions: 1, document: 1, history: 1, currentComment: 1, toolResults: 1 },
  document: { lines: 3, characters: 60, estimatedTokens: 15 },
  limitTokens: 60_000,
  aboveLimit: false,
};

const estimateCalls = () =>
  invokeMock.mock.calls.filter((c) => c[0] === "estimate_discussion_context");

async function mount(sessions = new EditSessionStore()) {
  const backend: Backend = { load: { body: BODY, checksum: "ck1" }, threads: [], calls: [] };
  wireBackend(invokeMock, backend);
  const base = invokeMock.getMockImplementation()!;
  invokeMock.mockImplementation(async (cmd: string, args: unknown) =>
    cmd === "estimate_discussion_context" ? ESTIMATE : base(cmd, args),
  );
  render(
    <Editor artifactId="a.md" artifactName="a.md" artifactType="skill" sessions={sessions} />,
  );
  await screen.findByLabelText("artifact body");
  return sessions;
}

beforeEach(() => {
  invokeMock.mockReset();
  resetProjectIdentity();
  resetLogBufferForTest();
  resetLayoutPreferencesCache();
  resetTurnContextsForTest();
  clearAllDiscussionSessions();
});

afterEach(cleanup);

describe("the context indicator in the Editor tab", () => {
  it("DDS-FR-CTXI: the Editor tab's column carries the indicator, estimated for the file", async () => {
    await mount();
    const column = await openColumn();
    expect(await within(column).findByTestId("context-indicator")).toHaveTextContent("~25%");
    expect(estimateCalls()[0][1]).toMatchObject({
      target: { kind: "artifact", artifactId: "a.md" },
    });
  });

  it("DDS-FR-HJMF: a save of the file reads the estimate again", async () => {
    const sessions = await mount();
    const column = await openColumn();
    await within(column).findByTestId("context-indicator");
    const before = estimateCalls().length;
    act(() => sessions.update("a.md", { dirty: true }));
    expect(estimateCalls()).toHaveLength(before);
    act(() => sessions.update("a.md", { dirty: false }));
    await waitFor(() => expect(estimateCalls()).toHaveLength(before + 1));
  });
});
