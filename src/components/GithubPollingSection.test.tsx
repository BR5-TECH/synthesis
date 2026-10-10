import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { GithubPollingSection } from "./GithubPollingSection";
import { Settings } from "./Settings";
import { pollingView } from "../test/githubPollingFixtures";
import { resetSettingsSections, settingsSectionsPending } from "../state/settingsSweep";
import type { GithubPollingView, GithubProjectOption } from "../types";

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

let state: GithubPollingView;
let listImpl: () => Promise<GithubProjectOption[]>;
let setImpl: (args: {
  projectNodeId: string | null;
  intervalMinutes: number | null;
}) => Promise<GithubPollingView>;

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

beforeEach(() => {
  handlers = [];
  state = pollingView();
  listImpl = async () => PROJECTS;
  setImpl = async (args) => {
    state = pollingView({
      settings: {
        projectNodeId: args.projectNodeId,
        intervalMinutes: args.intervalMinutes as GithubPollingView["settings"]["intervalMinutes"],
      },
      configuration:
        args.projectNodeId === null
          ? { state: "unset", errorCode: null, error: null, projectTitle: null }
          : {
              state: "valid",
              errorCode: null,
              error: null,
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
      case "set_github_polling_settings":
        return setImpl(args as never);
      default:
        return undefined;
    }
  });
});
afterEach(cleanup);

const projectSelect = () =>
  screen.getByLabelText("GitHub Project") as HTMLSelectElement;
const intervalSelect = () =>
  screen.getByLabelText("Polling interval") as HTMLSelectElement;

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

  it("SET-FR-VLQJ: a change persists at once, with no Save control", async () => {
    render(<GithubPollingSection />);
    await waitFor(() => expect(projectSelect().value).toBe("P1"));
    await userEvent.selectOptions(projectSelect(), "P2");
    await waitFor(() => expect(calls("set_github_polling_settings")).toHaveLength(1));
    expect(calls("set_github_polling_settings")[0][1]).toEqual({
      projectNodeId: "P2",
      intervalMinutes: 5,
    });
    await userEvent.selectOptions(intervalSelect(), "");
    await waitFor(() => expect(calls("set_github_polling_settings")).toHaveLength(2));
    expect(calls("set_github_polling_settings")[1][1]).toEqual({
      projectNodeId: "P2",
      intervalMinutes: null,
    });
    expect(screen.queryByRole("button", { name: /^Save/ })).toBeNull();
    await userEvent.selectOptions(projectSelect(), "");
    await waitFor(() =>
      expect(calls("set_github_polling_settings")[2][1]).toEqual({
        projectNodeId: null,
        intervalMinutes: null,
      }),
    );
  });

  it("SET-FR-VLQJ: a refused write keeps the previous values and renders the error inline", async () => {
    setImpl = () => Promise.reject("invalid_interval");
    render(<GithubPollingSection />);
    await waitFor(() => expect(intervalSelect().value).toBe("5"));
    await userEvent.selectOptions(intervalSelect(), "15");
    expect(await screen.findByTestId("github-polling-save-error")).toHaveTextContent(
      "That polling interval is not available.",
    );
    expect(intervalSelect().value).toBe("5");
    expect(projectSelect().value).toBe("P1");
  });

  it("SET-FR-GKTA: states no Project, a valid Project with its title, or the error with polling disabled", async () => {
    state = pollingView({
      settings: { projectNodeId: null, intervalMinutes: null },
      configuration: { state: "unset", errorCode: null, error: null, projectTitle: null },
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
        error: null,
        projectTitle: "Roadmap",
      },
    });
    render(<GithubPollingSection />);
    const report = await screen.findByTestId("github-polling-configuration");
    await waitFor(() => expect(report).toHaveTextContent(/“In Progress”/));
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
      ["ready_option_missing", "invalid", /no option named exactly “Ready”/],
      ["in_progress_option_missing", "invalid", /no option named exactly “In Progress”/],
      [null, "unchecked", /“Roadmap” is selected\. It is checked on the next poll\./],
    ];
    for (const [code, kind, wording] of cases) {
      cleanup();
      state = pollingView({
        configuration: { state: kind, errorCode: code, error: null, projectTitle: "Roadmap" },
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
    await userEvent.selectOptions(projectSelect(), "P2");
    await userEvent.selectOptions(intervalSelect(), "15");
    await waitFor(() => expect(calls("set_github_polling_settings")).toHaveLength(2));
    expect(settingsSectionsPending()).toEqual([]);
    expect(screen.queryByRole("button", { name: /save/i })).toBeNull();
  });
});
