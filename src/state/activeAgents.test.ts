/**
 * Who a conversation is being held with, and who a message in it reaches
 * (`CMT-comments.md` CTA-FR-LCFU … CTA-FR-DGOC, CTA-FR-JQDM, CTA-FR-IAKP).
 *
 * The rules are pure over the comments a surface already holds, so every case
 * that matters — an agent named and then not named again, an agent naming
 * another agent, a mention that resolves to nobody, a conversation nobody has
 * addressed anyone in — is exercised here without a backend and without a
 * render.
 */
import { describe, expect, it } from "vitest";

import {
  activeAgents,
  decisionTargets,
  dispatchTargets,
  composerPlaceholder,
  pendingContributionsOf,
  withoutAwaitingReply,
} from "./activeAgents";
import type { AgentTurn, Comment, Discussion, Participant } from "../types";
import { artifactCommentOrigin, artifactDiscussionOrigin } from "../test/origins";

const ROSTER = ["arch", "sec"];
/** Three enrolled agents, the last of which cannot answer (AGT-FR-36). */
const DEGRADED = { nicknames: ["arch", "sec", "scribe"], ready: ["arch", "sec"] };

const human: Participant = { kind: "human", login: "raver119" };
const arch: Participant = { kind: "agent", agentId: "a1", handle: "arch" };

function comment(id: string, author: Participant, body: string): Comment {
  return {
    id,
    author,
    body,
    quotes: [],
    attachments: [],
    createdAt: "2026-02-01T00:00:00Z",
  };
}

function discussion(comments: Comment[]): Discussion {
  return {
    id: "t1",
    target: { kind: "draft", draftId: "d1" },
    fragmentTarget: null,
    comments,
    locked: false,
    resolved: false,
    createdAt: "2026-02-01T00:00:00Z",
    updatedAt: "2026-02-01T00:00:00Z",
  };
}

/** The same conversation anchored in a passage rather than about the whole. */
function anchored(comments: Comment[]): Discussion {
  return {
    ...discussion(comments),
    target: { kind: "artifact", artifactId: "a.md" },
    fragmentTarget: {
      owner: { kind: "artifact", artifactId: "a.md" },
      path: "a.md",
      quote: "there are three",
      start: 0,
      end: 15,
    },
  };
}

