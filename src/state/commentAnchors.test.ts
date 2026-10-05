import { describe, expect, it } from "vitest";
import {
  diffEdit,
  driftedAnchors,
  findNearest,
  resolveAnchor,
  resolveThreads,
  shiftAnchor,
  shiftThreads,
  stackCards,
  unresolvedCount,
  type AnchoredThread,
} from "./commentAnchors";
import type { FragmentRange, Discussion } from "../types";

function anchor(start: number, end: number, quote: string): FragmentRange {
  return { start, end, quote };
}

function thread(
  id: string,
  a: FragmentRange,
  extra: Partial<Discussion> = {},
): Discussion {
  return {
    id,
    target: { kind: "artifact", artifactId: "specs/a.md" },
    fragmentTarget: {
      owner: { kind: "artifact", artifactId: "specs/a.md" },
      path: "specs/a.md",
      ...a,
    },
    comments: [
      {
        id: `${id}-c1`,
        author: { kind: "human", login: "raver119" },
        body: "body",
        quotes: [],
        attachments: [],
        createdAt: "2026-01-01T00:00:00Z",
      },
    ],
    locked: false,
    resolved: false,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    ...extra,
  };
}

describe("findNearest", () => {
  it("prefers the occurrence closest to the hint rather than the first", () => {
    // CMT-FR-18. An artifact that repeats a phrase is the ordinary case for a
    // spec or a checklist; anchoring to the first match would move every thread
    // on the later ones up to the top of the document.
    const content = "## Steps\n\n## Steps\n\n## Steps\n";
    expect(findNearest(content, "## Steps", 0)).toBe(0);
    expect(findNearest(content, "## Steps", 10)).toBe(10);
    expect(findNearest(content, "## Steps", 20)).toBe(20);
  });

  it("breaks a tie toward the earlier occurrence", () => {
    // Genuinely equidistant: A at 1 and A at 5, hint at 3. The result must not
    // depend on which direction the scan happened to run.
    const content = "xAyyyAx";
    expect(content.indexOf("A")).toBe(1);
    expect(content.lastIndexOf("A")).toBe(5);
    expect(findNearest(content, "A", 3)).toBe(1);
  });

  it("counts overlapping occurrences", () => {
    expect(findNearest("aaa", "aa", 1)).toBe(1);
    expect(findNearest("aaa", "aa", 0)).toBe(0);
  });

  it("reports a quote that appears nowhere, and never matches an empty one", () => {
    expect(findNearest("hello", "world", 0)).toBe(-1);
    expect(findNearest("hello", "", 0)).toBe(-1);
  });
});

describe("resolveAnchor", () => {
  const content = "Steps to run before the first session with the team.";
  const QUOTE = "the first session";
  const AT = content.indexOf(QUOTE);

  it("keeps an anchor whose stored offsets still cover its quote", () => {
    const stored = anchor(AT, AT + QUOTE.length, QUOTE);
    expect(content.slice(stored.start, stored.end)).toBe(QUOTE);
    expect(resolveAnchor(content, stored)).toEqual(stored);
  });

  it("re-finds an anchor after text grew above it", () => {
    // CMT-FR-18: the offsets are a hint, the quote is the identity.
    const grown = `${"x".repeat(200)}${content}`;
    const resolved = resolveAnchor(grown, anchor(AT, AT + QUOTE.length, QUOTE));
    expect(resolved).toEqual(anchor(200 + AT, 200 + AT + QUOTE.length, QUOTE));
  });

  it("orphans an anchor whose quote is gone", () => {
    // CMT-FR-18, CMT-FR-19: never silently re-pointed at unrelated text.
    expect(
      resolveAnchor("a wholly different document", anchor(AT, AT + QUOTE.length, QUOTE)),
    ).toBeNull();
  });

  it("picks the nearer occurrence when the quote repeats", () => {
    const doc = "the first session ... the first session";
    // Stored near the second occurrence.
    expect(resolveAnchor(doc, anchor(22, 39, "the first session"))).toEqual(
      anchor(22, 39, "the first session"),
    );
    // Stored near the first.
    expect(resolveAnchor(doc, anchor(0, 17, "the first session"))).toEqual(
      anchor(0, 17, "the first session"),
    );
  });
});

describe("resolveThreads", () => {
  it("resolves what it can and marks the rest orphaned without dropping any", () => {
    // CMT-FR-19: an orphaned thread is never hidden and never discarded.
    const content = "alpha beta gamma";
    const resolved = resolveThreads(content, [
      thread("t1", anchor(0, 5, "alpha")),
      thread("t2", anchor(99, 104, "delta")),
      thread("t3", anchor(11, 16, "gamma")),
    ]);
    expect(resolved).toHaveLength(3);
    expect(resolved[0].anchor).toEqual(anchor(0, 5, "alpha"));
    expect(resolved[1].anchor).toBeNull();
    expect(resolved[2].anchor).toEqual(anchor(11, 16, "gamma"));
  });
});

