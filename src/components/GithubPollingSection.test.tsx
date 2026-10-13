import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { GithubPollingSection } from "./GithubPollingSection";
import { Settings } from "../test/settingsHarness";
import { pollingView } from "../test/githubPollingFixtures";
import { resetSettingsSections, settingsSectionsPending } from "../state/settingsSweep";
import type {
  GithubPollingView,
  GithubProjectOption,
  GithubProjectStatusOption,
} from "../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
type Handler = (ev: { payload: unknown }) => void;
let handlers: [string, Handler][] = [];
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: Handler) => {
    handlers.push([name, handler]);
    return Promise.resolve(() => {
      handlers = handlers.filter(([, h]) => h !== handler);
    });
  },
}));

const PROJECTS: GithubProjectOption[] = [
  { nodeId: "P1", title: "Roadmap", ownerLogin: "acme", number: 3 },
  { nodeId: "P2", title: "Ops board", ownerLogin: "kira", number: 1 },
];

/** The `Status` options each Project holds. */
const OPTIONS: Record<string, GithubProjectStatusOption[]> = {
  P1: [
    { id: "o1", name: "Todo" },
    { id: "o2", name: "Ready" },
    { id: "o3", name: "In Progress" },
  ],
  P2: [
    { id: "o4", name: "Backlog" },
    { id: "o5", name: "ready" },
    { id: "o6", name: "In progress" },
  ],
};

interface SetArgs {
  projectNodeId: string | null;
  intervalMinutes: number | null;
  readyStatus: string | null;
  inProgressStatus: string | null;
}

let state: GithubPollingView;
let listImpl: () => Promise<GithubProjectOption[]>;
let statusesImpl: (projectNodeId: string) => Promise<GithubProjectStatusOption[]>;
let setImpl: (args: SetArgs) => Promise<GithubPollingView>;

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

beforeEach(() => {
  handlers = [];
  state = pollingView();
  listImpl = async () => PROJECTS;
  statusesImpl = async (id) => OPTIONS[id] ?? [];
  setImpl = async (args) => {
    state = pollingView({
      settings: {
        projectNodeId: args.projectNodeId,
        intervalMinutes: args.intervalMinutes as GithubPollingView["settings"]["intervalMinutes"],
        readyStatus: args.readyStatus,
        inProgressStatus: args.inProgressStatus,
      },
      configuration:
        args.projectNodeId === null
          ? { state: "unset", errorCode: null, error: null, missingStatus: null, projectTitle: null }
          : {
              state: "valid",
              errorCode: null,
              error: null,
              missingStatus: null,
              projectTitle: PROJECTS.find((p) => p.nodeId === args.projectNodeId)?.title ?? null,
            },
    });
    return state;
  };
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
    switch (cmd) {
      case "get_github_polling_state":
        return state;
      case "list_github_projects":
        return listImpl();
      case "list_github_project_statuses":
        return statusesImpl(String(args?.projectNodeId));
      case "set_github_polling_settings":
        return setImpl(args as never);
      case "append_log_records":
        return undefined;
      default:
        throw new Error(`unexpected command ${cmd}`);
    }
  });
});
afterEach(cleanup);

const projectSelect = () =>
  screen.getByLabelText("GitHub Project") as HTMLSelectElement;
const intervalSelect = () =>
  screen.getByLabelText("Polling interval") as HTMLSelectElement;
const readySelect = () => screen.getByLabelText("Ready status") as HTMLSelectElement;
const inProgressSelect = () =>
  screen.getByLabelText("In Progress status") as HTMLSelectElement;
/**
 * Every select is disabled while a status read or a write is outstanding, and
 * `selectOptions` on a disabled select does nothing. So each choice waits for
 * its select to be enabled first, however long the read takes.
 */
async function choose(select: () => HTMLSelectElement, value: string) {
  await waitFor(() => expect(select()).toBeEnabled());
  await userEvent.selectOptions(select(), value);
}
const optionLabels = (select: HTMLSelectElement) =>
  Array.from(select.options).map((o) => o.textContent);

