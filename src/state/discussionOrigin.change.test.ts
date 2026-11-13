import { describe, expect, it } from "vitest";

import { locatorOf, originChangeKey, originKind } from "./discussionOrigin";
import { ownerLabelOf } from "./ownerAvailability";

const TARGET = { kind: "change", streamId: "s-1", change: "add-logout" } as const;

describe("the origin of a change discussion", () => {
  it("AGC-FR-05: the origin of a whole-change discussion and of a fragment discussion of a change names its own kind", () => {
    expect(originKind({ discussionId: "d", target: TARGET, fragmentTarget: null })).toBe(
      "change_discussion",
    );
    expect(
      originKind({
        discussionId: "d",
        target: TARGET,
        fragmentTarget: { owner: TARGET, path: "proposal.md", start: 0, end: 1, quote: "#" },
      }),
    ).toBe("change_comment");
  });

  it("CVP-FR-FWOB: the origin of a change owner carries the target key of the change", () => {
    expect(originChangeKey({ discussionId: "d", target: TARGET, fragmentTarget: null })).toBe(
      "change:s-1:add-logout",
    );
    expect(
      originChangeKey({ discussionId: "d", target: { kind: "note", noteId: "n" }, fragmentTarget: null }),
    ).toBeNull();
  });

  it("CVP-FR-57: a change owner is called by its change name", () => {
    expect(ownerLabelOf(TARGET)).toBe("add-logout");
  });

  it("CMS-FR-UEXD: a change owner target names the change itself, so the storage commands take no locator hint for it", () => {
    expect(locatorOf(TARGET)).toEqual({});
  });
});
