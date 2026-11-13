/**
 * The apply progress hook and the log record of its short form
 * (`../../../specifications/ui/OFN-openspec-failure-notices.md` OFN-FR-LKAT,
 * OFN-FR-QZEC; `../../../specifications/ui/OCW-openspec-change-workspace.md`
 * OCW-FR-BTSI).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));
const { logWarn } = vi.hoisted(() => ({ logWarn: vi.fn() }));
vi.mock("../../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn,
  logError: vi.fn(),
}));

import { useApplyProgress } from "./applyProgress";

/** What `read_apply_progress` answers next: a value, or a rejection. */
let answer: { value: unknown } | { reject: unknown };
let unknown: string[];

const PROGRESS = { state: "ready", total: 4, complete: 2, nextTask: "t", missingArtifacts: [] };
const noticeRecords = () => logWarn.mock.calls.filter((c) => c[1] === "an openspec failure notice appeared");

beforeEach(() => {
  unknown = [];
  answer = { value: PROGRESS };
  logWarn.mockReset();
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "append_log_records") return undefined;
    if (cmd === "read_apply_progress") {
      if ("reject" in answer) throw answer.reject;
      return answer.value;
    }
    unknown.push(cmd);
    throw new Error(`unexpected invoke ${cmd}`);
  });
});
afterEach(() => {
  cleanup();
  expect(unknown).toEqual([]);
});

function follow() {
  return renderHook(() =>
    useApplyProgress({
      streamId: "s-1",
      change: "add-logout",
      runId: null,
      enabled: true,
      keepOnFailure: true,
      surface: "test_tasks",
    }),
  );
}

/** Read again, and wait until the read has answered. */
async function rereadWith(hook: ReturnType<typeof follow>, next: typeof answer) {
  answer = next;
  const reads = invokeMock.mock.calls.filter((c) => c[0] === "read_apply_progress").length;
  act(() => hook.result.current.reread());
  await waitFor(() =>
    expect(invokeMock.mock.calls.filter((c) => c[0] === "read_apply_progress")).toHaveLength(reads + 1),
  );
  // The hook took its handlers on the promise of the call before this one,
  // so they have run when this await returns.
  const settled = invokeMock.mock.results.filter((_, i) => invokeMock.mock.calls[i][0] === "read_apply_progress");
  await act(async () => {
    await Promise.resolve(settled[settled.length - 1].value).catch(() => {});
  });
}

describe("the short form of a failed read (OFN-FR-LKAT)", () => {
  it("OFN-FR-LKAT, OFN-FR-QZEC: the same failure read twice is one notice and one record", async () => {
    answer = { reject: { code: "cli_timed_out", diagnostics: [] } };
    const hook = follow();
    await waitFor(() =>
      expect(hook.result.current.failure).toBe("The OpenSpec CLI did not finish in time, also after one more try."),
    );
    expect(noticeRecords()).toHaveLength(1);
    expect(noticeRecords()[0][2]).toEqual({ surface: "test_tasks", code: "cli_timed_out", cause: "", class: "transient" });
    await rereadWith(hook, { reject: { code: "cli_timed_out", diagnostics: [{ message: "again" }] } });
    expect(hook.result.current.failure).toContain("did not finish in time");
    expect(noticeRecords()).toHaveLength(1);
  });

  it("OFN-FR-LKAT: another failure is another notice, with its own record", async () => {
    answer = { reject: { code: "cli_timed_out", diagnostics: [] } };
    const hook = follow();
    await waitFor(() => expect(noticeRecords()).toHaveLength(1));
    await rereadWith(hook, { reject: { code: "cli_failed", diagnostics: [] } });
    expect(hook.result.current.failure).toContain("stopped before it answered");
    expect(noticeRecords()).toHaveLength(2);
    expect(noticeRecords()[1][2]).toMatchObject({ code: "cli_failed" });
  });

  it("OFN-FR-LKAT, OCW-FR-BTSI: a read that answers takes the notice away, so the same failure after it is a new notice", async () => {
    answer = { reject: { code: "cli_failed", diagnostics: [] } };
    const hook = follow();
    await waitFor(() => expect(noticeRecords()).toHaveLength(1));
    await rereadWith(hook, { value: PROGRESS });
    expect(hook.result.current.failure).toBeNull();
    expect(hook.result.current.progress).toEqual(PROGRESS);
    await rereadWith(hook, { reject: { code: "cli_failed", diagnostics: [] } });
    expect(noticeRecords()).toHaveLength(2);
    // OCW-FR-BTSI: a failed read keeps the last answer.
    expect(hook.result.current.progress).toEqual(PROGRESS);
  });

  it("OFN-FR-LKAT: a failure that is not an OpenSpec failure gets no short form and no notice record", async () => {
    answer = { reject: "no_open_project" };
    const hook = follow();
    await waitFor(() =>
      expect(logWarn.mock.calls.some((c) => c[1] === "the apply progress of a change could not be read")).toBe(true),
    );
    expect(hook.result.current.failure).toBeNull();
    expect(noticeRecords()).toHaveLength(0);
  });
});