describe("the GitHub polling section", () => {
  it("SET-FR-03, SET-FR-BLQN: GitHub Project settings is a section of Project settings holding the polling controls and the Publication group, under one Project selector", async () => {
    render(
      <Settings contentRoot="/a#0" lineEndings="lf" onSelectLineEndings={vi.fn()} />,
    );
    expect(screen.queryByText("GitHub polling")).toBeNull();
    await userEvent.click(screen.getByText("GitHub Project settings"));
    expect(await screen.findByTestId("settings-github-polling")).toBeInTheDocument();
    expect(await screen.findByTestId("settings-github-publication")).toBeInTheDocument();
    expect(screen.queryByText(/stubbed in this UI kit/)).toBeNull();
    // The Project of the polling controls is the only Project the section names.
    expect(screen.getAllByLabelText(/^GitHub Project$/)).toHaveLength(1);
  });

  it("SET-FR-TTTB: lists the Projects by title and owner with None, and reads the selection", async () => {
    render(<GithubPollingSection />);
    await waitFor(() => expect(projectSelect().value).toBe("P1"));
    const labels = Array.from(projectSelect().options).map((o) => o.textContent);
    expect(labels).toEqual(["None", "Roadmap — acme (#3)", "Ops board — kira (#1)"]);
  });

  it("SET-FR-HSTF: the section works for a GitHub Enterprise host, asks for no host input, and states a host mismatch from the backend", async () => {
    state = pollingView({
      repository: { host: "company.ghe.com", owner: "acme", name: "platform" },
      configuration: {
        state: "invalid",
        errorCode: "github_host_mismatch",
        error: null,
        missingStatus: null,
        projectTitle: "Roadmap",
      },
    });
    render(<GithubPollingSection />);
    await waitFor(() => expect(projectSelect().value).toBe("P1"));
    const labels = Array.from(projectSelect().options).map((o) => o.textContent);
    expect(labels).toEqual(["None", "Roadmap — acme (#3)", "Ops board — kira (#1)"]);
    const section = screen.getByTestId("settings-github-polling");
    expect(section.querySelectorAll("input")).toHaveLength(0);
    expect(screen.queryByLabelText(/domain|host/i)).toBeNull();
    expect(screen.getByTestId("github-polling-configuration")).toHaveTextContent(
      "belongs to another GitHub host than this remote",
    );
  });

  it("SET-FR-NLIX: the interval is Off, 1, 5, 15, 30, or 60 minutes", async () => {
    render(<GithubPollingSection />);
    await waitFor(() => expect(intervalSelect().value).toBe("5"));
    expect(Array.from(intervalSelect().options).map((o) => o.textContent)).toEqual([
      "Off",
      "1 minute",
      "5 minutes",
      "15 minutes",
      "30 minutes",
      "60 minutes",
    ]);
  });

  it("SET-FR-VLQJ, SET-FR-CJZH: a change persists at once, with no Save control", async () => {
    render(<GithubPollingSection />);
    await waitFor(() => expect(projectSelect().value).toBe("P1"));
    await choose(projectSelect, "P2");
    await waitFor(() => expect(calls("set_github_polling_settings")).toHaveLength(1));
    expect(calls("set_github_polling_settings")[0][1]).toEqual({
      projectNodeId: "P2",
      intervalMinutes: 5,
      readyStatus: "ready",
      inProgressStatus: "In progress",
    });
    await waitFor(() => expect(intervalSelect()).not.toBeDisabled());
    await choose(intervalSelect, "");
    await waitFor(() => expect(calls("set_github_polling_settings")).toHaveLength(2));
    expect(calls("set_github_polling_settings")[1][1]).toEqual({
      projectNodeId: "P2",
      intervalMinutes: null,
      readyStatus: "ready",
      inProgressStatus: "In progress",
    });
    expect(screen.queryByRole("button", { name: /^Save/ })).toBeNull();
    await choose(projectSelect, "");
    await waitFor(() =>
      expect(calls("set_github_polling_settings")[2][1]).toEqual({
        projectNodeId: null,
        intervalMinutes: null,
        readyStatus: null,
        inProgressStatus: null,
      }),
    );
    // SET-FR-UGSP: the selectors are absent while no Project is selected.
    await waitFor(() => expect(screen.queryByLabelText("Ready status")).toBeNull());
    expect(screen.queryByLabelText("In Progress status")).toBeNull();
  });

  it("SET-FR-VLQJ: a refused write keeps the previous values and renders the error inline", async () => {
    setImpl = () => Promise.reject("invalid_interval");
    render(<GithubPollingSection />);
    await waitFor(() => expect(intervalSelect().value).toBe("5"));
    await choose(intervalSelect, "15");
    expect(await screen.findByTestId("github-polling-save-error")).toHaveTextContent(
      "That polling interval is not available.",
    );
    expect(intervalSelect().value).toBe("5");
    expect(projectSelect().value).toBe("P1");
  });

  it("SET-FR-GKTA: states no Project, a valid Project with its title, or the error with polling disabled", async () => {
    state = pollingView({
      settings: { projectNodeId: null, intervalMinutes: null, readyStatus: null, inProgressStatus: null },
      configuration: { state: "unset", errorCode: null, error: null, missingStatus: null, projectTitle: null },
    });
    const view = render(<GithubPollingSection />);
    expect(await screen.findByTestId("github-polling-configuration")).toHaveTextContent(
      "No GitHub Project is selected",
    );
    view.unmount();

    state = pollingView();
    const valid = render(<GithubPollingSection />);
    await waitFor(() =>
      expect(screen.getByTestId("github-polling-configuration")).toHaveTextContent(
        "Configured with “Roadmap”",
      ),
    );
    valid.unmount();

    state = pollingView({
      configuration: {
        state: "invalid",
        errorCode: "in_progress_option_missing",
        error: "The In Progress status “Doing” is not an option of the “Status” field of the GitHub Project “Roadmap”. Choose an option in the GitHub Project settings.",
        missingStatus: "Doing",
        projectTitle: "Roadmap",
      },
    });
    render(<GithubPollingSection />);
    const report = await screen.findByTestId("github-polling-configuration");
    await waitFor(() => expect(report).toHaveTextContent(/“Doing”/));
    expect(report).toHaveTextContent("Choose an option");
    expect(report).toHaveTextContent("Polling is disabled until this is fixed.");
  });

  it("SET-FR-GXJU: a Project list that cannot be read shows the error with Retry and keeps the selection", async () => {
    listImpl = () => Promise.reject("github_unreachable");
    render(<GithubPollingSection />);
    expect(await screen.findByTestId("github-projects-error")).toHaveTextContent(
      "GitHub could not be reached",
    );
    await waitFor(() => expect(projectSelect().value).toBe("P1"));
    expect(projectSelect().selectedOptions[0].textContent).toBe("Roadmap");

    listImpl = async () => PROJECTS;
    await userEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(screen.queryByTestId("github-projects-error")).toBeNull());
    expect(calls("list_github_projects")).toHaveLength(2);
    expect(projectSelect().value).toBe("P1");
  });

  it("SET-FR-GXJU: a configuration error found by a poll appears on github-polling-changed", async () => {
    render(<GithubPollingSection />);
    await waitFor(() =>
      expect(screen.getByTestId("github-polling-configuration")).toHaveTextContent(
        "Configured with",
      ),
    );
    state = pollingView({
      configuration: {
        state: "invalid",
        errorCode: "project_unavailable",
        error: "The Project is gone.",
        missingStatus: null,
        projectTitle: "Roadmap",
      },
    });
    await act(async () => {
      handlers
        .filter(([n]) => n === "github-polling-changed")
        .forEach(([, h]) => h({ payload: { newIssues: [] } }));
    });
    await waitFor(() =>
      expect(screen.getByTestId("github-polling-configuration")).toHaveTextContent(
        "The Project is gone.",
      ),
    );
  });

  it("SET-FR-GKTA: names what to fix for each configuration error, and the unchecked state", async () => {
    const cases: [string | null, "invalid" | "unchecked", RegExp][] = [
      ["project_unavailable", "invalid", /missing or the GitHub token cannot read it/],
      ["status_field_missing", "invalid", /no single-select field named “Status”/],
      ["ready_option_missing", "invalid", /The Ready status is not an option.*Choose an option/],
      ["in_progress_option_missing", "invalid", /The In Progress status is not an option.*Choose an option/],
      [null, "unchecked", /“Roadmap” is selected\. It is checked on the next poll\./],
    ];
    for (const [code, kind, wording] of cases) {
      cleanup();
      state = pollingView({
        configuration: { state: kind, errorCode: code, error: null, missingStatus: null, projectTitle: "Roadmap" },
      });
      render(<GithubPollingSection />);
      const report = await screen.findByTestId("github-polling-configuration");
      expect(report, String(code)).toHaveTextContent(wording);
      if (kind === "invalid")
        expect(report, String(code)).toHaveTextContent(
          "Polling is disabled until this is fixed.",
        );
      else expect(report).not.toHaveTextContent(/disabled/);
    }
  });

  it("SET-FR-VLQJ: a polling change marks no section dirty and offers no save", async () => {
    resetSettingsSections();
    render(
      <Settings contentRoot="/a#0" lineEndings="lf" onSelectLineEndings={vi.fn()} />,
    );
    await userEvent.click(screen.getByText("GitHub Project settings"));
    await waitFor(() => expect(projectSelect().value).toBe("P1"));
    await choose(projectSelect, "P2");
    await waitFor(() => expect(intervalSelect()).not.toBeDisabled());
    await choose(intervalSelect, "15");
    await waitFor(() => expect(calls("set_github_polling_settings")).toHaveLength(2));
    expect(settingsSectionsPending()).toEqual([]);
    expect(screen.queryByRole("button", { name: /save/i })).toBeNull();
  });
});

