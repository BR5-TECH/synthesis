import { describe, expect, it } from "vitest";

import { changedFacts, changedOnly, flowDiff } from "./flowDiff";
import type { FlowDiffEntry } from "./flowDiff";

type Obj = Record<string, unknown>;

/** A Flow body, from the parts a test cares about. */
function flow(doc: {
  name?: string;
  description?: string;
  references?: string[];
  items?: Obj[];
  edges?: Obj[];
  sync?: Obj;
}): string {
  return JSON.stringify({
    version: 2,
    name: doc.name ?? "F",
    ...(doc.description ? { description: doc.description } : {}),
    ...(doc.references ? { references: doc.references } : {}),
    items: doc.items ?? [],
    edges: doc.edges ?? [],
    ...(doc.sync ? { sync: doc.sync } : {}),
  });
}

const CONTAINERS = new Set(["deliverable", "milestone", "epic", "feature"]);

const it_ = (id: string, kind: string, name: string, extra: Obj = {}): Obj => ({
  id,
  kind,
  name,
  position: { x: 0, y: 0 },
  ...(CONTAINERS.has(kind) ? { size: { width: 400, height: 220 } } : {}),
  ...extra,
});

const task = (id: string, name: string, extra: Obj = {}) =>
  it_(id, "task", name, { parentId: "f1", ...extra });

const feature = it_("f1", "feature", "Capture", { parentId: "m1" });
const milestone = it_("m1", "milestone", "Beta", { parentId: "d1" });
const deliverable = it_("d1", "deliverable", "Payments");
const frame = [deliverable, milestone, feature];

const edge = (id: string, from: string, to: string, type = "blocks") => ({ id, from, to, type });

const issueLink = (itemId: string, number: number, baseName = "B") => ({
  itemId,
  ref: {
    kind: "issue",
    host: "github.com",
    owner: "acme",
    repo: "payments",
    number,
    url: `https://github.com/acme/payments/issues/${number}`,
  },
  base: {
    name: baseName,
    description: "",
    state: "open",
    type: null,
    parent: null,
    milestone: null,
    blockedBy: [],
  },
});

function entries(oldBody: string | null, newBody: string | null) {
  const result = flowDiff(oldBody, newBody);
  if (!result.ok) throw new Error(`expected a diff, got: ${result.error}`);
  return result.entries;
}

const byKey = (list: FlowDiffEntry[], key: string) => {
  const found = list.find((e) => e.key === key);
  if (!found) throw new Error(`no entry ${key}`);
  return found;
};

