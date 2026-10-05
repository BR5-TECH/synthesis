/**
 * What the author has entered against an escalation and not yet sent
 * (`../../specifications/ui/GRU-graduation-runs.md` GEA-FR-IHIZ).
 *
 * Held **here**, outside the component's lifetime, for the same reason the
 * reading position of GRH-FR-ODLT is: the section is unmounted every time the
 * author looks at the agent output, and a re-list, a re-read, and a selection
 * of another run all redraw the run region from what the backend reports
 * (GRU-FR-ZVTC). None of those has anything to redraw a half-answered set from —
 * the store has never held it — so a draft kept in the component would be lost
 * to the first event that arrived.
 *
 * It is keyed by the run's id together with each question's **recorded
 * position**, never by the question's text and never by the index of the page
 * showing (ESU-FR-16). It holds **every part** of what was entered — which row
 * of the answer group is selected and the free text — rather than the effective
 * answer computed from them (GEA-FR-UWLK), so a re-render restores exactly what
 * the author left rather than a value they never typed.
 *
 * It is **memory and not storage**: written to no preference and to no backend
 * operation, held for the life of the running application, and no draft of one
 * run is ever read for another.
 *
 * The rule earns its place at the refusal. An author who has answered six
 * questions and been refused on the seventh has done the work once, and a
 * surface that redrew itself from the store would have them do it again to
 * correct a value the store never received.
 */

/** GEA-FR-IHIZ: the parts of one question's unsent answer. */
export interface DraftAnswer {
  /**
   * The `answer` of the proposed response the author selected, or null where
   * they selected none. Held by its answer value rather than by its index, so a
   * re-read that reordered nothing still restores the same response.
   *
   * It is kept even while `ownWords` stands. Nothing renders it while that row
   * holds the selection — reselecting a response is the author's own act, not
   * one this surface makes on their behalf — so what the retention buys is that
   * a re-render never has to invent a value, and that the choice a re-read
   * still offers is the one it was.
   */
  selected: string | null;
  /**
   * GEA-FR-UEFS: the own-words choice is the one selected.
   *
   * The free-text field is a choice in the same group as the proposed
   * responses rather than a second answer beside them, so which of them the
   * author means is recorded rather than inferred from what is filled in.
   */
  ownWords: boolean;
  /** What they typed, exactly as they left it — spaces and all. */
  typed: string;
}

const EMPTY: DraftAnswer = { selected: null, ownWords: false, typed: "" };

/** run id -> recorded position -> what was entered against it. */
const byRun = new Map<string, Map<number, DraftAnswer>>();

/** GEA-FR-FJPG: what the author entered against one question, if anything. */
export function recallAnswer(runId: string, position: number): DraftAnswer {
  return byRun.get(runId)?.get(position) ?? EMPTY;
}

/** Every position this run holds an entry for. */
export function draftedPositions(runId: string): number[] {
  return [...(byRun.get(runId)?.keys() ?? [])];
}

/**
 * GEA-FR-IHIZ: record what the author entered against one question.
 *
 * An entry that holds nothing at all — no selection of either kind and no text
 * — is removed rather than kept as an empty one, so a question the author only
 * passed through leaves no trace. Clearing a field they had filled in leaves the
 * question unanswered rather than answered with nothing.
 */
export function rememberAnswer(
  runId: string,
  position: number,
  answer: DraftAnswer,
): void {
  const held = byRun.get(runId) ?? new Map<number, DraftAnswer>();
  // GEA-FR-UWLK: an own-words choice with nothing typed is an entry that holds
  // something — the author chose that row — even though it answers nothing.
  // Removed instead, the radio they just pressed would render unpressed.
  if (answer.selected === null && !answer.ownWords && answer.typed === "") {
    held.delete(position);
  } else {
    held.set(position, answer);
  }
  if (held.size === 0) byRun.delete(runId);
  else byRun.set(runId, held);
}

/**
 * GEA-FR-IHIZ: a run re-read while an escalation stands is **reconciled rather
 * than replaced**.
 *
 * Every entry whose position the re-read run still records is kept against that
 * position, and an entry for a position the run no longer records is dropped
 * with it. Nothing is moved: an entry belongs to a position rather than to a
 * place in a list.
 */
export function reconcile(runId: string, positions: Iterable<number>): void {
  const held = byRun.get(runId);
  if (!held) return;
  const alive = new Set(positions);
  for (const position of [...held.keys()]) {
    if (!alive.has(position)) held.delete(position);
  }
  if (held.size === 0) byRun.delete(runId);
}

/**
 * GEA-FR-IHIZ: the draft is discarded when the escalation it belongs to is
 * answered and the backend has accepted the set, when the run leaves
 * `awaiting_user_decision` by any other route, and when the run leaves the
 * queue.
 */
export function forgetRun(runId: string): void {
  byRun.delete(runId);
}

/** GEA-FR-KTME: a run that has left the queue takes its draft with it. */
export function forgetRunsExcept(listed: Iterable<string>): void {
  const alive = new Set(listed);
  for (const runId of [...byRun.keys()]) {
    if (!alive.has(runId)) byRun.delete(runId);
  }
}

/** Test seam: the whole of what is held, dropped. */
export function forgetEveryDraft(): void {
  byRun.clear();
}
