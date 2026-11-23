/**
 * Agent surfaces (`AUI-agent-ui-surfaces.md`): a small panel an agent composed
 * from the discussion catalog, and the author's one action on it.
 */
import type { DiscussionTarget, Participant } from "./comments";

/** AUI-FR-IAAP: one component as the agent gave it. The catalog decides its shape. */
export type SurfaceComponent = { id: string; component: string } & Record<string, unknown>;

/** AUI-FR-AIRH: the author's one action. */
export interface SurfaceSubmission {
  name: string;
  sourceComponentId: string;
  context: Record<string, unknown>;
  dataModel: Record<string, unknown>;
  by: Participant;
  at: string;
  commentId: string;
}

/** AUI-FR-IAAP: one agent surface, as the backend serves it. */
export interface AgentSurface {
  id: string;
  discussionId: string;
  target: DiscussionTarget;
  commentId: string;
  catalogId: string;
  components: SurfaceComponent[];
  dataModel: Record<string, unknown>;
  authoredBy: Participant;
  createdAt: string;
  state: "open" | "submitted";
  submission?: SurfaceSubmission;
}

/** AUI-FR-KBVY: the action the panel sends. */
export interface SurfaceAction {
  name: string;
  sourceComponentId: string;
  context: Record<string, unknown>;
  dataModel: Record<string, unknown>;
}

/** AUI-FR-MAXD: what an accepted action returns. */
export interface SurfaceSubmitResult {
  surface: AgentSurface;
  commentId: string;
}

/** CMS-FR-RWQQ: the `agent_surface` attachment of an agent's comment. */
export interface AgentSurfaceReference {
  kind: "agentSurface";
  surfaceId: string;
}