describe("flowDiff: items (DFV-FR-33, DFV-FR-RGBK)", () => {
  it("DFV-FR-33: an added and a removed item are the entries they are", () => {
    const list = entries(
      flow({ items: [...frame, task("t1", "Tokenize")] }),
      flow({ items: [...frame, task("t2", "Form")] }),
    );
    expect(byKey(list, "item:t1")).toMatchObject({
      kind: "item",
      itemKindBefore: "task",
      status: "removed",
      titleBefore: "Tokenize",
      titleAfter: null,
    });
    expect(byKey(list, "item:t2")).toMatchObject({
      status: "added",
      itemKindAfter: "task",
      titleBefore: null,
      titleAfter: "Form",
    });
    expect(byKey(list, "item:t2").itemKindBefore).toBeUndefined();
  });

  it("DFV-FR-33: items pair by id, so a rename is one changed item", () => {
    const list = entries(
      flow({ items: [...frame, task("t1", "Tokenize")] }),
      flow({ items: [...frame, task("t1", "Tokenize cards")] }),
    );
    expect(list.filter((e) => e.kind === "item")).toHaveLength(4);
    expect(byKey(list, "item:t1")).toMatchObject({
      status: "changed",
      facts: [{ label: "Name", status: "changed", before: "Tokenize", after: "Tokenize cards" }],
    });
  });

  it("DFV-FR-33: items pair by id however the two revisions order them", () => {
    const list = entries(
      flow({ items: [...frame, task("t1", "A"), task("t2", "B")] }),
      flow({ items: [task("t2", "B"), ...frame, task("t1", "A2")] }),
    );
    expect(list.filter((e) => e.status === "added" || e.status === "removed")).toEqual([]);
    expect(byKey(list, "item:t1").status).toBe("changed");
  });

  it("DFV-FR-RGBK: a changed kind states the old and the new kind", () => {
    const list = entries(
      flow({ items: [...frame, task("t1", "A")] }),
      flow({ items: [...frame, it_("t1", "defect", "A", { parentId: "f1" })] }),
    );
    const entry = byKey(list, "item:t1");
    expect(entry).toMatchObject({ itemKindBefore: "task", itemKindAfter: "defect" });
    expect(entry.facts).toEqual([
      { label: "Kind", status: "changed", before: "task", after: "defect" },
    ]);
  });

  it("DFV-FR-RGBK: a description added, changed, and removed", () => {
    const none = flow({ items: [...frame, task("t1", "A")] });
    const one = flow({ items: [...frame, task("t1", "A", { description: "one" })] });
    const two = flow({ items: [...frame, task("t1", "A", { description: "two" })] });
    expect(byKey(entries(none, one), "item:t1").facts).toEqual([
      { label: "Description", status: "added", after: "one" },
    ]);
    expect(byKey(entries(one, none), "item:t1").facts).toEqual([
      { label: "Description", status: "removed", before: "one" },
    ]);
    expect(byKey(entries(one, two), "item:t1").facts).toEqual([
      { label: "Description", status: "changed", before: "one", after: "two" },
    ]);
  });

  it("DFV-FR-RGBK: each reference gained and lost is its own fact", () => {
    const facts = byKey(
      entries(
        flow({ items: [...frame, task("t1", "A", { artifactIds: ["a.md", "kept.md"] })] }),
        flow({ items: [...frame, task("t1", "A", { artifactIds: ["kept.md", "b.md"] })] }),
      ),
      "item:t1",
    ).facts;
    expect(facts).toEqual([
      { label: "Reference", status: "unchanged", before: "kept.md", after: "kept.md" },
      { label: "Reference", status: "added", after: "b.md" },
      { label: "Reference", status: "removed", before: "a.md" },
    ]);
  });

  it("DFV-FR-RGBK: each footprint entry gained and lost is its own fact", () => {
    const facts = byKey(
      entries(
        flow({ items: [...frame, task("t1", "A", { footprint: ["src/a/"] })] }),
        flow({ items: [...frame, task("t1", "A", { footprint: ["src/b/"] })] }),
      ),
      "item:t1",
    ).facts;
    expect(facts).toEqual([
      { label: "Footprint", status: "added", after: "src/b/" },
      { label: "Footprint", status: "removed", before: "src/a/" },
    ]);
  });

  it("DFV-FR-RGBK: the draft link, the due date and the local-only flag are facts", () => {
    const list = entries(
      flow({ items: [deliverable, it_("m1", "milestone", "Beta", { parentId: "d1" }), feature, task("t1", "A", { draftId: "dr1" })] }),
      flow({
        items: [
          deliverable,
          it_("m1", "milestone", "Beta", { parentId: "d1", due: "2027-03-31" }),
          feature,
          task("t1", "A", { draftId: "dr2", localOnly: true }),
        ],
      }),
    );
    expect(byKey(list, "item:m1").facts).toEqual([
      { label: "Due", status: "added", after: "2027-03-31" },
    ]);
    expect(byKey(list, "item:t1").facts).toEqual([
      { label: "Draft", status: "changed", before: "dr1", after: "dr2" },
      { label: "Local only", status: "added", after: "Yes" },
    ]);
  });

  it("DFV-FR-RGBK: the tracker link is a fact, and the sync base is not compared", () => {
    const linked = flow({
      items: [...frame, task("t1", "A")],
      sync: { tracker: "github", links: [issueLink("t1", 42, "Old base")] },
    });
    const rebased = flow({
      items: [...frame, task("t1", "A")],
      sync: { tracker: "github", links: [issueLink("t1", 42, "New base")] },
    });
    const relinked = flow({
      items: [...frame, task("t1", "A")],
      sync: { tracker: "github", links: [issueLink("t1", 43)] },
    });
    const unlinked = flow({ items: [...frame, task("t1", "A")] });
    expect(changedOnly(entries(linked, rebased))).toEqual([]);
    expect(byKey(entries(linked, relinked), "item:t1").facts).toEqual([
      { label: "Tracker link", status: "changed", before: "acme/payments#42", after: "acme/payments#43" },
    ]);
    expect(byKey(entries(unlinked, linked), "item:t1").facts).toEqual([
      { label: "Tracker link", status: "added", after: "acme/payments#42" },
    ]);
  });

  it("DFV-FR-RGBK: a milestone link and a link on another host name themselves", () => {
    const milestoneLink = {
      itemId: "m1",
      ref: { kind: "milestone", host: "github.com", owner: "acme", repo: "payments", number: 3, url: "u" },
      base: { name: "Beta", description: "", state: "open", due: null },
    };
    const enterprise = issueLink("t1", 9);
    enterprise.ref.host = "git.acme.dev";
    const list = entries(
      null,
      flow({
        items: [...frame, task("t1", "A")],
        sync: { tracker: "github", links: [milestoneLink, enterprise] },
      }),
    );
    expect(byKey(list, "item:m1").facts).toContainEqual({
      label: "Tracker link",
      status: "unchanged",
      after: "acme/payments milestone 3",
    });
    expect(byKey(list, "item:t1").facts).toContainEqual({
      label: "Tracker link",
      status: "unchanged",
      after: "git.acme.dev/acme/payments#9",
    });
  });

  it("DFV-FR-RGBK: the container is named by name, ahead of position and size, which come last", () => {
    const list = entries(
      flow({ items: [...frame, it_("f2", "feature", "Refunds", { parentId: "m1" }), task("t1", "A")] }),
      flow({
        items: [
          ...frame,
          it_("f2", "feature", "Refunds", { parentId: "m1", size: { width: 500, height: 220 } }),
          task("t1", "A", { parentId: "f2", position: { x: 5, y: 6 }, description: "d" }),
        ],
      }),
    );
    expect(byKey(list, "item:t1").facts).toEqual([
      { label: "Description", status: "added", after: "d" },
      { label: "Container", status: "changed", before: "Capture", after: "Refunds" },
      { label: "Position", status: "changed", before: "0, 0", after: "5, 6" },
    ]);
    expect(byKey(list, "item:f2").facts).toEqual([
      { label: "Size", status: "changed", before: "400 × 220", after: "500 × 220" },
    ]);
  });

  it("DFV-FR-RGBK: a move to the top level names the top level", () => {
    const list = entries(
      flow({ items: [deliverable, it_("d2", "deliverable", "Other"), it_("m1", "milestone", "Beta", { parentId: "d1" })] }),
      flow({ items: [deliverable, it_("d2", "deliverable", "Other"), it_("m1", "milestone", "Beta", { parentId: "d2" })] }),
    );
    expect(byKey(list, "item:m1").facts).toEqual([
      { label: "Container", status: "changed", before: "Payments", after: "Other" },
    ]);
  });

  it("DFV-FR-RGBK: a deliverable that loses its parent names the top level", () => {
    // A deliverable never has a parent in a valid plan; the diff reads any revision.
    const list = entries(
      flow({ items: [deliverable, it_("d2", "deliverable", "Other", { parentId: "d1" })] }),
      flow({ items: [deliverable, it_("d2", "deliverable", "Other")] }),
    );
    expect(byKey(list, "item:d2").facts).toEqual([
      { label: "Container", status: "changed", before: "Payments", after: "(top level)" },
    ]);
  });

  it("DFV-FR-RGBK: a leaf that becomes a container states its kind, its lost draft, and its new size", () => {
    const list = entries(
      flow({ items: [...frame, it_("x1", "defect", "Bug", { parentId: "m1", draftId: "dr1" })] }),
      flow({ items: [...frame, it_("x1", "feature", "Bug", { parentId: "m1" })] }),
    );
    expect(byKey(list, "item:x1").facts).toEqual([
      { label: "Kind", status: "changed", before: "defect", after: "feature" },
      { label: "Draft", status: "removed", before: "dr1" },
      { label: "Size", status: "added", after: "400 × 220" },
    ]);
  });

  it("DFV-FR-RGBK: a due date changed and removed, and a local-only flag removed", () => {
    const due = (d?: string) => it_("m1", "milestone", "Beta", { parentId: "d1", ...(d ? { due: d } : {}) });
    expect(byKey(entries(flow({ items: [deliverable, due("2027-01-01")] }), flow({ items: [deliverable, due("2027-02-01")] })), "item:m1").facts).toEqual([
      { label: "Due", status: "changed", before: "2027-01-01", after: "2027-02-01" },
    ]);
    expect(byKey(entries(flow({ items: [deliverable, due("2027-01-01")] }), flow({ items: [deliverable, due()] })), "item:m1").facts).toEqual([
      { label: "Due", status: "removed", before: "2027-01-01" },
    ]);
    expect(
      byKey(entries(flow({ items: [it_("d1", "deliverable", "P", { localOnly: true })] }), flow({ items: [deliverable] })), "item:d1").facts,
    ).toContainEqual({ label: "Local only", status: "removed", before: "Yes" });
  });

  it("DFV-FR-33: a revision with two items of one id or two edges on one pair is reported, not paired", () => {
    const twice = flow({ items: [deliverable, it_("d1", "deliverable", "Again")] });
    const r = flowDiff(twice, flow({}));
    expect(r.ok ? "" : r.error).toMatch(/^old revision: more than one item carries the id "d1"/);
    const items = [...frame, task("t1", "A"), task("t2", "B")];
    const pair = flow({ items, edges: [edge("e1", "t1", "t2"), edge("e2", "t2", "t1", "relates")] });
    const p = flowDiff(flow({ items }), pair);
    expect(p.ok ? "" : p.error).toMatch(/^new revision: more than one edge joins "t2" and "t1"/);
  });

    it("DFV-FR-33: an added item states its own facts unmarked, on its own side", () => {
    const list = entries(
      flow({ items: frame }),
      flow({
        items: [
          ...frame,
          task("t1", "A", { description: "d", artifactIds: ["a.md"], footprint: ["src/"], draftId: "dr1" }),
        ],
      }),
    );
    expect(byKey(list, "item:t1").facts).toEqual([
      { label: "Description", status: "unchanged", after: "d" },
      { label: "Reference", status: "unchanged", after: "a.md" },
      { label: "Footprint", status: "unchanged", after: "src/" },
      { label: "Draft", status: "unchanged", after: "dr1" },
      { label: "Container", status: "unchanged", after: "Capture" },
    ]);
  });

  it("DFV-FR-35: a removed item states its own facts on the side it exists on", () => {
    const list = entries(
      flow({ items: [...frame, task("t1", "A", { description: "d" })] }),
      flow({ items: frame }),
    );
    expect(byKey(list, "item:t1").facts).toEqual([
      { label: "Description", status: "unchanged", before: "d" },
      { label: "Container", status: "unchanged", before: "Capture" },
    ]);
    // A top-level item has no container fact.
    expect(byKey(entries(flow({ items: [deliverable] }), flow({})), "item:d1").facts).toEqual([]);
  });

  it("DFV-FR-33: a revision that is not a version 2 Flow is reported, not compared", () => {
    const v1 = JSON.stringify({ version: 1, nodes: [], edges: [] });
    const r = flowDiff(v1, flow({}));
    expect(r.ok).toBe(false);
    if (!r.ok) expect(r.error).toMatch(/^old revision: unknown Flow version 1/);
    const n = flowDiff(flow({}), "{ not json");
    if (!n.ok) expect(n.error).toMatch(/^new revision: not valid JSON/);
    expect(n.ok).toBe(false);
  });
});

