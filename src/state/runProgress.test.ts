/**
 * `RPV-run-progress.md` — the pure derivation behind the visualization.
 *
 * Positional completion, the six conditions, the backward edges, and the bound
 * on the extent are all decided here, so the component can be a rendering of a
 * view rather than a place where rules live.
 */
import { describe, expect, it } from "vitest";
import {
  animates,
  conditionWord,
  progressView,
  type ProgressStage,
  type StageCondition,
} from "./runProgress";

const FOUR: ProgressStage[] = [
  { id: "queued", label: "Queued" },
  { id: "authoring", label: "Author" },
  { id: "validation", label: "Validate" },
  { id: "acceptance", label: "Accept" },
];

const THREE: ProgressStage[] = [
  { id: "draft", label: "Draft" },
  { id: "check", label: "Check" },
  { id: "ship", label: "Ship" },
];

const statuses = (
  stages: ProgressStage[],
  current: string | null,
  condition: StageCondition,
) => progressView(stages, current, condition).stages.map((s) => s.status);

describe("progressView", () => {
  /** RPV-FR-02, RPV-FR-04, RPV-FR-05: completion is positional. */
  it("completes what is before the current stage and starts nothing after it", () => {
    expect(statuses(FOUR, "validation", "active")).toEqual([
      "complete",
      "complete",
      "current",
      "not_started",
    ]);
  });

  /** RPV-FR-03, RPV-FR-02: the same rules against a different configuration. */
  it("renders any configuration under the same rules", () => {
    expect(statuses(THREE, "check", "active")).toEqual([
      "complete",
      "current",
      "not_started",
    ]);
    expect(progressView(THREE, "check", "active").total).toBe(3);
    // Seven stages, no stage id of any host written into the derivation.
    const seven = Array.from({ length: 7 }, (_, i) => ({
      id: `s${i}`,
      label: `S${i}`,
    }));
    expect(statuses(seven, "s5", "waiting")).toEqual([
      "complete",
      "complete",
      "complete",
      "complete",
      "complete",
      "current",
      "not_started",
    ]);
  });

  /** RPV-FR-04: an id the configuration does not hold guesses nothing. */
  it("renders every stage as not started when the current stage is unknown", () => {
    expect(statuses(FOUR, "nowhere", "active")).toEqual([
      "not_started",
      "not_started",
      "not_started",
      "not_started",
    ]);
    expect(statuses(FOUR, null, "active")).toEqual([
      "not_started",
      "not_started",
      "not_started",
      "not_started",
    ]);
    expect(progressView(FOUR, "nowhere", "active").animates).toBe(false);
  });

  /** RPV-FR-06, RPV-FR-05: every stage is complete in exactly one arrangement. */
  it("completes every stage only for complete on the last stage", () => {
    expect(statuses(FOUR, "acceptance", "complete")).toEqual([
      "complete",
      "complete",
      "complete",
      "complete",
    ]);
    expect(statuses(FOUR, "authoring", "complete")).toEqual([
      "complete",
      "complete",
      "not_started",
      "not_started",
    ]);
  });

  /** RPV-FR-05, RPV-FR-12: a stopped run completes nothing after where it stopped. */
  it("never completes a stage after a stopped one", () => {
    const view = progressView(FOUR, "validation", "stopped");
    expect(view.stages.map((s) => s.status)).toEqual([
      "complete",
      "complete",
      "current",
      "not_started",
    ]);
    expect(view.animates).toBe(false);
    expect(view.showsOutcome).toBe(true);
  });

  /** RPV-FR-07: only `active` is work in progress. */
  it("animates for active alone", () => {
    const conditions: StageCondition[] = [
      "active",
      "waiting",
      "paused",
      "blocked",
      "stopped",
      "complete",
    ];
    for (const condition of conditions) {
      expect(animates(condition)).toBe(condition === "active");
      expect(progressView(FOUR, "validation", condition).animates).toBe(
        condition === "active",
      );
    }
  });

  /** RPV-FR-08: a condition is never rendered without a word. */
  it("gives every condition a word", () => {
    const conditions: StageCondition[] = [
      "active",
      "waiting",
      "paused",
      "blocked",
      "stopped",
      "complete",
    ];
    for (const condition of conditions) {
      expect(conditionWord(condition).trim().length).toBeGreaterThan(0);
    }
  });

  /** RPV-FR-10: a backward move is a loop; a forward one is not. */
  it("finds one loop per backward edge and none for forward ones", () => {
    const view = progressView(FOUR, "authoring", "active", [
      { from: null, to: "queued", iteration: 0 },
      { from: "queued", to: "authoring", iteration: 0 },
      { from: "authoring", to: "validation", iteration: 1 },
      { from: "validation", to: "authoring", iteration: 1 },
      { from: "authoring", to: "validation", iteration: 2 },
      { from: "acceptance", to: "authoring", iteration: 3 },
    ]);
    expect(view.loops.map((l) => [l.from.id, l.to.id, l.iteration])).toEqual([
      ["validation", "authoring", 1],
      ["acceptance", "authoring", 3],
    ]);
  });

  it("renders no loop for a history of forward moves alone", () => {
    const view = progressView(FOUR, "acceptance", "waiting", [
      { from: null, to: "queued" },
      { from: "queued", to: "authoring" },
      { from: "authoring", to: "validation" },
      { from: "validation", to: "acceptance" },
    ]);
    expect(view.loops).toEqual([]);
  });

  it("ignores a transition naming a stage the configuration does not hold", () => {
    const view = progressView(FOUR, "authoring", "active", [
      { from: "elsewhere", to: "authoring" },
      { from: "validation", to: "nowhere" },
    ]);
    expect(view.loops).toEqual([]);
  });

  /** RPV-FR-11: the extent is the current stage's own position, whatever the
   * history holds — a repeated stage is never progress past the end. */
  it("bounds the extent by the configured list however often it looped", () => {
    const looped = progressView(FOUR, "authoring", "active", [
      { from: "validation", to: "authoring", iteration: 1 },
      { from: "validation", to: "authoring", iteration: 2 },
      { from: "acceptance", to: "authoring", iteration: 3 },
    ]);
    const fresh = progressView(FOUR, "authoring", "active", []);
    expect(looped.reached).toBe(fresh.reached);
    expect(looped.reached).toBe(1);
    expect(looped.reached).toBeLessThanOrEqual(looped.total);

    const done = progressView(FOUR, "acceptance", "complete", [
      { from: "acceptance", to: "authoring", iteration: 3 },
    ]);
    expect(done.reached).toBe(done.total);
  });

  /** RPV-FR-05, RPV-FR-12 / RPV-FR-14, RPV-FR-15: an outcome belongs to work that has ended. */
  it("shows an outcome for stopped and complete alone", () => {
    const shown: StageCondition[] = ["stopped", "complete"];
    const hidden: StageCondition[] = ["active", "waiting", "paused", "blocked"];
    for (const condition of shown) {
      expect(progressView(FOUR, "acceptance", condition).showsOutcome).toBe(true);
    }
    for (const condition of hidden) {
      expect(progressView(FOUR, "acceptance", condition).showsOutcome).toBe(
        false,
      );
    }
  });
});
