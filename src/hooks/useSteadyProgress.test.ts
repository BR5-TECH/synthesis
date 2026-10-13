import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, renderHook } from "@testing-library/react";

import {
  HIDE_DELAY_MS,
  MIN_VISIBLE_MS,
  SHOW_DELAY_MS,
  useSteadyProgress,
} from "./useSteadyProgress";
import type { Operation } from "../types";

// STB-status-bar.md — when the center region shows an operation, and which
// one. Fake timers only: no result here depends on real time.

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

function op(id: string, overrides: Partial<Operation> = {}): Operation {
  return {
    id,
    kind: "index",
    label: `Operation ${id}`,
    state: "running",
    sequence: Number(id.replace(/\D/g, "")) || 1,
    ...overrides,
  };
}

function mount(initial: Operation[] = []) {
  return renderHook(({ ops }) => useSteadyProgress(ops), {
    initialProps: { ops: initial },
  });
}

function advance(ms: number) {
  act(() => {
    vi.advanceTimersByTime(ms);
  });
}

/** Mounts with `a` in flight and advances until the bar shows it. */
function shown(a: Operation) {
  const hook = mount([a]);
  advance(SHOW_DELAY_MS);
  expect(hook.result.current).toBe(a);
  return hook;
}

describe("show delay (STB-FR-NAEX)", () => {
  it("STB-FR-NAEX, STB-FR-07: an operation that ends within the delay never shows", () => {
    const hook = mount([op("op-1")]);
    advance(SHOW_DELAY_MS - 1);
    expect(hook.result.current).toBeUndefined();
    hook.rerender({ ops: [] });
    advance(SHOW_DELAY_MS * 5);
    expect(hook.result.current).toBeUndefined();
  });

  it("STB-FR-NAEX: an operation shows once it has been in flight for the delay", () => {
    const a = op("op-1");
    const hook = mount([a]);
    advance(SHOW_DELAY_MS - 1);
    expect(hook.result.current).toBeUndefined();
    advance(1);
    expect(hook.result.current).toBe(a);
  });

  it("STB-FR-NAEX: the delay runs on when one operation follows another with no gap", () => {
    const b = op("op-2");
    const hook = mount([op("op-1")]);
    advance(200);
    hook.rerender({ ops: [b] });
    advance(SHOW_DELAY_MS - 200);
    expect(hook.result.current).toBe(b);
  });

  it("STB-FR-NAEX: a gap with nothing in flight starts the delay again", () => {
    const b = op("op-2");
    const hook = mount([op("op-1")]);
    advance(200);
    hook.rerender({ ops: [] });
    advance(50);
    hook.rerender({ ops: [b] });
    advance(SHOW_DELAY_MS - 1);
    expect(hook.result.current).toBeUndefined();
    advance(1);
    expect(hook.result.current).toBe(b);
  });

  it("STB-FR-NAEX, STB-FR-07: after the bar hides, a new operation waits the full delay again", () => {
    const a = op("op-1");
    const hook = shown(a);
    hook.rerender({ ops: [] });
    advance(MIN_VISIBLE_MS);
    expect(hook.result.current).toBeUndefined();

    const b = op("op-2");
    hook.rerender({ ops: [b] });
    // The held operation does not come back while the delay runs.
    expect(hook.result.current).toBeUndefined();
    advance(SHOW_DELAY_MS - 1);
    expect(hook.result.current).toBeUndefined();
    advance(1);
    expect(hook.result.current).toBe(b);
  });
});

