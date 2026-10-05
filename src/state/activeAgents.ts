/**
 * Who a conversation is being held with, and who a message in it reaches
 * (`CMT-comments.md` CTA-FR-LCFU … CTA-FR-DGOC, CTA-FR-JQDM).
 *
 * A conversation's **active agents** are the agents its author last addressed in
 * it. Who a message reaches is who the author last named: the newest human
 * comment that names anybody at all settles the set, and every message written
 * afterwards without naming anyone goes to exactly those — so a collaborator
 * asked to look at something keeps up with what follows rather than being
 * re-summoned line by line, and naming somebody else hands the conversation to
 * them instead.
 *
 * Three properties hold this module together, and every rule below exists to
 * keep one of them:
 *
 * - **Derived, never recorded** (CTA-FR-LCFU). Nothing stores the set, no
 *   operation saves it, and no payload carries it. It is read back out of the
 *   comments — which persist in the project's own append-only log — whenever it
 *   is wanted, which is why it is the same after a relaunch as it was before
 *   one, and the same in a card, in an overlay, and in a tab.
 * - **Read against the roster as it stands** (AGT-FR-37). `@all` means the room
 *   as it now is, so an agent enrolled since the comment was written is active
 *   under it and one since withdrawn is not. Nothing here caches a resolution.
 * - **The author's decision alone** (CTA-FR-DWCK). An agent's own comment settles
 *   nothing wherever it sits in the order, so two agents cannot recruit each
 *   other and go on talking without the person paying for it.
 *
 * The set binds **every** conversation this application holds and distinguishes
 * none of them (CTA-FR-DGOC): a thread anchored in a passage and a discussion
 * about the whole derive it from their own comments on identical terms and
 * dispatch by the same rules, in the artifact, draft, and note scopes alike.
 *
 * Pure and separate from the hooks so every rule is testable without a backend:
 * who is active is a fact about the comments a surface already holds, and costs
 * no call at all.
 */

import { addressedNicknames } from "../components/agentTags";
import type { Roster } from "../components/agentTags";
import type { AgentTurn, Discussion } from "../types";

/**
 * CTA-FR-XBIN: the conversation's **active agents** — exactly the agents the
 * newest tag-bearing **human** comment names, and no other.
 *
 * The reading runs newest to oldest and stops at the first human comment
 * carrying at least one tag that resolves against `roster`. That one comment
 * settles the whole set, which is what makes an explicit mention **replace**
 * rather than join (CTA-FR-QUXJ): an author who turns to somebody else has turned
 * to them, and the agents named before are no longer being talked to.
 *
 * Four cases fall out of the loop rather than needing rules of their own:
 * - a comment naming several agents makes all of them active together;
 * - a comment carrying `@all` makes every enrolled agent that can answer active,
 *   re-resolved here rather than frozen when it was typed (AGT-FR-36/37);
 * - a comment carrying no tag, or none that resolves to anybody, settles nothing
 *   and the reading passes over it — an unresolved mention replaces nothing;
 * - a conversation in which no human comment carries a resolvable tag has no
 *   active agent at all, which is an ordinary state rather than an error.
 *
 * An **agent's** comment is skipped wherever it sits (CTA-FR-DWCK): a tag one
 * agent writes — naming a colleague, quoting an earlier message, or carrying
 * `@all` — addresses nobody and hands the conversation to nobody.
 */
export function activeAgents(
  thread: Pick<Discussion, "comments">,
  roster: Roster,
): string[] {
  for (let i = thread.comments.length - 1; i >= 0; i -= 1) {
    const comment = thread.comments[i];
    if (comment.author.kind !== "human") continue;
    const named = addressedNicknames(comment.body, roster);
    if (named.length > 0) return named;
  }
  return [];
}

