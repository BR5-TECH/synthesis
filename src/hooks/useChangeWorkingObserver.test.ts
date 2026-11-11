/**
 * The working-activity observer of OpenSpec changes
 * (`../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-MIEY, on
 * the terms of `DFI-draft-information.md` DFI-FR-XKRM, DFI-FR-UKFR, and
 * DFI-FR-JDWS).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { IDLE_THRESHOLD_MS } from "../draftEditingObserver";
import { flushLogs, resetLogBufferForTest } from "../logging";
import type { Tab } from "../types";
import {
  changeObserverKey,
  useChangeWorkingObserver,
  visibleChangesOf,
  type ObservedChange,
} from "./useChangeWorkingObserver";

const calls = () => invokeMock.mock.calls.filter((c) => c[0] === "record_change_working_interval");
const A: ObservedChange = { streamId: "s-1", change: "add-logout" };
const B: ObservedChange = { streamId: "s-2", change: "add-login" };
const START = Date.parse("2026-01-01T10:00:00Z");

beforeEach(() => {
  vi.useFakeTimers();
  vi.setSystemTime(START);
  resetLogBufferForTest();
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "append_log_records" || cmd === "record_change_working_interval") return null;
    throw new Error(`unexpected invoke ${cmd}`);
  });
});
afterEach(() => {
  // No flush timer of the log buffer goes to the next test.
  resetLogBufferForTest();
  vi.useRealTimers();
});

/** The records with this message that the log buffer sent so far. */
function logRecords(message: string): Array<{ level: string; fields: Record<string, unknown> }> {
  flushLogs();
  return invokeMock.mock.calls
    .filter((c) => c[0] === "append_log_records")
    .flatMap((c) => (c[1] as { records: Array<Record<string, unknown>> }).records)
    .filter((r) => r.message === message) as never;
}

const interact = () => act(() => void window.dispatchEvent(new Event("pointerdown")));
const advance = (ms: number) => act(() => void vi.advanceTimersByTime(ms));

function observe(initial: readonly ObservedChange[]) {
  return renderHook(({ changes }) => useChangeWorkingObserver(changes), {
    initialProps: { changes: initial },
  });
}

describe("the related surfaces (OCW-FR-MIEY)", () => {
  const tab = (over: Partial<Tab>): Tab => ({ id: "t", label: "x", kind: "editor", ...over }) as Tab;

  it("OCW-FR-MIEY: the active change tab on a stream and the active conversation tab of a change are related, and nothing else is", () => {
    const changeTab = tab({
      id: "c",
      kind: "openspecChange",
      openspecChange: { location: { kind: "stream", streamId: "s-1" }, name: "add-logout", file: null, openSeq: 1 },
    });
    const worktreeTab = tab({
      id: "w",
      kind: "openspecChange",
      openspecChange: { location: { kind: "active_worktree" }, name: "add-logout", file: null, openSeq: 1 },
    });
    const chat = tab({ id: "v", kind: "conversation", ownerTarget: { kind: "change", streamId: "s-2", change: "add-login" } });
    const draftChat = tab({ id: "d", kind: "conversation", ownerTarget: { kind: "draft", draftId: "d-1" } });
    const tabs = [changeTab, worktreeTab, chat, draftChat];
    expect(visibleChangesOf(tabs, "c")).toEqual([A]);
    expect(visibleChangesOf(tabs, "v")).toEqual([B]);
    expect(visibleChangesOf(tabs, "w")).toEqual([]);
    expect(visibleChangesOf(tabs, "d")).toEqual([]);
    expect(visibleChangesOf(tabs, null)).toEqual([]);
  });

  it("OCW-FR-MIEY: the key of a change keeps a name with quotes and commas apart from another", () => {
    expect(changeObserverKey({ streamId: "s", change: 'a,"b' })).not.toBe(changeObserverKey({ streamId: "s,a", change: '"b' }));
  });
});

describe("the intervals (OCW-FR-MIEY)", () => {
  it("OCW-FR-MIEY / DFI-FR-XKRM: an interaction opens an interval, and a loss of focus settles and reports it whole", async () => {
    const hook = observe([A]);
    expect(calls()).toHaveLength(0);
    interact();
    advance(90_000);
    act(() => void window.dispatchEvent(new Event("blur")));
    expect(calls()).toHaveLength(1);
    expect(calls()[0][1]).toEqual({
      streamId: "s-1",
      change: "add-logout",
      startedAt: "2026-01-01T10:00:00.000Z",
      endedAt: "2026-01-01T10:01:30.000Z",
    });
    hook.unmount();
  });

  it("OCW-FR-MIEY / DFI-FR-XKRM: five minutes with no interaction settle the interval at the five-minute mark", () => {
    const hook = observe([A]);
    interact();
    advance(60_000);
    interact();
    advance(IDLE_THRESHOLD_MS - 1);
    expect(calls()).toHaveLength(0);
    advance(1);
    expect(calls()).toHaveLength(1);
    expect(calls()[0][1]).toMatchObject({
      startedAt: "2026-01-01T10:00:00.000Z",
      endedAt: "2026-01-01T10:06:00.000Z",
    });
    hook.unmount();
  });

  it("OCW-FR-MIEY: the last related surface going settles the interval, and an equal list settles nothing", () => {
    const hook = observe([A]);
    interact();
    advance(30_000);
    hook.rerender({ changes: [{ ...A }] });
    expect(calls()).toHaveLength(0);
    hook.rerender({ changes: [] });
    expect(calls()).toHaveLength(1);
    hook.unmount();
  });

  it("OCW-FR-MIEY / DFI-FR-JDWS: one change holds one interval, and two changes hold one each", () => {
    const hook = observe([A, { ...A }, B]);
    interact();
    advance(10_000);
    act(() => void window.dispatchEvent(new Event("blur")));
    expect(calls().map((c) => (c[1] as ObservedChange).change).sort()).toEqual(["add-login", "add-logout"]);
    hook.unmount();
  });

  it("OCW-FR-MIEY / DFI-FR-UKFR: an interval open at unmount is reported not at all, and no interaction while unfocused opens one", () => {
    const hook = observe([A]);
    act(() => void window.dispatchEvent(new Event("blur")));
    interact();
    advance(IDLE_THRESHOLD_MS * 2);
    expect(calls()).toHaveLength(0);
    act(() => void window.dispatchEvent(new Event("focus")));
    interact();
    advance(10_000);
    hook.unmount();
    advance(IDLE_THRESHOLD_MS * 2);
    expect(calls()).toHaveLength(0);
  });

  it("OCW-FR-MIEY: a refused record is logged and not thrown", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "append_log_records") return null;
      throw "invalid_interval";
    });
    const hook = observe([A]);
    interact();
    advance(10_000);
    act(() => void window.dispatchEvent(new Event("blur")));
    expect(calls()).toHaveLength(1);
    // vi.waitFor advances the fake clock on each check.
    await vi.waitFor(() =>
      expect(logRecords("a change working interval could not be recorded")).toEqual([
        expect.objectContaining({
          level: "WARN",
          fields: { streamId: "s-1", reason: "invalid_interval" },
        }),
      ]),
    );
    hook.unmount();
  });
});