describe("CTA-FR-XBIN: a conversation's active agents", () => {
  it("is exactly what the newest tag-bearing human comment names", () => {
    // CTA-FR-QUXJ: the author turned from `@arch` to the other two, so the two are
    // who the conversation is now being held with and `@arch` is not.
    const thread = discussion([
      comment("c1", human, "@arch please review"),
      comment("c2", arch, "Here is what I would do…"),
      comment("c3", human, "@sec @scribe now you two"),
      comment("c4", arch, "…"),
    ]);
    expect(activeAgents(thread, ["arch", "sec", "scribe"])).toEqual([
      "sec",
      "scribe",
    ]);
  });

  it("passes over a comment carrying no tag to the one before it", () => {
    // CTA-FR-HCMJ: an untagged comment settles nothing, so the set is still what
    // the newest naming comment said.
    const thread = discussion([
      comment("c1", human, "@arch please review"),
      comment("c2", arch, "Two, and the second is smaller."),
      comment("c3", human, "and the second one?"),
    ]);
    expect(activeAgents(thread, ROSTER)).toEqual(["arch"]);
  });

  it("passes over a mention that resolves to nobody", () => {
    // CTA-FR-BMHF, CTA-FR-IMKG / AGT-FR-28: an unmatched `@` is ordinary text, so it replaces
    // nothing and the reading carries on past it.
    const thread = discussion([
      comment("c1", human, "@arch please review"),
      comment("c2", human, "@nobody what about this?"),
    ]);
    expect(activeAgents(thread, ROSTER)).toEqual(["arch"]);
  });

  it("settles nothing from an agent's own comment, wherever it sits", () => {
    // CTA-FR-XTZC / CTA-FR-DWCK: a tag an agent writes addresses nobody and hands
    // the conversation to nobody, so who the author is talking to stays the
    // author's decision alone.
    const thread = discussion([
      comment("c1", human, "@arch thoughts?"),
      comment("c2", arch, "@sec should weigh in"),
    ]);
    expect(activeAgents(thread, ROSTER)).toEqual(["arch"]);
  });

  it("is nobody in a conversation whose only tag an agent wrote", () => {
    // CTA-FR-XTZC, CTA-FR-DWCK: no human comment names anybody, so nothing is dispatched
    // anywhere and the composer says so.
    const thread = discussion([
      comment("c1", human, "thinking aloud"),
      comment("c2", arch, "@sec should weigh in"),
    ]);
    expect(activeAgents(thread, ROSTER)).toEqual([]);
  });

  it("is nobody in a conversation nobody has addressed an agent in", () => {
    expect(activeAgents(discussion([comment("c1", human, "hm")]), ROSTER)).toEqual(
      [],
    );
    expect(activeAgents(discussion([]), ROSTER)).toEqual([]);
  });

  it("makes every agent one comment names active together", () => {
    // CTA-FR-LCFU, CTA-FR-XBIN.
    const thread = discussion([comment("c1", human, "@arch @sec please review")]);
    expect(activeAgents(thread, ROSTER)).toEqual(["arch", "sec"]);
  });

  it("names an agent whatever case it was tagged in", () => {
    // AGT-FR-24: matched without regard to case, reported in the spelling the
    // registry holds, so the backend is named the way it names itself.
    const thread = discussion([comment("c1", human, "@ARCH thoughts?")]);
    expect(activeAgents(thread, ROSTER)).toEqual(["arch"]);
  });

  it("reads an anchored thread on exactly the same terms", () => {
    // CTA-FR-DGOC: the kind decides nothing. A remark pinned to a line is a
    // conversation the author is holding with somebody as much as a discussion.
    const thread = anchored([
      comment("c1", human, "@arch have another look"),
      comment("c2", arch, "Done."),
    ]);
    expect(activeAgents(thread, ROSTER)).toEqual(["arch"]);
  });

  it("re-resolves `@all` against the roster as it now stands", () => {
    // CTA-FR-YGYP / AGT-FR-37, AGT-FR-40: the stored body still reads `@all`, and
    // what it means moves with the project's enrolment.
    const thread = discussion([
      comment("c1", human, "@all where should graduation live?"),
    ]);
    expect(activeAgents(thread, DEGRADED)).toEqual(["arch", "sec"]);
    expect(
      activeAgents(thread, {
        nicknames: ["arch", "sec", "scribe"],
        ready: ["arch", "sec", "scribe"],
      }),
    ).toEqual(["arch", "sec", "scribe"]);
    expect(activeAgents(thread, { nicknames: ["arch"], ready: ["arch"] })).toEqual([
      "arch",
    ]);
    expect(thread.comments[0].body).toBe("@all where should graduation live?");
  });

  it("is nobody for an `@all` no enrolled agent can answer", () => {
    // AGT-FR-38: the handle resolves to nobody and the body carrying it is an
    // ordinary message in every respect.
    const thread = discussion([comment("c1", human, "@all thoughts?")]);
    expect(activeAgents(thread, { nicknames: [], ready: [] })).toEqual([]);
    expect(activeAgents(thread, { nicknames: ["arch"], ready: [] })).toEqual([]);
  });

  it("settles nothing from an agent's own `@all`", () => {
    // CTA-FR-YWSU: otherwise one agent could summon the whole room and the
    // conversation would go on without the person paying for it.
    const thread = discussion([
      comment("c1", human, "@arch have a look"),
      comment("c2", arch, "@all should weigh in"),
    ]);
    expect(activeAgents(thread, DEGRADED)).toEqual(["arch"]);
  });

  it("reads a locked conversation like any other", () => {
    // CMT-FR-15: the lock is what stops the conversation — no composer, so
    // nothing to route — rather than a second rule in here.
    const thread = { ...discussion([comment("c1", human, "@arch ?")]), locked: true };
    expect(activeAgents(thread, ROSTER)).toEqual(["arch"]);
  });
});

