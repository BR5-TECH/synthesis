/**
 * What a pending contribution says an agent is doing at this moment
 * (`CMT-comments.md` CTA-FR-FBJR, CTA-FR-IWOJ).
 *
 * One short line of fixed wording that says *what* is being done rather than
 * how far through it is. The wording belongs to this surface and never to the
 * tool (per `../../specifications/tools/TLC-tool-conventions.md` TLC-FR-27): no
 * tool declares a status, carries one in its definition, or returns one, so a
 * tool added to the loop leaves no surface blank and a status can never be
 * composed from an argument the model wrote or a value a tool returned —
 * neither of which reaches this process at all (CVL-FR-26).
 *
 * Pure and separate from the components so every rule is testable without a
 * backend and without a render: what an agent is doing is a fact about the turn
 * record a surface already holds, and costs no call of its own.
 */

import type { ActiveToolCall, AgentTurn } from "../types";

/**
 * CTA-FR-IHOB: a turn waiting on a model response, which is where a turn begins,
 * where it returns between tool calls, and where it stays through every
 * automatic retry and every wait between them (CTA-FR-AZEU).
 */
export const THINKING = "Thinking…";

/**
 * CTA-FR-VSIM / TLC-FR-27: a call whose tool this vocabulary does not name.
 *
 * Never empty and never the tool's own name: a tool this surface has not been
 * taught still reads as work in progress, and an author is never shown an
 * identifier out of the model's mouth.
 */
export const WORKING = "Working…";

/**
 * CTA-FR-HEYB: the whole of the vocabulary a conversation needs — the tools a
 * conversation turn is lent (CVL-FR-08, CVL-FR-30) and no others.
 *
 * A tool no conversation turn carries has no status here, the graduation loop's
 * own tools among them; one of those would read {@link WORKING}, which is the
 * point of having a fallback at all.
 */
export const TOOL_ACTIVITY_STATUS: Readonly<Record<string, string>> = {
  search_specifications: "Searching related specifications…",
  read_file: "Reading a project file…",
  search_drafts: "Searching drafts…",
  read_draft: "Reading a draft…",
  search_notes: "Searching notes…",
  search_skills: "Searching available skills…",
  list_skills: "Listing available skills…",
  load_skill: "Loading skill instructions…",
  ask_user_comment: "Asking a clarifying question…",
  ask_discussion_questions: "Preparing questions for you…",
  propose_draft_changes: "Preparing draft changes…",
  propose_prompt_changes: "Preparing prompt changes…",
  "openrouter:web_search": "Searching the web…",
  "openrouter:web_fetch": "Fetching from the web…",
};

/**
 * CTA-FR-HEYB: the fixed wording one active call reads.
 *
 * Exported for the surfaces that hold a call rather than a turn; every caller
 * reaches the same table, so the same call reads the same line wherever it is
 * rendered (CVP-FR-32).
 */
export function toolActivityStatus(tool: string): string {
  return TOOL_ACTIVITY_STATUS[tool] ?? WORKING;
}

/**
 * CTA-FR-IWOJ: the status a pending contribution reads — that of the **most
 * recently activated** call still active, and {@link THINKING} where none is.
 *
 * Which one that is comes from the **activation order the turn itself carries**
 * (AGC-FR-33) and never from the order events reached this process, so calls
 * activated by one model response are ordered among themselves and two events
 * arriving out of order settle on the same status either way.
 *
 * Every transition lands on exactly one non-empty status: a call that
 * completes, fails, or is cancelled hands the line to the most recently
 * activated call still active *at once* — the status is never cleared, never
 * blanked, and never held at the finished call while another call is running —
 * and a turn with nothing active is back to waiting on a model response.
 */
export function activityStatus(
  turn: Pick<AgentTurn, "activeToolCalls">,
): string {
  const latest = latestActiveCall(turn.activeToolCalls);
  return latest ? toolActivityStatus(latest.tool) : THINKING;
}

/**
 * AGC-FR-33 / CTA-FR-KWOF: the call with the highest activation sequence, or
 * `undefined` where none is active.
 *
 * The maximum rather than the last entry: the list arrives in activation order
 * today, and reading the sequence is what makes that a fact of the record
 * rather than a hope about the payload.
 */
function latestActiveCall(
  calls: readonly ActiveToolCall[] | undefined,
): ActiveToolCall | undefined {
  let latest: ActiveToolCall | undefined;
  for (const call of calls ?? []) {
    if (!latest || call.activationSeq > latest.activationSeq) latest = call;
  }
  return latest;
}
