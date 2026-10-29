/**
 * The context indicator in the New Artifact tab's discussion column, driven
 * through the real workspace (`../../specifications/ui/DDS-draft-discussion.md`
 * DDS-FR-CTXI, DDS-FR-HJMF).
 *
 * The indicator's own logic is covered against the shared column in
 * `DraftDiscussion.context.test.tsx`. What is checked here is the wiring of the
 * tab: that its column carries the indicator for the draft, and that the
 * prompt's dirty flag reaches the column, so a save reads the estimate again.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, screen, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import { PROMPT, makeStubs, renderWorkspace } from "../test/newArtifactFixtures";
import { DraftSessionStore } from "../state/draftSessions";
import { hunkCandidateBuffers } from "../state/candidateBuffers";
import { resetDraftDiscussions } from "../state/draftDiscussion";
import { resetLayoutPreferencesCache } from "../state/layoutPreferences";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { resetDiscussionFocus } from "../state/discussionFocus";
import { resetTurnContextsForTest } from "../state/turnContexts";
import { resetLogBufferForTest } from "../logging";
import type { DiscussionContext } from "../types";

const { stub } = makeStubs(invokeMock);

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

beforeEach(() => {
  invokeMock.mockReset();
  hunkCandidateBuffers.clear();
  resetDraftDiscussions();
  resetLayoutPreferencesCache();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  resetTurnContextsForTest();
  resetLogBufferForTest();
  stub();
  const base = invokeMock.getMockImplementation()!;
  invokeMock.mockImplementation(async (cmd: string, args?: unknown) =>
    cmd === "estimate_discussion_context" ? ESTIMATE : base(cmd, args),
  );
});

afterEach(cleanup);

describe("the context indicator in the New Artifact tab", () => {
  it("DDS-FR-CTXI: the New Artifact tab's column carries the indicator, estimated for the draft", async () => {
    renderWorkspace();
    expect(await screen.findByTestId("context-indicator")).toHaveTextContent("~25%");
    expect(estimateCalls()[0][1]).toMatchObject({ target: { kind: "draft", draftId: "d1" } });
  });

  it("DDS-FR-HJMF: a save of the draft's prompt reads the estimate again", async () => {
    const drafts = new DraftSessionStore();
    renderWorkspace(drafts);
    await screen.findByTestId("context-indicator");
    const key = drafts.key("d1", PROMPT);
    await waitFor(() => expect(drafts.docs.get(key)).toBeDefined());
    const before = estimateCalls().length;
    act(() => drafts.docs.update(key, { dirty: true }));
    expect(estimateCalls()).toHaveLength(before);
    act(() => drafts.docs.update(key, { dirty: false }));
    await waitFor(() => expect(estimateCalls()).toHaveLength(before + 1));
  });
});
