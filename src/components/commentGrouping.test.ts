/**
 * The Comments panel's pure view logic (`CMP-comments-panel.md`): grouping by
 * status (CMP-FR-03 / CMP-FR-04), ordering (CMP-FR-09) and filtering
 * (CMP-FR-14).
 */
import { describe, expect, it } from "vitest";

import {
  attachmentCount,
  COMMENT_GROUP_ORDER,
  firstComment,
  groupKeyFor,
  groupThreads,
  matchesFilter,
  replyCount,
  sortMostRecentFirst,
} from "./commentGrouping";
import {
  buildDiscussion,
  buildListItem,
  type DiscussionOverrides,
} from "../test/discussionFixtures";
import type { Comment, DiscussionListItem, Participant } from "../types";

const author: Participant = { kind: "human", login: "raver119" };

function comment(id: string, body: string): Comment {
  return {
    id,
    author,
    body,
    quotes: [],
    attachments: [],
    createdAt: "2024-01-01T00:00:00Z",
  };
}

function item(
  over: DiscussionOverrides,
  ownerUnavailable = false,
): DiscussionListItem {
  return buildListItem(
    buildDiscussion(over, {
      path: "specs/a.md",
      range: { start: 0, end: 3, quote: "one" },
      comments: [comment("c1", "opening")],
    }),
    ownerUnavailable,
  );
}

describe("CMP-FR-03 / CMP-FR-04: each thread lands in exactly one group", () => {
  it("puts a plain thread in Active, a locked one in Locked, a resolved one in Resolved", () => {
    expect(groupKeyFor(item({ id: "t1" }))).toBe("active");
    expect(groupKeyFor(item({ id: "t2", locked: true }))).toBe("locked");
    expect(groupKeyFor(item({ id: "t3", resolved: true }))).toBe("resolved");
  });

  it("resolves ahead of locks, so a thread that is both is Resolved only", () => {
    const both = item({ id: "t4", locked: true, resolved: true });
    expect(groupKeyFor(both)).toBe("resolved");

    const groups = groupThreads([both], "");
    expect(groups.map((g) => g.key)).toEqual(["resolved"]);
    expect(groups[0].items).toHaveLength(1);
  });

  it("keeps the counts summing to the project's thread total", () => {
    const items = [
      item({ id: "t1" }),
      item({ id: "t2", locked: true }),
      item({ id: "t3", resolved: true }),
      item({ id: "t4", locked: true, resolved: true }),
      item({ id: "t5" }),
    ];
    const groups = groupThreads(items, "");
    const total = groups.reduce((n, g) => n + g.items.length, 0);
    expect(total).toBe(items.length);
    expect(
      Object.fromEntries(groups.map((g) => [g.key, g.items.length])),
    ).toEqual({ active: 2, locked: 1, resolved: 2 });
  });

  it("renders the groups in the fixed order Active, Locked, Resolved", () => {
    expect(COMMENT_GROUP_ORDER).toEqual(["active", "locked", "resolved"]);
    const groups = groupThreads(
      [
        item({ id: "t3", resolved: true }),
        item({ id: "t2", locked: true }),
        item({ id: "t1" }),
      ],
      "",
    );
    expect(groups.map((g) => g.label)).toEqual([
      "Active",
      "Locked",
      "Resolved",
    ]);
  });
});

describe("CMP-FR-05: an empty group is not rendered", () => {
  it("omits the groups the project has nothing for", () => {
    const groups = groupThreads([item({ id: "t1", resolved: true })], "");
    expect(groups.map((g) => g.key)).toEqual(["resolved"]);
  });

  it("omits a group the filter emptied, and yields nothing at all when it empties every one", () => {
    const items = [
      item({ id: "t1", anchor: { start: 0, end: 3, quote: "alpha" } }),
      item({
        id: "t2",
        resolved: true,
        anchor: { start: 0, end: 4, quote: "beta" },
      }),
    ];
    expect(groupThreads(items, "alpha").map((g) => g.key)).toEqual(["active"]);
    expect(groupThreads(items, "nothing-matches-this")).toEqual([]);
  });
});

describe("CMP-FR-06: what a row shows of its thread", () => {
  it("takes the opening comment and counts the replies after it", () => {
    const one = item({ id: "t1" });
    expect(firstComment(one)?.body).toBe("opening");
    expect(replyCount(one)).toBe(0);

    const three = item({
      id: "t2",
      comments: [comment("c1", "opening"), comment("c2", "a"), comment("c3", "b")],
    });
    expect(firstComment(three)?.id).toBe("c1");
    expect(replyCount(three)).toBe(2);
  });

  it("survives a thread the fold left with no comments rather than throwing", () => {
    const none = item({ id: "t1", comments: [] });
    expect(firstComment(none)).toBeUndefined();
    expect(replyCount(none)).toBe(0);
  });
});

