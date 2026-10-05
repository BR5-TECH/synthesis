import { beforeEach, describe, expect, it } from "vitest";

import {
  clearAllComposers,
  getComposer,
  setComposerAttachments,
  setComposerBody,
} from "./discussionComposers";
import { discussionTargetKey } from "../types";
import type { DiscussionTarget } from "../types";

/**
 * ACT-FR-14: the Discuss composer's unposted text and its pending attachments
 * belong to the **item** rather than to the tab.
 *
 * A pure store, so both halves of that claim are testable without a backend or a
 * render: what is typed against one item is not visible against another, and it
 * survives everything short of the project going away.
 */
const artifact = (path: string): DiscussionTarget => ({
  kind: "artifact",
  artifactId: path,
});
const draft = (id: string): DiscussionTarget => ({ kind: "draft", draftId: id });
const key = discussionTargetKey;

beforeEach(() => clearAllComposers());

describe("the composer's text belongs to the item (ACT-FR-14)", () => {
  it("keeps one body per item and starts every item empty", () => {
    expect(getComposer(key(artifact("a.md"))).body).toBe("");

    setComposerBody(key(artifact("a.md")), "is this ready?");
    setComposerBody(key(artifact("b.md")), "something else");

    expect(getComposer(key(artifact("a.md"))).body).toBe("is this ready?");
    expect(getComposer(key(artifact("b.md"))).body).toBe("something else");
    // The store is not a mount: reading it again returns what was typed, which
    // is what makes dismissing the composer and reopening it lossless.
    expect(getComposer(key(artifact("a.md"))).body).toBe("is this ready?");
  });

  it("never confuses a draft with an artifact of the same name", () => {
    // The two are different things that can share a name, so the prefix is part
    // of the key rather than a formatting nicety (`discussionTargetKey`).
    setComposerBody(key(draft("spec")), "about the draft");
    setComposerBody(key(artifact("spec")), "about the file");

    expect(getComposer(key(draft("spec"))).body).toBe("about the draft");
    expect(getComposer(key(artifact("spec"))).body).toBe("about the file");
    expect(key(draft("spec"))).not.toBe(key(artifact("spec")));
  });

  it("holds the pending attachments on exactly the same terms", () => {
    const shot = {
      input: {
        kind: "inline" as const,
        mediaType: "image/png",
        filename: "shot.png",
        data: "aGk=",
      },
      name: "shot.png",
    };
    expect(getComposer(key(artifact("a.md"))).attachments).toEqual([]);

    setComposerAttachments(key(artifact("a.md")), [shot]);
    expect(getComposer(key(artifact("a.md"))).attachments).toEqual([shot]);
    // Queued against one item and invisible against every other.
    expect(getComposer(key(artifact("b.md"))).attachments).toEqual([]);
  });

  it("goes with the project, because a path names a file in one content root", () => {
    setComposerBody(key(artifact("a.md")), "half a thought");
    setComposerAttachments(key(draft("d1")), [
      { input: { kind: "url", url: "https://x", mediaType: "image/png" }, name: "x" },
    ]);

    clearAllComposers();

    // ACT-FR-14: none of it is persisted, and it does not outlive the project it
    // was typed in — otherwise a message would reappear over a file of the same
    // path in a different worktree.
    expect(getComposer(key(artifact("a.md"))).body).toBe("");
    expect(getComposer(key(draft("d1"))).attachments).toEqual([]);
  });

  it("an item never typed into reads as empty without being created", () => {
    // Reading must not allocate, or every artifact ever opened would accumulate
    // a session whether or not the author ever reached for Discuss.
    const before = getComposer(key(artifact("never.md")));
    expect(before.body).toBe("");
    expect(before.attachments).toEqual([]);
    // The same shared empty, so a read costs nothing and identity is stable.
    expect(getComposer(key(artifact("also-never.md")))).toBe(before);
  });
});