describe("flowDiff: edges (DFV-FR-33, DFV-FR-RGBK)", () => {
  const items = [...frame, task("t1", "Tokenize"), task("t2", "Form")];

  it("DFV-FR-33: an edge pairs by its two endpoints, in either order", () => {
    const list = entries(
      flow({ items, edges: [edge("e1", "t1", "t2", "relates")] }),
      flow({ items, edges: [edge("e9", "t2", "t1", "relates")] }),
    );
    const edges = list.filter((e) => e.kind === "edge");
    expect(edges).toHaveLength(1);
    expect(edges[0]).toMatchObject({ key: "edge:t1 t2", status: "unchanged" });
  });

  it("DFV-FR-RGBK: a reversed blocks edge states its direction", () => {
    const list = entries(
      flow({ items, edges: [edge("e1", "t1", "t2")] }),
      flow({ items, edges: [edge("e1", "t2", "t1")] }),
    );
    expect(byKey(list, "edge:t1 t2")).toMatchObject({
      status: "changed",
      titleBefore: "Tokenize → Form",
      titleAfter: "Form → Tokenize",
      facts: [
        { label: "Direction", status: "changed", before: "Tokenize → Form", after: "Form → Tokenize" },
      ],
    });
  });

  it("DFV-FR-RGBK: a changed type states the old and the new type", () => {
    const list = entries(
      flow({ items, edges: [edge("e1", "t1", "t2")] }),
      flow({ items, edges: [edge("e1", "t1", "t2", "relates")] }),
    );
    expect(byKey(list, "edge:t1 t2")).toMatchObject({
      titleAfter: "Tokenize — Form",
      facts: [{ label: "Type", status: "changed", before: "blocks", after: "relates" }],
    });
  });

  it("DFV-FR-33: an added and a removed edge state their type on their own side", () => {
    const list = entries(
      flow({ items, edges: [edge("e1", "t1", "t2")] }),
      flow({ items, edges: [edge("e2", "f1", "t2", "relates")] }),
    );
    expect(byKey(list, "edge:t1 t2")).toMatchObject({
      status: "removed",
      titleAfter: null,
      facts: [{ label: "Type", status: "unchanged", before: "blocks" }],
    });
    // Endpoints sort as text, so `f1` comes before `t2` in the key.
    expect(byKey(list, "edge:f1 t2")).toMatchObject({
      status: "added",
      titleBefore: null,
      titleAfter: "Capture — Form",
      facts: [{ label: "Type", status: "unchanged", after: "relates" }],
    });
  });

  it("DFV-FR-33, DFV-FR-35: an edge whose item was renamed is unchanged, and each side names it its own way", () => {
    const list = entries(
      flow({ items, edges: [edge("e1", "t1", "t2")] }),
      flow({ items: [...frame, task("t1", "Tokenize cards"), task("t2", "Form")], edges: [edge("e1", "t1", "t2")] }),
    );
    expect(byKey(list, "edge:t1 t2")).toMatchObject({
      status: "unchanged",
      titleBefore: "Tokenize → Form",
      titleAfter: "Tokenize cards → Form",
      facts: [],
    });
  });

  it("DFV-FR-RGBK: a relates edge that becomes blocks states the type, and no direction", () => {
    const list = entries(
      flow({ items, edges: [edge("e1", "t1", "t2", "relates")] }),
      flow({ items, edges: [edge("e1", "t2", "t1")] }),
    );
    expect(byKey(list, "edge:t1 t2").facts).toEqual([
      { label: "Type", status: "changed", before: "relates", after: "blocks" },
    ]);
  });

    it("DFV-FR-33: an edge is named by item id when the item has no name", () => {
    const list = entries(
      flow({ items: [...frame, task("t1", ""), task("t2", "Form")] }),
      flow({ items: [...frame, task("t1", ""), task("t2", "Form")], edges: [edge("e1", "t1", "t2")] }),
    );
    expect(byKey(list, "edge:t1 t2").titleAfter).toBe("t1 → Form");
  });
});

