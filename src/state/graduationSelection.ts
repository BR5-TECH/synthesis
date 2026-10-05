/**
 * What the graduation section remembers between renders of itself
 * (`../../specifications/ui/GRH-graduation-history.md` GRH-FR-ODLT, GRU-FR-YYXN).
 *
 * The section is unmounted every time the author looks at the agent output and
 * mounted again when they come back, so anything held in its own state is a
 * reading position they lose by glancing elsewhere — which is exactly what
 * GRH-FR-ODLT says must not happen. What they were reading lives here instead:
 * outside the component's lifetime, for the life of the running application.
 *
 * It is **memory and not storage**. Nothing here is written to a preference or
 * to a backend operation and nothing survives a relaunch: an unread selection
 * is worth remembering for an afternoon rather than for good, and a run's own
 * state is the backend's in any case (GRU-FR-ZVTC). One project's selection is
 * never read for another, and one run's iteration is never read for another.
 *
 * This is the same footing the unsent answer draft of GEA-FR-IHIZ is held on.
 */
import type { GraduationView } from "./graduationRail";

/** GRH-FR-ODLT: the run each project was left reading. */
const runByProject = new Map<string, string>();

/**
 * GRH-FR-QVEX: the view and the text filter each project was left narrowed by.
 *
 * Held here for the same reason the selection is, and on the same footing: the
 * section is unmounted every time the author looks at the agent output, so a
 * filter kept in the component's own state would be one they lose by glancing
 * elsewhere. Keyed by project, so one project's narrowing is never read for
 * another. Written to no preference and to no backend operation, and not
 * carried across a relaunch — a project this map holds nothing for opens in
 * **In-flight** with the text filter empty.
 */
const filtersByProject = new Map<string, RailFilters>();

/** GRH-FR-QVEX: what the rail's two controls are holding. */
export interface RailFilters {
  view: GraduationView;
  text: string;
}

/**
 * GRH-FR-XBWU: the route request this section has already answered.
 *
 * A surface that names a run mints a nonce for the request, so a request that
 * has been taken can be told from one that has not. It is held out here rather
 * than in the component for the same reason everything else in this module is:
 * the section is unmounted every time the author looks at the agent output, and
 * a request answered before that must not be answered again on the way back —
 * a route taken minutes ago is not a naming event, and replaying it would take
 * the author off the row and out of the position they have since chosen.
 */
let answeredRoute: number | null = null;

/** Whether this request is one the section has answered already. */
export function routeAnswered(nonce: number | undefined): boolean {
  return nonce !== undefined && answeredRoute === nonce;
}

export function rememberRoute(nonce: number | undefined): void {
  if (nonce !== undefined) answeredRoute = nonce;
}

/**
 * GRU-FR-YYXN: what the author chose on a run, **per project**.
 *
 * Keyed by project first because pruning is per project: the section lists one
 * project's queue at a time, so what it can conclude from a listing is only
 * about that project's runs. Keyed by run id alone, opening a second project
 * would prune every run of the first — the runs are absent from the listing
 * because the author changed project, not because they left the queue.
 */
const chosenByProject = new Map<string, Map<string, Choice>>();

/**
 * GRU-FR-YYXN: an author's choice of iteration, and what was already accounted
 * for when they made it.
 *
 * `knownAccounted` is what lets a row be marked **new**: an account that was
 * already on the record when the author pinned is not news to them, and marking
 * it would light up half the list the moment they chose a row. Only an account
 * that arrived afterwards is new.
 */
export interface Choice {
  /**
   * The pass whose account is open, or **null** while none is: an account the
   * author opened is one they can close again, and a history with every row
   * shut is a position worth returning them to as much as any other.
   *
   * GRU-FR-YYXN: identified by the row's own key rather than by its iteration
   * number, because the two parts count their passes separately and an
   * iteration number alone names two rows the moment a run has made a pass of
   * each.
   */
  row: string | null;
  /**
   * GRU-FR-YYXN: whether the choice **follows the newest pass**, which is what
   * opening the newest row means — the author asked to be on the latest, so a
   * later one arriving is where they want to be.
   *
   * Expressed as a property of the choice rather than as the absence of one,
   * because the two are different positions: a history the author has closed
   * altogether is a choice too, and "no choice" would read as one of them.
   */
  follows: boolean;
  knownAccounted: string[];
}

/** GRH-FR-ODLT: the run this project was left reading, if it is still known. */
export function recallRun(projectKey: string): string | null {
  return runByProject.get(projectKey) ?? null;
}

export function rememberRun(projectKey: string, runId: string): void {
  runByProject.set(projectKey, runId);
}

/** GRH-FR-QVEX: how this project's rail was last narrowed, if it was. */
export function recallFilters(projectKey: string): RailFilters | null {
  return filtersByProject.get(projectKey) ?? null;
}

export function rememberFilters(
  projectKey: string,
  filters: RailFilters,
): void {
  filtersByProject.set(projectKey, filters);
}

/** GRU-FR-YYXN: what the author chose on this run, if anything. */
export function recallChoice(projectKey: string, runId: string): Choice | null {
  return chosenByProject.get(projectKey)?.get(runId) ?? null;
}

export function chooseIteration(
  projectKey: string,
  runId: string,
  choice: Choice,
): void {
  const held = chosenByProject.get(projectKey) ?? new Map<string, Choice>();
  held.set(runId, choice);
  chosenByProject.set(projectKey, held);
}

/**
 * GRU-FR-YYXN: a run that has left the queue takes its choice with it.
 *
 * Scoped to the project the listing is of, so a project switch — which lists
 * another project's runs and none of this one's — drops nothing. What the
 * listing evidences is which of **its own** project's runs are still queued.
 */
export function forgetRunsExcept(
  projectKey: string,
  listed: Iterable<string>,
): void {
  const held = chosenByProject.get(projectKey);
  if (!held) return;
  const alive = new Set(listed);
  for (const runId of [...held.keys()]) {
    if (!alive.has(runId)) held.delete(runId);
  }
}

/** Test seam: the whole of what is remembered, dropped. */
export function forgetEverything(): void {
  runByProject.clear();
  chosenByProject.clear();
  filtersByProject.clear();
  answeredRoute = null;
}
