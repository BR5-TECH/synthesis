/**
 * What a surface says about a run resting on a blocker
 * (`../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-LBPR,
 * GRU-FR-FZCN).
 */

import { describe, expect, it } from "vitest";

import { blockerCode, blockerDetail, blockerStatement } from "./blockers";
import { makeRun } from "../../test/graduationFixtures";

const RAW =
  "symlink refused: /Users/someone/.synthesis/g/g18d3/rv/node_modules/.pnpm/css-tree@3.2.1/node_modules/mdn-data";

function blocked(state: "blocked" | "awaiting_author", attempt: number) {
  return makeRun("r1", state, {
    blocker: {
      code: "review_checkout_failed",
      message: RAW,
      clearsBy: "Continue the run to try the review again.",
      attempt,
    },
  });
}

describe("blockerStatement", () => {
  it("GRU-FR-LBPR: says what blocks the run in the author's terms, not the backend's", () => {
    const statement = blockerStatement(blocked("blocked", 1)) ?? "";

    expect(statement).toContain("The review could not be given a checkout to stand in.");
    // The raw message names an absolute path and an internal error shape. It is
    // detail beside the sentence, never the sentence itself.
    expect(statement).not.toContain("symlink refused");
    expect(statement).not.toContain("/Users/");
    expect(statement).not.toContain("node_modules");
  });

  it("GRU-FR-LBPR: states nothing was committed and the act that clears it", () => {
    const statement = blockerStatement(blocked("blocked", 1)) ?? "";

    expect(statement).toContain("Nothing was committed.");
    expect(statement).toContain("Continue the run to try the review again.");
  });

  it("GRU-FR-LBPR: states how many times the run stopped on the same thing", () => {
    expect(blockerStatement(blocked("blocked", 1))).toContain(
      "This is the first time it stopped here.",
    );
    expect(blockerStatement(blocked("awaiting_author", 2))).toContain(
      "It stopped here 2 times in a row.",
    );
  });

  it("GRU-FR-FZCN: a run resting for the author on a blocker states it too", () => {
    const statement = blockerStatement(blocked("awaiting_author", 2)) ?? "";

    expect(statement).toContain("The review could not be given a checkout to stand in.");
    // Continue is still offered, but it no longer claims to clear the fault.
    expect(statement).toContain("Continue the run to try again, or discard it.");
  });

  it("GRU-FR-LBPR: a run in no blocking state says nothing", () => {
    expect(blockerStatement(makeRun("r1", "working"))).toBeNull();
    expect(blockerStatement(makeRun("r1", "completed"))).toBeNull();
    expect(blockerStatement(makeRun("r1", "awaiting_author"))).toBeNull();
  });

  it("GRU-FR-LBPR: a code this surface does not know falls back to the backend's account", () => {
    const run = makeRun("r1", "blocked", {
      blocker: { code: "something_new", message: "A thing went wrong.", clearsBy: "Continue.", attempt: 1 },
    });

    expect(blockerStatement(run)).toContain("A thing went wrong.");
  });

  it("GRU-FR-LBPR: a blocker written before it carried an attempt claims none", () => {
    const statement = blockerStatement(blocked("blocked", 0)) ?? "";

    expect(statement).not.toContain("first time");
    expect(statement).not.toContain("times in a row");
  });
});

describe("blockerDetail and blockerCode", () => {
  it("GRU-FR-LBPR: the backend's own account is available as the secondary line", () => {
    expect(blockerDetail(blocked("blocked", 1))).toBe(RAW);
    expect(blockerDetail(makeRun("r1", "working"))).toBeNull();
  });

  it("GRU-FR-LBPR: the typed code is read in both states that carry a blocker", () => {
    expect(blockerCode(blocked("blocked", 1))).toBe("review_checkout_failed");
    expect(blockerCode(blocked("awaiting_author", 2))).toBe("review_checkout_failed");
    expect(blockerCode(makeRun("r1", "working"))).toBeNull();
  });
});
