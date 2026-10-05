import { beforeEach, describe, expect, it } from "vitest";
import {
  chooseIteration,
  forgetEverything,
  forgetRunsExcept,
  recallChoice,
  recallFilters,
  recallRun,
  rememberFilters,
  rememberRun,
  type Choice,
} from "./graduationSelection";

/**
 * What the graduation section remembers between renders of itself
 * (GRH-FR-ODLT, GRU-FR-YYXN).
 *
 * Tested directly rather than only through the component, because the whole
 * point of this module is that it outlives the component: a bug here is a
 * reading position quietly lost, or one project's positions destroyed by
 * opening another, and neither shows up as a render that looks wrong.
 */

const choice = (iteration: number, over: Partial<Choice> = {}): Choice => ({
  // GRU-FR-YYXN: a row is identified by its part and its iteration together, the
  // two parts counting their passes separately.
  row: `specification:${iteration}`,
  follows: false,
  knownAccounted: [],
  ...over,
});

beforeEach(() => forgetEverything());

describe("the run each project was left reading (GRH-FR-ODLT)", () => {
  it("recalls nothing for a project it has never been told about", () => {
    expect(recallRun("/dev/acme")).toBeNull();
  });

  it("holds one run per project, independently", () => {
    rememberRun("/dev/acme", "run-a");
    rememberRun("/dev/notes", "run-b");
    expect(recallRun("/dev/acme")).toBe("run-a");
    expect(recallRun("/dev/notes")).toBe("run-b");
  });

  it("takes the latest run told for a project", () => {
    rememberRun("/dev/acme", "run-a");
    rememberRun("/dev/acme", "run-c");
    expect(recallRun("/dev/acme")).toBe("run-c");
  });
});

describe("the pass each run was left on (GRU-FR-YYXN)", () => {
  it("recalls nothing for a run nobody has chosen on", () => {
    expect(recallChoice("/dev/acme", "run-a")).toBeNull();
  });

  it("holds a choice per run, and never reads one run's for another", () => {
    chooseIteration("/dev/acme", "run-a", choice(3));
    chooseIteration("/dev/acme", "run-b", choice(1, { follows: true }));
    expect(recallChoice("/dev/acme", "run-a")?.row).toBe("specification:3");
    expect(recallChoice("/dev/acme", "run-b")?.row).toBe("specification:1");
    expect(recallChoice("/dev/acme", "run-b")?.follows).toBe(true);
  });

  it("holds a closed history as a choice of its own", () => {
    // Closing the open row is a reading position, not the absence of one: a
    // null read back as "nothing chosen" would reopen the newest pass on the
    // next render, which is the surface undoing what the author did.
    chooseIteration("/dev/acme", "run-a", choice(3));
    chooseIteration("/dev/acme", "run-a", {
      row: null,
      follows: false,
      knownAccounted: ["specification:1", "specification:2", "specification:3"],
    });
    const held = recallChoice("/dev/acme", "run-a");
    expect(held).not.toBeNull();
    expect(held?.row).toBeNull();
    expect(held?.knownAccounted).toEqual(["specification:1", "specification:2", "specification:3"]);
  });

  it("keeps what was already accounted for when the choice was made", () => {
    chooseIteration(
      "/dev/acme",
      "run-a",
      choice(1, { knownAccounted: ["specification:1", "specification:2"] }),
    );
    expect(recallChoice("/dev/acme", "run-a")?.knownAccounted).toEqual(["specification:1", "specification:2"]);
  });

  it("holds the same run id in two projects separately", () => {
    // Run ids are opaque and generated per project; nothing guarantees they
    // differ across projects, and reading one project's for another would be
    // wrong even if they happened to.
    chooseIteration("/dev/acme", "run-1", choice(4));
    chooseIteration("/dev/notes", "run-1", choice(2));
    expect(recallChoice("/dev/acme", "run-1")?.row).toBe("specification:4");
    expect(recallChoice("/dev/notes", "run-1")?.row).toBe("specification:2");
  });
});

