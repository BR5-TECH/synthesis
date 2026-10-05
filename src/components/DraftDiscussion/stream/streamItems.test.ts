/**
 * What the draft discussion column draws, in the order it draws it
 * (`../../../../specifications/ui/DDS-draft-discussion.md`).
 *
 * The grouping, the dividers and the folding are derived when the column renders
 * (DDS-FR-QJFE), so this is where they are established: a rendering test sees
 * the result and not the rule, and every case below is one a transcript actually
 * reaches.
 */
import { describe, expect, it } from "vitest";

import { coveredBy, dayLabel, streamItems } from "./streamItems";
import type { PairedHalf } from "../../CommentRail/questionPairs";
import type { Attachment, Comment, Participant } from "../../../types";

const HELGA: Participant = { kind: "agent", agentId: "a1", handle: "helga" };
const OLAF: Participant = { kind: "agent", agentId: "a2", handle: "olaf" };
const ME: Participant = { kind: "human", login: "raver119" };

function comment(
  id: string,
  author: Participant,
  body: string,
  createdAt = "2026-02-01T10:00:00Z",
  attachments: Attachment[] = [],
): Comment {
  return { id, author, body, quotes: [], attachments, createdAt };
}

const PROPOSAL: Attachment = {
  kind: "proposal",
  proposalId: "p1",
  draftId: "d1",
  path: "Post draft to Github.md",
};

const mine = (c: Comment) => c.author.kind === "human";
const NO_PAIRS: ReadonlyMap<string, PairedHalf> = new Map();

describe("consecutive messages by one author are one block (DDS-FR-QJFE)", () => {
  it("DDS-FR-QJFE: merges a run by one author and keeps its bodies in order", () => {
    const items = streamItems({
      comments: [
        comment("c1", HELGA, "first"),
        comment("c2", HELGA, "second"),
        comment("c3", OLAF, "third"),
      ],
      pairing: NO_PAIRS,
      isOwn: mine,
    });
    const messages = items.filter((i) => i.kind === "message");
    expect(messages).toHaveLength(2);
    expect(messages[0].kind === "message" && messages[0].comments).toHaveLength(2);
    expect(
      messages[0].kind === "message" && messages[0].comments.map((c) => c.body),
    ).toEqual(["first", "second"]);
  });

  it("DDS-FR-QJFE: two agents with the same handle but different ids do not merge", () => {
    // The participant the backend stamped is what decides, not the name it
    // renders under — two agents may answer under one handle.
    const twin: Participant = { kind: "agent", agentId: "a2", handle: "helga" };
    const items = streamItems({
      comments: [comment("c1", HELGA, "one"), comment("c2", twin, "two")],
      pairing: NO_PAIRS,
      isOwn: mine,
    });
    expect(items.filter((i) => i.kind === "message")).toHaveLength(2);
  });

  it("DDS-FR-QJFE: a card between two messages by one author breaks the run", () => {
    const items = streamItems({
      comments: [
        comment("c1", HELGA, "before", "2026-02-01T10:00:00Z", [PROPOSAL]),
        comment("c2", HELGA, "after"),
      ],
      pairing: NO_PAIRS,
      isOwn: mine,
    });
    expect(items.map((i) => i.kind)).toEqual([
      "date",
      "message",
      "change",
      "message",
    ]);
  });

  it("DDS-FR-GKMT: a run breaks where the column draws a divider", () => {
    // A block has one edge, so a divider inside a merged run would have nowhere
    // to go and the reading position would be lost with nothing to show it.
    const items = streamItems({
      comments: [
        comment("c1", HELGA, "read"),
        comment("c2", HELGA, "unread"),
      ],
      pairing: NO_PAIRS,
      isOwn: mine,
      breakBefore: new Set(["c2"]),
    });
    expect(items.filter((i) => i.kind === "message")).toHaveLength(2);
  });
});

