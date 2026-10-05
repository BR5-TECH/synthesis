import { useEffect } from "react";
import type { EditSessionStore } from "../state/editSessions";
import type { FlowSessionStore } from "../state/flowSessions";

/**
 * EXC-FR-LKHZ / EXC-FR-UWYK / FLO-FR-31: keep a store's external-change watch alive
 * for as long as this component is mounted.
 *
 * The watch itself lives on the store (see `watchExternalChanges`), because only
 * the active tab's surface is mounted and a change to any other open artifact or
 * Flow must still be caught. It is ref-counted, so the shell and a mounted
 * surface can both depend on it while exactly one subscription exists.
 *
 * The two stores answer the same event differently — an artifact raises the
 * blocking divergence modal, a Flow reloads or keeps its edits — which is why
 * each owns its own handler rather than sharing one here.
 */
export function useExternalChanges(
  store: EditSessionStore | FlowSessionStore,
): void {
  useEffect(() => store.watchExternalChanges(), [store]);
}
