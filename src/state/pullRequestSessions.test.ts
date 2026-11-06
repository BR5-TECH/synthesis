/**
 * The saved state of the Create a PR window, per head branch
 * (`../../specifications/ui/CPR-create-pull-request.md` CPR-FR-JOIG,
 * CPR-FR-SSQI, CPR-FR-ESXZ).
 */
import { beforeEach, describe, expect, it } from "vitest";
import { act, renderHook } from "@testing-library/react";

import {
  clearAllPullRequestSessions,
  clearPullRequestSession,
  getPullRequestSession,
  onPullRequestSessionsCleared,
  openPullRequestSession,
  pullRequestSessionEpoch,
  updatePullRequestSession,
  usePullRequestSession,
} from "./pullRequestSessions";

const SOURCE = { head: "feature", base: "main", title: "Feature" };

beforeEach(() => {
  clearAllPullRequestSessions();
});

describe("the saved state of a head branch", () => {
  it("CPR-FR-JOIG: with no saved state it starts from the source: description empty, draft off, no feature", () => {
    const { session, created } = openPullRequestSession(SOURCE);
    expect(created).toBe(true);
    expect(session).toMatchObject({
      head: "feature",
      title: "Feature",
      body: "",
      base: "main",
      draft: false,
      features: [],
      summary: null,
      combined: false,
    });
  });

  it("CPR-FR-SSQI: a new opening for the head branch restores the saved state instead of the source", () => {
    openPullRequestSession(SOURCE);
    updatePullRequestSession("feature", {
      title: "Typed",
      body: "Body",
      base: "develop",
      draft: true,
      features: [{ owner: "acme", repository: "platform", number: 12, title: "Git flow" }],
    });
    const again = openPullRequestSession({ ...SOURCE, title: "Another source" });
    expect(again.created).toBe(false);
    expect(again.session).toMatchObject({
      title: "Typed",
      body: "Body",
      base: "develop",
      draft: true,
      features: [{ number: 12 }],
    });
  });

  it("CPR-FR-SSQI: each head branch has its own saved state", () => {
    openPullRequestSession(SOURCE);
    openPullRequestSession({ head: "other", base: "main", title: "Other" });
    updatePullRequestSession("feature", { title: "Changed" });
    expect(getPullRequestSession("other")?.title).toBe("Other");
  });

  it("CPR-FR-ESXZ: a created pull request clears the state of its head branch alone", () => {
    openPullRequestSession(SOURCE);
    openPullRequestSession({ head: "other", base: "main", title: "Other" });
    clearPullRequestSession("feature");
    expect(getPullRequestSession("feature")).toBeNull();
    expect(getPullRequestSession("other")).not.toBeNull();
    expect(openPullRequestSession(SOURCE).created).toBe(true);
  });

  it("CPR-FR-ESXZ: a change of the project clears every saved state and starts a new epoch", () => {
    let cleared = 0;
    const stop = onPullRequestSessionsCleared(() => (cleared += 1));
    openPullRequestSession(SOURCE);
    openPullRequestSession({ head: "other", base: "main", title: "Other" });
    const before = pullRequestSessionEpoch();
    clearAllPullRequestSessions();
    stop();
    expect(getPullRequestSession("feature")).toBeNull();
    expect(getPullRequestSession("other")).toBeNull();
    expect(pullRequestSessionEpoch()).toBe(before + 1);
    expect(cleared).toBe(1);
  });

  it("CPR-FR-SSQI: a component that reads the saved state renders each change", () => {
    openPullRequestSession(SOURCE);
    const { result } = renderHook(() => usePullRequestSession("feature"));
    expect(result.current?.title).toBe("Feature");
    act(() => updatePullRequestSession("feature", { title: "Next" }));
    expect(result.current?.title).toBe("Next");
    act(() => clearPullRequestSession("feature"));
    expect(result.current).toBeNull();
  });

  it("CPR-FR-SSQI: a change for a head branch with no saved state does nothing", () => {
    updatePullRequestSession("nobody", { title: "x" });
    expect(getPullRequestSession("nobody")).toBeNull();
  });
});
