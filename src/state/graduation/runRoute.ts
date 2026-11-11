/**
 * The route that opens one graduation run in the Runs bottom panel, for a
 * surface that sits several layers below the shell
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-LYIX,
 * per `../../../specifications/ui/GRU-graduation-runs.md` GRU-FR-PAHN).
 *
 * Registered by the shell as it mounts, on the terms of `../openspec/changeRoutes.ts`.
 */
import { logDebug } from "../../logging";

let route: ((runId: string) => void) | null = null;

/** Wire the route to the running shell. Called as the shell mounts. */
export function configureRunRoute(next: ((runId: string) => void) | null): void {
  route = next;
}

/** OCW-FR-LYIX: open the Runs panel on its graduation section with this run selected. */
export function openGraduationRunRoute(runId: string): void {
  if (!route) {
    logDebug(["frontend"], "the run route is not configured", { runId });
    return;
  }
  route(runId);
}
