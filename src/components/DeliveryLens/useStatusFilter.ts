/**
 * MDL-FR-ZAGM / MDL-FR-BWEF: the status filter of the Delivery lens for the
 * layout scope of the plan that shows, and the count of what it hides.
 */
import { useCallback, useEffect, useState, useSyncExternalStore } from "react";

import { logInfo } from "../../logging";
import type { DeliveryViewStore } from "../../state/deliveryViews";
import {
  flowLayoutScope,
  githubLayoutScope,
  type DeliverySource,
  type DeliveryView,
} from "../../types";
import type { BoardStatusFilter } from "../Flow/boardFilter";

export interface StatusFilterState {
  /** The view of the scope, or null with no plan, or until its read lands. */
  view: DeliveryView | null;
  /** The filter the canvas applies, or undefined to show every item. */
  canvasFilter: BoardStatusFilter | undefined;
  /** MDL-FR-BWEF: the count of the cards and groups the filter hides. */
  hiddenCount: number;
  onHiddenCount: (count: number) => void;
  /** MDL-FR-ZAGM: keep and store a change of the filter. */
  change: (next: DeliveryView) => void;
}

/** DMS-FR-DMWD: the layout scope of the plan of `source`, or null. */
export function viewScopeOf(githubNodeId: string | null, source: DeliverySource | null): string | null {
  if (githubNodeId !== null) return githubLayoutScope(githubNodeId);
  if (source?.kind === "file") return flowLayoutScope(source.flowId);
  return null;
}

export function useStatusFilter(
  views: DeliveryViewStore,
  githubNodeId: string | null,
  source: DeliverySource | null,
): StatusFilterState {
  useSyncExternalStore(views.subscribe, views.getVersion);
  const scope = viewScopeOf(githubNodeId, source);
  // MDL-FR-ZAGM: read the view when the lens first renders and when the scope changes.
  useEffect(() => {
    if (scope) views.ensure(scope);
  }, [scope, views]);
  const view = scope ? views.get(scope) : null;
  const [hidden, setHidden] = useState<{ scope: string | null; count: number }>({ scope: null, count: 0 });
  const onHiddenCount = useCallback((count: number) => setHidden({ scope, count }), [scope]);
  const change = useCallback(
    (next: DeliveryView) => {
      if (!scope) return;
      logInfo(["frontend"], "delivery status filter changed", {
        scope_kind: scope.split(":")[0] ?? "",
        hidden: next.hidden.length,
        blocked_only: next.blockedOnly,
      });
      void views.save(scope, next);
    },
    [scope, views],
  );
  return {
    view,
    canvasFilter: view ?? undefined,
    hiddenCount: hidden.scope === scope ? hidden.count : 0,
    onHiddenCount,
    change,
  };
}
