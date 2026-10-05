// The `@nickname` tag syntax — `specifications/ui/AGT-agents.md` AGT-FR-24,
// AGT-FR-25, AGT-FR-27, AGT-FR-28, AGT-FR-29.
//
// Pure rules, tested without rendering: what counts as a tag, which tags reach
// an agent, and what the mention picker is looking at while the author types.
import { describe, expect, it } from "vitest";

import {
  activeMention,
  addressesEveryone,
  addressedNicknames,
  applyMention,
  resolveTags,
  segmentBody,
  tagCandidates,
} from "./agentTags";

const ENROLLED = ["arch", "sec", "scribe"];

describe("AGT-FR-24 / AGT-FR-28: what is a tag, and what reaches an agent", () => {
  it("matches a nickname without regard to case", () => {
    // AGT-FR-24, AGT-FR-25, AGT-FR-28's last clause.
    expect(addressedNicknames("@ARCH what do you think", ENROLLED)).toEqual([
      "arch",
    ]);
    expect(addressedNicknames("@Arch and @SEC", ENROLLED)).toEqual([
      "arch",
      "sec",
    ]);
  });

  it("returns the registry's spelling, not the author's", () => {
    // A caller dispatching per tag must name the agent the way the backend
    // does, or `@ARCH` and `@arch` in one message would look like two agents.
    expect(resolveTags("@ARCH and @arch", ENROLLED).map((t) => t.nickname)).toEqual(
      ["arch", "arch"],
    );
  });

  it("leaves an email address alone", () => {
    // AGT-FR-24, AGT-FR-25, AGT-FR-28: an author writing an email address must not be interrupted, so
    // an `@` preceded by a letter never starts a tag at all.
    expect(tagCandidates("mail me at me@example.com")).toEqual([]);
    expect(addressedNicknames("mail me at me@example.com", ENROLLED)).toEqual([]);
  });

  it("dispatches and marks a tag a dash introduces", () => {
    // AGT-FR-25's boundary set is shared with `tagCandidates`, so a dash begins a
    // word for *dispatch* and *rendering* too, not only for the picker — an
    // author writing an em-dash aside addresses the agent they named in it.
    expect(addressedNicknames("settled—@arch disagrees", ENROLLED)).toEqual([
      "arch",
    ]);
    expect(segmentBody("settled—@arch disagrees", ENROLLED)).toEqual([
      { kind: "text", text: "settled—" },
      { kind: "tag", text: "@arch" },
      { kind: "text", text: " disagrees" },
    ]);
    // A hyphenated word is not a boundary the author meant, but the same rule
    // covers it: `well-@arch` names the agent rather than swallowing it.
    expect(addressedNicknames("well-@arch said so", ENROLLED)).toEqual(["arch"]);
  });

  it("addresses nobody with a nickname the project has not enrolled", () => {
    // AGT-FR-24, AGT-FR-25 / AGT-FR-28: ordinary text, no error, nothing dispatched.
    expect(addressedNicknames("@nobody take a look", ENROLLED)).toEqual([]);
    // The same text in a project that *has* enrolled it does reach them, which
    // is the whole reason resolution is against a roster.
    expect(addressedNicknames("@nobody take a look", ["nobody"])).toEqual([
      "nobody",
    ]);
  });

  it("ends a nickname at sentence punctuation", () => {
    expect(addressedNicknames("@arch, thoughts?", ENROLLED)).toEqual(["arch"]);
    expect(addressedNicknames("(@sec) please", ENROLLED)).toEqual(["sec"]);
    expect(addressedNicknames("ask @scribe.", ENROLLED)).toEqual(["scribe"]);
  });

  it("names each addressed agent once however often it is tagged", () => {
    // CTA-FR-RPVU dispatches once per *distinct* agent: a repeated tag in one
    // message is emphasis, not a second question.
    expect(addressedNicknames("@arch @arch @sec have a look", ENROLLED)).toEqual([
      "arch",
      "sec",
    ]);
  });

  it("ignores a tag inside a code span", () => {
    // The rule that dispatches and the rule that marks must agree: a code sample
    // is rendered as code and never marked (AGT-FR-29), so it must not summon an
    // agent either.
    expect(addressedNicknames("write `@arch` in the body", ENROLLED)).toEqual([]);
    expect(segmentBody("write `@arch` here", ENROLLED)).toEqual([
      { kind: "text", text: "write `@arch` here" },
    ]);
    // And prose after a closed span is ordinary again.
    expect(addressedNicknames("`code` then @arch", ENROLLED)).toEqual(["arch"]);
  });

  it("treats a bare sigil as ordinary text", () => {
    expect(addressedNicknames("@ arch", ENROLLED)).toEqual([]);
    expect(addressedNicknames("email@@example", ENROLLED)).toEqual([]);
  });
});

