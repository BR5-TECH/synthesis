/**
 * What the Delivery lens keeps in the map session (`SMP-specification-map.md`
 * SMP-FR-QNUH, SMP-FR-RNLH): the GitHub plan with its pending edits, and the
 * layouts of its canvas. A project or worktree switch clears the session and
 * this with it (SMP-FR-MEZK).
 */
import { FlowLayoutStore } from "../flowLayouts";
import type { SpecMapSessionStore } from "../specMap/session";
import { GithubPlanSession, type GithubPlanOps } from "./githubPlan";

export class DeliveryLensData {
  /** The GitHub plan, or null before a GitHub source is shown. */
  github: GithubPlanSession | null = null;
  /** FLO-FR-21: the layout of the GitHub plan outlives each mount of its canvas. */
  readonly layouts = new FlowLayoutStore();
  /**
   * SMP-FR-RNLH / SMP-FR-BXAP: the selection of the canvas. It stays when the
   * tab closes; the view, which a close resets, is in the session's slot.
   */
  selection: string[] = [];

  constructor(private readonly ops?: GithubPlanOps) {}

  /** The plan of the GitHub Project `projectNodeId`. Another Project starts a new one. */
  githubFor(projectNodeId: string): GithubPlanSession {
    if (this.github?.projectNodeId !== projectNodeId) {
      this.github = new GithubPlanSession(projectNodeId, this.ops);
    }
    return this.github;
  }
}

const isLensData = (data: unknown): data is DeliveryLensData => data instanceof DeliveryLensData;

/** The data of the Delivery lens in `store`, made on first use. */
export function deliveryLensData(store: SpecMapSessionStore, ops?: GithubPlanOps): DeliveryLensData {
  return store.ensureDeliveryData(isLensData, () => new DeliveryLensData(ops));
}