describe("CTA-FR-QNBS: who one posted comment reaches", () => {
  const active = discussion([comment("c1", human, "@arch @sec please review")]);

  it("reaches exactly the agents its own tags resolve to", () => {
    expect(dispatchTargets(active, "@arch only, please", ROSTER)).toEqual(["arch"]);
  });

  it("makes an explicit mention REPLACE the set rather than join it", () => {
    // CTA-FR-LCFU, CTA-FR-XBIN: an author who turns to somebody else has turned to them.
    const turned = discussion([
      ...active.comments,
      comment("c2", human, "@sec what do you think?"),
    ]);
    expect(dispatchTargets(turned, "and now?", ROSTER)).toEqual(["sec"]);
  });

  it("reaches the active agents when it carries no tag", () => {
    // CTA-FR-HCMJ, CTA-FR-LCFU, CTA-FR-XBIN: a follow-up reaches whoever the author was last
    // talking to without their being named again.
    expect(dispatchTargets(active, "and the archive part?", ROSTER)).toEqual([
      "arch",
      "sec",
    ]);
  });

  it("leaves the set exactly as it stands for an untagged comment", () => {
    // A remark addressed to nobody dismisses nobody.
    const after = discussion([
      ...active.comments,
      comment("c2", human, "and the archive part?"),
    ]);
    expect(activeAgents(after, ROSTER)).toEqual(["arch", "sec"]);
    expect(dispatchTargets(after, "still?", ROSTER)).toEqual(["arch", "sec"]);
  });

  it("reaches nobody in a conversation with no active agent", () => {
    const alone = discussion([comment("c1", human, "thinking aloud")]);
    expect(dispatchTargets(alone, "still thinking", ROSTER)).toEqual([]);
  });

  it("dispatches for no unresolved mention, and lets it replace nothing", () => {
    // CTA-FR-BMHF, CTA-FR-IMKG / AGT-FR-28: `@nobody` is ordinary text, so no turn is
    // dispatched **for it**. CTA-FR-QUXJ decides the rest: a comment carrying "no
    // tag, or none that resolves to anybody" reaches the conversation's active
    // agents — "the agents its own resolvable tags name, **or failing those**
    // the conversation's active agents". So the comment reaches `@arch` and the
    // unresolved name reaches nobody.
    const thread = discussion([comment("c1", human, "@arch please review")]);
    expect(dispatchTargets(thread, "@nobody thoughts?", ROSTER)).toEqual(["arch"]);
    // And the mention replaced nothing: the comment after it still reaches the
    // agent that was active before it.
    const after = discussion([
      ...thread.comments,
      comment("c2", human, "@nobody thoughts?"),
    ]);
    expect(activeAgents(after, ROSTER)).toEqual(["arch"]);
    expect(dispatchTargets(after, "well?", ROSTER)).toEqual(["arch"]);
  });

  it("dispatches once per distinct agent however often a message names it", () => {
    // CTA-FR-FAWE: a repeated tag in one message is emphasis, not a
    // second question.
    expect(dispatchTargets(active, "@arch @arch again please", ROSTER)).toEqual([
      "arch",
    ]);
  });

  it("dispatches once per agent rather than once per tag for `@all @arch`", () => {
    // CTA-FR-FAWE: the handle and the nickname are two ways of naming one agent.
    expect(
      dispatchTargets(active, "@all @arch again please", {
        nicknames: ["arch", "sec"],
        ready: ["arch", "sec"],
      }),
    ).toEqual(["arch", "sec"]);
  });

  it("treats the opening message as the degenerate case of the same rule", () => {
    const opened = discussion([comment("c1", human, "@arch where to?")]);
    expect(dispatchTargets(opened, "@arch where to?", ROSTER)).toEqual(["arch"]);
    const untagged = discussion([comment("c1", human, "where to?")]);
    expect(dispatchTargets(untagged, "where to?", ROSTER)).toEqual([]);
  });

  it("routes an anchored thread exactly as it routes a discussion", () => {
    // CTA-FR-DGOC, CMT-FR-61, CTA-FR-RPVU, ACT-FR-13, CTA-FR-DNMV.
    const thread = anchored([
      comment("c1", human, "@arch have another look"),
      comment("c2", arch, "Done."),
    ]);
    expect(dispatchTargets(thread, "and the second one?", ROSTER)).toEqual(["arch"]);
  });

  it("is the same before a relaunch and after one", () => {
    // CTA-FR-KVIF, CTA-FR-ELIJ, CTA-FR-QUXJ / AGC-FR-23: the set is read out of the comments the
    // relaunch did not touch, so nothing had to survive it.
    const thread = discussion([
      comment("c1", human, "@arch please review"),
      comment("c2", arch, "?"),
      comment("c3", human, "@sec @scribe now you two"),
    ]);
    const roster = ["arch", "sec", "scribe"];
    const before = dispatchTargets(thread, "go on", roster);
    // A relaunch keeps the comments and drops every turn; nothing else fed this.
    const after = dispatchTargets(thread, "go on", roster);
    expect(before).toEqual(["sec", "scribe"]);
    expect(after).toEqual(before);
  });
});

