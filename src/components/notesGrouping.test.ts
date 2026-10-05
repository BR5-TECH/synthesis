// The Notes panel's client-side view of a loaded note set: ordering
// (NTS-FR-15), grouping (NTS-FR-10) and filtering (NTS-FR-12). These pin the
// rules behind NTS-FR-10, NTS-FR-15 / NTS-FR-12 / NTS-FR-23, NTS-FR-22 at the level they are actually
// decided, so the rendering tests can stay about rendering.
import { describe, expect, it } from "vitest";

import type { NoteListItem } from "../types";
import {
  groupByEntity,
  groupKey,
  groupsForFilter,
  lastKnownPath,
  matchesFilter,
  sortMostRecentFirst,
} from "./notesGrouping";

function entityNote(
  id: string,
  entityId: string,
  body: string,
  updatedAt: string,
  opts: { unresolved?: boolean; name?: string } = {},
): NoteListItem {
  return {
    note: {
      id,
      scope: { kind: "entity", entityId, entityPath: entityId },
      body,
      createdAt: "2026-01-01T00:00:00Z",
      updatedAt,
    },
    entityName: opts.unresolved
      ? undefined
      : (opts.name ?? entityId.split("/").pop()),
    unresolved: !!opts.unresolved,
  };
}

function projectNote(id: string, body: string, updatedAt: string): NoteListItem {
  return {
    note: {
      id,
      scope: { kind: "project" },
      body,
      createdAt: "2026-01-01T00:00:00Z",
      updatedAt,
    },
    unresolved: false,
  };
}

describe("sortMostRecentFirst (NTS-FR-15)", () => {
  it("orders by last-edited time, most recent first", () => {
    const items = [
      projectNote("a", "A", "2026-03-01T00:00:00Z"),
      projectNote("b", "B", "2026-05-01T00:00:00Z"),
      projectNote("c", "C", "2026-04-01T00:00:00Z"),
    ];
    expect(sortMostRecentFirst(items).map((i) => i.note.id)).toEqual([
      "b",
      "c",
      "a",
    ]);
  });

  it("breaks an updatedAt tie the same way the backend does", () => {
    // `updated_at` is stored to whole-second resolution, so ties are ordinary.
    // The two layers must agree, or a row jumps on reload for no reason the
    // user did — this mirrors `notes.rs::sort_most_recent_first`.
    const older = projectNote("a", "A", "2026-03-01T00:00:00Z");
    older.note.createdAt = "2026-01-01T00:00:00Z";
    const newer = projectNote("b", "B", "2026-03-01T00:00:00Z");
    newer.note.createdAt = "2026-02-01T00:00:00Z";
    expect(sortMostRecentFirst([older, newer]).map((i) => i.note.id)).toEqual([
      "b",
      "a",
    ]);
  });

  it("breaks a tie deterministically and leaves the input untouched", () => {
    const items = [
      projectNote("z", "Z", "2026-03-01T00:00:00Z"),
      projectNote("a", "A", "2026-03-01T00:00:00Z"),
    ];
    expect(sortMostRecentFirst(items).map((i) => i.note.id)).toEqual(["a", "z"]);
    expect(items.map((i) => i.note.id)).toEqual(["z", "a"]);
  });
});