describe("CMP-FR-09: most-recently-active first, across artifacts", () => {
  it("orders by last activity rather than by artifact or by creation", () => {
    const items = [
      item({ id: "t1", artifactId: "specs/a.md", updatedAt: "2024-01-01T00:00:00Z" }),
      item({ id: "t2", artifactId: "specs/b.md", updatedAt: "2024-03-01T00:00:00Z" }),
      item({ id: "t3", artifactId: "specs/a.md", updatedAt: "2024-02-01T00:00:00Z" }),
    ];
    expect(sortMostRecentFirst(items).map((i) => i.discussion.id)).toEqual([
      "t2",
      "t3",
      "t1",
    ]);
  });

  it("breaks a tie on id so the order is stable across reads", () => {
    const at = "2024-01-01T00:00:00Z";
    const forward = [item({ id: "b", updatedAt: at }), item({ id: "a", updatedAt: at })];
    const reversed = [...forward].reverse();
    expect(sortMostRecentFirst(forward).map((i) => i.discussion.id)).toEqual(["a", "b"]);
    expect(sortMostRecentFirst(reversed).map((i) => i.discussion.id)).toEqual(["a", "b"]);
  });

  it("does not mutate the array it was given", () => {
    const items = [
      item({ id: "t1", updatedAt: "2024-01-01T00:00:00Z" }),
      item({ id: "t2", updatedAt: "2024-05-01T00:00:00Z" }),
    ];
    sortMostRecentFirst(items);
    expect(items.map((i) => i.discussion.id)).toEqual(["t1", "t2"]);
  });
});

describe("CMP-FR-14: what the filter matches on", () => {
  const subject = item({
    id: "t1",
    artifactId: "specs/onboarding.md",
    anchor: { start: 0, end: 17, quote: "the first session" },
    comments: [comment("c1", "Which session? Say **which** one.")],
  });

  it("matches the anchor quote, the opening comment's body, and the artifact path", () => {
    expect(matchesFilter(subject, "first session")).toBe(true);
    expect(matchesFilter(subject, "which")).toBe(true);
    expect(matchesFilter(subject, "onboarding")).toBe(true);
  });

  it("is case-insensitive and ignores surrounding whitespace", () => {
    expect(matchesFilter(subject, "FIRST SESSION")).toBe(true);
    expect(matchesFilter(subject, "  onboarding  ")).toBe(true);
  });

  it("matches everything on an empty or blank query", () => {
    expect(matchesFilter(subject, "")).toBe(true);
    expect(matchesFilter(subject, "   ")).toBe(true);
  });

  it("does not match a reply's body — only the comment the row shows", () => {
    const withReply = item({
      id: "t2",
      comments: [comment("c1", "opening"), comment("c2", "buried-in-a-reply")],
    });
    expect(matchesFilter(withReply, "buried-in-a-reply")).toBe(false);
    expect(matchesFilter(withReply, "opening")).toBe(true);
  });

  it("rejects a thread nothing about it matches", () => {
    expect(matchesFilter(subject, "kickoff")).toBe(false);
  });
});

describe("CMP-FR-14 / CMP-FR-25: attachments in the filter and the count", () => {
  function withAttachments(id: string, attachments: Comment["attachments"]) {
    const base = item({ id });
    return {
      ...base,
      discussion: {
        ...base.discussion,
        comments: [{ ...base.discussion.comments[0], attachments }],
      },
    };
  }

  const png = {
    kind: "blob" as const,
    digest: "a3f9",
    mediaType: "image/png",
    filename: "kickoff-flow.png",
    bytes: 12,
  };
  const link = {
    kind: "url" as const,
    url: "https://example.test/spec-v2.png",
    mediaType: "image/png",
    label: "spec v2",
  };

  it("matches a stored attachment by filename and a link by its label", () => {
    const withPng = withAttachments("t1", [png]);
    const withLink = withAttachments("t2", [link]);
    expect(matchesFilter(withPng, "kickoff")).toBe(true);
    expect(matchesFilter(withPng, "KICKOFF-FLOW.PNG")).toBe(true);
    expect(matchesFilter(withLink, "spec v2")).toBe(true);
    // And does not match the other one's name.
    expect(matchesFilter(withLink, "kickoff")).toBe(false);
  });

  it("matches a link with no label by its address", () => {
    const bare = withAttachments("t3", [{ ...link, label: undefined }]);
    expect(matchesFilter(bare, "example.test")).toBe(true);
  });

  it("counts what the opening comment carries", () => {
    expect(attachmentCount(withAttachments("t4", [png, link]))).toBe(2);
    expect(attachmentCount(item({ id: "t5" }))).toBe(0);
  });
});