// ---------------------------------------------------------------------------
// CTA-FR-IEPG: an agent that asked the author something
// ---------------------------------------------------------------------------

function turn(
  id: string,
  nickname: string,
  state: AgentTurn["state"],
  threadId = "t1",
): AgentTurn {
  return {
    id,
    agentId: `agent-${nickname}`,
    nickname,
    origin: artifactCommentOrigin(threadId),
    triggerCommentId: "c1",
    state,
    failure: null,
    retryPermitted: false,
    imagesOmitted: false,
    activeToolCalls: [],
    startedAt: "2026-02-01T00:00:00Z",
    endedAt: state === "running" ? null : "2026-02-01T00:01:00Z",
  };
}

describe("CTA-FR-DMNI: an awaiting turn adds no recipient", () => {
  it("does not dispatch to an agent that asked but is not active", () => {
    // CTA-FR-JQDM, CTA-FR-DMNI, CTA-FR-IEPG, CTA-FR-JQDM: the author steered at somebody else, and an author
    // who has turned to somebody else has turned to them.
    const thread = anchored([
      comment("c1", human, "@arch please review"),
      comment("c2", arch, "One spec or two?"),
      comment("c3", human, "@sec what do you think?"),
    ]);
    expect(dispatchTargets(thread, "and now?", ROSTER)).toEqual(["sec"]);
  });

  it("does dispatch to it when it is still the active agent", () => {
    // CTA-FR-HCMJ: the agent that asked is answered because the author's next
    // comment reaches it, not because a wait was remembered.
    const thread = anchored([
      comment("c1", human, "@arch which session?"),
      comment("c2", arch, "One spec or two?"),
    ]);
    expect(dispatchTargets(thread, "two, definitely", ROSTER)).toEqual(["arch"]);
    // And again for the comment after that — the set is what the comments say.
    const after = anchored([
      ...thread.comments,
      comment("c3", human, "two, definitely"),
      comment("c4", arch, "Understood."),
    ]);
    expect(dispatchTargets(after, "and the rest?", ROSTER)).toEqual(["arch"]);
  });
});

describe("CTA-FR-JQDM / AGC-FR-29: dispatching retires the wait", () => {
  const awaiting = [
    turn("1", "arch", "awaiting_reply"),
    turn("2", "sec", "awaiting_reply", "t-other"),
    turn("3", "scribe", "running"),
  ];

  it("drops only the dispatched agent's wait in the dispatched conversation", () => {
    expect(withoutAwaitingReply(awaiting, ["arch"], "t1").map((t) => t.id)).toEqual(
      ["2", "3"],
    );
  });

  it("leaves the same agent's wait in another conversation alone", () => {
    expect(withoutAwaitingReply(awaiting, ["sec"], "t1").map((t) => t.id)).toEqual([
      "1",
      "2",
      "3",
    ]);
  });

  it("leaves a wait nobody was dispatched for standing", () => {
    // DCR-FR-15, CTA-FR-LCFU, CTA-FR-QUXJ, CTA-FR-JQDM / PCR-FR-14: an agent the author has turned away from keeps its
    // awaiting turn, no dispatch having retired it.
    expect(withoutAwaitingReply(awaiting, ["sec"], "t1")[0].nickname).toBe("arch");
  });
});

// ---------------------------------------------------------------------------
// DCR-FR-15 / PCR-FR-14: a proposal decision
// ---------------------------------------------------------------------------