describe("flowDiff: the Flow's own fields (DFV-FR-33, DFV-FR-RGBK)", () => {
  it("DFV-FR-RGBK: the name, the description and each reference are facts", () => {
    const list = entries(
      flow({ name: "Old", references: ["hld.md", "kept.md"] }),
      flow({ name: "New", description: "D", references: ["kept.md", "adr.md"] }),
    );
    expect(byKey(list, "flow")).toMatchObject({
      status: "changed",
      facts: [
        { label: "Name", status: "changed", before: "Old", after: "New" },
        { label: "Description", status: "added", after: "D" },
        { label: "Reference", status: "unchanged", before: "kept.md", after: "kept.md" },
        { label: "Reference", status: "added", after: "adr.md" },
        { label: "Reference", status: "removed", before: "hld.md" },
      ],
    });
  });

  it("DFV-FR-RGBK: a Flow name that is cleared is a removed fact", () => {
    expect(byKey(entries(flow({ name: "Old" }), flow({ name: "" })), "flow").facts).toEqual([
      { label: "Name", status: "removed", before: "Old" },
    ]);
  });

    it("DFV-FR-33: a missing revision reads as an empty plan, and the Flow's entry goes with the file", () => {
    const added = entries(null, flow({ name: "New", items: [deliverable] }));
    expect(byKey(added, "flow")).toMatchObject({
      status: "added",
      titleBefore: null,
      titleAfter: "Flow",
      facts: [{ label: "Name", status: "unchanged", after: "New" }],
    });
    expect(byKey(added, "item:d1").status).toBe("added");
    const removed = entries(flow({ name: "Old" }), null);
    expect(byKey(removed, "flow")).toMatchObject({
      status: "removed",
      titleAfter: null,
      facts: [{ label: "Name", status: "unchanged", before: "Old" }],
    });
  });

  it("DFV-FR-33: a change of formatting alone changes nothing", () => {
    const doc = {
      version: 2,
      name: "F",
      items: [...frame, task("t1", "A", { footprint: ["src/"] }), task("t2", "B")],
      edges: [edge("e1", "t1", "t2", "relates")],
    };
    const compact = JSON.stringify(doc);
    const pretty = JSON.stringify({ edges: doc.edges, items: doc.items, name: "F", version: 2 }, null, 4);
    expect(changedOnly(entries(compact, pretty))).toEqual([]);
  });
});