describe("the author's own messages (DDS-FR-VTKD)", () => {
  it("DDS-FR-VTKD: marks a block the reader wrote, and one written to them", () => {
    const items = streamItems({
      comments: [comment("c1", ME, "mine"), comment("c2", HELGA, "theirs")],
      pairing: NO_PAIRS,
      isOwn: mine,
    });
    const messages = items.filter((i) => i.kind === "message");
    expect(messages[0].kind === "message" && messages[0].own).toBe(true);
    expect(messages[1].kind === "message" && messages[1].own).toBe(false);
  });
});

describe("a submitted exchange is one card (DQA-FR-KYWR)", () => {
  const pairing: ReadonlyMap<string, PairedHalf> = new Map([
    ["c1", "question"],
    ["c2", "answer"],
  ]);

  it("DQA-FR-KYWR: draws the pair once, with the answer folded into it", () => {
    const items = streamItems({
      comments: [
        comment("c1", HELGA, "which one?\n\n1. a\n2. b"),
        comment("c2", ME, "**Selected option:** b"),
      ],
      pairing,
      isOwn: mine,
    });
    expect(items.map((i) => i.kind)).toEqual(["date", "question"]);
    const card = items[1];
    expect(card.kind === "question" && card.answer?.id).toBe("c2");
  });

  it("DQA-FR-KYWR: the answer half is never drawn as a message of its own", () => {
    const items = streamItems({
      comments: [
        comment("c1", HELGA, "which one?\n\n1. a\n2. b"),
        comment("c2", ME, "**Selected option:** b"),
        comment("c3", ME, "and one more thing"),
      ],
      pairing,
      isOwn: mine,
    });
    const messages = items.filter((i) => i.kind === "message");
    expect(messages).toHaveLength(1);
    expect(
      messages[0].kind === "message" && messages[0].comments[0].id,
    ).toBe("c3");
  });

  it("DDS-FR-CLBK: a fold between the two halves still draws the answer", () => {
    // The answer is drawn inside its question's card, so folding the question
    // away leaves it with no card to be in. Skipping it there would drop a
    // committed comment out of the transcript with nothing to show that it had
    // been dropped — the worst kind of loss, because it is silent.
    const items = streamItems({
      comments: [
        comment("c1", HELGA, "which one?\n\n1. a\n2. b"),
        comment("c2", ME, "**Selected option:** b"),
      ],
      pairing,
      isOwn: mine,
      visibleFrom: 1,
    });
    expect(items.map((i) => i.kind)).toEqual(["date", "message"]);
    const shown = items.find((i) => i.kind === "message")!;
    expect(shown.kind === "message" && shown.comments[0].id).toBe("c2");
  });

  it("DDS-FR-GKMT: a break on the answer half lands above the whole card", () => {
    const items = streamItems({
      comments: [
        comment("c1", HELGA, "which one?\n\n1. a\n2. b"),
        comment("c2", ME, "**Selected option:** b"),
      ],
      pairing,
      isOwn: mine,
      breakBefore: new Set(["c2"]),
    });
    const card = items.find((i) => i.kind === "question")!;
    expect(coveredBy(card)).toEqual(["c1", "c2"]);
  });
});

