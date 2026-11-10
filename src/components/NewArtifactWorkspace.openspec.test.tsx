/**
 * The draft tab of a propose run's draft
 * (`../../specifications/ui/NAW-new-artifact.md` NAW-FR-XDDQ).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, screen } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import { draft, makeStubs, renderWorkspace } from "../test/newArtifactFixtures";
import { resetLogBufferForTest } from "../logging";
import { resetDraftDiscussions } from "../state/draftDiscussion";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { resetDiscussionFocus } from "../state/discussionFocus";
import { configureOpenSpecChangeRoutes } from "../state/openspec/changeRoutes";
import type { GraduationRunState } from "../types";

const { stub } = makeStubs(invokeMock);
const open = vi.fn();

/** The base stub, with the run of the draft answering as `run`. */
function stubRun(run: { state: GraduationRunState; commits: string[]; openspec: unknown }) {
  stub();
  const base = invokeMock.getMockImplementation()!;
  invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
    if (cmd === "append_log_records") return undefined;
    if (cmd === "open_draft") return { ...draft(run.state === "completed" ? "graduated" : "active") };
    if (cmd === "get_draft_graduation")
      return { id: "g-7", streamId: "s-7", streamName: "auth-work", ...run };
    return base(cmd, args);
  });
}

beforeEach(() => {
  resetLogBufferForTest();
  invokeMock.mockReset();
  resetDraftDiscussions();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  open.mockReset();
  configureOpenSpecChangeRoutes({ open });
});
afterEach(() => {
  configureOpenSpecChangeRoutes(null);
  cleanup();
});

const PROPOSE = { phase: "propose", change: "add-logout-3f2a" };

describe("New Artifact workspace — a propose run's draft", () => {
  it("NAW-FR-XDDQ: a graduated draft of a completed propose run carries Open change beside its run-state tag", async () => {
    stubRun({ state: "completed", commits: ["c1"], openspec: PROPOSE });
    renderWorkspace();
    const action = await screen.findByTestId("draft-open-change");
    expect(action).toHaveTextContent("Open change");
    expect(screen.getByTestId("draft-graduation-state").nextElementSibling).toBe(action);
    await act(async () => action.click());
    expect(open).toHaveBeenCalledWith({ kind: "stream", streamId: "s-7" }, "add-logout-3f2a", {});
  });

  it("NAW-FR-XDDQ: a propose run that is not completed carries no Open change", async () => {
    stubRun({ state: "working", commits: [], openspec: PROPOSE });
    renderWorkspace();
    await screen.findByTestId("draft-graduation-state");
    expect(screen.queryByTestId("draft-open-change")).toBeNull();
  });

  it("NAW-FR-XDDQ: a completed draft run that is no propose run carries no Open change", async () => {
    stubRun({ state: "completed", commits: ["c1"], openspec: null });
    renderWorkspace();
    await screen.findByTestId("draft-graduation-state");
    expect(screen.queryByTestId("draft-open-change")).toBeNull();
  });
});