describe("groupByEntity (NTS-FR-10)", () => {
  it("makes one group per entity plus Project and Unresolved", () => {
    const groups = groupByEntity([
      entityNote("1", "specs/a.md", "on A", "2026-05-01T00:00:00Z"),
      entityNote("2", "specs/b.md", "on B", "2026-04-01T00:00:00Z"),
      entityNote("3", "specs/a.md", "also A", "2026-03-01T00:00:00Z"),
      projectNote("4", "project-wide", "2026-02-01T00:00:00Z"),
      entityNote("5", "gone.md", "orphan", "2026-01-01T00:00:00Z", {
        unresolved: true,
      }),
    ]);
    expect(groups.map((g) => g.label)).toEqual([
      "a.md",
      "b.md",
      "Project",
      "Unresolved",
    ]);
    // Two notes on the same entity share one group, most-recent first.
    expect(groups[0].items.map((i) => i.note.id)).toEqual(["1", "3"]);
    expect(groups[0].key).toEqual({
      kind: "entity",
      entityId: "specs/a.md",
      name: "a.md",
    });
    expect(groups[2].key.kind).toBe("project");
    expect(groups[3].key.kind).toBe("unresolved");
  });

  it("orders groups by their most-recently-edited note, whatever the input order", () => {
    const groups = groupByEntity([
      entityNote("1", "a.md", "old", "2026-01-01T00:00:00Z"),
      projectNote("2", "newest", "2026-09-01T00:00:00Z"),
      entityNote("3", "b.md", "middle", "2026-05-01T00:00:00Z"),
    ]);
    expect(groups.map((g) => g.label)).toEqual(["Project", "b.md", "a.md"]);
  });

  it("keys two entities sharing a basename apart", () => {
    // The label is what the header renders and it collides; the key must not,
    // or React reconciles two distinct groups as one.
    const groups = groupByEntity([
      entityNote("1", "specs/README.md", "spec", "2026-05-02T00:00:00Z"),
      entityNote("2", "docs/README.md", "docs", "2026-05-01T00:00:00Z"),
    ]);
    expect(groups).toHaveLength(2);
    expect(groups.map((g) => g.label)).toEqual(["README.md", "README.md"]);
    expect(groups.map(groupKey)).toEqual([
      "entity:specs/README.md",
      "entity:docs/README.md",
    ]);
    expect(new Set(groups.map(groupKey)).size).toBe(2);
  });

  it("gives the Project and Unresolved groups keys of their own", () => {
    const groups = groupByEntity([
      projectNote("1", "project", "2026-05-02T00:00:00Z"),
      entityNote("2", "gone.md", "orphan", "2026-05-01T00:00:00Z", {
        unresolved: true,
      }),
    ]);
    expect(groups.map(groupKey)).toEqual(["project", "unresolved"]);
  });

  it("keeps every unresolved note in one group whatever entity each named", () => {
    // NTS-FR-23: they are grouped by being unresolved, not by their dead paths.
    const groups = groupByEntity([
      entityNote("1", "gone/a.md", "one", "2026-05-01T00:00:00Z", {
        unresolved: true,
      }),
      entityNote("2", "gone/b.md", "two", "2026-04-01T00:00:00Z", {
        unresolved: true,
      }),
    ]);
    expect(groups).toHaveLength(1);
    expect(groups[0].label).toBe("Unresolved");
    expect(groups[0].items).toHaveLength(2);
  });
});

describe("matchesFilter (NTS-FR-12)", () => {
  const onA = entityNote("1", "specs/handoff.md", "check the numbering", "t");

  it("matches the body case-insensitively", () => {
    expect(matchesFilter(onA, "NUMBER", false)).toBe(true);
    expect(matchesFilter(onA, "numbering", false)).toBe(true);
    expect(matchesFilter(onA, "missing", false)).toBe(false);
  });

  it("matches the entity's name only where one is rendered", () => {
    expect(matchesFilter(onA, "handoff", true)).toBe(true);
    expect(matchesFilter(onA, "handoff", false)).toBe(false);
  });

  it("matches an unresolved note on its last-known path", () => {
    const orphan = entityNote("2", "specs/old.md", "body", "t", {
      unresolved: true,
    });
    expect(matchesFilter(orphan, "old.md", true)).toBe(true);
    expect(lastKnownPath(orphan)).toBe("specs/old.md");
  });

  it("keeps everything when the filter is empty or whitespace", () => {
    expect(matchesFilter(onA, "", true)).toBe(true);
    expect(matchesFilter(onA, "   ", true)).toBe(true);
  });
});

describe("groupsForFilter (NTS-FR-12)", () => {
  const items = [
    entityNote("1", "specs/a.md", "alpha note", "2026-05-01T00:00:00Z"),
    entityNote("2", "specs/b.md", "beta note", "2026-04-01T00:00:00Z"),
    projectNote("3", "gamma note", "2026-03-01T00:00:00Z"),
  ];

  it("hides a group left with no matching note", () => {
    const groups = groupsForFilter(items, "alpha");
    expect(groups.map((g) => g.label)).toEqual(["a.md"]);
    expect(groups[0].items).toHaveLength(1);
  });

  it("shows a group whose entity name matches even when no body does", () => {
    expect(groupsForFilter(items, "b.md").map((g) => g.label)).toEqual(["b.md"]);
  });

  it("returns no group at all when nothing matches", () => {
    expect(groupsForFilter(items, "delta")).toEqual([]);
  });
});
