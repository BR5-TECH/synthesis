/**
 * The Flow sync group of GitHub Project settings
 * (`../../specifications/ui/SET-project-settings.md` SET-FR-ZJRC, SET-FR-UFXD,
 * SET-FR-PQWW).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import { FlowSyncSettingsGroup } from "./FlowSyncSettings";
import { resetLogBufferForTest } from "../logging";
import { resetSettingsSections, settingsSectionsPending } from "../state/settingsSweep";
import { pollingView } from "../test/githubPollingFixtures";
import { Settings } from "../test/settingsHarness";
import type {
  FlowTrackerSettings,
  GithubPublicationSettings,
  MetadataList,
  PublicationIssueType,
} from "../types";

/** FTS-FR-LMBE: what the backend reads for unset values. */
const DEFAULTS: FlowTrackerSettings = {
  epicType: "Epic",
  featureType: "Feature",
  taskType: "Task",
  defectType: "Bug",
};

const types = (...names: string[]): MetadataList<PublicationIssueType> => ({
  state: "loaded",
  items: names.map((name) => ({ name })),
  errorCode: null,
  error: null,
});

/** What the Publication group of the same section reads. */
const PUBLICATION: GithubPublicationSettings = {
  parentIssueTypes: ["Feature"],
  subIssueType: "Task",
  subIssueMilestonePolicy: "inherit_parent",
  rootIssueType: null,
  noteIssueType: "Feature",
};

let stored: FlowTrackerSettings;
let getImpl: () => Promise<FlowTrackerSettings>;
let typeList: () => Promise<MetadataList<PublicationIssueType>>;
let setImpl: (args: FlowTrackerSettings) => Promise<FlowTrackerSettings>;
const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);
const select = (label: string) => screen.getByLabelText(label) as HTMLSelectElement;

beforeEach(() => {
  resetLogBufferForTest();
  resetSettingsSections();
  stored = { ...DEFAULTS };
  getImpl = () => Promise.resolve(stored);
  typeList = async () => types("Feature", "Task", "Bug", "Epic");
  setImpl = async (args) => {
    stored = args;
    return stored;
  };
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string, args?: unknown) => {
    switch (cmd) {
      case "append_log_records":
        return Promise.resolve();
      case "get_flow_tracker_settings":
        return getImpl();
      case "get_github_polling_state":
        return Promise.resolve(pollingView());
      case "list_github_projects":
      case "list_github_project_statuses":
        return Promise.resolve([]);
      case "get_github_publication_settings":
        return Promise.resolve(PUBLICATION);
      case "list_github_issue_types":
        return typeList();
      case "set_flow_tracker_settings":
        return setImpl(args as FlowTrackerSettings);
      default:
        return Promise.reject(new Error(`unexpected command ${cmd}`));
    }
  });
});
afterEach(cleanup);

