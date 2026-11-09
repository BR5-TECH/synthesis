/**
 * NAW-FR-TXKN, NAW-FR-SGVS: the plan tags of a draft's action row — one per
 * delivery plan item the draft is worked for, each naming the Flow and the
 * item and opening the Flow.
 *
 * The tab mounts each time it becomes active, so reading on mount reads both
 * when the tab opens and each time it becomes the active tab.
 */
import { useEffect, useState } from "react";

import * as api from "../../api";
import { logWarn } from "../../logging";
import type { FlowItemMatch, OpenableArtifact } from "../../types";
import { kindGlyph, kindLabel } from "../Flow/kindGlyphs";

const basename = (path: string) => path.slice(path.lastIndexOf("/") + 1);

export function PlanTags({
  draftId,
  onOpenArtifact,
}: {
  draftId: string;
  onOpenArtifact?: (item: OpenableArtifact) => void;
}) {
  const [items, setItems] = useState<FlowItemMatch[]>([]);

  useEffect(() => {
    let live = true;
    api.findFlowItemsForDraft(draftId).then(
      (found) => {
        if (live) setItems(Array.isArray(found) ? found : []);
      },
      (e: unknown) => {
        // NAW-FR-SGVS: a failed read shows no tag and is logged.
        const code =
          typeof e === "object" && e !== null && "code" in e
            ? String((e as { code: unknown }).code)
            : "unknown";
        logWarn(["frontend"], "plan items of a draft could not be read", { draftId, code });
        if (live) setItems([]);
      },
    );
    return () => {
      live = false;
    };
  }, [draftId]);

  return (
    <>
      {items.map((m) => {
        const flow = m.flowName.trim() || basename(m.flowId);
        const label = `${kindLabel(m.kind)} ${m.itemName} in the plan ${flow}`;
        return (
          <button
            key={`${m.flowId}#${m.itemId}`}
            type="button"
            className="draft-workspace__graduation t-ui-xs"
            data-testid="draft-plan-tag"
            title={`Open the plan ${flow}`}
            aria-label={`Open ${label}`}
            onClick={() =>
              onOpenArtifact?.({ id: m.flowId, name: basename(m.flowId), artifactType: "flow" })
            }
          >
            <span aria-hidden="true">{kindGlyph(m.kind)}</span> {flow} › {m.itemName}
          </button>
        );
      })}
    </>
  );
}