describe("AGT-FR-29: marking a tag for rendering", () => {
  it("splits only around tags that resolve", () => {
    // AGT-FR-24, AGT-FR-29: the same body, in a project that enrolled the agent and in one
    // that did not.
    expect(segmentBody("hey @arch look", ENROLLED)).toEqual([
      { kind: "text", text: "hey " },
      { kind: "tag", text: "@arch" },
      { kind: "text", text: " look" },
    ]);
    expect(segmentBody("hey @arch look", [])).toEqual([
      { kind: "text", text: "hey @arch look" },
    ]);
  });

  it("leaves a body with no resolving tag as one run", () => {
    expect(segmentBody("mail me@example.com", ENROLLED)).toEqual([
      { kind: "text", text: "mail me@example.com" },
    ]);
  });
});

describe("AGT-FR-25: the mention being typed", () => {
  it("reports the fragment between the sigil and the caret", () => {
    expect(activeMention("ask @sc", 7)).toEqual({ query: "sc", start: 4 });
    expect(activeMention("ask @", 5)).toEqual({ query: "", start: 4 });
  });

  it("reports nothing once a space has ended the tag", () => {
    // AGT-FR-34: a nickname carries no whitespace, so what is being typed is no
    // longer a tag and the picker closes.
    expect(activeMention("ask @sc more", 12)).toBeNull();
  });

  it("reports nothing for a sigil that is not at a word boundary", () => {
    // AGT-FR-25: an `@` inside a word opens nothing, so an email address is
    // written straight through.
    expect(activeMention("me@example", 10)).toBeNull();
    expect(activeMention("a@b", 3)).toBeNull();
  });

  it("treats an opening bracket, a quote and a dash as beginning a word", () => {
    // AGT-FR-25 / AGT-FR-24, AGT-FR-28. Every character in the boundary set, because the
    // set is shared with `tagCandidates` and widening it silently widens what
    // dispatches a turn.
    for (const before of ["(", "[", "{", "<", "'", '"', "-", "–", "—", " "]) {
      expect(activeMention(`${before}@sc`, 4)).toEqual({ query: "sc", start: 1 });
    }
  });

  it("shortens the fragment as the author deletes back into it", () => {
    // AGT-FR-34: the picker follows the text under the caret, so a mistyped
    // nickname is corrected without the sigil being retyped.
    expect(activeMention("ask @scx", 8)).toEqual({ query: "scx", start: 4 });
    expect(activeMention("ask @sc", 7)).toEqual({ query: "sc", start: 4 });
  });

  it("reports nothing when the caret is before the sigil", () => {
    expect(activeMention("ask @sc", 3)).toBeNull();
  });

  it("replaces what was typed with the full tag and a trailing space", () => {
    const next = applyMention("ask @sc", 7, 4, "scribe");
    expect(next.body).toBe("ask @scribe ");
    // The caret belongs after the tag: the author is mid-sentence.
    expect(next.caret).toBe(next.body.length);
  });

  it("keeps whatever followed the caret without doubling the space", () => {
    const next = applyMention("ask @sc about this", 7, 4, "scribe");
    expect(next.body).toBe("ask @scribe about this");
    expect(next.caret).toBe("ask @scribe".length);
  });
});

// ---------------------------------------------------------------------------
// The `@all` handle (AGT-FR-35 … AGT-FR-38, AGT-FR-40)
// ---------------------------------------------------------------------------

