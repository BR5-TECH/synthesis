/**
 * The routes of the decomposition surfaces to the shell
 * (`DCR-draft-change-review.md` DCR-FR-HWLA, DCR-FR-RJZP, DCR-FR-QQLE;
 * `CMT-comments.md` CMT-FR-MPKL).
 *
 * Registered by the shell rather than threaded as props, on the terms
 * `promptProposals.ts` registers its route: the cards sit several layers below
 * the tab strip that knows how to open a tab.
 */
import { useSyncExternalStore } from "react";

import { logInfo } from "../logging";
import type { DraftSummary } from "../types";

export interface DecompositionRoutes {
  openDraft: (draftId: string) => void;
  /** The Drafts panel's row of a draft, for its status and its marker. */
  draftOf: (draftId: string) => DraftSummary | undefined;
  openPreview: (proposalId: string, label: string) => void;
  closePreview: (proposalId: string) => void;
}

let routes: DecompositionRoutes | null = null;

/** Wire the routes to the running shell. Called once, as the shell mounts. */
export function configureDecompositionRoutes(next: DecompositionRoutes | null): void {
  routes = next;
}

/** The row the shell holds for a draft, if any. */
export function draftSummaryOf(draftId: string): DraftSummary | undefined {
  return routes?.draftOf(draftId);
}

/** DCR-FR-HWLA: open the draft preview tab of one proposal. */
export function openDraftPreview(proposalId: string, label: string): void {
  routes?.openPreview(proposalId, label);
}

/** DCR-FR-RJZP / TAB-FR-OMWY: close it when its proposal is decided or withdrawn. */
export function closeDraftPreview(proposalId: string): void {
  routes?.closePreview(proposalId);
}

/** The card the author asked to be shown, which the card scrolls to and clears. */
let revealing: string | null = null;
const listeners = new Set<() => void>();

/** CMT-FR-MPKL / DCR-FR-QQLE: open the original draft's tab and show the card. */
export function showNewDraftCard(draftId: string, proposalId: string): void {
  routes?.openDraft(draftId);
  revealing = proposalId;
  logInfo(["frontend"], "a new-draft proposal was asked to be shown", { draftId, proposalId });
  for (const listener of listeners) listener();
}

/** The card to reveal now, if any. */
export function useRevealedCard(): string | null {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => revealing,
  );
}

/** The card has been shown; nothing more to reveal. */
export function clearRevealedCard(proposalId: string): void {
  if (revealing !== proposalId) return;
  revealing = null;
  for (const listener of listeners) listener();
}

/** DCR-FR-AMLR: the Discuss route of each open draft tab, by draft. */
const discussRoutes = new Map<string, (proposalId: string) => void>();

/** A New Artifact tab registers its Discuss route while it is mounted. */
export function registerDiscussRoute(
  draftId: string,
  route: (proposalId: string) => void,
): () => void {
  discussRoutes.set(draftId, route);
  return () => {
    if (discussRoutes.get(draftId) === route) discussRoutes.delete(draftId);
  };
}

/** DCR-FR-AMLR: Discuss on one proposed draft, in its draft's tab. */
export function discussNewDraft(draftId: string, proposalId: string): void {
  discussRoutes.get(draftId)?.(proposalId);
}
