/**
 * The tag syntax — `specifications/ui/AGT-agents.md` AGT-FR-24, AGT-FR-28,
 * AGT-FR-29, AGT-FR-35 … AGT-FR-38.
 *
 * An agent is addressed by writing its nickname inline where a person's name
 * would go, or every agent at once by writing the reserved handle `@all`. Three
 * properties follow, and everything here exists to hold them:
 *
 * - **A tag reaches only agents the open project enrolled** (AGT-FR-24). The
 *   same text in a project that has not enrolled that agent addresses nobody, so
 *   resolution is always against a roster and never against the text alone.
 * - **An unmatched `@` is ordinary text** (AGT-FR-28). An author writing an
 *   email address or a code sample must be able to type one without being
 *   interrupted, so nothing here reports an error and nothing marks a tag that
 *   resolves to no one.
 * - **A tag resolves to one agent, to several, or to none** (AGT-FR-24). `@all`
 *   is the case that resolves to several, and it resolves *afresh every time it
 *   is read* rather than being expanded when it was typed (AGT-FR-37) — which is
 *   what makes it mean the project's roster as it stands rather than the names
 *   that were in it. Nothing here caches an expansion for that reason.
 *
 * Pure and separate from the components so every rule is testable without
 * rendering, and so the composer, the rail's renderer, and the mention picker
 * cannot drift about what a tag is.
 */
import type { ProjectAgent } from "../types";

/** Trailing characters that end a sentence rather than a nickname. */
const TRAILING = new Set([".", ",", ";", ":", "!", "?", ")", "]", "}", ">", '"', "'"]);

/**
 * AGT-FR-25: what may precede a `@` that begins a word — nothing at all, or
 * whitespace, an opening bracket, a quote, or a dash.
 *
 * The set is deliberately narrow. Every character outside it makes the `@` part
 * of the word already being written, which is what keeps `me@example.com` and
 * `a@b` from addressing anybody and from interrupting the author with a picker.
 */