describe("a change reference is a row of its own (DDS-FR-XHRB)", () => {
  it("DDS-FR-XHRB: draws the reference as a row after the body that announced it", () => {
    const items = streamItems({
      comments: [
        comment("c1", HELGA, "I changed two things.", "2026-02-01T10:00:00Z", [
          PROPOSAL,
        ]),
      ],
      pairing: NO_PAIRS,
      isOwn: mine,
    });
    expect(items.map((i) => i.kind)).toEqual(["date", "message", "change"]);
  });

  it("DDS-FR-XHRB: a comment carrying only a reference draws no empty message", () => {
    const items = streamItems({
      comments: [
        comment("c0", OLAF, "opening"),
        comment("c1", HELGA, "", "2026-02-01T10:00:00Z", [PROPOSAL]),
      ],
      pairing: NO_PAIRS,
      isOwn: mine,
    });
    expect(items.map((i) => i.kind)).toEqual(["date", "message", "change"]);
  });

  it("CMT-FR-15: the comment the thread opens with keeps a block for the thread's actions", () => {
    // The opening comment carries the lock, resolve, detach and maximize
    // controls. A proposal announced with an empty body opens plenty of
    // discussions, and a row is a button — it can hold none of them.
    const items = streamItems({
      comments: [comment("c1", HELGA, "", "2026-02-01T10:00:00Z", [PROPOSAL])],
      pairing: NO_PAIRS,
      isOwn: mine,
    });
    expect(items.map((i) => i.kind)).toEqual(["date", "message", "change"]);
  });

  it("DDS-FR-XHRB: a reference-only comment keeps its quotes and its other attachments", () => {
    // Drawing the row alone would drop them from the transcript, silently.
    const image = {
      kind: "blob" as const,
      digest: "abc",
      mediaType: "image/png",
      filename: "shot.png",
      bytes: 10,
    };
    const items = streamItems({
      comments: [
        comment("c0", OLAF, "opening"),
        {
          ...comment("c1", HELGA, "", "2026-02-01T10:00:00Z", [PROPOSAL, image]),
          quotes: [{ commentId: "c0", excerpt: "opening" }],
        },
      ],
      pairing: NO_PAIRS,
      isOwn: mine,
    });
    // Two blocks, because two authors — and the second is the one that would
    // otherwise have been dropped.
    expect(items.map((i) => i.kind)).toEqual([
      "date",
      "message",
      "message",
      "change",
    ]);
  });

  it("DDS-FR-XHRB: a comment that says nothing and carries nothing still renders", () => {
    // A message that arrived blank is visible rather than silently dropped.
    const items = streamItems({
      comments: [comment("c1", HELGA, "")],
      pairing: NO_PAIRS,
      isOwn: mine,
    });
    expect(items.map((i) => i.kind)).toEqual(["date", "message"]);
  });
});

describe("the date divider (DDS-FR-NDSA)", () => {
  const now = new Date("2026-02-03T12:00:00Z");

  it("DDS-FR-NDSA: names today, yesterday, and a date further back, in lowercase", () => {
    expect(dayLabel("2026-02-03T09:00:00Z", now)).toBe("today");
    expect(dayLabel("2026-02-02T09:00:00Z", now)).toBe("yesterday");
    expect(dayLabel("2026-01-12T09:00:00Z", now)).toBe("12 jan");
  });

  it("DDS-FR-NDSA: divides at each day boundary and nowhere else", () => {
    const items = streamItems({
      comments: [
        comment("c1", HELGA, "one", "2026-02-02T09:00:00Z"),
        comment("c2", OLAF, "two", "2026-02-02T10:00:00Z"),
        comment("c3", HELGA, "three", "2026-02-03T09:00:00Z"),
      ],
      pairing: NO_PAIRS,
      isOwn: mine,
      now,
    });
    expect(items.map((i) => i.kind)).toEqual([
      "date",
      "message",
      "message",
      "date",
      "message",
    ]);
    expect(items[0].kind === "date" && items[0].label).toBe("yesterday");
    expect(items[3].kind === "date" && items[3].label).toBe("today");
  });

  it("DDS-FR-NDSA: a timestamp that parses to nothing draws no divider", () => {
    const items = streamItems({
      comments: [comment("c1", HELGA, "one", "not a date")],
      pairing: NO_PAIRS,
      isOwn: mine,
      now,
    });
    expect(items.map((i) => i.kind)).toEqual(["message"]);
  });
});

describe("the folded head of a long discussion (DDS-FR-CLBK)", () => {
  it("DDS-FR-CLBK: draws nothing above the first message that is on screen", () => {
    const items = streamItems({
      comments: [
        comment("c1", HELGA, "old"),
        comment("c2", HELGA, "older"),
        comment("c3", OLAF, "shown"),
      ],
      pairing: NO_PAIRS,
      isOwn: mine,
      visibleFrom: 2,
    });
    const messages = items.filter((i) => i.kind === "message");
    expect(messages).toHaveLength(1);
    expect(
      messages[0].kind === "message" && messages[0].comments[0].id,
    ).toBe("c3");
  });
});