describe("the Ready status and In Progress status selectors", () => {
  it("SET-FR-UGSP: reads the selected Project's Status options and lists them in both selectors", async () => {
    render(<GithubPollingSection />);
    await waitFor(() => expect(readySelect()).not.toBeDisabled());
    expect(calls("list_github_project_statuses")[0][1]).toEqual({ projectNodeId: "P1" });
    expect(readySelect().value).toBe("Ready");
    expect(inProgressSelect().value).toBe("In Progress");
    expect(optionLabels(readySelect())).toEqual([
      "Todo",
      "Ready",
      "In Progress (the In Progress status)",
    ]);
    expect(calls("set_github_polling_settings")).toHaveLength(0);
  });

  it("SET-FR-JGAK: a loading read keeps the stored values; a failed read shows the error with Retry and writes nothing", async () => {
    let fail = true;
    let release: () => void = () => {};
    const gate = new Promise<void>((resolve) => (release = resolve));
    statusesImpl = async (id) => {
      await gate;
      if (fail) throw new Error("github_unreachable");
      return OPTIONS[id];
    };
    render(<GithubPollingSection />);
    expect(await screen.findByTestId("github-polling-ready-status-loading")).toBeInTheDocument();
    expect(readySelect()).toBeDisabled();
    expect(readySelect().value).toBe("Ready");
    await act(async () => release());
    expect(await screen.findByTestId("github-statuses-error")).toHaveTextContent(
      "GitHub could not be reached",
    );
    expect(readySelect().value).toBe("Ready");
    expect(calls("set_github_polling_settings")).toHaveLength(0);

    fail = false;
    await userEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(screen.queryByTestId("github-statuses-error")).toBeNull());
    await waitFor(() => expect(readySelect()).not.toBeDisabled());
    expect(calls("list_github_project_statuses")).toHaveLength(2);
  });

  it("SET-FR-CJZH: a Project without a default match leaves that selector unset and writes null for it", async () => {
    OPTIONS.P3 = [
      { id: "q1", name: "Queued" },
      { id: "q2", name: "IN PROGRESS" },
    ];
    listImpl = async () => [
      ...PROJECTS,
      { nodeId: "P3", title: "Lab", ownerLogin: "acme", number: 9 },
    ];
    try {
      render(<GithubPollingSection />);
      await waitFor(() => expect(readySelect()).not.toBeDisabled());
      await choose(projectSelect, "P3");
      await waitFor(() => expect(calls("set_github_polling_settings")).toHaveLength(1));
      expect(calls("set_github_polling_settings")[0][1]).toEqual({
        projectNodeId: "P3",
        intervalMinutes: 5,
        readyStatus: null,
        inProgressStatus: "IN PROGRESS",
      });
      await waitFor(() => expect(readySelect().value).toBe(""));
      expect(readySelect().selectedOptions[0].textContent).toBe("Choose an option");
    } finally {
      delete OPTIONS.P3;
    }
  });

  it("SET-FR-SDKP: a change of the Project whose status read fails writes both names as null", async () => {
    statusesImpl = async (id) => {
      if (id === "P2") throw new Error("github_unreachable");
      return OPTIONS[id];
    };
    render(<GithubPollingSection />);
    await waitFor(() => expect(readySelect()).not.toBeDisabled());
    await choose(projectSelect, "P2");
    await waitFor(() => expect(calls("set_github_polling_settings")).toHaveLength(1));
    expect(calls("set_github_polling_settings")[0][1]).toEqual({
      projectNodeId: "P2",
      intervalMinutes: 5,
      readyStatus: null,
      inProgressStatus: null,
    });
    expect(await screen.findByTestId("github-statuses-error")).toBeInTheDocument();
  });

  it("SET-FR-SDKP: a read while a stored name is unset selects its default and persists it at once; a stored name stays", async () => {
    state = pollingView({
      settings: {
        projectNodeId: "P1",
        intervalMinutes: 15,
        readyStatus: null,
        inProgressStatus: "Todo",
      },
    });
    render(<GithubPollingSection />);
    await waitFor(() => expect(calls("set_github_polling_settings")).toHaveLength(1));
    expect(calls("set_github_polling_settings")[0][1]).toEqual({
      projectNodeId: "P1",
      intervalMinutes: 15,
      readyStatus: "Ready",
      inProgressStatus: "Todo",
    });
    await waitFor(() => expect(readySelect().value).toBe("Ready"));
    expect(inProgressSelect().value).toBe("Todo");
  });

  it("SET-FR-JNLK: a stored name the Project no longer holds stays selected, marked unavailable, and is not written", async () => {
    state = pollingView({
      settings: {
        projectNodeId: "P1",
        intervalMinutes: 5,
        readyStatus: "Queued",
        inProgressStatus: "In Progress",
      },
    });
    render(<GithubPollingSection />);
    expect(
      await screen.findByTestId("github-polling-ready-status-unavailable"),
    ).toBeInTheDocument();
    expect(readySelect().value).toBe("Queued");
    expect(readySelect().selectedOptions[0].textContent).toBe("Queued (unavailable)");
    expect(calls("set_github_polling_settings")).toHaveLength(0);

    await choose(readySelect, "Todo");
    await waitFor(() => expect(calls("set_github_polling_settings")).toHaveLength(1));
    expect(calls("set_github_polling_settings")[0][1]).toEqual({
      projectNodeId: "P1",
      intervalMinutes: 5,
      readyStatus: "Todo",
      inProgressStatus: "In Progress",
    });
  });

  it("SET-FR-QZYV, SET-FR-VLQJ: each selector disables the other's option, and a refused write renders inline", async () => {
    render(<GithubPollingSection />);
    await waitFor(() => expect(inProgressSelect()).not.toBeDisabled());
    const ready = Array.from(inProgressSelect().options).find((o) => o.value === "Ready");
    expect(ready?.disabled).toBe(true);
    const inProgress = Array.from(readySelect().options).find((o) => o.value === "In Progress");
    expect(inProgress?.disabled).toBe(true);

    setImpl = () => Promise.reject("duplicate_status_option");
    await choose(inProgressSelect, "Todo");
    expect(await screen.findByTestId("github-polling-save-error")).toHaveTextContent(
      "must be different options",
    );
    expect(inProgressSelect().value).toBe("In Progress");
  });
});