describe("the Flow sync group of GitHub Project settings", () => {
  it("SET-FR-ZJRC, SET-FR-UFXD: one Type selector per issue kind, showing the stored defaults and the offered Types", async () => {
    render(<FlowSyncSettingsGroup />);
    await waitFor(() => expect(select("Epic").value).toBe("Epic"));
    expect(select("Feature").value).toBe("Feature");
    expect(select("Task").value).toBe("Task");
    expect(select("Defect").value).toBe("Bug");
    await waitFor(() =>
      expect([...select("Task").options].map((o) => o.value)).toEqual(["Task", "Feature", "Bug", "Epic"]),
    );
    expect(screen.queryByLabelText("Milestone")).toBeNull();
    // The plan has no deliverable kind (FLO-FR-07), so the group offers four selectors.
    expect(screen.queryByLabelText("Deliverable")).toBeNull();
    expect(screen.getAllByRole("combobox")).toHaveLength(4);
    expect(calls("get_flow_tracker_settings")).toHaveLength(1);
  });

  it("SET-FR-UFXD: a stored Type the list does not hold is selected and marked unavailable", async () => {
    stored = { ...DEFAULTS, defectType: "Incident" };
    render(<FlowSyncSettingsGroup />);
    await waitFor(() => expect(select("Defect").value).toBe("Incident"));
    await waitFor(() => expect(select("Defect").selectedOptions[0].textContent).toBe("Incident (unavailable)"));
  });

  it("SET-FR-UFXD: a Type list that cannot be read keeps every stored value and marks nothing unavailable", async () => {
    typeList = () => Promise.reject(new Error("github_unreachable"));
    stored = { ...DEFAULTS, defectType: "Incident" };
    render(<FlowSyncSettingsGroup />);
    await waitFor(() => expect(select("Defect").value).toBe("Incident"));
    expect(select("Defect").selectedOptions[0].textContent).toBe("Incident");
    expect(calls("set_flow_tracker_settings")).toHaveLength(0);
  });

  it("SET-FR-PQWW: a change persists at once with all four Types", async () => {
    render(<FlowSyncSettingsGroup />);
    await waitFor(() => expect(select("Epic").options.length).toBe(4));
    await userEvent.selectOptions(select("Epic"), "Feature");
    await waitFor(() => expect(select("Epic").value).toBe("Feature"));
    expect(calls("set_flow_tracker_settings")[0][1]).toEqual({ ...DEFAULTS, epicType: "Feature" });
  });

  it("SET-FR-PQWW, FTS-FR-ALOJ: a write sends only the four Types, also when the read held another field", async () => {
    stored = { ...DEFAULTS, deliverableType: "Feature" } as FlowTrackerSettings;
    render(<FlowSyncSettingsGroup />);
    await waitFor(() => expect(select("Defect").options.length).toBe(4));
    await userEvent.selectOptions(select("Defect"), "Task");
    await waitFor(() => expect(calls("set_flow_tracker_settings")).toHaveLength(1));
    expect(calls("set_flow_tracker_settings")[0][1]).toEqual({ ...DEFAULTS, defectType: "Task" });
  });

  it("SET-FR-PQWW: a refused write keeps the previous value and shows the error", async () => {
    setImpl = () => Promise.reject("invalid_flow_tracker_settings");
    render(<FlowSyncSettingsGroup />);
    await waitFor(() => expect(select("Task").options.length).toBe(4));
    await userEvent.selectOptions(select("Task"), "Bug");
    expect(await screen.findByTestId("flow-sync-save-error")).toHaveTextContent(
      "✗ The GitHub operation failed (invalid_flow_tracker_settings).",
    );
    await waitFor(() => expect(select("Task")).toBeEnabled());
    expect(select("Task").value).toBe("Task");
  });

  it("SET-FR-UFXD: a stored-value read that fails shows its error and offers no selector", async () => {
    getImpl = () => Promise.reject("no_project_open");
    render(<FlowSyncSettingsGroup />);
    expect(await screen.findByRole("alert")).toHaveTextContent("✗ No project is open.");
    expect(screen.queryByRole("combobox")).toBeNull();
    expect(screen.queryByText("Loading…")).toBeNull();
  });
});

describe("the Flow sync group in GitHub Project settings (SET-FR-ZJRC, SET-FR-PQWW)", () => {
  /** The Project group of the Settings window, on GitHub Project settings. */
  async function renderSection() {
    render(
      <Settings
        contentRoot="/p#0"
        lineEndings="lf"
        onSelectLineEndings={vi.fn()}
        sectionRequest={{ section: "project/github-polling", nonce: 1 }}
      />,
    );
    const group = await screen.findByTestId("settings-flow-sync");
    await waitFor(() => expect(within(group).getByLabelText("Epic")).toBeInTheDocument());
    return group;
  }

  it("SET-FR-ZJRC: the Flow sync group renders below the Publication group", async () => {
    const group = await renderSection();

    const publication = screen.getByTestId("settings-github-publication");
    expect(publication.compareDocumentPosition(group) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("SET-FR-PQWW, SET-FR-08: a change persists at once and leaves the section dirty state and its Save untouched", async () => {
    const group = await renderSection();
    await waitFor(() => expect((within(group).getByLabelText("Epic") as HTMLSelectElement).options.length).toBe(4));
    const pendingBefore = settingsSectionsPending();
    expect(screen.queryByRole("button", { name: /^Save/ })).toBeNull();

    await userEvent.selectOptions(within(group).getByLabelText("Epic"), "Feature");
    await waitFor(() => expect(calls("set_flow_tracker_settings")).toHaveLength(1));
    await waitFor(() => expect(within(group).getByLabelText("Epic")).toBeEnabled());
    expect(settingsSectionsPending()).toEqual(pendingBefore);
    expect(screen.queryByRole("button", { name: /^Save/ })).toBeNull();
  });
});
