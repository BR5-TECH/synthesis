/**
 * The proposal, conversation and roster shapes every prompt-change-review test
 * builds from (`../../specifications/ui/PCR-prompt-change-review.md`).
 *
 * Shared rather than copied, on the model of `streamFixtures.ts`: more than one
 * test file renders the same modal, and a proposal that drifted between them
 * would let one of them pass against a shape the backend never sends.
 */

import type {
  Comment,
  Discussion,
  Participant,
  ProjectAgent,
  PromptChangeProposal,
} from "../types";

export const ARTIFACT = "prompts/review.md";
export const CURRENT = "# Review\n\nThe original line.\n";
export const PROPOSED = "# Review\n\nA better line.\n";

export function proposal(over: Partial<PromptChangeProposal> = {}): PromptChangeProposal {
  return {
    id: "prop-1",
    artifactId: ARTIFACT,
    path: ARTIFACT,
    agent: { kind: "agent", agentId: "a1", handle: "arch", model: "m", title: "Architect" },
    rationale: "The instructions bury the important step.",
    threadId: "t1",
    commentId: "c1",
    state: "pending",
    candidateEdited: false,
    commentOwed: false,
    createdAt: "2026-01-01T00:00:00Z",
    ...over,
  };
}

// ---------------------------------------------------------------------------
// The conversation the proposal was made in (PCR-FR-14, per CTA-FR-LCFU)
// ---------------------------------------------------------------------------

export const HUMAN: Participant = { kind: "human", login: "raver119" };
export const ARCH: Participant = { kind: "agent", agentId: "a1", handle: "arch" };

export function message(id: string, author: Participant, body: string): Comment {
  return {
    id,
    author,
    body,
    quotes: [],
    attachments: [],
    createdAt: "2026-01-01T00:00:00Z",
  };
}

export function conversation(comments: Comment[]): Discussion {
  return {
    id: "t1",
    target: { kind: "artifact", artifactId: ARTIFACT },
    fragmentTarget: null,
    comments,
    locked: false,
    resolved: false,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
  };
}

/**
 * The default: the author addressed `@arch`, and `@arch` answered with the
 * proposal — so `@arch` is the conversation's one active agent and an untagged
 * decision reaches it (PCR-FR-14, per CTA-FR-HCMJ).
 */
export function addressedConversation(): Discussion {
  return conversation([
    message("c0", HUMAN, "@arch please review"),
    message("c1", ARCH, "Here is a change I would make."),
  ]);
}

export function agent(nickname: string, id: string): ProjectAgent {
  return {
    agent: {
      id,
      nickname,
      title: "",
      modelId: "anthropic/claude-opus-5",
      instructions: "",
      reasoning: null,
      createdAt: "2026-01-01T00:00:00Z",
      updatedAt: "2026-01-01T00:00:00Z",
    },
    availability: "ready",
  };
}

export const ROSTER = [agent("arch", "a1"), agent("sec", "a2"), agent("scribe", "a3")];