describe("hold (STB-FR-JWLN)", () => {
  it("STB-FR-JWLN: an operation that ends right after it shows stays for the minimum time", () => {
    const a = op("op-1");
    const hook = shown(a);
    advance(1);
    hook.rerender({ ops: [] });
    advance(MIN_VISIBLE_MS - 2);
    expect(hook.result.current).toBe(a);
    advance(1);
    expect(hook.result.current).toBeUndefined();
  });

  it("STB-FR-JWLN: after a long operation ends the bar stays for the hide delay", () => {
    const a = op("op-1");
    const hook = shown(a);
    advance(2000);
    hook.rerender({ ops: [] });
    advance(HIDE_DELAY_MS - 1);
    expect(hook.result.current).toBe(a);
    advance(1);
    expect(hook.result.current).toBeUndefined();
  });

  it("STB-FR-JWLN: the hold keeps the last operation in its last state", () => {
    const hook = shown(op("op-1", { completed: 3, total: 10 }));
    const last = op("op-1", { completed: 7, total: 10 });
    hook.rerender({ ops: [last] });
    expect(hook.result.current).toBe(last);
    hook.rerender({ ops: [] });
    advance(HIDE_DELAY_MS - 1);
    expect(hook.result.current).toBe(last);
  });

  it("STB-FR-06, STB-FR-JWLN: when the head ends and another operation runs on, the bar falls back at once", () => {
    const a = op("op-1");
    const b = op("op-2");
    const hook = mount([b, a]);
    advance(SHOW_DELAY_MS);
    expect(hook.result.current).toBe(b);
    hook.rerender({ ops: [a] });
    expect(hook.result.current).toBe(a);
  });

  it("STB-FR-JWLN, STB-FR-KPRU: the minimum time counts from the first show, not from a replacement", () => {
    const hook = shown(op("op-1"));
    advance(50);
    hook.rerender({ ops: [] });
    advance(50);
    const c = op("op-3");
    hook.rerender({ ops: [c] });
    advance(50);
    hook.rerender({ ops: [] });
    // Shown at 0; the set empties at 150; the minimum time ends at 600.
    advance(MIN_VISIBLE_MS - 150 - 1);
    expect(hook.result.current).toBe(c);
    advance(1);
    expect(hook.result.current).toBeUndefined();
  });

  it("STB-FR-05, STB-FR-15: while shown the bar follows the head and its updates at once", () => {
    const hook = shown(op("op-1"));
    const b = op("op-2");
    hook.rerender({ ops: [b, op("op-1")] });
    expect(hook.result.current).toBe(b);
    const bLater = op("op-2", { completed: 5, total: 9 });
    hook.rerender({ ops: [bLater, op("op-1")] });
    expect(hook.result.current).toBe(bLater);
  });
});

describe("replace in place (STB-FR-KPRU)", () => {
  it("STB-FR-KPRU: an operation that starts during the hold replaces the held one and cancels the hide", () => {
    const hook = shown(op("op-1"));
    advance(2000);
    hook.rerender({ ops: [] });
    advance(100);

    const c = op("op-3");
    hook.rerender({ ops: [c] });
    expect(hook.result.current).toBe(c);
    // The hide the hold had planned passes, and nothing hides.
    advance(HIDE_DELAY_MS * 2);
    expect(hook.result.current).toBe(c);
  });

  it("STB-FR-JWLN, STB-FR-KPRU: when the replacing operation ends, a new hold starts", () => {
    const hook = shown(op("op-1"));
    advance(2000);
    hook.rerender({ ops: [] });
    advance(100);
    const c = op("op-3");
    hook.rerender({ ops: [c] });
    advance(1000);
    hook.rerender({ ops: [] });
    advance(HIDE_DELAY_MS - 1);
    expect(hook.result.current).toBe(c);
    advance(1);
    expect(hook.result.current).toBeUndefined();
  });
});

describe("timers do not outlive the hook", () => {
  it("leaves no timer behind on unmount while the delay runs", () => {
    const hook = mount([op("op-1")]);
    expect(vi.getTimerCount()).toBeGreaterThan(0);
    hook.unmount();
    expect(vi.getTimerCount()).toBe(0);
  });

  it("leaves no timer behind on unmount during the hold", () => {
    const hook = shown(op("op-1"));
    hook.rerender({ ops: [] });
    expect(vi.getTimerCount()).toBeGreaterThan(0);
    hook.unmount();
    expect(vi.getTimerCount()).toBe(0);
  });
});
