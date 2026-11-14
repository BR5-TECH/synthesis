/**
 * What the shell gives the Delivery lens (`MDL-map-delivery-lens.md`) beside
 * the map session: the Flow sessions, the claim flow of the Ready tasks
 * section, and the routes of the shell.
 */
import type { GithubPollingController } from "../../hooks/useGithubPolling";
import type { FlowSessionStore } from "../../state/flowSessions";
import type { CreatedDraftOutcome } from "../Flow";

export interface DeliveryHost {
  /** MDL-FR-VLQJ / FLO-FR-30: the store the delivery's Flow file is edited in. */
  flows: FlowSessionStore;
  /**
   * MDL-FR-QEXS / MDL-FR-UKGH / MDL-FR-JRGK: the polling view, and the claim
   * flow of GIT-FR-NQTZ and GIT-FR-FTHT.
   */
  claims?: GithubPollingController | null;
  /** MDL-FR-XMQU / MDL-FR-WAOZ: open a section of Project settings. */
  onOpenProjectSettings?: (section: "delivery" | "github-polling") => void;
  /** MDL-FR-DHHW / LIB-FR-18: reveal the Flow file in the Project panel, and open no tab. */
  onRevealInProject?: (artifactId: string) => void;
  /** FLO-FR-NRBV / NAW-FR-03: create the draft of a plan task. */
  onCreateDraft?: (name: string) => Promise<CreatedDraftOutcome>;
  /** FLO-FR-NRBV: open a draft a plan task just created. */
  onOpenDraft?: (draft: { id: string; name: string }) => void;
  /** FLO-FR-QZUR: open a linked draft. */
  onOpenDraftById?: (draftId: string) => void;
}
