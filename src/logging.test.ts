import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

// The emit module reaches the backend through `api.appendLogRecords`, which is
// an `invoke`. Mocked at the Tauri boundary rather than at `../api`, so the
// wrapper's own command name and argument shape are exercised too.
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));

import { invoke } from "@tauri-apps/api/core";
import {
  FLUSH_DELAY_MS,
  MAX_BATCH,
  flushLogs,
  log,
  logError,
  logWarn,
  resetLogBufferForTest,
} from "./logging";

const invoked = vi.mocked(invoke);

/** Every `append_log_records` call's `records` argument, in call order. */
const batches = (): unknown[][] =>
  invoked.mock.calls
    .filter(([name]) => name === "append_log_records")
    .map(([, args]) => (args as { records: unknown[] }).records);

beforeEach(() => {
  vi.useFakeTimers();
  resetLogBufferForTest();
  invoked.mockClear();
  invoked.mockImplementation(async () => undefined);
});

afterEach(() => {
  resetLogBufferForTest();
  vi.useRealTimers();
});

// LOG-FR-19: the frontend emits through one module that batches and
// never blocks a render.

describe("frontend log emission (LOG-FR-19)", () => {
  it("returns without awaiting the round trip, and batches the burst", () => {
    // The synchronous return is the requirement: `log` is called from render
    // paths and event handlers, and an emit that awaited an IPC round trip
    // would put the backend on the critical path of drawing a frame.
    for (let n = 0; n < 20; n += 1) {
      const returned = log("INFO", ["frontend"], `m${n}`);
      expect(returned).toBeUndefined();
    }
    expect(invoked).not.toHaveBeenCalled();

    vi.advanceTimersByTime(FLUSH_DELAY_MS);
    // LOG-FR-19: twenty records, fewer than twenty calls — one, in fact.
    expect(batches()).toHaveLength(1);
    expect(batches()[0]).toHaveLength(20);
  });

  it("sends the command name and argument shape the backend expects", () => {
    log("WARN", ["ai", "remote"], "model request retried", { attempt: 2 });
    vi.advanceTimersByTime(FLUSH_DELAY_MS);

    expect(invoked).toHaveBeenCalledWith("append_log_records", {
      records: [
        {
          // LGC-FR-06: the instant of the emit, not of the flush.
          ts: expect.stringMatching(/^\d{4}-\d{2}-\d{2}T[\d:.]+Z$/),
          level: "WARN",
          domains: ["ai", "remote"],
          message: "model request retried",
          fields: { attempt: 2 },
        },
      ],
    });
  });

  it("stamps each record when it was emitted, not when the batch flushed", () => {
    // LGC-FR-06 is what makes a batched record's time trustworthy; a stamp
    // taken at flush would collapse a burst onto one instant.
    vi.setSystemTime(new Date("2026-08-04T12:00:00.000Z"));
    log("INFO", ["frontend"], "first");
    vi.setSystemTime(new Date("2026-08-04T12:00:00.150Z"));
    log("INFO", ["frontend"], "second");
    vi.advanceTimersByTime(FLUSH_DELAY_MS);

    const sent = batches()[0] as { ts: string }[];
    expect(sent[0].ts).toBe("2026-08-04T12:00:00.000Z");
    expect(sent[1].ts).toBe("2026-08-04T12:00:00.150Z");
  });

  it("flushes on size before the timer when a burst is large", () => {
    for (let n = 0; n < MAX_BATCH; n += 1) log("DEBUG", ["frontend"], `m${n}`);
    // The bound is on count as well as on time, so a render loop emitting per
    // frame cannot grow one unbounded batch between flushes.
    expect(batches()).toHaveLength(1);
    expect(batches()[0]).toHaveLength(MAX_BATCH);
  });

  it("keeps emitting after the backend rejects a batch, and surfaces nothing", () => {
    // LOG-FR-19: dropped, not retried. A retry loop over a failing channel is
    // how a logging system becomes the outage it was meant to explain.
    invoked.mockRejectedValueOnce(new Error("no backend"));
    log("ERROR", ["backend"], "first");
    vi.advanceTimersByTime(FLUSH_DELAY_MS);
    expect(batches()).toHaveLength(1);

    log("ERROR", ["backend"], "second");
    vi.advanceTimersByTime(FLUSH_DELAY_MS);
    const sent = batches();
    expect(sent).toHaveLength(2);
    // The failed batch is gone rather than prepended to the next one.
    expect(sent[1]).toHaveLength(1);
    expect((sent[1][0] as { message: string }).message).toBe("second");
  });

  it("does not lose records emitted while a flush is in flight", () => {
    // The pending array is taken before the call, so a record emitted during
    // the round trip lands in the next batch rather than in the one whose
    // rejection would discard it.
    let release: (() => void) | undefined;
    invoked.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          release = () => resolve(undefined);
        }) as Promise<void>,
    );
    log("INFO", ["frontend"], "in-batch");
    vi.advanceTimersByTime(FLUSH_DELAY_MS);
    log("INFO", ["frontend"], "during-flight");
    release?.();
    vi.advanceTimersByTime(FLUSH_DELAY_MS);

    const sent = batches();
    expect(sent).toHaveLength(2);
    expect((sent[0][0] as { message: string }).message).toBe("in-batch");
    expect((sent[1][0] as { message: string }).message).toBe("during-flight");
  });

  it("drops a record it cannot attribute rather than making a round trip", () => {
    // LGC-FR-03 rejects an empty domain set backend-side; sending one anyway
    // would be a round trip whose only outcome is a discard.
    log("INFO", [], "unattributable");
    vi.advanceTimersByTime(FLUSH_DELAY_MS);
    expect(invoked).not.toHaveBeenCalled();
  });

  it("flushes on demand for the paths that end the window's life", () => {
    // A project close or a quit tears the window down before the timer fires,
    // and the records worth keeping are exactly the ones explaining why.
    logError(["backend"], "closing");
    expect(invoked).not.toHaveBeenCalled();
    flushLogs();
    expect(batches()).toHaveLength(1);
    // Idempotent: a second flush with nothing pending sends nothing.
    flushLogs();
    expect(batches()).toHaveLength(1);
  });

  it("fixes the level from each named emitter", () => {
    logWarn(["backend"], "w");
    logError(["backend"], "e");
    vi.advanceTimersByTime(FLUSH_DELAY_MS);
    const sent = batches()[0] as { level: string }[];
    expect(sent.map((r) => r.level)).toEqual(["WARN", "ERROR"]);
  });
});