describe("what a listing forgets (GRU-FR-YYXN)", () => {
  it("drops a run the listing no longer holds, and keeps the rest", () => {
    chooseIteration("/dev/acme", "gone", choice(1));
    chooseIteration("/dev/acme", "still-here", choice(2));
    forgetRunsExcept("/dev/acme", ["still-here"]);
    expect(recallChoice("/dev/acme", "gone")).toBeNull();
    expect(recallChoice("/dev/acme", "still-here")?.row).toBe("specification:2");
  });

  it("GRH-FR-ODLT: another project's listing drops nothing of this one's", () => {
    // The section lists one project's queue at a time, so what a listing
    // evidences is only about that project's runs. A run of `/dev/acme` is
    // absent from `/dev/notes`'s queue because the author changed project —
    // not because it left the queue — and a position held for the life of the
    // running application must survive that.
    chooseIteration("/dev/acme", "run-a", choice(3));
    forgetRunsExcept("/dev/notes", ["run-x", "run-y"]);
    expect(recallChoice("/dev/acme", "run-a")?.row).toBe("specification:3");
  });

  it("forgets nothing for a project it holds nothing for", () => {
    expect(() => forgetRunsExcept("/dev/unknown", ["run-a"])).not.toThrow();
  });

  it("drops everything when the listing is empty", () => {
    chooseIteration("/dev/acme", "run-a", choice(1));
    forgetRunsExcept("/dev/acme", []);
    expect(recallChoice("/dev/acme", "run-a")).toBeNull();
  });

  it("leaves the run each project was left reading alone", () => {
    // A run that has left the queue is still the run the author was last on;
    // the section decides for itself that it is no longer selectable, and
    // dropping it here would be this module deciding that instead.
    rememberRun("/dev/acme", "gone");
    chooseIteration("/dev/acme", "gone", choice(1));
    forgetRunsExcept("/dev/acme", ["other"]);
    expect(recallRun("/dev/acme")).toBe("gone");
    expect(recallChoice("/dev/acme", "gone")).toBeNull();
  });
});

describe("the test seam", () => {
  it("drops both of the things it holds", () => {
    rememberRun("/dev/acme", "run-a");
    chooseIteration("/dev/acme", "run-a", choice(1));
    forgetEverything();
    expect(recallRun("/dev/acme")).toBeNull();
    expect(recallChoice("/dev/acme", "run-a")).toBeNull();
  });
});

describe("the rail's filters, per project (GRH-FR-QVEX)", () => {
  it("recalls nothing for a project that has not been narrowed", () => {
    expect(recallFilters("/dev/acme")).toBeNull();
  });

  it("holds each project's view and text apart from every other's", () => {
    // GRH-FR-QVEX: one project's filters are never read for another, on the same
    // footing the selection is held on (GRH-FR-ODLT).
    rememberFilters("/dev/acme", { view: "archived", text: "overlay" });
    rememberFilters("/dev/other", { view: "in-flight", text: "" });

    expect(recallFilters("/dev/acme")).toEqual({
      view: "archived",
      text: "overlay",
    });
    expect(recallFilters("/dev/other")).toEqual({ view: "in-flight", text: "" });
    expect(recallFilters("/dev/never-opened")).toBeNull();
  });

  it("replaces what a project was narrowed by rather than merging it", () => {
    rememberFilters("/dev/acme", { view: "archived", text: "overlay" });
    rememberFilters("/dev/acme", { view: "all", text: "" });
    expect(recallFilters("/dev/acme")).toEqual({ view: "all", text: "" });
  });

  it("is dropped by the test seam like everything else held here", () => {
    // It is memory rather than storage: nothing here survives a relaunch.
    rememberFilters("/dev/acme", { view: "all", text: "x" });
    forgetEverything();
    expect(recallFilters("/dev/acme")).toBeNull();
  });

  it("is not touched by pruning a project's runs", () => {
    // GRU-FR-YYXN's pruning is about which runs are still listed; a filter is
    // about what the author is looking at and outlives any of them.
    rememberFilters("/dev/acme", { view: "all", text: "x" });
    forgetRunsExcept("/dev/acme", []);
    expect(recallFilters("/dev/acme")).toEqual({ view: "all", text: "x" });
  });
});
