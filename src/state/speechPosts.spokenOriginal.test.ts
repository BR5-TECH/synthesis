/** The transcript that the post of a speech comment carries (`CMS-comments-storage.md` CMS-FR-UYVK). */
import { beforeEach, describe, expect, it } from "vitest";

import {
  beginSpeechPost,
  claimSpeechPost,
  finishSpeechPost,
  resetSpeechPostsForTest,
  spokenOriginalFor,
} from "./speechPosts";

beforeEach(() => resetSpeechPostsForTest());

describe("the transcript of a speech post", () => {
  it("VOM-FR-QWMK, CMS-FR-UYVK: only the speech body carries the transcript, until it is claimed", () => {
    beginSpeechPost("d1", ["arch"], "Look here.", "Mira aquí.");
    expect(spokenOriginalFor(" Look here. ")).toBe("Mira aquí.");
    expect(spokenOriginalFor("A typed comment.")).toBeUndefined();
    claimSpeechPost({ id: "d1", comments: [{ id: "k1" }] as never }, "Look here.");
    expect(spokenOriginalFor("Look here.")).toBeUndefined();
    finishSpeechPost();
    expect(spokenOriginalFor("Look here.")).toBeUndefined();
  });

  it("CMS-FR-UYVK: a speech post with no translation carries no transcript", () => {
    beginSpeechPost("d1", [], "Look here.");
    expect(spokenOriginalFor("Look here.")).toBeUndefined();
  });
});
