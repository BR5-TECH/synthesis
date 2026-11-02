/**
 * What the shell raises and retracts when a proposal event arrives
 * (`../../specifications/ui/DCR-draft-change-review.md` DCR-FR-18,
 * DCR-FR-QXFU; `PCR-prompt-change-review.md` PCR-FR-17, PCR-FR-26;
 * `NTF-notifications.md` NTF-FR-24).
 *
 * Pure functions of the event and of the open root. The shell's subscribers in
 * `../hooks/useAppNotifications.ts` call them and give the result to the
 * notification facility, because a raise is made from that file alone.
 */
import type {
  DraftChangeProposal,
  PromptChangeProposal,
  PromptProposalsChangedPayload,
  ProposalsChangedPayload,
} from "../types";
import type { Raise } from "./notifications";
import { mintAddress } from "./notificationAddress";

/** The project and worktree the shell has open, or nulls where none is. */
export interface OpenRoot {
  projectKey: string | null;
  worktree: string | null;
}

/**
 * DCR-FR-18 / DCR-FR-QXFU / PCR-FR-17: whether the cause of an event is one
 * that raises — a recording, or an update.
 *
 * An event from an older backend carries no cause. It raises on the old rule,
 * which raised on every event that left the proposal pending.
 */
function raisingCause(cause: string | undefined): boolean {
  return cause === undefined || cause === "recorded" || cause === "updated";
}

/** NTF-FR-07: the one key a draft's proposal raises under. */
export function draftRaiseKey(draftId: string): string {
  return `draft-proposal:${draftId}`;
}

/**
 * PCR-FR-17: the key a proposal against a prompt artifact is raised under.
 *
 * Derived from the proposal's **own id** rather than from its artifact, so two
 * proposals against one file never share a key — and one proposal's retraction
 * never withdraws another's notification (NTF-FR-38).
 */
export function promptRaiseKey(proposalId: string): string {
  return `prompt-proposal:${proposalId}`;
}

/**
 * DCR-FR-18 / DCR-FR-QXFU: the raise for one `draft-change-proposals-changed`
 * event, or `null` where it raises nothing.
 *
 * A recording raises, and so does an update that leaves the proposal open,
 * with the same key, so it replaces an earlier raise of that proposal. A
 * decision and a hold raise nothing, and neither does an update that closes
 * the proposal.
 */
export function draftProposalRaise(
  payload: ProposalsChangedPayload,
  root: OpenRoot,
): Raise | null {
  if (payload.proposal.state !== "pending") return null;
  if (!raisingCause(payload.cause)) return null;
  if (!root.projectKey || !root.worktree) return null;
  return {
    // NTF-FR-07: one slot per draft, so an agent that proposes, is declined,
    // and proposes again replaces its own notification rather than stacking a
    // second beside it.
    key: draftRaiseKey(payload.draftId),
    // DCR-FR-18: a proposal waiting for review is Info.
    level: "Info",
    title: `${proposerHandle(payload.proposal)} proposed a change`,
    body: payload.proposal.path,
    address: mintAddress(root.projectKey, root.worktree, {
      kind: "draft",
      draftId: payload.draftId,
    }),
  };
}

/** PCR-FR-17 / PCR-FR-26: what one prompt proposal event asks of the shell. */
export interface PromptProposalReaction {
  /** The key to retract, or `null`. */
  retract: string | null;
  /** The raise to make, or `null`. */
  raise: Raise | null;
}

/**
 * PCR-FR-17 / PCR-FR-26: the reaction to one
 * `prompt-change-proposals-changed` event.
 *
 * A proposal that is not `pending` — accepted, rejected, accepted with its
 * comment owed, or closed by an update — retracts its raise, whatever the
 * cause, and raises nothing. A pending proposal raises on a recording and on
 * an update. A decision, a hold, and a reconciliation that leave it pending do
 * nothing.
 */
export function promptProposalReaction(
  payload: PromptProposalsChangedPayload,
  root: OpenRoot,
): PromptProposalReaction {
  const key = promptRaiseKey(payload.proposal.id);
  if (payload.proposal.state !== "pending") return { retract: key, raise: null };
  if (!raisingCause(payload.cause)) return NOTHING;
  if (!root.projectKey || !root.worktree) return NOTHING;
  return {
    retract: null,
    raise: {
      // PCR-FR-17: derived from the **proposal's own id**.
      key,
      // PCR-FR-17: a proposal waiting for review is Info.
      level: "Info",
      // PCR-FR-17: a title naming the agent **and the file**, and a body
      // naming the prompt — so a notification read in a centre beside a dozen
      // others says which file is waiting without being opened.
      title: `${proposerHandle(payload.proposal)} proposed a change to ${basename(payload.proposal.path)}`,
      body: payload.proposal.path,
      // NTF-FR-03: the project-file address, so activating it opens or
      // focuses that artifact's Editor tab (PCR-FR-18).
      address: mintAddress(root.projectKey, root.worktree, {
        kind: "file",
        path: payload.proposal.path,
      }),
    },
  };
}

const NOTHING: PromptProposalReaction = Object.freeze({ retract: null, raise: null });

/** The file's own name, for a title that has to name it in a few words. */
function basename(path: string): string {
  const tail = path.split("/").pop();
  return tail === undefined || tail === "" ? path : tail;
}

/** DCR-FR-18 / PCR-FR-17: who a raise says proposed the change. */
function proposerHandle(proposal: DraftChangeProposal | PromptChangeProposal): string {
  return proposal.agent.kind === "agent" ? `@${proposal.agent.handle}` : proposal.agent.login;
}
