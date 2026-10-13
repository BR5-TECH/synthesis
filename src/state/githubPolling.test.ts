import { describe, expect, it } from "vitest";

import {
  autoSelectedStatuses,
  configurationErrorBody,
  configurationErrorTransition,
  defaultStatusOption,
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
        pollingView({ settings: { projectNodeId: "P1", intervalMinutes: null, readyStatus: null, inProgressStatus: null } }),
      ),
    ).toBe(false);
    expect(
      timedPollingEnabled(
        pollingView({ settings: { projectNodeId: null, intervalMinutes: 5, readyStatus: null, inProgressStatus: null } }),
      ),
    ).toBe(false);
    expect(
      timedPollingEnabled(
        pollingView({
          configuration: {
            state: "invalid",
            errorCode: "status_field_missing",
            error: "No Status field.",
            missingStatus: null,
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
            missingStatus: null,
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
        pollingView({ settings: { projectNodeId: "P1", intervalMinutes: null, readyStatus: null, inProgressStatus: null } }),
      ),
    ).toBe(true);
    expect(
      refreshAvailable(
        pollingView({ settings: { projectNodeId: null, intervalMinutes: 5, readyStatus: null, inProgressStatus: null } }),
      ),
    ).toBe(false);
    expect(
      refreshAvailable(
        pollingView({
          configuration: {
            state: "invalid",
            errorCode: "project_unavailable",
            error: null,
            missingStatus: null,
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
        pollingView({ settings: { projectNodeId: null, intervalMinutes: 5, readyStatus: null, inProgressStatus: null } }),
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
            missingStatus: null,
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

  it("GHA-FR-LBLM: a host mismatch is stated, and no remote text does not claim github.com only", () => {
    expect(githubPollingErrorMessage("github_host_mismatch")).toBe(
      "The project token belongs to another GitHub host than this remote. Pick or add a token for the host of the remote.",
    );
    expect(githubPollingErrorMessage("no_github_remote")).not.toContain("github.com");
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

describe("status names", () => {
  const options = [
    { id: "a", name: "Todo" },
    { id: "b", name: "ready" },
    { id: "c", name: "In progress" },
    { id: "d", name: "IN PROGRESS" },
  ];

  it("SET-FR-CJZH: the default is the first option whose name matches case-insensitively, or none", () => {
    expect(defaultStatusOption(options, "Ready")).toBe("ready");
    expect(defaultStatusOption(options, "In Progress")).toBe("In progress");
    expect(defaultStatusOption([{ id: "x", name: "Queued" }], "Ready")).toBeNull();
  });

  it("SET-FR-SDKP: only an unset name takes its default; a stored name stays; no change writes nothing", () => {
    expect(autoSelectedStatuses({ readyStatus: null, inProgressStatus: "Todo" }, options)).toEqual({
      readyStatus: "ready",
      inProgressStatus: "Todo",
    });
    expect(
      autoSelectedStatuses({ readyStatus: "Gone", inProgressStatus: "Todo" }, options),
    ).toBeNull();
    expect(
      autoSelectedStatuses({ readyStatus: null, inProgressStatus: null }, [{ id: "x", name: "Queued" }]),
    ).toBeNull();
  });

  it("SET-FR-QZYV: a default never takes the option the other status holds", () => {
    const both = [
      { id: "r", name: "Ready" },
      { id: "p", name: "In Progress" },
    ];
    expect(
      autoSelectedStatuses({ readyStatus: null, inProgressStatus: "Ready" }, both),
    ).toBeNull();
    expect(
      autoSelectedStatuses({ readyStatus: "In Progress", inProgressStatus: null }, both),
    ).toBeNull();
    expect(defaultStatusOption(both, "Ready", "Ready")).toBeNull();
  });
});

describe("the configuration-error raise", () => {
  const invalid = (code: string, missingStatus: string | null = null) =>
    pollingView({
      configuration: {
        state: "invalid",
        errorCode: code,
        error: "The Project is gone.",
        missingStatus,
        projectTitle: "Roadmap",
      },
    }).configuration;
  const valid = pollingView().configuration;

  it("GIT-FR-AKYZ: raises on a change into invalid or of its code, once, and retracts when it leaves invalid", () => {
    expect(configurationErrorTransition(null, valid)).toEqual({ action: "none", code: null });
    expect(configurationErrorTransition(null, invalid("ready_option_missing"))).toEqual({
      action: "raise",
      code: "ready_option_missing",
    });
    expect(
      configurationErrorTransition("ready_option_missing", invalid("ready_option_missing")),
    ).toEqual({ action: "none", code: "ready_option_missing" });
    expect(
      configurationErrorTransition("ready_option_missing", invalid("project_unavailable")),
    ).toEqual({ action: "raise", code: "project_unavailable" });
    expect(configurationErrorTransition("project_unavailable", valid)).toEqual({
      action: "retract",
      code: null,
    });
  });

  it("NTF-FR-RNGG: the body names the missing status and the Project, or carries the error", () => {
    expect(configurationErrorBody(invalid("in_progress_option_missing", "Doing"))).toBe(
      "The In Progress status “Doing” is not an option in the GitHub Project “Roadmap”. Choose an option in GitHub Project settings.",
    );
    expect(configurationErrorBody(invalid("project_unavailable"))).toBe("The Project is gone.");
  });
});