/**
 * CTA-FR-QNBS: the distinct agents one posted **human** comment dispatches a turn
 * for. Three rules, and they are the whole of who a comment reaches:
 *
 * - the comment's own tags resolve to at least one enrolled agent → exactly
 *   those, and by standing in the conversation they become its active agents
 *   from then on;
 * - the comment carries no tag, or none that resolves → the conversation's
 *   active agents, the set left exactly as it stands, so a follow-up reaches
 *   whoever the author was last talking to and a remark addressed to nobody
 *   dismisses nobody;
 * - the conversation has no active agent at all → nobody, and no call is made.
 *
 * **No other state of the conversation adds a recipient** — an agent awaiting a
 * reply among them (CTA-FR-DMNI). An agent that asked a question is answered
 * because the author's next comment names it or because it is the conversation's
 * active agent, and an author who has turned to somebody else has turned to
 * them.
 *
 * `thread` is the conversation **as it stands with `body` appended**, which is
 * what makes the opening message of a conversation the degenerate case of this
 * rule rather than a second one. Passing it without the appended comment gives
 * the same answer, an untagged comment settling nothing and a tagged one being
 * answered for by `body` itself.
 */
export function dispatchTargets(
  thread: Pick<Discussion, "comments">,
  body: string,
  roster: Roster,
): string[] {
  // Taken over the *agents* rather than over the tags, so `@all @arch` asks
  // `@arch` once and a nickname written twice over asks once: the handle and the
  // nickname are two ways of naming one agent, and the author asked once
  // (CTA-FR-RPVU).
  const tagged = addressedNicknames(body, roster);
  if (tagged.length > 0) return tagged;
  return activeAgents(thread, roster);
}

/**
 * DCR-FR-15 / PCR-FR-14: who a **proposal decision** reaches.
 *
 * A decision's comment is one the author wrote (DCP-FR-15, PCP-FR-16), so it
 * routes on exactly the terms any other human comment in that conversation
 * routes on (CTA-FR-QNBS) — there is no route by which a proposal's own agent is
 * dispatched to for having proposed.
 *
 * Two readings of the same rule, and which one applies is decided by what the
 * surface already holds rather than by anything about the decision:
 *
 * - the conversation it holds **already carries** the decision's comment, the
 *   change event having landed before the decision call resolved → the set is
 *   read straight off the conversation as it now stands, that comment's own body
 *   carrying whatever the author's feedback named;
 * - it does not yet → the same answer is derived from the conversation as it
 *   stood plus the `feedback` the author typed, which is where every tag in the
 *   decision's body comes from. The sentence the backend prefixes names the
 *   path inside a code span and carries no tag of its own.
 *
 * `thread` is optional because a conversation that could not be read leaves the
 * decision standing: it dispatches to nobody rather than to somebody guessed.
 */
export function decisionTargets(
  thread: Pick<Discussion, "comments"> | undefined,
  decisionCommentId: string,
  feedback: string,
  roster: Roster,
): string[] {
  if (thread?.comments.some((c) => c.id === decisionCommentId)) {
    return activeAgents(thread, roster);
  }
  return dispatchTargets(thread ?? { comments: [] }, feedback, roster);
}

/**
 * CTA-FR-JQDM / AGC-FR-29: the turns left once dispatching for `nicknames` has
 * ended their wait in this conversation.
 *
 * A fresh turn for an agent in an origin retires whatever that origin owed it,
 * and the backend does it silently — a terminal event was already emitted when
 * the turn ended awaiting a reply, and AGC-FR-21 allows exactly one. So the
 * surface that issued the dispatch is the thing that must forget it, being
 * itself the cause.
 *
 * Nothing about routing turns on this any more: an awaiting turn adds no
 * recipient (CTA-FR-QNBS). What it keeps honest is the turn list a surface holds,
 * so what it reports about this conversation matches what `"list agent turns"`
 * would now return.
 */
export function withoutAwaitingReply(
  turns: readonly AgentTurn[],
  nicknames: readonly string[],
  threadId: string,
): AgentTurn[] {
  return turns.filter(
    (turn) =>
      !(
        turn.state === "awaiting_reply" &&
        turn.origin.discussionId === threadId &&
        nicknames.includes(turn.nickname)
      ),
  );
}

/**
 * CTA-FR-QXIG / AGC-FR-34: a turn's record, folded into the set a surface holds.
 *
 * A turn already held is replaced **in the position it already occupies**, and
 * only a turn the surface has not seen is appended. That is what CTA-FR-QXIG's
 * "occupying the position that agent's answer will take" costs now that a
 * running turn is reported many times rather than once: a card holding a
 * pending contribution for `@arch` and one for `@sec` (CMT-FR-79) would
 * otherwise swap them every time either agent began or finished a tool call,
 * and the answer that replaces one would land where the other's used to be.
 *
 * A terminal turn leaves — except `awaiting_reply`, which is terminal and
 * outstanding at once (CMT-FR-22, CTA-FR-JQDM): it is still returned by
 * `"list agent turns"`, it renders nothing, and it obliges no dispatch, so a
 * surface holds it only so that what it reports about the conversation matches
 * what a fresh read would say.
 */
