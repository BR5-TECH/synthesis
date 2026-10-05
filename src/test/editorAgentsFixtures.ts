// Fixtures the agent-conversation Editor suites share — the thread, the agent,
// and the turn each of them starts from.
//
// Shared rather than copied, on the model of `graduationFixtures.ts`: the
// suites split by topic (`Editor.agents.test.tsx` and its siblings) and each
// one needs the same shapes. Nothing here touches a mock, so a suite wires its
// own `invoke` and reads these for the data alone.
import type {
  AgentTurn,
  AiApiCatalog,
  Discussion,
  ProjectAgent,
} from "../types";
import {
  artifactCommentOrigin,
} from "./origins";

export { fragment } from "./discussionFixtures";

export const BODY = "# Onboarding\n\nSteps to run before the first session.\n";
export const QUOTE = "the first session";

export const human = (login: string) => ({ kind: "human", login }) as const;

export function makeThread(over: Partial<Discussion> = {}): Discussion {
  return {
    id: "t1",
    target: { kind: "artifact", artifactId: "a.md" },
    fragmentTarget: {
      owner: { kind: "artifact", artifactId: "a.md" },
      path: "a.md",
      start: BODY.indexOf(QUOTE), end: BODY.indexOf(QUOTE) + QUOTE.length, quote: QUOTE,
    },
    comments: [
      {
        id: "c1",
        author: human("raver119"),
        body: "Which session?",
        quotes: [],
        attachments: [],
        createdAt: "2026-01-01T00:00:00Z",
      },
    ],
    locked: false,
    resolved: false,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    ...over,
  };
}

/**
 * CTA-FR-LCFU: a thread the author addressed `@arch` in, so `@arch` is the
 * conversation's active agent and an untagged reply reaches it.
 */
export function addressedThread(over: Partial<Discussion> = {}): Discussion {
  return makeThread({
    comments: [
      {
        id: "c1",
        author: human("raver119"),
        body: "@arch which session?",
        quotes: [],
        attachments: [],
        createdAt: "2026-01-01T00:00:00Z",
      },
    ],
    ...over,
  });
}

export function projectAgent(
  nickname: string,
  id: string,
  availability: ProjectAgent["availability"] = "ready",
  title = "",
): ProjectAgent {
  return {
    availability,
    agent: {
      id,
      nickname,
      title,
      modelId: "anthropic/claude-opus-5",
      instructions: "",
      createdAt: "2026-01-01T00:00:00Z",
      updatedAt: "2026-01-01T00:00:00Z",
      reasoning: null,
    },
  };
}

export function turn(over: Partial<AgentTurn> & { id: string; nickname: string }): AgentTurn {
  return {
    agentId: `agent-${over.nickname}`,
    origin: artifactCommentOrigin("t1"),
    triggerCommentId: "c1",
    state: "running",
    failure: null,
    retryPermitted: false,
    imagesOmitted: false,
    activeToolCalls: [],
    startedAt: "2026-01-01T00:00:00Z",
    endedAt: null,
    ...over,
  };
}

export interface Backend {
  threads: Discussion[];
  agents: ProjectAgent[];
  integrations: AiApiCatalog[];
  turns: AgentTurn[];
  /** AGC-FR-31: what `list_recoverable_agent_turn_failures` returns. */
  recoverable?: AgentTurn[];
  /** AGC-FR-39: what `list_agent_turn_image_notices` returns (CTA-FR-XSGX). */
  imageNotices?: AgentTurn[];
  calls: { cmd: string; args: unknown }[];
  dispatchError?: string;
  /** AGC-FR-32: a synchronous refusal from `retry_agent_turn`. */
  retryError?: string;
  /**
   * Held open so a retry can be observed *while it is dispatching*, which is the
   * only window in which CTA-FR-ESNJ's disable is observable at all.
   */
  retryGate?: Promise<void>;
}

export const calls = (b: Backend, cmd: string) => b.calls.filter((c) => c.cmd === cmd);
