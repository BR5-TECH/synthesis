import { useEffect } from "react";
import * as api from "../../api";
import { logWarn } from "../../logging";
import type { SpecMapSessionStore } from "../../state/specMap/session";

/**
 * SMD-FR-LKVU: a planned chip shows its draft's current name, and leaves the
 * map when its draft is no longer active — archived, graduated or deleted.
 *
 * Keyed on the drafts revision, which moves on every change to the draft set
 * wherever it was made. The listing is read only while the map holds a planned
 * draft, so a session that never planned one reads nothing.
 */
export function useSpecMapDraftSync(specMap: SpecMapSessionStore, draftsRevision: number) {
  useEffect(() => {
    const tree = specMap.tree();
    if (!tree || tree.planned.size === 0) return;
    // Only the placements that existed when the listing was asked for are
    // judged by it: a draft placed while the listing was in flight is newer
    // than that listing, and its absence there says nothing about it.
    const asked = new Set(tree.planned.keys());
    let cancelled = false;
    api
      .listDrafts()
      .then((listing) => {
        const current = specMap.tree();
        if (cancelled || !current) return;
        // An active or a published draft keeps its chip; an archived or a
        // graduated one, and one the listing no longer holds, leaves the map.
        const active = new Map(
          listing.drafts
            .filter((d) => d.status === "active" || d.status === "published")
            .map((d) => [d.id, d.name]),
        );
        for (const draft of [...current.planned.values()]) {
          if (!asked.has(draft.draftId)) continue;
          const name = active.get(draft.draftId);
          if (name === undefined) {
            specMap.dispatch({ kind: "detachDraft", draftId: draft.draftId });
          } else if (name !== draft.name) {
            specMap.dispatch({ kind: "renameDraft", draftId: draft.draftId, name });
          }
        }
      })
      .catch((e) => {
        logWarn(["frontend"], "planned drafts could not be checked against the draft list", {
          reason: e instanceof Error ? e.message : String(e),
        });
      });
    return () => {
      cancelled = true;
    };
  }, [specMap, draftsRevision]);
}
