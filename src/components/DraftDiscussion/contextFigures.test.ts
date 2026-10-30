import { describe, expect, it } from "vitest";

import {
  indicatorName,
  indicatorText,
  levelOf,
  percentOf,
  ringFill,
  shownContext,
  splitTotal,
} from "./contextFigures";
import type { DiscussionContext, TurnContext } from "../../types";

const SPLIT = {
  instructions: 1000,
  document: 2000,
  history: 300,
  currentComment: 20,
  toolResults: 0,
};

const reported = (over: Partial<TurnContext> = {}): TurnContext => ({
  window: 10_000,
  inputTokens: 4_000,
  split: SPLIT,
  document: { lines: 10, characters: 8000, estimatedTokens: 2000 },
  limitTokens: 3000,
  aboveLimit: false,
  ...over,
});

const estimated = (over: Partial<DiscussionContext> = {}): DiscussionContext => ({
  window: 10_000,
  estimatedTokens: 3320,
  split: SPLIT,
  document: { lines: 10, characters: 8000, estimatedTokens: 2000 },
  limitTokens: 3000,
  aboveLimit: false,
  ...over,
});

describe("the indicator's figures", () => {
  it("DDS-FR-CTXI: the share of the window is a whole percent", () => {
    expect(percentOf(4_210, 10_000)).toBe(42);
    expect(percentOf(4_250, 10_000)).toBe(43);
    expect(percentOf(12_000, 10_000)).toBe(120);
    expect(percentOf(100, 0)).toBe(0);
  });

  it("DDS-FR-KXWQ: the reported input wins over the estimate", () => {
    const shown = shownContext(reported(), estimated(), false)!;
    expect(shown.tokens).toBe(4000);
    expect(shown.estimate).toBe(false);
    expect(indicatorText(shown)).toBe("40%");
  });

  it("DDS-FR-KXWQ: a turn whose provider reported no input shows the sum of its split, marked", () => {
    const shown = shownContext(reported({ inputTokens: null }), null, false)!;
    expect(shown.tokens).toBe(splitTotal(SPLIT));
    expect(shown.estimate).toBe(true);
    expect(indicatorText(shown)).toBe("~33%");
  });

  it("DDS-FR-KXWQ: before any turn the value is the estimate, marked", () => {
    const shown = shownContext(null, estimated(), false)!;
    expect(indicatorText(shown)).toBe("~33%");
    expect(indicatorName(shown)).toBe(
      "Context use: 33% of the model's context window, level normal, estimated",
    );
  });

  it("DDS-FR-KXWQ: nothing to show before either answers", () => {
    expect(shownContext(null, null, false)).toBeNull();
  });

  it("DDS-FR-WUCD: normal below 50%, high from 50% to below 85%, critical from 85% up", () => {
    expect(levelOf(0)).toBe("normal");
    expect(levelOf(49)).toBe("normal");
    expect(levelOf(50)).toBe("high");
    expect(levelOf(84)).toBe("high");
    expect(levelOf(85)).toBe("critical");
    expect(levelOf(140)).toBe("critical");
  });

  it("DDS-FR-WUCD, DDS-FR-ZQVE: the level comes from the percent and not from the warning", () => {
    const shown = shownContext(null, estimated({ aboveLimit: true }), false)!;
    expect(shown.level).toBe("normal");
    expect(shown.aboveLimit).toBe(true);
    const full = shownContext(reported({ inputTokens: 9_000 }), null, false)!;
    expect(full.level).toBe("critical");
    expect(full.aboveLimit).toBe(false);
    expect(indicatorName(full)).toBe(
      "Context use: 90% of the model's context window, level critical",
    );
  });

  it("DDS-FR-WCET: the ring fills with the share and is full at 100% and above", () => {
    expect(ringFill(0)).toBe(0);
    expect(ringFill(42)).toBeCloseTo(0.42);
    expect(ringFill(100)).toBe(1);
    expect(ringFill(150)).toBe(1);
  });

  it("DDS-FR-ZQVE, DDS-FR-HJMF: the newer estimate decides the document size and the warning", () => {
    const newer = estimated({
      aboveLimit: true,
      document: { lines: 99, characters: 40_000, estimatedTokens: 10_000 },
    });
    const fromEstimate = shownContext(reported(), newer, true)!;
    expect(fromEstimate.aboveLimit).toBe(true);
    expect(fromEstimate.document?.estimatedTokens).toBe(10_000);
    expect(fromEstimate.tokens).toBe(4000);
    const fromTurn = shownContext(reported(), newer, false)!;
    expect(fromTurn.aboveLimit).toBe(false);
  });
});