describe("diffEdit", () => {
  it("returns null when nothing changed", () => {
    expect(diffEdit("same", "same")).toBeNull();
  });

  it("describes an insertion, a deletion, and a replacement", () => {
    expect(diffEdit("ac", "abc")).toEqual({ start: 1, end: 1, insertedLength: 1 });
    expect(diffEdit("abc", "ac")).toEqual({ start: 1, end: 2, insertedLength: 0 });
    expect(diffEdit("abc", "aXc")).toEqual({ start: 1, end: 2, insertedLength: 1 });
  });

  it("describes appending and emptying", () => {
    expect(diffEdit("ab", "abcd")).toEqual({ start: 2, end: 2, insertedLength: 2 });
    expect(diffEdit("ab", "")).toEqual({ start: 0, end: 2, insertedLength: 0 });
    expect(diffEdit("", "ab")).toEqual({ start: 0, end: 0, insertedLength: 2 });
  });
});

describe("shiftAnchor", () => {
  const a = anchor(10, 20, "0123456789");

  it("leaves an anchor alone when the edit is after it", () => {
    expect(shiftAnchor(a, { start: 20, end: 20, insertedLength: 5 })).toEqual(a);
    expect(shiftAnchor(a, { start: 50, end: 60, insertedLength: 0 })).toEqual(a);
  });

  it("slides an anchor when text before it grows or shrinks", () => {
    // CMT-FR-20.
    expect(shiftAnchor(a, { start: 0, end: 0, insertedLength: 7 })).toEqual(
      anchor(17, 27, "0123456789"),
    );
    expect(shiftAnchor(a, { start: 0, end: 4, insertedLength: 0 })).toEqual(
      anchor(6, 16, "0123456789"),
    );
    // An insertion exactly at the anchor's start is before it: the anchor slides
    // and the new text is not absorbed into the quote.
    expect(shiftAnchor(a, { start: 10, end: 10, insertedLength: 3 })).toEqual(
      anchor(13, 23, "0123456789"),
    );
  });

  it("orphans an anchor whose own text the edit destroyed", () => {
    // Strict on purpose: a partially-overwritten anchor would leave the card
    // pointing at text nobody wrote.
    expect(shiftAnchor(a, { start: 10, end: 20, insertedLength: 0 })).toBeNull();
    expect(shiftAnchor(a, { start: 12, end: 14, insertedLength: 1 })).toBeNull();
    expect(shiftAnchor(a, { start: 5, end: 15, insertedLength: 2 })).toBeNull();
    expect(shiftAnchor(a, { start: 15, end: 25, insertedLength: 2 })).toBeNull();
    // Select-all-and-delete takes everything with it.
    expect(shiftAnchor(a, { start: 0, end: 100, insertedLength: 0 })).toBeNull();
  });

  it("orphans on an insertion strictly inside the anchored text", () => {
    // A keystroke inside a commented sentence changes what the quote says, so
    // the stored quote no longer describes the range. Keeping the anchor would
    // leave the card pointing at text that is no longer what was commented on;
    // orphaning is the conservative answer and the thread stays fully readable
    // and repliable in the rail (CMT-FR-19). Pinned here because it is a real
    // behavioural choice, not an accident of the boundary tests above.
    expect(shiftAnchor(a, { start: 15, end: 15, insertedLength: 3 })).toBeNull();
    // The two boundaries are NOT inside, and both keep the anchor.
    expect(shiftAnchor(a, { start: 10, end: 10, insertedLength: 3 })).toEqual(
      anchor(13, 23, "0123456789"),
    );
    expect(shiftAnchor(a, { start: 20, end: 20, insertedLength: 3 })).toEqual(a);
  });

  it("leaves an already-orphaned thread orphaned through further edits", () => {
    const anchored: AnchoredThread[] = [
      { thread: thread("t1", a), anchor: null },
      { thread: thread("t2", a), anchor: a },
    ];
    const after = shiftThreads(anchored, { start: 0, end: 0, insertedLength: 4 });
    expect(after[0].anchor).toBeNull();
    expect(after[1].anchor).toEqual(anchor(14, 24, "0123456789"));
  });
});

