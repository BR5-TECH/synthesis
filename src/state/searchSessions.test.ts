import { describe, expect, it } from "vitest";
import { SearchSessionStore } from "./searchSessions";
import { hit, resetHitOrdinals } from "../test/searchEvents";

// SCH-FR-11: the finished result set of a Search results tab lives outside the
// component, so an inactive tab (which the viewport unmounts) keeps its answer.
// The store's lifecycle is what decides when a tab re-searches and when it does
// not, so each transition is pinned here rather than only through a render.

function session(paths: string[]) {
  resetHitOrdinals();
  return {
    hits: paths.map((path) => hit({ path, group: "file" as const })),
    reason: "completed" as const,
    error: null,
  };
}

describe("SearchSessionStore (SCH-FR-11)", () => {
  it("returns undefined for a tab it has never recorded", () => {
    expect(new SearchSessionStore().get("search:1")).toBeUndefined();
  });

  it("returns the recorded result set for a tab", () => {
    const store = new SearchSessionStore();
    store.save("search:1", session(["src/a.rs", "src/b.rs"]));
    expect(store.get("search:1")?.hits.map((h) => h.path)).toEqual([
      "src/a.rs",
      "src/b.rs",
    ]);
    expect(store.get("search:1")?.reason).toBe("completed");
  });

  it("keeps tabs independent, so one query's answer is never served to another", () => {
    const store = new SearchSessionStore();
    store.save("search:literal_insensitive:needle", session(["src/a.rs"]));
    store.save("search:regex:need.e", session(["src/b.rs"]));
    expect(store.get("search:literal_insensitive:needle")?.hits[0].path).toBe(
      "src/a.rs",
    );
    expect(store.get("search:regex:need.e")?.hits[0].path).toBe("src/b.rs");
  });

  it("a later save replaces the tab's answer rather than accumulating", () => {
    const store = new SearchSessionStore();
    store.save("search:1", session(["src/old.rs"]));
    store.save("search:1", session(["src/new.rs"]));
    expect(store.get("search:1")?.hits.map((h) => h.path)).toEqual(["src/new.rs"]);
  });

  it("SCH-FR-20: dropping a tab forgets its answer, so reopening re-searches", () => {
    const store = new SearchSessionStore();
    store.save("search:1", session(["src/a.rs"]));
    store.drop("search:1");
    expect(store.get("search:1")).toBeUndefined();
    // Dropping a tab that recorded nothing is a no-op, not an error — a tab
    // closed mid-sweep never saved anything.
    store.drop("search:never-saved");
  });

  it("TAB-FR-14: clearing forgets every tab, because the paths named the old root", () => {
    const store = new SearchSessionStore();
    store.save("search:1", session(["src/a.rs"]));
    store.save("search:2", session(["src/b.rs"]));

    store.clear();

    expect(store.get("search:1")).toBeUndefined();
    expect(store.get("search:2")).toBeUndefined();
  });

  it("records an error outcome, so a tab does not silently re-run an uncompilable pattern", () => {
    // SCH-FR-21: the error is the terminal answer to that query, and resuming
    // it is what keeps the tab from re-dispatching a pattern that cannot compile.
    const store = new SearchSessionStore();
    store.save("search:1", { hits: [], reason: "failed", error: "invalid query" });
    expect(store.get("search:1")?.error).toBe("invalid query");
  });
});
