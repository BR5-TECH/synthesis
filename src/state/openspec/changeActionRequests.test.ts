/**
 * The store of the change action requests of each change
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-QCPB,
 * `../../../specifications/core/CAR-change-action-requests.md` CAR-FR-ALHA).
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { resetLogBufferForTest } from "../../logging";
import type { ChangeActionRequest } from "../../types";
import {
  changeActionRequestOf,
  changeActionRequestsRead,
  noteChangeActionRequest,
  rereadChangeActionRequests,
  resetChangeActionRequests,
} from "./changeActionRequests";

const STREAM = "s-1";
const CHANGE = "add-auth";

function request(state: ChangeActionRequest["state"]): ChangeActionRequest {
  return {
    id: "r1",
    streamId: STREAM,
    change: CHANGE,
    action: "apply",
    rationale: "Ready. <spoken>Ready.</spoken>",
    threadId: "t1",
    commentId: "c1",
    agent: { kind: "agent", agentId: "a1", handle: "arch" },
    state,
    createdAt: "2026-01-01T00:00:00Z",
    ...(state === "started" ? { runId: "g-9", decidedAt: "2026-01-01T00:01:00Z" } : {}),
  } as ChangeActionRequest;
}

/** Each list call waits until the test answers it. */
let answers: Array<(list: ChangeActionRequest[]) => void>;

beforeEach(() => {
  resetLogBufferForTest();
  resetChangeActionRequests();
  answers = [];
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "append_log_records") return Promise.resolve(null);
    if (cmd === "list_change_action_requests") {
      return new Promise((resolve) => answers.push(resolve));
    }
    return Promise.reject(new Error(`unexpected invoke ${cmd}`));
  });
});

describe("change action requests store", () => {
  it("OCW-FR-QCPB / CAR-FR-ALHA: a read asked before a decision does not put back the old state", async () => {
    const first = rereadChangeActionRequests(STREAM, CHANGE);
    noteChangeActionRequest(request("started"));
    expect(changeActionRequestOf(STREAM, CHANGE, "r1")?.state).toBe("started");
    expect(answers).toHaveLength(2);
    answers[0]([request("pending")]);
    await first;
    expect(changeActionRequestOf(STREAM, CHANGE, "r1")?.state).toBe("started");
    answers[1]([request("started")]);
    await vi.waitFor(() => expect(changeActionRequestsRead(STREAM, CHANGE)).toBe(true));
    expect(changeActionRequestOf(STREAM, CHANGE, "r1")?.runId).toBe("g-9");
  });

  it("OCW-FR-QCPB: a decision with no read in flight asks for no read", () => {
    noteChangeActionRequest(request("started"));
    expect(answers).toHaveLength(0);
    expect(changeActionRequestOf(STREAM, CHANGE, "r1")?.state).toBe("started");
  });
});