describe("AGT-FR-35 … AGT-FR-38: the @all handle", () => {
  /** A roster where every enrolled agent can answer. */
  const allReady = { nicknames: ENROLLED, ready: ENROLLED };

  it("addresses every agent that can answer", () => {
    // AGT-FR-35 / AGT-FR-36: one tag, several agents.
    expect(addressedNicknames("@all is this two specs?", allReady)).toEqual([
      "arch",
      "sec",
      "scribe",
    ]);
  });

  it("is matched without regard to case, like any other tag", () => {
    // AGT-FR-35, on the terms AGT-FR-24 sets.
    for (const written of ["@all", "@ALL", "@All"]) {
      expect(addressedNicknames(`${written} thoughts?`, allReady)).toEqual(
        ENROLLED,
      );
    }
  });

  it("leaves out an agent that cannot answer", () => {
    // AGT-FR-36: `@scribe`'s provider is no longer verified, so the handle does
    // not name it — a dispatch there would surface as that agent's own silence.
    const roster = { nicknames: ENROLLED, ready: ["arch", "sec"] };
    expect(addressedNicknames("@all is this two specs?", roster)).toEqual([
      "arch",
      "sec",
    ]);
    // Named by hand it still resolves: the backend is what refuses it, and the
    // rail renders that refusal (CMT-FR-34).
    expect(addressedNicknames("@scribe thoughts?", roster)).toEqual(["scribe"]);
  });

  it("asks an agent once when the handle and its nickname are both written", () => {
    // CTA-FR-RPVU: the union is over the agents, not the tags. `@all @arch` is two
    // ways of naming one agent, and the author asked once.
    expect(addressedNicknames("@all @arch again please", allReady)).toEqual([
      "arch",
      "sec",
      "scribe",
    ]);
  });

  it("addresses nobody where no enrolled agent can answer", () => {
    // AGT-FR-38: an ordinary message in every respect — nobody addressed, and
    // (below) nothing marked.
    expect(addressedNicknames("@all take a look", { nicknames: [], ready: [] })).toEqual(
      [],
    );
    // Enrolled, but every one of them degraded.
    expect(
      addressedNicknames("@all take a look", { nicknames: ENROLLED, ready: [] }),
    ).toEqual([]);
  });

  it("resolves afresh against the roster as it stands", () => {
    // AGT-FR-37: the same stored body says different things about two rosters,
    // because nothing records what the handle meant when it was posted.
    const body = "@all where should graduation live?";
    expect(addressedNicknames(body, { nicknames: ["arch"], ready: ["arch"] })).toEqual(
      ["arch"],
    );
    // An agent enrolled since is addressed by the very same text.
    expect(
      addressedNicknames(body, {
        nicknames: ["arch", "scribe"],
        ready: ["arch", "scribe"],
      }),
    ).toEqual(["arch", "scribe"]);
  });

  it("segments a live handle as the text @all, not as the names it stands for", () => {
    // AGT-FR-29 / AGT-FR-40: a reader sees the question asked of everyone rather
    // than a list they have to count.
    expect(segmentBody("@all is this two specs?", allReady)).toEqual([
      { kind: "tag", text: "@all" },
      { kind: "text", text: " is this two specs?" },
    ]);
  });

  it("leaves a handle that resolves to nobody as prose", () => {
    // AGT-FR-38 / AGT-FR-28: marking it would promise a reader participants who
    // were never in the conversation.
    const body = "@all take a look";
    expect(segmentBody(body, { nicknames: [], ready: [] })).toEqual([
      { kind: "text", text: body },
    ]);
  });

  it("still leaves an address and a code sample alone beside a live handle", () => {
    // AGT-FR-29, AGT-FR-28, AGT-FR-38: the emphasis marks a live tag and nothing else.
    expect(
      segmentBody("@all and @arch, plus me@example.com and the @media rule", allReady),
    ).toEqual([
      { kind: "tag", text: "@all" },
      { kind: "text", text: " and " },
      { kind: "tag", text: "@arch" },
      { kind: "text", text: ", plus me@example.com and the @media rule" },
    ]);
  });

  it("treats a bare nickname list as a roster that can answer", () => {
    // The normalisation the pure callers rely on: a fixture with no availability
    // to model means "every one of these can answer".
    expect(addressedNicknames("@all thoughts?", ENROLLED)).toEqual(ENROLLED);
  });
});

describe("the handle's edges", () => {
  const allReady = { nicknames: ["arch", "sec"], ready: ["arch", "sec"] };

  it("means the room even if an agent somehow carries the nickname", () => {
    // AGT-FR-41 reserves `all`, so no agent should carry it — but a store written
    // before the reservation, or edited by hand, can still hold one. The handle is
    // matched before the roster is consulted precisely so it keeps meaning one
    // thing in every project rather than quietly addressing that one agent.
    const roster = { nicknames: ["all", "arch"], ready: ["all", "arch"] };
    const tags = resolveTags("@all thoughts?", roster);
    expect(tags).toHaveLength(1);
    expect(tags[0].nickname).toBe("all");
    expect(tags[0].nicknames).toEqual(["all", "arch"]);
  });

  it("does not fire on a nickname that merely begins with the word", () => {
    // `@allan` is one agent, not everybody. The comparison is exact, not a prefix.
    const roster = { nicknames: ["allan", "call"], ready: ["allan", "call"] };
    expect(addressedNicknames("@allan take a look", roster)).toEqual(["allan"]);
    expect(addressedNicknames("@call take a look", roster)).toEqual(["call"]);
  });

  it("resolves through the punctuation that ends a sentence", () => {
    // The `TRAILING` strip is shared with every nickname, and the handle is
    // compared exactly against what it leaves — so a change to that strip would
    // otherwise silently stop `@all,` from addressing anybody.
    for (const written of ["@all.", "@all,", "@all!", "@ALL?", "(@all)"]) {
      expect(addressedNicknames(`${written} thoughts`, allReady)).toEqual([
        "arch",
        "sec",
      ]);
    }
  });

  it("reports whether a body addressed the room", () => {
    // Feeds the one log record that explains why several agents answered at once.
    // Unconditionally true would put a record in a bounded ring buffer for every
    // ordinary tagged comment, pushing out the evidence someone is looking for.
    expect(addressesEveryone("@all thoughts?", allReady)).toBe(true);
    expect(addressesEveryone("@arch thoughts?", allReady)).toBe(false);
    expect(addressesEveryone("no tags here", allReady)).toBe(false);
    // Not "addressed the room" when the room is empty — nobody was addressed.
    expect(addressesEveryone("@all thoughts?", { nicknames: [], ready: [] })).toBe(
      false,
    );
  });
});
