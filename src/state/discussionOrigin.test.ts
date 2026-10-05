import { describe, expect, it } from "vitest";

import contract from "../test/contracts/conversationOrigin.json";
import type { ConversationOrigin, Discussion } from "../types";
import {
  originArtifactId,
  originDraftId,
  originFor,
  originKind,
  type OriginKind,
} from "./discussionOrigin";

/**
 * The origins in `conversationOrigin.json` are the ones the backend's
 * `ConversationOrigin` reads. The Rust test
 * `src-tauri/src/agent_conversations/tests/origin_contract.rs` deserializes the
 * same file, so a change to the shape on one side fails a test on the other.
 */
const CASES = contract as { kind: OriginKind; origin: ConversationOrigin }[];

function discussionOf(origin: ConversationOrigin): Discussion {
  return {
    id: origin.discussionId,
    target: origin.target,
    fragmentTarget: origin.fragmentTarget ?? null,
    comments: [],
    locked: false,
    resolved: false,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
  };
}

describe("the origin a turn is dispatched with", () => {
  it("AGC-FR-05: the contract holds every origin kind", () => {
    expect(CASES.map((c) => c.kind).sort()).toEqual([
      "artifact_comment",
      "artifact_discussion",
      "draft_comment",
      "draft_discussion",
      "note_discussion",
    ]);
  });

  it.each(CASES.map((c) => [c.kind, c] as const))(
    "AGC-FR-05: a %s discussion sends exactly the shape the backend reads",
    (_kind, { origin }) => {
      expect(originFor(discussionOf(origin))).toStrictEqual(origin);
    },
  );

  it.each(CASES.map((c) => [c.kind, c] as const))(
    "AGC-FR-05: a %s origin reads back as its kind",
    (kind, { origin }) => {
      expect(originKind(origin)).toBe(kind);
    },
  );

  it("AGC-FR-05: an origin with no fragment target key reads as a whole-target kind", () => {
    // The backend reads an absent `fragmentTarget` as none (`#[serde(default)]`).
    expect(originKind({ discussionId: "d", target: { kind: "draft", draftId: "x" } })).toBe(
      "draft_discussion",
    );
    expect(
      originKind({ discussionId: "d", target: { kind: "artifact", artifactId: "a.md" } }),
    ).toBe("artifact_discussion");
  });

  it("AGC-FR-05: a discussion that omits its fragment target sends null", () => {
    const d = discussionOf(CASES[0].origin);
    delete (d as Partial<Discussion>).fragmentTarget;
    expect(originFor(d).fragmentTarget).toBeNull();
  });

  it("AGC-FR-05: the owner's id reads from the target alone", () => {
    const byKind = Object.fromEntries(CASES.map((c) => [c.kind, c.origin]));
    expect(originDraftId(byKind.draft_comment)).toBe("draft-1");
    expect(originDraftId(byKind.artifact_comment)).toBeNull();
    expect(originArtifactId(byKind.artifact_discussion)).toBe(
      "specifications/ui/a.md",
    );
    expect(originArtifactId(byKind.note_discussion)).toBeNull();
  });
});