export function mergeKnownTurn(
  prev: readonly AgentTurn[],
  turn: AgentTurn,
): AgentTurn[] {
  const outstanding = turn.state === "running" || turn.state === "awaiting_reply";
  const at = prev.findIndex((t) => t.id === turn.id);
  if (!outstanding) return prev.filter((t) => t.id !== turn.id);
  if (at === -1) return [...prev, turn];
  const next = [...prev];
  next[at] = turn;
  return next;
}

/** CTA-FR-ZOLW: the one pending contribution of one agent in one discussion. */
export interface PendingContribution {
  /** The agent's newest running turn, whose status the row reads (CTA-FR-XMCQ). */
  turn: AgentTurn;
  /** Every running turn of that agent there, which the row's cancel ends. */
  turnIds: readonly string[];
}

function startedMs(turn: AgentTurn): number {
  const ms = Date.parse(turn.startedAt);
  return Number.isNaN(ms) ? -Infinity : ms;
}

/**
 * CTA-FR-ZOLW, CTA-FR-XMCQ: the running turns as one pending contribution for
 * each agent in each discussion. A contribution reads that agent's newest turn
 * and stands in the order of that agent's oldest turn. The order comes from the
 * start of each turn and not from the held order, which a late dispatch result
 * changes. A start that cannot be read counts as the oldest.
 */
export function pendingContributionsOf(
  turns: readonly AgentTurn[],
): PendingContribution[] {
  const groups = new Map<
    string,
    { turn: AgentTurn; turnIds: string[]; oldest: number }
  >();
  for (const turn of turns) {
    if (turn.state !== "running") continue;
    const key = `${turn.origin.discussionId}\u0000${turn.agentId}`;
    const held = groups.get(key);
    if (!held) {
      groups.set(key, { turn, turnIds: [turn.id], oldest: startedMs(turn) });
      continue;
    }
    held.turnIds.push(turn.id);
    held.oldest = Math.min(held.oldest, startedMs(turn));
    if (startedMs(turn) >= startedMs(held.turn)) held.turn = turn;
  }
  // A stable sort, so equal starts keep the held order.
  return [...groups.values()]
    .sort((a, b) => (a.oldest === b.oldest ? 0 : a.oldest < b.oldest ? -1 : 1))
    .map(({ turn, turnIds }) => ({ turn, turnIds }));
}

/**
 * CTA-FR-IAKP: the **placeholder** a composer ready to take an **untagged**
 * comment carries — who that comment will reach, and the only place a
 * conversation states it.
 *
 * A **reading** and presented as one, rather than as anything the conversation
 * has saved: the caller recomputes it from the two inputs it already holds —
 * the conversation's comments and the project's roster — so an agent enrolled
 * or withdrawn is named or dropped as soon as the surface knows of it, and an
 * `@all` standing behind the set is answered for by the room as it now is
 * (AGT-FR-37).
 *
 * A conversation with no active agent reads `Reply…`, an ordinary state of a
 * conversation nobody has addressed an agent in rather than an error: it
 * neither disables the composer nor refuses a post (CTA-FR-FKLG).
 *
 * The placeholder is presentation and nothing more. The moment the author
 * types it gives way to what they typed, no part of it is carried in the body
 * that is posted (CMT-FR-11), and it returns when the field is emptied again —
 * all three of which the field itself gives for free, which is exactly why the
 * recipients are stated here rather than on a line of their own.
 *
 * It says nothing about the agents the **typed** body itself tags. Those are
 * the author's own words, legible where they wrote them and emphasised where
 * they resolve (AGT-FR-29), and a composer that also listed them would be
 * reading the message back to the person writing it.
 */
export function composerPlaceholder(active: readonly string[]): string {
  if (active.length === 0) return "Reply…";
  return `Reply to ${active.map((n) => `@${n}`).join(", ")}…`;
}
