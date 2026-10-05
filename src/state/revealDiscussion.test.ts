/**
 * The one reveal route (`CVP-conversation-presentation.md` CVP-FR-06,
 * CVP-FR-07, CVP-FR-54).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  consumePendingFocus,
  registerSurface,
  resetDiscussionFocus,
} from "./discussionFocus";
import {
  publishKnownArtifacts,
  resetOwnerAvailability,
} from "./ownerAvailability";
import {
  revealDiscussion,
  type DiscussionReveal,
  type RevealDeps,
} from "./revealDiscussion";

function deps(over: Partial<RevealDeps> = {}): RevealDeps {
  return {
    findConversationTab: vi.fn(() => null),
    openConversationTab: vi.fn(),
    openOwner: vi.fn(),
    ...over,
  };
}

const artifact = (over: Partial<DiscussionReveal> = {}): DiscussionReveal => ({
  discussionId: "d1",
  target: { kind: "artifact", artifactId: "a.md" },
  ownerLabel: "a.md",
  subject: "the first session",
  ...over,
});

beforeEach(() => {
  resetDiscussionFocus();
  resetOwnerAvailability();
});
afterEach(() => {
  resetDiscussionFocus();
  resetOwnerAvailability();
});

describe("CVP-FR-54: the route table", () => {
  it("focuses the surface that already shows the discussion and opens nothing", () => {
    const focus = vi.fn();
    registerSurface("d1", "rail", focus);
    const d = deps();

    expect(revealDiscussion(artifact(), d)).toBe("focused");
    expect(focus).toHaveBeenCalledWith("discussion");
    expect(d.openOwner).not.toHaveBeenCalled();
    expect(d.openConversationTab).not.toHaveBeenCalled();
  });

  it("opens the owner of an available artifact and holds the focus request", () => {
    publishKnownArtifacts(["a.md"]);
    const d = deps();

    expect(revealDiscussion(artifact(), d)).toBe("owner");
    expect(d.openOwner).toHaveBeenCalledTimes(1);
    expect(d.openConversationTab).not.toHaveBeenCalled();
    expect(consumePendingFocus("d1")).toBe("discussion");
  });

  it("opens the conversation tab for a note, with or without a discussion", () => {
    const d = deps();
    const note = (id?: string): DiscussionReveal => ({
      discussionId: id,
      target: { kind: "note", noteId: "n1" },
      ownerLabel: "Ask legal",
      subject: "s",
    });

    expect(revealDiscussion(note("d9"), d)).toBe("tab");
    expect(revealDiscussion(note(undefined), d)).toBe("tab");
    expect(d.openConversationTab).toHaveBeenCalledTimes(2);
    expect(d.openOwner).not.toHaveBeenCalled();
    // An opening state lands focus in the composer, under the note's key.
    expect(consumePendingFocus("note:n1")).toBe("composer");
    expect(consumePendingFocus("d9")).toBe("discussion");
  });

  it("opens the fallback tab when the owner is unavailable, by the caller's word or the store's", () => {
    const d = deps();
    expect(revealDiscussion(artifact({ ownerUnavailable: true }), d)).toBe("tab");

    publishKnownArtifacts(["other.md"]);
    expect(revealDiscussion(artifact(), d)).toBe("tab");
    expect(d.openOwner).not.toHaveBeenCalled();
  });

  it("focuses a conversation tab that already shows the discussion, even for an available owner", () => {
    publishKnownArtifacts(["a.md"]);
    const d = deps({ findConversationTab: vi.fn(() => "conv:d1") });

    expect(revealDiscussion(artifact(), d)).toBe("tab");
    expect(d.openOwner).not.toHaveBeenCalled();
  });

  it("creates nothing itself: duplicates are the shell and the registry's to refuse (CVP-FR-07)", () => {
    publishKnownArtifacts(["a.md"]);
    const d = deps();
    revealDiscussion(artifact(), d);
    revealDiscussion(artifact(), d);
    expect(d.openOwner).toHaveBeenCalledTimes(2);
    // The route itself creates nothing: the shell's deps dedupe by identity, and
    // the focus registry admits one surface per discussion.
    expect(consumePendingFocus("d1")).toBe("discussion");
  });
});
