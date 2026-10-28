import { beforeEach, describe, expect, it, vi } from "vitest";

import { resetLogBufferForTest } from "../logging";
import { GITHUB_TOKEN_ERRORS, type PullOutcome } from "../types";
import { PULL_RESULT_KEY, showPullFailure, showPullOutcome } from "./pullToast";
import { resetToasts, visibleToasts } from "./toasts";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string) =>
    cmd === "append_log_records" ? undefined : Promise.reject(new Error(`unexpected ${cmd}`)),
  ),
}));

beforeEach(() => {
  resetToasts();
  resetLogBufferForTest();
});

function outcome(over: Partial<PullOutcome>): PullOutcome {
  return {
    state: "up_to_date",
    branch: "main",
    upstream: "origin/main",
    ahead: 0,
    behind: 0,
    updatedPaths: [],
    refresh: { context: null as never, remoteState: "refreshed" },
    ...over,
  };
}

const shown = () => visibleToasts().filter((t) => t.key === PULL_RESULT_KEY);

describe("the pull toast (NTF-FR-PLQV)", () => {
  it.each([
    [{ state: "fast_forwarded", behind: 3 }, "Info", "Branch pulled", "Pulled 3 commits into main."],
    [{ state: "up_to_date" }, "Info", "Branch up to date", "main is up to date with origin/main."],
    [{ state: "refused", error: "no_upstream" }, "Error", "Pull failed", "tracks no remote branch"],
    [
      { state: "fetch_failed", error: GITHUB_TOKEN_ERRORS.githubUnreachable },
      "Error",
      "Pull failed",
      "Couldn't reach the remote",
    ],
    [
      { state: "fetch_failed", error: GITHUB_TOKEN_ERRORS.selectionRequired },
      "Error",
      "Pull failed",
      "No GitHub token was selected",
    ],
  ] as const)(
    "NTF-FR-PLQV: %o shows an %s toast titled %s",
    (over, level, title, body) => {
      showPullOutcome(outcome(over as Partial<PullOutcome>), vi.fn());
      expect(shown()).toHaveLength(1);
      expect(shown()[0]).toMatchObject({ level, title, address: null });
      expect(shown()[0].body).toContain(body);
      // Only a missing token carries a route.
      expect(shown()[0].link ?? null).toBeNull();
    },
  );

  it("NTF-FR-PLQV: a diverged branch shows no toast, because the dialog answers it", () => {
    showPullOutcome(outcome({ state: "diverged", ahead: 2, behind: 1 }), vi.fn());
    expect(shown()).toHaveLength(0);
  });

  it("NTF-FR-PLQV, WTS-FR-35: a missing token carries the link to the GitHub settings", () => {
    const openSettings = vi.fn();
    showPullOutcome(
      outcome({ state: "fetch_failed", error: GITHUB_TOKEN_ERRORS.tokenMissing }),
      openSettings,
    );
    expect(shown()[0]).toMatchObject({ level: "Error", title: "Pull failed" });
    expect(shown()[0].link?.label).toBe("Open GitHub settings");
    shown()[0].link?.onActivate();
    expect(openSettings).toHaveBeenCalledTimes(1);
  });

  it("NTF-FR-PLQV: a missing token with no route to the settings shows no dead link", () => {
    showPullOutcome(outcome({ state: "fetch_failed", error: GITHUB_TOKEN_ERRORS.tokenMissing }));
    expect(shown()[0]).toMatchObject({ level: "Error", title: "Pull failed" });
    expect(shown()[0].link ?? null).toBeNull();
  });

  it("NTF-FR-PLQV: a second pull replaces the toast of the first", () => {
    showPullFailure("A push or a pull is already running.");
    showPullOutcome(outcome({ state: "fast_forwarded", behind: 1 }));
    expect(shown()).toHaveLength(1);
    expect(shown()[0]).toMatchObject({ level: "Info", body: "Pulled 1 commit into main." });
  });
});
