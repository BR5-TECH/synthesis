/**
 * The agent-registry invalidation store, and the enrolment it publishes.
 *
 * The counter half exists because the three surfaces of `AGT-agents.md` are peers
 * with no shared parent below `App`. The **roster** half exists because
 * `AGT-agents.md` AGT-FR-29 has a tag render bold wherever a message body is
 * rendered — the Comments panel included — while `CMP-comments-panel.md`
 * CMP-FR-18 gives that panel exactly one list call and says it invokes nothing
 * else. Sharing the read the chrome control has already made is what satisfies
 * both, so the properties tested here are load-bearing for that pairing:
 * a stable snapshot, and a notification only when the roster actually changed.
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

import {
  notifyAgentRegistryChanged,
  publishProjectAgents,
  readProjectAgents,
  resetAgentRegistry,
  subscribe,
} from "./agentRegistry";
import type { ProjectAgent } from "../types";

function enrolled(
  nickname: string,
  availability: ProjectAgent["availability"] = "ready",
  id = `id-${nickname}`,
): ProjectAgent {
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
    availability,
  };
}

/**
 * The snapshot as `useSyncExternalStore` reads it — the hook passes
 * `readProjectAgents` as its `getSnapshot`, so reading it directly tests exactly
 * what a component would be handed, and tests it without a renderer.
 */
const snapshot = readProjectAgents;

beforeEach(() => {
  resetAgentRegistry();
});

describe("the published enrolment", () => {
  it("starts empty, so a body renders as prose before any read has landed", () => {
    // Losing emphasis for that moment is a far better failure than bolding a
    // name the project does not enrol.
    expect(snapshot()).toEqual([]);
  });

  it("hands out a stable reference until the roster actually changes", () => {
    // Load-bearing for `useSyncExternalStore`: a fresh array on every read is an
    // infinite re-render loop, not a cosmetic inefficiency.
    publishProjectAgents([enrolled("arch")]);
    const first = snapshot();
    expect(snapshot()).toBe(first);

    // An equal roster from a re-read is not a change.
    publishProjectAgents([enrolled("arch")]);
    expect(snapshot()).toBe(first);

    // A different one is.
    publishProjectAgents([enrolled("arch"), enrolled("sec")]);
    expect(snapshot()).not.toBe(first);
  });

  it("notifies only when the roster changed", () => {
    // The chrome control publishes on every registry revision, so waking every
    // subscriber for an unchanged list would re-render them for nothing.
    const notify = vi.fn();
    subscribe(notify);

    publishProjectAgents([enrolled("arch")]);
    expect(notify).toHaveBeenCalledTimes(1);

    publishProjectAgents([enrolled("arch")]);
    expect(notify).toHaveBeenCalledTimes(1);

    // Availability is part of what changed: `@all` reads it (AGT-FR-36), and a
    // degraded agent leaving the handle's expansion is a visible change.
    publishProjectAgents([enrolled("arch", "provider_unverified")]);
    expect(notify).toHaveBeenCalledTimes(2);

    // So is a rename, and so is a different agent at the same position.
    publishProjectAgents([enrolled("archer", "provider_unverified")]);
    expect(notify).toHaveBeenCalledTimes(3);
    publishProjectAgents([enrolled("archer", "provider_unverified", "other")]);
    expect(notify).toHaveBeenCalledTimes(4);
  });

  it("replaces the roster wholesale, so a withdrawn agent actually leaves", () => {
    publishProjectAgents([enrolled("arch"), enrolled("sec")]);
    publishProjectAgents([enrolled("arch")]);
    expect((snapshot()).map((a) => a.agent.nickname)).toEqual(["arch"]);

    // A failed read publishes nothing enrolled rather than leaving a stale list.
    publishProjectAgents([]);
    expect(snapshot()).toEqual([]);
  });

  it("keeps the revision counter and the roster independent", () => {
    // They share a subscriber set; a bump must not invent a roster, and a
    // publish must not be mistaken for a bump.
    notifyAgentRegistryChanged();
    expect(snapshot()).toEqual([]);

    publishProjectAgents([enrolled("arch")]);
    expect((snapshot()).map((a) => a.agent.nickname)).toEqual(["arch"]);
  });

  it("forgets the roster on reset, so one test's is not served to the next", () => {
    publishProjectAgents([enrolled("arch")]);
    resetAgentRegistry();
    expect(snapshot()).toEqual([]);
  });
});
