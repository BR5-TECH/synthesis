/**
 * Follows the backend's `"agents changed"` event (`AGR-agent-registry.md`
 * AGR-FR-ZRJD) and turns it into the registry revision every surface showing
 * agents reads again on (`AGT-agents.md` AGT-FR-NJXP).
 *
 * The Settings window is a webview of its own, so a write made there reaches the
 * main window only as this event. A surface mounted in either window calls this
 * once, and a write made in either window bumps the revision of both.
 */
import { useEffect } from "react";
import { onAgentsChanged } from "../events";
import { logWarn } from "../logging";
import { notifyAgentRegistryChanged } from "../state/agentRegistry";

export function useAgentsChanged(): void {
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    onAgentsChanged(() => notifyAgentRegistryChanged())
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      })
      .catch((e) => {
        // A surface that cannot hear the event still shows what it read; it
        // corrects itself on its next read.
        logWarn(["frontend"], "could not follow agent changes", {
          reason: e instanceof Error ? e.message : String(e),
        });
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);
}
