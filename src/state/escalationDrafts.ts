/**
 * What the author has entered against an escalation and not yet sent
 * (`../../specifications/ui/GEA-graduation-escalation-answering.md` GEA-FR-IHIZ).
 *
 * Held **here**, outside the component's lifetime: the section is unmounted
 * every time the author looks at the agent output, and a re-list, a re-read,
 * and a selection of another run all redraw the run region from what the
 * backend reports (GRU-FR-ZVTC). None of those has anything to redraw a
 * half-answered set from, so a draft kept in the component would be lost to the
 * first event that arrived (GEA-FR-QULY).
 *
 * It holds what the shared question block holds for every host
 * (`DQA-discussion-question-answering.md` DQA-FR-EGZS): per question, the option
 * selected by its position, whether the own answer is chosen, the own words, and
 * the note text. It is keyed by the owner's id — a run's, or a stream's for a
 * work stream update (GEA-FR-LWQD) — together with each question's **recorded
 * position**, never by the question's text and never by the index of the page
 * showing (GEA-FR-XTUE).
 *
 * It is **memory and not storage** (GEA-FR-THNY): written to no preference and
 * to no backend operation, held for the life of the running application, and no
 * draft of one run is ever read for another.
 */
import type { QuestionDraftAnswer } from "./questionAnswerDrafts";

const EMPTY: QuestionDraftAnswer = Object.freeze({
  selected: null,
  ownWords: false,
  typed: "",
  note: "",
});

/**
 * GEA-FR-LWQD: the owner id of a work stream update's draft. A run's draft is
 * keyed by the run's bare id, and an update's by this, so the run listing's
 * clean-up (`forgetRunsExcept`) never reads a stream for a run that left.
 */
export function streamOwner(streamId: string): string {
  return `stream:${streamId}`;
}

/** owner id -> recorded position -> what was entered against it. */
const byOwner = new Map<string, Map<number, QuestionDraftAnswer>>();
/** owner id -> the text of the refusal that the last submission met. */
const refusals = new Map<string, string>();
/** owner id -> the `raisedAt` of the escalation the draft was entered against. */
const escalations = new Map<string, string>();

/**
 * GEA-FR-KTME: the draft belongs to one escalation. Where the owner now holds
 * another — the earlier one left `awaiting_author` by a route other than an
 * accepted answer — the draft and the refusal of the earlier one are dropped.
 * Idempotent, so it is safe to call on every render.
 */
export function adoptEscalation(ownerId: string, raisedAt: string): void {
  const held = escalations.get(ownerId);
  if (held !== undefined && held !== raisedAt) {
    byOwner.delete(ownerId);
    refusals.delete(ownerId);
  }
  escalations.set(ownerId, raisedAt);
}

/** GEA-FR-IHIZ: what the author entered against one question, if anything. */
export function recallAnswer(ownerId: string, position: number): QuestionDraftAnswer {
  return byOwner.get(ownerId)?.get(position) ?? EMPTY;
}

/** Every position this owner holds an entry for. */
export function draftedPositions(ownerId: string): number[] {
  return [...(byOwner.get(ownerId)?.keys() ?? [])];
}

/**
 * GEA-FR-IHIZ: record what the author entered against one question.
 *
 * An entry that holds nothing at all is removed rather than kept as an empty
 * one, so a question the author only passed through leaves no trace. The own
 * answer chosen with nothing typed is a real state and is kept, so the row the
 * author pressed stays pressed.
 */
export function rememberAnswer(
  ownerId: string,
  position: number,
  answer: QuestionDraftAnswer,
): void {
  const held = byOwner.get(ownerId) ?? new Map<number, QuestionDraftAnswer>();
  if (
    answer.selected === null &&
    !answer.ownWords &&
    answer.typed === "" &&
    answer.note === ""
  ) {
    held.delete(position);
  } else {
    held.set(position, answer);
  }
  if (held.size === 0) byOwner.delete(ownerId);
  else byOwner.set(ownerId, held);
}

/**
 * GEA-FR-VBUG: a run re-read while an escalation stands is **reconciled rather
 * than replaced**. Every entry whose position the re-read run still records is
 * kept against that position, and an entry for a position the run no longer
 * records is dropped with it.
 */
export function reconcile(ownerId: string, positions: Iterable<number>): void {
  const held = byOwner.get(ownerId);
  if (!held) return;
  const alive = new Set(positions);
  for (const position of [...held.keys()]) {
    if (!alive.has(position)) held.delete(position);
  }
  if (held.size === 0) byOwner.delete(ownerId);
}

/**
 * GEA-FR-KTME: the draft is discarded when the escalation it belongs to is
 * answered and the backend has accepted the set.
 */
export function forgetRun(ownerId: string): void {
  byOwner.delete(ownerId);
  refusals.delete(ownerId);
  escalations.delete(ownerId);
}

/**
 * GEA-FR-KTME: a run that has left the queue takes its draft with it. A work
 * stream update's draft (`streamOwner`) belongs to no run and is left alone.
 */
export function forgetRunsExcept(listed: Iterable<string>): void {
  const alive = new Set(listed);
  const owners = new Set([...byOwner.keys(), ...refusals.keys(), ...escalations.keys()]);
  for (const ownerId of owners) {
    if (alive.has(ownerId) || ownerId.startsWith(streamOwner(""))) continue;
    forgetRun(ownerId);
  }
}

/**
 * GEA-FR-TEMG: the refusal that the last submission met, held with the draft so
 * a switch to another run and back still shows it. `null` clears it.
 */
export function rememberRefusal(ownerId: string, text: string | null): void {
  if (text === null) refusals.delete(ownerId);
  else refusals.set(ownerId, text);
}

export function recallRefusal(ownerId: string): string | null {
  return refusals.get(ownerId) ?? null;
}

/** Test seam: the whole of what is held, dropped. */
export function forgetEveryDraft(): void {
  byOwner.clear();
  refusals.clear();
  escalations.clear();
}