describe("DCR-FR-15 / PCR-FR-14: who a decision reaches", () => {
  const proposalThread = discussion([
    comment("c1", human, "@arch @sec please review"),
    comment("c2", arch, "Here is a change I would make."),
  ]);

  it("reaches every active agent when the decision carries no feedback", () => {
    // DCR-FR-15, AGC-FR-04, AGC-FR-29, CMT-FR-79, CTA-FR-LCFU, CTA-FR-QUXJ / PCR-FR-14: the decision is an untagged human comment routed by
    // the active set — both agents, not the proposer alone.
    expect(decisionTargets(proposalThread, "c9", "", ROSTER)).toEqual([
      "arch",
      "sec",
    ]);
  });

  it("reaches the agents the feedback names, and nobody else", () => {
    // DCR-FR-15, CTA-FR-LCFU, CTA-FR-QUXJ / PCR-FR-14, PCR-FR-13: feedback that names agents is a tagged comment.
    expect(
      decisionTargets(proposalThread, "c9", "@scribe please re-read this", {
        nicknames: ["arch", "sec", "scribe"],
        ready: ["arch", "sec", "scribe"],
      }),
    ).toEqual(["scribe"]);
  });

  it("does not reach the proposing agent where the author has turned away", () => {
    // DCR-FR-15, CTA-FR-LCFU, CTA-FR-QUXJ, CTA-FR-JQDM / PCR-FR-14: there is no route by which a proposal's own agent
    // is dispatched to for having proposed.
    const turned = discussion([
      ...proposalThread.comments,
      comment("c3", human, "@sec take this over"),
    ]);
    expect(decisionTargets(turned, "c9", "", ROSTER)).toEqual(["sec"]);
  });

  it("reaches nobody where the conversation is addressed to nobody", () => {
    // DCR-FR-15, CTA-FR-LCFU, CTA-FR-QUXJ / PCR-FR-14, PCR-FR-13: the decision still stands and is not refused.
    const unaddressed = discussion([
      comment("c1", human, "leaving this here"),
      comment("c2", arch, "Here is a change I would make."),
    ]);
    expect(decisionTargets(unaddressed, "c9", "", ROSTER)).toEqual([]);
  });

  it("reaches nobody where the conversation could not be read", () => {
    expect(decisionTargets(undefined, "c9", "", ROSTER)).toEqual([]);
    // …but a decision naming somebody still reaches them.
    expect(decisionTargets(undefined, "c9", "@arch see this", ROSTER)).toEqual([
      "arch",
    ]);
  });

  it("reads the set off the decision's own comment once it has landed", () => {
    // The change event may arrive before the decision call resolves, in which
    // case the appended body — feedback included — is what settles the set.
    const landed = discussion([
      ...proposalThread.comments,
      comment(
        "c9",
        human,
        "Accepted the proposed change to `ui/spec.md`.\n\n@scribe please re-read this",
      ),
    ]);
    expect(
      decisionTargets(landed, "c9", "", {
        nicknames: ["arch", "sec", "scribe"],
        ready: ["arch", "sec", "scribe"],
      }),
    ).toEqual(["scribe"]);
  });

  it("is unmoved by the path the backend's own sentence names", () => {
    // The sentence carries the path inside a code span, which `tagCandidates`
    // skips — so a path holding an `@` addresses nobody.
    const landed = discussion([
      ...proposalThread.comments,
      comment("c9", human, "Accepted the proposed change to `ui/@arch.md`."),
    ]);
    expect(decisionTargets(landed, "c9", "", ROSTER)).toEqual(["arch", "sec"]);
  });
});

// ---------------------------------------------------------------------------
// CTA-FR-IAKP: what the composer's placeholder names
// ---------------------------------------------------------------------------

describe("CTA-FR-IAKP: the composer placeholder", () => {
  it("names the active agents of a conversation that has some", () => {
    // CTA-FR-RDSD, CMT-FR-11, AGT-FR-29, AGT-FR-37: `Reply to @arch, @sec…`, in the order the comment that
    // settled the set names them.
    expect(composerPlaceholder(["arch", "sec"])).toBe("Reply to @arch, @sec…");
  });

  it("reads `Reply…` where the conversation has no active agent", () => {
    // CTA-FR-XTZC, CTA-FR-DWCK: an ordinary state rather than an error — it names no agent
    // anywhere.
    expect(composerPlaceholder([])).toBe("Reply…");
  });

  it("reads back what the conversation's own comments settle", () => {
    const thread = discussion([
      comment("c1", human, "@arch please review"),
      comment("c2", arch, "@sec should weigh in"),
    ]);
    expect(composerPlaceholder(activeAgents(thread, ROSTER))).toBe(
      "Reply to @arch…",
    );
  });
});

