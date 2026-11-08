import { describe, expect, it, vi } from "vitest";

import {
  currentMainBranchWrite,
  dismissMainBranchWrite,
  parseMainBranchRefusal,
  requestMainBranchWrite,
  setMainBranchWriteOpening,
} from ".";
import { mainGuard, unconfirmedRefusal } from "../../test/mainBranchWriteFixtures";

describe("the standalone confirmation as an overlay (SNV-FR-56, MBW-FR-GYVD)", () => {
  it("SNV-FR-56, MBW-FR-GYVD: opening the confirmation closes every other overlay, and another overlay opening closes it with no write", async () => {
    const closeOthers = vi.fn();
    setMainBranchWriteOpening(closeOthers);
    const answer = requestMainBranchWrite({
      write: "commit",
      branch: null,
      guard: mainGuard(),
      readError: null,
    });
    expect(closeOthers).toHaveBeenCalledTimes(1);
    expect(currentMainBranchWrite()).not.toBeNull();

    dismissMainBranchWrite();
    await expect(answer).resolves.toBe(false);
    expect(currentMainBranchWrite()).toBeNull();
  });

  it("SNV-FR-56: a second confirmation answers the first one, so no caller waits for ever", async () => {
    const first = requestMainBranchWrite({ write: "push", branch: null, guard: mainGuard(), readError: null });
    const second = requestMainBranchWrite({ write: "commit", branch: null, guard: mainGuard(), readError: null });
    await expect(first).resolves.toBe(false);
    expect(currentMainBranchWrite()?.write).toBe("commit");
    dismissMainBranchWrite();
    await expect(second).resolves.toBe(false);
  });
});

describe("the refusals of the check (MBW-FR-YSMC)", () => {
  it("MBW-FR-YSMC: a refusal carries the guard, and any other error is no refusal of the check", () => {
    expect(parseMainBranchRefusal(unconfirmedRefusal(mainGuard({ branch: "trunk" })))).toEqual({
      code: "main_branch_write_unconfirmed",
      guard: mainGuard({ branch: "trunk" }),
    });
    expect(parseMainBranchRefusal(new Error(unconfirmedRefusal()))?.code).toBe(
      "main_branch_write_unconfirmed",
    );
    expect(parseMainBranchRefusal("main_branch_write_unconfirmed")).toEqual({
      code: "main_branch_write_unconfirmed",
      guard: null,
    });
    expect(parseMainBranchRefusal("main_branch_protected: {}")).toBeNull();
    expect(parseMainBranchRefusal("worktree_dirty: a.md")).toBeNull();
  });
});
