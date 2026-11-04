/**
 * The draft preview tab of a new-draft proposal (`TAB-tabs.md` TAB-FR-CIGU,
 * TAB-FR-OMWY; `DCR-draft-change-review.md` DCR-FR-HWLA, DCR-FR-RJZP).
 *
 * A viewer-only tab, identified by the proposal id. It holds no editing
 * session and no savable buffer, and it is not a draft's tab: it carries no
 * `draftId`, so draft rename and drop code never reaches it.
 */
import type { Dispatch, MutableRefObject, SetStateAction } from "react";

import { logDebug } from "../../logging";
import type { Tab } from "../../types";
import { neverEmpty } from "./tabRecords";

/** TAB-FR-CIGU: the tab id of one proposal's preview. */
export const draftPreviewTabId = (proposalId: string) => `draft-preview:${proposalId}`;

export interface DraftPreviewDeps {
  tabsRef: MutableRefObject<Tab[]>;
  setTabs: Dispatch<SetStateAction<Tab[]>>;
  setActiveTab: Dispatch<SetStateAction<string>>;
  activateTab: (id: string) => void;
}

export interface DraftPreviewActions {
  openDraftPreview: (proposalId: string, label: string) => void;
  closeDraftPreview: (proposalId: string) => void;
}

export function createDraftPreviewActions(deps: DraftPreviewDeps): DraftPreviewActions {
  const { tabsRef, setTabs, setActiveTab, activateTab } = deps;

  /** DCR-FR-HWLA / TAB-FR-CIGU: open the preview, or focus the one open. */
  const openDraftPreview = (proposalId: string, label: string) => {
    const id = draftPreviewTabId(proposalId);
    setTabs((ts) =>
      ts.some((t) => t.id === id)
        ? ts.map((t) => (t.id === id ? { ...t, label } : t))
        : [...ts, { id, label, tooltip: `Preview: ${label}`, kind: "draftPreview", proposalId }],
    );
    activateTab(id);
    logDebug(["frontend"], "draft preview opened", { proposalId });
  };

  /** DCR-FR-RJZP / TAB-FR-OMWY: close with no prompt when the proposal is decided. */
  const closeDraftPreview = (proposalId: string) => {
    const id = draftPreviewTabId(proposalId);
    if (!tabsRef.current.some((t) => t.id === id)) return;
    // Computed before either setter, so the focus move never reads an empty list.
    const remaining: Tab[] = neverEmpty(tabsRef.current.filter((t) => t.id !== id));
    tabsRef.current = remaining;
    setTabs((ts) => neverEmpty(ts.filter((t) => t.id !== id)));
    setActiveTab((current) =>
      remaining.some((t) => t.id === current)
        ? current
        : (remaining[remaining.length - 1]?.id ?? current),
    );
    logDebug(["frontend"], "draft preview closed", { proposalId, rule: "TAB-FR-OMWY" });
  };

  return { openDraftPreview, closeDraftPreview };
}