function isTagBoundary(ch: string | undefined): boolean {
  return ch === undefined || /[\s([{<'"\-–—]/.test(ch);
}

/**
 * AGT-FR-35: the handle that addresses every enrolled agent that can answer.
 *
 * Reserved across the registry, so no agent carries it as a nickname
 * (AGT-FR-41 / AGR-FR-04) and it means one thing in every project.
 */
export const ALL_HANDLE = "all";

/**
 * Who a tag may reach, in the one project it is being read in.
 *
 * Two lists rather than one because the two tags resolve against different
 * sets, and the difference is a requirement rather than an accident:
 *
 * - a **nickname** resolves against every enrolled agent (AGT-FR-24), degraded
 *   or not — the backend is what refuses an unavailable one, and the rail
 *   renders that refusal (CMT-FR-34), so an author who named an agent by hand
 *   learns what is wrong with it;
 * - **`@all`** resolves against those that can answer alone (AGT-FR-36),
 *   because a handle that quietly dispatched to an expired key would render the
 *   refusal as that agent's own failure to reply — a name nobody typed.
 */
export interface AgentRoster {
  /** Every enrolled nickname, in the registry's own spelling. */
  readonly nicknames: readonly string[];
  /** Those whose availability is `ready` — what `@all` stands for. */
  readonly ready: readonly string[];
}

/**
 * A roster, or the bare nickname list that means "every one of these can
 * answer".
 *
 * The bare form exists for the callers that have no availability to model — a
 * fixture, or a surface holding nothing but names. Production surfaces build the
 * two-list form from the enrolment they already hold, which is the only way the
 * ready/enrolled distinction above can be honoured.
 */
export type Roster = readonly string[] | AgentRoster;

/** Normalise either accepted form into the two-list one. */
export function rosterOf(roster: Roster): AgentRoster {
  return Array.isArray(roster)
    ? { nicknames: roster, ready: roster }
    : (roster as AgentRoster);
}

/**
 * The roster a project's enrolment amounts to (AGT-FR-24, AGT-FR-36).
 *
 * One place builds this so no surface has to remember which of the two lists
 * `@all` reads — a call site that got that wrong would dispatch to an agent that
 * cannot answer, and the symptom would be a turn failure attributed to a name
 * the author never typed.
 */
export function agentRoster(agents: readonly ProjectAgent[]): AgentRoster {
  return {
    nicknames: agents.map((a) => a.agent.nickname),
    ready: agents.filter((a) => a.availability === "ready").map((a) => a.agent.nickname),
  };
}

export interface TagCandidate {
  /** The handle as typed, without the sigil and without trailing punctuation. */
  nickname: string;
  /** Offset of the `@` in the source string. */
  start: number;
  /** Offset one past the last character of the handle. */
  end: number;
}

export interface TagMatch extends TagCandidate {
  /**
   * The agents this one tag addresses: one for a nickname, every agent that can
   * answer for `@all` (AGT-FR-36). Canonically spelled, so a caller dispatching
   * from here names each agent the way the backend does.
   */
  nicknames: string[];
}

/**
 * Every syntactic tag candidate in `body`, in the order they appear.
 *
 * A candidate is a `@` at a word boundary followed by a run of characters that a
 * nickname may contain — which is anything but whitespace and a second `@`
 * (AGR-FR-04). `me@example.com` therefore yields nothing at all: its `@` is
 * preceded by a letter, so it never starts a tag.
 *
 * Candidates are *syntax*, not agents. Turning one into an agent is
 * [`resolveTags`]'s, because whether a nickname names anybody is a fact about
 * the project's enrolment rather than about the text.
 */
export function tagCandidates(body: string): TagCandidate[] {
  const out: TagCandidate[] = [];
  let inCode = false;
  for (let i = 0; i < body.length; i += 1) {
    // A tag inside a code span is a code sample rather than an address. Skipped
    // here so the rule that *dispatches* a turn and the rule that *marks* one
    // agree — `CommentMarkdown` renders a code span as code and never marks
    // inside it, and a message that dispatched an agent without showing that it
    // had would be the two disagreeing.
    if (body[i] === "`") {
      inCode = !inCode;
      continue;
    }
    if (inCode) continue;
    if (body[i] !== "@") continue;
    if (!isTagBoundary(body[i - 1])) continue;
    let end = i + 1;
    while (end < body.length && !/\s/.test(body[end]) && body[end] !== "@") {
      end += 1;
    }
    // A nickname carries no whitespace and no `@`, but the sentence around it
    // does carry punctuation: `@arch, thoughts?` names `arch`.
    while (end > i + 1 && TRAILING.has(body[end - 1])) end -= 1;
    if (end > i + 1) out.push({ nickname: body.slice(i + 1, end), start: i, end });
    i = end - 1;
  }
  return out;
}

/**
 * AGT-FR-24: the tags in `body` that this roster actually resolves, matched
 * without regard to case.
 *
 * Returns the *canonical* nicknames — the spelling the registry holds — so a
 * caller dispatching per tag names the agent the way the backend does, and so
 * `@ARCH` and `@arch` in one message are recognisably the same agent.
 *
 * The handle is tried before the roster is consulted, which is what makes it mean
 * the same thing in every project (AGT-FR-41): were a stored agent somehow
 * carrying `all` as its nickname, the handle would still address the room rather
 * than that one agent.
 */
export function resolveTags(body: string, roster: Roster): TagMatch[] {
  const { nicknames, ready } = rosterOf(roster);
  const canonical = new Map(nicknames.map((n) => [n.toLowerCase(), n]));
  const out: TagMatch[] = [];
  for (const candidate of tagCandidates(body)) {
    const typed = candidate.nickname.toLowerCase();
    if (typed === ALL_HANDLE) {
      // AGT-FR-38: in a project where nobody can answer, the handle resolves to
      // nobody and the body carrying it is an ordinary message — not marked, not
      // dispatched, and not an error. Dropping it here is what gives all three at
      // once, every one of them being "this tag resolved to no agent".
      if (ready.length === 0) continue;
      out.push({ ...candidate, nickname: ALL_HANDLE, nicknames: [...ready] });
      continue;
    }
    const match = canonical.get(typed);
    if (match) out.push({ ...candidate, nickname: match, nicknames: [match] });
  }
  return out;
}

/**
 * The distinct agents a message addresses, in the order they were first named.
 *
 * "Distinct" is the whole point: CTA-FR-RPVU dispatches once per distinct agent a
 * message's tags resolve to, so `@arch @arch have a look` asks one agent once
 * rather than twice — a repeated tag in one message is emphasis, not a second
 * question. (Asking twice is what a *second message* does, CTA-FR-ZVKL.)
 *
 * The union is taken over the *agents* rather than over the tags, which is what
 * makes `@all @arch` dispatch once for `@arch` instead of twice: the handle and
 * the nickname are two ways of naming one agent, and the author asked once.
 */
export function addressedNicknames(body: string, roster: Roster): string[] {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const tag of resolveTags(body, roster)) {
    for (const nickname of tag.nicknames) {
      if (seen.has(nickname)) continue;
      seen.add(nickname);
      out.push(nickname);
    }
  }
  return out;
}

/**
 * Whether `body` carries a **live** `@all` — one that resolved to at least one
 * agent (AGT-FR-36), rather than the ordinary text an unresolvable handle is
 * (AGT-FR-38).
 *
 * Exists for the dispatching surfaces to report with: every turn is logged
 * individually by the backend, so what those records cannot show is that one
 * handle rather than several nicknames is why there are three of them.
 */
export function addressesEveryone(body: string, roster: Roster): boolean {
  return resolveTags(body, roster).some((tag) => tag.nickname === ALL_HANDLE);
}

/** One piece of a body split for rendering: prose, or a tag that resolves. */
export type BodySegment =
  | { kind: "text"; text: string }
  | { kind: "tag"; text: string };

/**
 * AGT-FR-29: split `body` so a tag resolving to at least one enrolled agent can
 * be rendered bold against the surrounding prose — and, per AGT-FR-28, so one
 * resolving to nobody stays part of it.
 *
 * A live `@all` is one segment carrying the text `@all` rather than the names it
 * stands for (AGT-FR-40): a reader sees the question that was asked of everyone,
 * not a list they have to count.
 */
export function segmentBody(body: string, roster: Roster): BodySegment[] {
  const tags = resolveTags(body, roster);
  if (tags.length === 0) return [{ kind: "text", text: body }];
  const out: BodySegment[] = [];
  let cursor = 0;
  for (const tag of tags) {
    if (tag.start > cursor) {
      out.push({ kind: "text", text: body.slice(cursor, tag.start) });
    }
    out.push({ kind: "tag", text: body.slice(tag.start, tag.end) });
    cursor = tag.end;
  }
  if (cursor < body.length) out.push({ kind: "text", text: body.slice(cursor) });
  return out;
}

/**
 * AGT-FR-25 / AGT-FR-34: the **candidate tag** being typed at `caret`, or null
 * when the caret is not sitting at the end of one.
 *
 * A candidate tag is a word-initial `@` followed only by characters a nickname
 * may carry, and what this reports is the fragment between that sigil and the
 * caret. Making the picker's presence a function of the text under the caret —
 * rather than of the keystroke that opened it — is what gives AGT-FR-34 its
 * whole lifecycle for free: whitespace ends the candidate tag (a nickname
 * carries none, AGR-FR-04), moving the caret out of it reports null, and
 * deleting back into it reports the shorter fragment, so a mistyped nickname is
 * corrected without the author retyping the sigil.
 */
export function activeMention(
  body: string,
  caret: number,
): { query: string; start: number } | null {
  for (let i = caret - 1; i >= 0; i -= 1) {
    const ch = body[i];
    if (/\s/.test(ch)) return null;
    if (ch === "@") {
      if (!isTagBoundary(body[i - 1])) return null;
      return { query: body.slice(i + 1, caret), start: i };
    }
  }
  return null;
}

/**
 * Replace the mention being typed with `nickname`'s full tag, returning the new
 * body and where the caret belongs afterwards.
 *
 * A trailing space is appended because the author is mid-sentence: the next
 * thing they type is the rest of the message, not more of the nickname.
 */
export function applyMention(
  body: string,
  caret: number,
  start: number,
  nickname: string,
): { body: string; caret: number } {
  // The trailing space is what lets the author keep typing the sentence — but
  // not a second one where the text already carries it.
  const rest = body.slice(caret);
  const inserted = /^\s/.test(rest) ? `@${nickname}` : `@${nickname} `;
  return {
    body: body.slice(0, start) + inserted + rest,
    caret: start + inserted.length,
  };
}
