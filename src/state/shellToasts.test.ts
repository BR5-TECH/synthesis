import { beforeEach, describe, expect, it, vi } from "vitest";

import { showPushFailure, showPushSucceeded } from "./pushToast";
import { showDraftCreateFailed, showDraftOpenFailed, showWorktreeFallback } from "./shellToasts";
import { clearToasts, releaseToastsForRoot, resetToasts, visibleToasts } from "./toasts";

beforeEach(() => {
  resetToasts();
});

const byKey = (key: string) => visibleToasts().filter((t) => t.key === key);

describe("the push toast (NTF-FR-PSHT)", () => {
  it("NTF-FR-PSHT: a push that succeeds shows an Info toast with no address", () => {
    showPushSucceeded();
    expect(byKey("push-result")).toEqual([
      expect.objectContaining({ level: "Info", title: "Branch pushed", address: null }),
    ]);
  });

  it("NTF-FR-PSHT: a failure with a route carries the GitHub settings link, and one without has none", () => {
    const open = vi.fn();
    showPushFailure("Pushing to the remote needs a GitHub token.", open);
    expect(byKey("push-result")[0]).toMatchObject({ level: "Error", title: "Push failed" });
    expect(byKey("push-result")[0].link?.label).toBe("Open GitHub settings");
    byKey("push-result")[0].link?.onActivate();
    expect(open).toHaveBeenCalledTimes(1);

    showPushFailure("remote rejected the push");
    expect(byKey("push-result")).toHaveLength(1);
    expect(byKey("push-result")[0].link ?? null).toBeNull();
  });

  it("NTF-FR-PSHT: a second push replaces the toast of the first", () => {
    showPushFailure("A push is already running.");
    showPushSucceeded();
    expect(byKey("push-result")).toHaveLength(1);
    expect(byKey("push-result")[0].level).toBe("Info");
  });
});

describe("the draft toasts (NTF-FR-DRFT)", () => {
  it("NTF-FR-DRFT: a failed creation and a failed open each show an Error toast with the failure text", () => {
    showDraftCreateFailed("drafts store is malformed");
    showDraftOpenFailed("draft_not_found");
    expect(byKey("draft-create-failed")[0]).toMatchObject({
      level: "Error",
      title: "Draft not created",
      body: "drafts store is malformed",
      address: null,
    });
    expect(byKey("draft-open-failed")[0]).toMatchObject({
      level: "Error",
      title: "Draft not opened",
      body: "draft_not_found",
      address: null,
    });
  });
});

describe("the worktree fallback toast (NTF-FR-WTFB)", () => {
  it("NTF-FR-WTFB: the toast waits for the content root of its project and survives the removal at the opening", () => {
    showWorktreeFallback("~/dev/acme");
    expect(byKey("worktree-fallback")).toHaveLength(0);

    // Another project opening does not show it.
    releaseToastsForRoot("~/dev/other");
    expect(byKey("worktree-fallback")).toHaveLength(0);

    // NTF-FR-15: the opening removes every toast, then the root is open.
    clearToasts();
    releaseToastsForRoot("~/dev/acme");
    expect(byKey("worktree-fallback")[0]).toMatchObject({
      level: "Warn",
      title: "Worktree not found",
      address: null,
    });

    // It shows once: a later root event of the same project shows nothing more.
    clearToasts();
    releaseToastsForRoot("~/dev/acme");
    expect(byKey("worktree-fallback")).toHaveLength(0);
  });
});
