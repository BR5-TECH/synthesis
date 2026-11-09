import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

import { logWarn } from "../logging";
import * as engine from "./engine";
import { workerEngine } from "./engine";
import type { ElkNode } from "./layout";
import {
  cachedCalm,
  forgetCalmTransientFailures,
  readCalm,
  resetCalmForTest,
  setCalmEngineLoaderForTest,
} from "./read";

const valid = JSON.stringify({
  $schema: "https://calm.finos.org/release/1.3/meta/calm.json",
  nodes: [{ "unique-id": "a", "node-type": "service", name: "A", description: "A." }],
});

beforeEach(() => {
  resetCalmForTest();
  vi.mocked(logWarn).mockClear();
});

afterEach(() => {
  setCalmEngineLoaderForTest(null);
});

describe("CALM read: failures of the libraries", () => {
  it("CLM-FR-GFZK: an error of the layout engine fails the layout step with its message", async () => {
    setCalmEngineLoaderForTest(async () => ({
      ...engine,
      layoutEngine: async () => ({
        layout: async () => {
          throw new Error("ELK broke");
        },
      }),
    }));
    const result = await readCalm(valid);
    expect(result).toMatchObject({
      state: "error",
      step: "layout",
      summary: "The application cannot compute the layout.",
      detail: "ELK broke",
    });
  });

  it("CLM-FR-HPJT: a failed load fails the read, logs a WARN, and a later read tries again", async () => {
    const loader = vi.fn(() => Promise.reject(new Error("chunk missing")));
    setCalmEngineLoaderForTest(loader);
    const first = await readCalm(valid);
    expect(first).toMatchObject({
      state: "error",
      step: "load",
      summary: "The diagram library cannot be loaded.",
      detail: "chunk missing",
      transient: true,
    });
    expect(logWarn).toHaveBeenCalledWith(
      ["frontend"],
      "the CALM libraries cannot be loaded",
      expect.objectContaining({ error: "Error" }),
    );
    // No surface has to forget the failure: it is not kept.
    expect(cachedCalm(valid)).toBeUndefined();
    await readCalm(valid);
    expect(loader).toHaveBeenCalledTimes(2);
    setCalmEngineLoaderForTest(null);
    expect((await readCalm(valid)).state).toBe("rendered");
  });

  it("CLM-FR-HPJT: a layout engine that cannot be loaded is a failed library load", async () => {
    setCalmEngineLoaderForTest(async () => ({
      ...engine,
      layoutEngine: () => Promise.reject(new Error("worker chunk missing")),
    }));
    const result = await readCalm(valid);
    expect(result).toMatchObject({ state: "error", step: "load", transient: true });
    expect(logWarn).toHaveBeenCalledTimes(1);
    expect(logWarn).toHaveBeenCalledWith(["frontend"], "the CALM layout engine cannot be loaded", {
      error: "Error",
    });
  });

  it("CLM-FR-HPJT: a kept result that is not transient stays after forgetting failures", async () => {
    const first = await readCalm(valid);
    forgetCalmTransientFailures();
    expect(await readCalm(valid)).toBe(first);
  });

  it("CLM-FR-GFZK, CLM-FR-JIWR: a failed layout logs a WARN with the node count and no document text", async () => {
    setCalmEngineLoaderForTest(async () => ({
      ...engine,
      layoutEngine: async () => ({
        layout: async () => {
          throw new Error("ELK broke");
        },
      }),
    }));
    await readCalm(valid);
    expect(logWarn).toHaveBeenCalledWith(["frontend"], "the CALM layout failed", {
      error: "Error",
      nodes: 1,
    });
  });

  it("CLM-FR-HPJT: a schema check that cannot run is a failed library load, with a WARN", async () => {
    setCalmEngineLoaderForTest(async () => ({
      ...engine,
      validateCalm: () => {
        throw new Error("schema did not compile");
      },
    }));
    const result = await readCalm(valid);
    expect(result).toMatchObject({ state: "error", step: "load", transient: true });
    expect(logWarn).toHaveBeenCalledWith(["frontend"], "the CALM schema check cannot run", {
      error: "Error",
    });
  });

  it("a read that nobody wants any more stops, gives null, and keeps nothing", async () => {
    const result = await readCalm(valid, () => false);
    expect(result).toBeNull();
    expect(cachedCalm(valid)).toBeUndefined();
    // A caller that wants it reads it afresh.
    expect((await readCalm(valid)).state).toBe("rendered");
  });

  it("a read goes on while one of its callers still wants it", async () => {
    let first = true;
    const one = readCalm(valid, () => first);
    const two = readCalm(valid);
    first = false;
    expect((await two)?.state).toBe("rendered");
    expect((await one)?.state).toBe("rendered");
  });
});

describe("the layout worker", () => {
  it("CLM-FR-GFZK: a worker that fails ends each layout that waits for it", async () => {
    const worker = new EventTarget() as unknown as Worker;
    const elk = workerEngine(
      (factory) => {
        factory("worker.js");
        return { layout: () => new Promise<ElkNode>(() => {}) };
      },
      () => worker,
    );
    const waiting = elk.layout({ id: "root" });
    worker.dispatchEvent(new Event("error"));
    await expect(waiting).rejects.toThrow("the layout worker failed");
  });
});
