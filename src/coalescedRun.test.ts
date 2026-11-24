import { describe, expect, it } from "vitest";
import { coalescedRun } from "./coalescedRun";

/** A task whose runs the test ends one by one. */
function controlledTask() {
  const pending: Array<() => void> = [];
  let started = 0;
  const task = () =>
    new Promise<void>((resolve) => {
      started += 1;
      pending.push(resolve);
    });
  const flush = () => new Promise<void>((r) => setTimeout(r, 0));
  return {
    task,
    started: () => started,
    async finishNext() {
      pending.shift()?.();
      await flush();
    },
    flush,
  };
}

describe("coalescedRun", () => {
  it("STB-FR-GAYM, CHG-FR-RUAG, DFV-FR-HTXT: a burst during a run starts exactly one trailing run", async () => {
    const t = controlledTask();
    const runner = coalescedRun(t.task);
    runner.run();
    await t.flush();
    expect(t.started()).toBe(1);
    for (let i = 0; i < 50; i += 1) runner.run();
    await t.flush();
    expect(t.started()).toBe(1);
    await t.finishNext();
    expect(t.started()).toBe(2);
    await t.finishNext();
    expect(t.started()).toBe(2);
  });

  it("STB-FR-GAYM: the trailing run starts only after the run in flight ends", async () => {
    const order: string[] = [];
    let release!: () => void;
    let runs = 0;
    const runner = coalescedRun(async () => {
      runs += 1;
      order.push(`start ${runs}`);
      if (runs === 1) await new Promise<void>((r) => (release = r));
      order.push(`end ${runs}`);
    });
    runner.run();
    await Promise.resolve();
    runner.run();
    await new Promise((r) => setTimeout(r, 0));
    release();
    await new Promise((r) => setTimeout(r, 0));
    expect(order).toEqual(["start 1", "end 1", "start 2", "end 2"]);
  });

  it("CHG-FR-RUAG: a request after a run ended starts a new run at once", async () => {
    const t = controlledTask();
    const runner = coalescedRun(t.task);
    runner.run();
    await t.flush();
    await t.finishNext();
    runner.run();
    await t.flush();
    expect(t.started()).toBe(2);
  });

  it("DFV-FR-HTXT: a task that rejects or throws still ends its run, and the trailing run follows", async () => {
    let calls = 0;
    const runner = coalescedRun(() => {
      calls += 1;
      if (calls === 1) return Promise.reject(new Error("read failed"));
      if (calls === 2) throw new Error("threw");
      return undefined;
    });
    runner.run();
    runner.run();
    await new Promise((r) => setTimeout(r, 0));
    expect(calls).toBe(2);
    runner.run();
    await new Promise((r) => setTimeout(r, 0));
    expect(calls).toBe(3);
  });

  it("STB-FR-GAYM: after dispose no run starts, and no trailing run follows the one in flight", async () => {
    const t = controlledTask();
    const runner = coalescedRun(t.task);
    runner.run();
    await t.flush();
    runner.run();
    runner.dispose();
    await t.finishNext();
    runner.run();
    await t.flush();
    expect(t.started()).toBe(1);
  });
});

describe("coalescedRun start", () => {
  it("STB-FR-GAYM: the first run starts in the call that asks for it", () => {
    let calls = 0;
    const runner = coalescedRun(async () => {
      calls += 1;
    });
    runner.run();
    expect(calls).toBe(1);
  });
});