describe("driftedAnchors", () => {
  it("reports only the threads whose live anchor left the stored one", () => {
    // CMT-FR-21: a thread that did not move is not re-anchored.
    const moved = thread("t1", anchor(10, 15, "hello"));
    const still = thread("t2", anchor(30, 35, "world"));
    const orphan = thread("t3", anchor(50, 55, "gone!"));
    const drifted = driftedAnchors([
      { thread: moved, anchor: anchor(42, 47, "hello") },
      { thread: still, anchor: anchor(30, 35, "world") },
      { thread: orphan, anchor: null },
    ]);
    expect(drifted).toEqual([{ threadId: "t1", anchor: anchor(42, 47, "hello") }]);
  });

  it("never proposes an anchor for an orphaned thread", () => {
    const orphan = thread("t1", anchor(50, 55, "gone!"));
    expect(driftedAnchors([{ thread: orphan, anchor: null }])).toEqual([]);
  });

  it("skips a thread that stores no anchor at all", () => {
    // CMT-FR-55 / CMS-FR-59: a discussion is never reanchored, and asking the
    // backend to would be a typed `not_fragment_targeted` refusal surfacing on a card the
    // author never touched.
    const discussion: Discussion = {
      ...thread("d1", anchor(0, 5, "hello")),
      target: { kind: "draft", draftId: "draft-1" },
      fragmentTarget: null,
    };
    expect(driftedAnchors([{ thread: discussion, anchor: anchor(9, 14, "hello") }])).toEqual([]);
  });

  it("resolves a thread that stores no anchor to no anchor, rather than orphaning it", () => {
    // CMT-FR-53: there is nothing to search for, so there is nothing to fail to
    // find. The rail tells the two apart by the STORED anchor, which is what
    // keeps a discussion out of the orphaned section (CMT-FR-55).
    const discussion: Discussion = {
      ...thread("d1", anchor(0, 5, "hello")),
      target: { kind: "draft", draftId: "draft-1" },
      fragmentTarget: null,
    };
    const [entry] = resolveThreads("hello there", [discussion]);
    expect(entry.anchor).toBeNull();
    expect(entry.thread.fragmentTarget).toBeNull();
  });

  it("reads a missing anchor field the same way it reads a null one", () => {
    // A field the backend skips rather than serializes arrives as `undefined`,
    // and `undefined` is not `null`: the search below would have run against it
    // and thrown. Both readers go through `discussionFragment` for that reason.
    const bare = {
      ...thread("d1", anchor(0, 5, "hello")),
      target: { kind: "draft" as const, draftId: "draft-1" },
    };
    delete (bare as { fragmentTarget?: unknown }).fragmentTarget;

    const [entry] = resolveThreads("hello there", [bare as Discussion]);
    expect(entry.anchor).toBeNull();
    expect(
      driftedAnchors([{ thread: bare as Discussion, anchor: anchor(9, 14, "hello") }]),
    ).toEqual([]);
  });
});

describe("stackCards", () => {
  it("aligns each card to its anchor when they are far apart", () => {
    // CMT-FR-27: a card is never above where its anchor is.
    const stacked = stackCards(
      [
        { threadId: "a", anchorTop: 0 },
        { threadId: "b", anchorTop: 400 },
      ],
      { a: 100, b: 100 },
      80,
    );
    expect(stacked).toEqual([
      { threadId: "a", top: 0 },
      { threadId: "b", top: 400 },
    ]);
  });

  it("pushes a card down only as far as it must go to clear the one above", () => {
    // CMT-FR-27: three threads anchored to consecutive short lines.
    const stacked = stackCards(
      [
        { threadId: "a", anchorTop: 0 },
        { threadId: "b", anchorTop: 20 },
        { threadId: "c", anchorTop: 40 },
      ],
      { a: 100, b: 60, c: 60 },
      80,
      8,
    );
    expect(stacked).toEqual([
      { threadId: "a", top: 0 },
      { threadId: "b", top: 108 },
      { threadId: "c", top: 176 },
    ]);
    // No two cards overlap, and document order is preserved.
    expect(stacked[1].top).toBeGreaterThanOrEqual(stacked[0].top + 100);
    expect(stacked[2].top).toBeGreaterThanOrEqual(stacked[1].top + 60);
  });

  it("packs a run tightly rather than compounding the first card's push", () => {
    // A card that clears its predecessor returns to its own anchor as soon as
    // there is room, instead of every later card inheriting the offset.
    const stacked = stackCards(
      [
        { threadId: "a", anchorTop: 0 },
        { threadId: "b", anchorTop: 10 },
        { threadId: "c", anchorTop: 900 },
      ],
      { a: 50, b: 50, c: 50 },
      50,
      0,
    );
    expect(stacked.map((s) => s.top)).toEqual([0, 50, 900]);
  });

  it("falls back to a default height before anything has been measured", () => {
    const stacked = stackCards(
      [
        { threadId: "a", anchorTop: 0 },
        { threadId: "b", anchorTop: 0 },
      ],
      {},
      80,
      8,
    );
    expect(stacked.map((s) => s.top)).toEqual([0, 88]);
  });
});

describe("unresolvedCount", () => {
  it("counts unresolved threads including orphaned ones", () => {
    // CMT-FR-29: an orphaned thread is the one most at risk of being forgotten.
    const entries: AnchoredThread[] = [
      { thread: thread("t1", anchor(0, 1, "a")), anchor: anchor(0, 1, "a") },
      { thread: thread("t2", anchor(0, 1, "a"), { resolved: true }), anchor: anchor(0, 1, "a") },
      { thread: thread("t3", anchor(0, 1, "a")), anchor: null },
    ];
    expect(unresolvedCount(entries)).toBe(2);
  });

  it("is zero for an artifact whose threads are all resolved", () => {
    const entries: AnchoredThread[] = [
      { thread: thread("t1", anchor(0, 1, "a"), { resolved: true }), anchor: anchor(0, 1, "a") },
    ];
    expect(unresolvedCount(entries)).toBe(0);
  });
});
