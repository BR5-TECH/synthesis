import { useEffect, useState } from "react";
import * as api from "../api";
import { logWarn } from "../logging";
import { useAgentRegistryRevision } from "../state/agentRegistry";
import type { ActiveAiApiCatalog } from "../types";

/**
 * AGT-FR-14 / AGT-FR-PRVQ / AGT-FR-RFSH: the catalog of the AI API provider
 * that is active for the open project, read from `get_active_ai_api_catalog`.
 *
 * It reads when the consumer mounts and again whenever the agent registry
 * revision changes. The AI API level bumps that revision after a
 * verification, a clear, an activation, a model change, and a project
 * override, so a provider switch reaches every open Agents surface in the
 * same window without a restart.
 *
 * The result is `null` until the first read lands. A read that fails resolves
 * to "no catalog" with the resolution `none_selected`: the surface then says
 * that no provider is active, which is the safe reading, and a row still
 * renders with its model identifier (AGR-FR-17).
 *
 * The call is a registry read. It makes no network request and no keychain
 * probe (AAP-FR-APRV), so listing agents never waits on the keychain.
 */
export function useActiveAiApiCatalog(enabled = true): ActiveAiApiCatalog | null {
  const [active, setActive] = useState<ActiveAiApiCatalog | null>(null);
  const revision = useAgentRegistryRevision();
  useEffect(() => {
    if (!enabled) return;
    let cancelled = false;
    api
      .getActiveAiApiCatalog()
      .then((result) => {
        if (cancelled) return;
        setActive({
          resolution: result?.resolution ?? "none_selected",
          catalog: result?.catalog ?? null,
        });
      })
      .catch(() => {
        logWarn(["frontend"], "active AI API catalog read failed");
        if (!cancelled) setActive({ resolution: "none_selected", catalog: null });
      });
    return () => {
      cancelled = true;
    };
  }, [enabled, revision]);
  return active;
}
