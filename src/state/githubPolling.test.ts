import { describe, expect, it } from "vitest";

import {
  githubPollingErrorMessage,
  intervalMs,
  newIssuesBody,
  newIssuesTitle,
  readyTasksState,
  refreshAvailable,
  refusalCode,
  timedPollingEnabled,
} from "./githubPolling";
import { pollingView } from "../test/githubPollingFixtures";

describe("polling rules", () => {
  it("GIT-FR-CVSB: timed polls need a Project, an interval, and a configuration that is not invalid", () => {
    expect(timedPollingEnabled(null)).toBe(false);
    expect(timedPollingEnabled(pollingView())).toBe(true);
    expect(
      timedPollingEnabled(
        pollingView({ settings: { projectNodeId: "P1", intervalMinutes: null } }),
      ),
    ).toBe(false);
    expect(
      timedPollingEnabled(
        pollingView({ settings: { projectNodeId: null, intervalMinutes: 5 } }),
      ),
    ).toBe(false);
    expect(
      timedPollingEnabled(
        pollingView({
          configuration: {
            state: "invalid",
            errorCode: "status_field_missing",
            error: "No Status field.",
            projectTitle: "Roadmap",
          },
        }),
      ),
    ).toBe(false);
    // `unchecked` permits polling (GPP contract surface).
    expect(
      timedPollingEnabled(
        pollingView({
          configuration: {
            state: "unchecked",
            errorCode: null,
            error: null,
            projectTitle: "Roadmap",
          },
        }),
      ),
    ).toBe(true);
  });

  it("GIT-FR-EZFL / SET-FR-NLIX: Refresh needs a Project and no invalid configuration, whatever the interval", () => {
    expect(refreshAvailable(null)).toBe(false);
    expect(
      refreshAvailable(
        pollingView({ settings: { projectNodeId: "P1", intervalMinutes: null } }),
      ),
    ).toBe(true);
    expect(
      refreshAvailable(
        pollingView({ settings: { projectNodeId: null, intervalMinutes: 5 } }),
      ),
    ).toBe(false);
    expect(
      refreshAvailable(
        pollingView({
          configuration: {
            state: "invalid",
            errorCode: "project_unavailable",
            error: null,
            projectTitle: null,
          },
        }),
      ),
    ).toBe(false);
  });

  it("GIT-FR-CVSB: the interval is minutes, and Off is no interval", () => {
    expect(intervalMs(5)).toBe(300_000);
    expect(intervalMs(null)).toBeNull();
  });
});

describe("GIT-FR-HNGQ / GIT-FR-RUAH: the one state the section renders", () => {
  it("GIT-FR-HNGQ: names not configured, configuration error, loading, empty, and rows", () => {
    expect(readyTasksState(null, false)).toBe("unread");
    expect(
      readyTasksState(
        pollingView({ settings: { projectNodeId: null, intervalMinutes: 5 } }),
        false,
      ),
    ).toBe("not-configured");
    expect(
      readyTasksState(
        pollingView({
          configuration: {
            state: "invalid",
            errorCode: "ready_option_missing",
            error: "x",
            projectTitle: "Roadmap",
          },
        }),
        false,
      ),
    ).toBe("configuration-error");
    expect(
      readyTasksState(pollingView({ tasks: [], lastSuccessAt: null, polling: true }), false),
    ).toBe("loading");
    expect(readyTasksState(pollingView({ tasks: [], lastSuccessAt: null }), true)).toBe(
      "loading",
    );
    expect(readyTasksState(pollingView({ tasks: [] }), false)).toBe("empty");
    expect(readyTasksState(pollingView({ tasks: [], lastSuccessAt: null }), false)).toBe(
      "unpolled",
    );
    expect(readyTasksState(pollingView(), false)).toBe("rows");
  });

  it("GIT-FR-RUAH: a stale result is never the empty state", () => {
    expect(
      readyTasksState(
        pollingView({ tasks: [], stale: true, lastErrorCode: "github_unreachable" }),
        false,
      ),
    ).toBe("rows");
  });
});

describe("NTF-FR-JLXL: the raise text", () => {
  const issue = (n: number) => ({ issueNumber: n, title: `Task ${n}` });

  it("NTF-FR-JLXL: names the titles of at most three new issues", () => {
    expect(newIssuesBody([issue(1)])).toBe("Task 1");
    expect(newIssuesBody([issue(1), issue(2), issue(3)])).toBe(
      "Task 1, Task 2, Task 3",
    );
    expect(newIssuesTitle([issue(1)])).toBe("New ready task on GitHub");
  });

  it("NTF-FR-JLXL: names the count of more than three new issues", () => {
    const four = [issue(1), issue(2), issue(3), issue(4)];
    expect(newIssuesBody(four)).toBe("4 new ready tasks");
    expect(newIssuesBody(four)).not.toContain("Task 1");
    expect(newIssuesTitle(four)).toBe("4 new ready tasks on GitHub");
  });
});

describe("GIT-FR-NQTZ: refusal text", () => {
  it("GIT-FR-NQTZ: maps a bare typed code to displayable text", () => {
    expect(refusalCode("task_not_ready")).toBe("task_not_ready");
    expect(refusalCode("Error: claim_pending: detail")).toBe("claim_pending");
    expect(githubPollingErrorMessage("task_not_ready")).toMatch(/no longer a ready Task/);
    expect(githubPollingErrorMessage("status_update_failed")).toMatch(/In Progress/);
    expect(githubPollingErrorMessage("something_new")).toContain("something_new");
  });

  it("AAP-FR-LRTC: a refused certificate names the host and the cause", () => {
    const wire = "tls_untrusted:unknown_issuer:api.github.com";
    for (const rejection of [wire, `Error: ${wire}`, new Error(wire)]) {
      const text = githubPollingErrorMessage(rejection);
      expect(text).toContain("api.github.com");
      expect(text).toMatch(/issuer.*unknown/);
    }
  });

  it("AAP-FR-LRTC: the bare code has no sentence of its own, so no host-less text is shown for it", () => {
    expect(githubPollingErrorMessage("tls_untrusted")).toBe(
      "The GitHub operation failed (tls_untrusted).",
    );
  });
});
