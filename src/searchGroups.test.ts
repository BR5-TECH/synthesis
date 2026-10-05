import { describe, expect, it } from "vitest";
import { groupHits, routeForHit, subtypeOf } from "./searchGroups";
import { hit, resetHitOrdinals } from "./test/searchEvents";
import type { SearchHit } from "./types";

// SCH-FR-03 / SCH-FR-04 / SCH-FR-09: the grouping and the click-through routing
// both surfaces share. Kept pure and tested here rather than through a render,
// so a drift between the overlay's answer and the full page's is impossible
// rather than merely unlikely.

describe("groupHits (SCH-FR-03 / SCH-FR-04 / SCH-FR-16)", () => {
  it("SCH-FR-03: renders seven groups, with Artifacts split by subtype", () => {
    resetHitOrdinals();
    const groups = groupHits([
      hit({ path: "a.skill.md", group: "artifact", subtype: "skill" }),
      hit({ path: "b.spec.md", group: "artifact", subtype: "spec" }),
      hit({ path: ".synthesis/playbooks/p.md", group: "playbook" }),
      hit({ path: ".synthesis/workstreams/w.md", group: "workstream" }),
      hit({ path: ".synthesis/roles/r.md", group: "role" }),
      hit({ path: "src/main.rs", group: "file" }),
    ]);

    expect(groups.map((g) => g.title)).toEqual([
      "Artifacts",
      "Playbooks",
      "Workstreams",
      "Roles",
      "Files",
    ]);
    const artifacts = groups[0];
    expect(artifacts.subgroups?.map((s) => s.title)).toEqual(["Skill", "Spec"]);
    // Non-artifact groups are flat.
    expect(groups[1].subgroups).toBeUndefined();
  });

  it("SCH-FR-03: the Artifacts subtypes are exactly the eight built-in types", () => {
    resetHitOrdinals();
    const eight = [
      "skill",
      "agent",
      "prompt",
      "spec",
      "flow",
      "instructions",
      "scenario",
      "scratchpad",
    ] as const;
    const groups = groupHits(
      eight.map((subtype, i) =>
        hit({ path: `${subtype}-${i}.md`, group: "artifact", subtype }),
      ),
    );
    expect(groups[0].subgroups).toHaveLength(8);
    // In the fixed order of ASC-FR-02, not the order the hits arrived in.
    expect(groups[0].subgroups?.map((s) => s.title)).toEqual([
      "Skill",
      "Agent",
      "Prompt",
      "Spec",
      "Flow",
      "Instructions",
      "Scenario",
      "Scratchpad",
    ]);
  });

  it("SCH-FR-04: an empty group is omitted rather than rendered as a bare header", () => {
    resetHitOrdinals();
    const groups = groupHits([hit({ path: "src/main.rs", group: "file" })]);
    expect(groups).toHaveLength(1);
    expect(groups[0].title).toBe("Files");
  });

  it("SCC-FR-09: the run and history groups are carried by the contract but never render", () => {
    // Neither is ever populated in v1, and an empty group is hidden — so a
    // consumer renders them from the same shape without either arriving.
    expect(groupHits([])).toEqual([]);
    // And if one ever did arrive, it would still be rendered rather than lost.
    resetHitOrdinals();
    const groups = groupHits([hit({ path: "r", group: "run", matchKind: "name" })]);
    expect(groups.map((g) => g.title)).toEqual(["Runs"]);
  });

  it("SCH-FR-16: hits stay in ascending ordinal within every group and subgroup", () => {
    const out = groupHits([
      hit({ path: "z.md", ordinal: 9, group: "artifact", subtype: "skill" }),
      hit({ path: "a.md", ordinal: 1, group: "artifact", subtype: "skill" }),
      hit({ path: "m.md", ordinal: 5, group: "artifact", subtype: "skill" }),
      hit({ path: "f2.rs", ordinal: 8, group: "file" }),
      hit({ path: "f1.rs", ordinal: 2, group: "file" }),
    ]);
    expect(out[0].subgroups?.[0].hits.map((h) => h.ordinal)).toEqual([1, 5, 9]);
    expect(out[1].hits.map((h) => h.ordinal)).toEqual([2, 8]);
  });

  it("an artifact hit with an unrecognised subtype is grouped rather than dropped", () => {
    // Nothing produces one today (SCC-FR-08 only ever sets one of the eight),
    // but a result the user can see no trace of is worse than an odd heading.
    const groups = groupHits([
      {
        id: "x.md",
        name: "x.md",
        path: "x.md",
        ordinal: 0,
        group: "artifact",
        matchKind: "name",
      } as SearchHit,
    ]);
    expect(groups[0].subgroups?.map((s) => s.title)).toEqual(["Other"]);
    expect(groups[0].subgroups?.[0].hits).toHaveLength(1);
  });
});

describe("routeForHit (SCH-FR-09)", () => {
  it("SCH-FR-09: an artifact in standalone edit context routes to the Editor", () => {
    const route = routeForHit(
      hit({
        path: "a.skill.md",
        group: "artifact",
        subtype: "skill",
        editContext: "standalone",
      }),
    );
    expect(route.kind).toBe("editor");
  });

  it("an artifact in flow context, and a Flow, route to Flow", () => {
    expect(
      routeForHit(
        hit({
          path: "f.md",
          group: "artifact",
          subtype: "flow",
          editContext: "flow",
        }),
      ).kind,
    ).toBe("flow");
    // A Flow routes to Flow even if the edit context were somehow absent.
    expect(
      routeForHit(hit({ path: "f.md", group: "artifact", subtype: "flow" })).kind,
    ).toBe("flow");
  });

  it("SCH-FR-09: a history match routes to the History viewer, and a run to Runs", () => {
    expect(routeForHit(hit({ path: "h", group: "history" })).kind).toBe("history");
    expect(routeForHit(hit({ path: "r", group: "run" })).kind).toBe("runs");
  });

  it("a workstream routes to the Library filtered to it", () => {
    expect(
      routeForHit(hit({ path: ".synthesis/workstreams/w.md", group: "workstream" }))
        .kind,
    ).toBe("workstream");
  });

  it("SCH-FR-09, ESH-FR-ATDS, TAB-FR-02: a Files result opens in the Editor, carrying no artifact type", () => {
    // ESH-FR-ATDS: it opens as a plain text file, which is exactly "in the Editor
    // with no type" — the absent type is what makes the Editor drop the
    // artifact-scoped actions.
    const file = hit({ path: "src/main.rs", group: "file" });
    expect(routeForHit(file).kind).toBe("editor");
    expect(subtypeOf(file)).toBeUndefined();
  });

  it("a playbook and a role open as content in the Editor", () => {
    expect(
      routeForHit(hit({ path: ".synthesis/playbooks/p.md", group: "playbook" })).kind,
    ).toBe("editor");
    expect(
      routeForHit(hit({ path: ".synthesis/roles/r.md", group: "role" })).kind,
    ).toBe("editor");
  });

  it("subtypeOf carries the type only for artifacts", () => {
    expect(
      subtypeOf(hit({ path: "a.md", group: "artifact", subtype: "spec" })),
    ).toBe("spec");
    // A non-artifact group carries no subtype even if one were somehow present.
    expect(
      subtypeOf({
        ...hit({ path: "p.md", group: "playbook" }),
        subtype: "spec",
      }),
    ).toBeUndefined();
  });
});