describe("what each mode shows (DFV-FR-34)", () => {
  const before = flow({ items: [...frame, task("t1", "A", { description: "keep" })] });
  const after = flow({
    items: [...frame, task("t1", "A", { description: "keep", artifactIds: ["new.md"] })],
  });

  it("DFV-FR-34: unified shows only what changed, and only the facts that differ", () => {
    const list = entries(before, after);
    expect(list.map((e) => e.key)).toEqual(["flow", "item:d1", "item:m1", "item:f1", "item:t1"]);
    const changed = changedOnly(list);
    expect(changed.map((e) => e.key)).toEqual(["item:t1"]);
    expect(changedFacts(changed[0])).toEqual([
      { label: "Reference", status: "added", after: "new.md" },
    ]);
  });

  it("DFV-FR-34: an added or removed entry keeps every fact", () => {
    const list = entries(flow({ items: frame }), flow({ items: [...frame, task("t1", "A", { description: "p" })] }));
    const added = byKey(list, "item:t1");
    expect(changedFacts(added)).toEqual(added.facts);
  });

  it("DFV-FR-34: a removed item keeps the place its old neighbours gave it", () => {
    const list = entries(
      flow({ items: [...frame, task("t1", "A"), task("t2", "B"), task("t3", "C")] }),
      flow({ items: [...frame, task("t1", "A"), task("t3", "C")] }),
    );
    expect(list.filter((e) => e.kind === "item").map((e) => e.key)).toEqual([
      "item:d1",
      "item:m1",
      "item:f1",
      "item:t1",
      "item:t2",
      "item:t3",
    ]);
  });
});