describe("CTA-FR-ZOLW, CTA-FR-XMCQ: one pending contribution for each agent", () => {
  function running(
    id: string,
    agentId: string,
    over: { discussionId?: string; startedAt?: string; state?: AgentTurn["state"] } = {},
  ): AgentTurn {
    return {
      id,
      agentId,
      nickname: agentId,
      origin: artifactDiscussionOrigin(over.discussionId ?? "d1", "a.md"),
      triggerCommentId: "c1",
      state: over.state ?? "running",
      failure: null,
      retryPermitted: false,
      startedAt: over.startedAt ?? "2026-02-01T10:00:00Z",
      endedAt: null,
      activeToolCalls: [],
    } as unknown as AgentTurn;
  }
  const shape = (turns: AgentTurn[]) =>
    pendingContributionsOf(turns).map((c) => ({ turn: c.turn.id, ids: [...c.turnIds] }));

  it("two running turns of one agent are one contribution holding both", () => {
    expect(shape([running("t1", "arch"), running("t2", "arch")])).toEqual([
      { turn: "t2", ids: ["t1", "t2"] },
    ]);
  });

  it("two agents are two contributions, and one agent in two discussions is two", () => {
    expect(shape([running("t1", "arch"), running("t2", "sec")])).toHaveLength(2);
    expect(
      shape([running("t1", "arch"), running("t2", "arch", { discussionId: "d2" })]),
    ).toHaveLength(2);
  });

  it.each(["delivered", "failed", "cancelled", "awaiting_reply"] as const)(
    "a %s turn is no contribution",
    (state) => {
      expect(shape([running("t1", "arch", { state })])).toEqual([]);
    },
  );

  it("no turns give no contributions", () => {
    expect(pendingContributionsOf([])).toEqual([]);
  });

  it("the contribution reads the newest turn by its start, not by its place", () => {
    const newer = running("t2", "arch", { startedAt: "2026-02-01T10:05:00Z" });
    const older = running("t1", "arch", { startedAt: "2026-02-01T10:00:00Z" });
    expect(shape([newer, older])).toEqual([{ turn: "t2", ids: ["t2", "t1"] }]);
  });

  it("an equal start goes to the later turn, and an unreadable start loses", () => {
    expect(shape([running("t1", "arch"), running("t2", "arch")])[0].turn).toBe("t2");
    expect(
      shape([running("t1", "arch"), running("t2", "arch", { startedAt: "?" })])[0].turn,
    ).toBe("t1");
    expect(
      shape([
        running("t1", "arch", { startedAt: "?" }),
        running("t2", "arch", { startedAt: "?" }),
      ])[0].turn,
    ).toBe("t2");
  });

  it("each contribution stands in the order of that agent's oldest running turn", () => {
    const order = (turns: AgentTurn[]) =>
      pendingContributionsOf(turns).map((c) => c.turn.agentId);
    const t1 = running("t1", "arch", { startedAt: "2026-02-01T10:00:00Z" });
    const t2 = running("t2", "sec", { startedAt: "2026-02-01T10:01:00Z" });
    const t3 = running("t3", "arch", { startedAt: "2026-02-01T10:02:00Z" });
    expect(order([t1, t2, t3])).toEqual(["arch", "sec"]);
    expect(order([t2, t3])).toEqual(["sec", "arch"]);
  });

  it("a late dispatch result that moves a turn to the end does not move its row", () => {
    // `upsertDiscussionTurn` appends the turn the dispatch call returned, so the
    // held order can differ from the order the turns started in.
    const t1 = running("t1", "arch", { startedAt: "2026-02-01T10:00:00Z" });
    const t2 = running("t2", "sec", { startedAt: "2026-02-01T10:01:00Z" });
    expect(pendingContributionsOf([t2, t1]).map((c) => c.turn.agentId)).toEqual([
      "arch",
      "sec",
    ]);
  });

  it("equal starts keep the held order", () => {
    const order = pendingContributionsOf([running("t2", "sec"), running("t1", "arch")]);
    expect(order.map((c) => c.turn.agentId)).toEqual(["sec", "arch"]);
  });
});
