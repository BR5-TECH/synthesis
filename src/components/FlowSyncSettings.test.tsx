/**
 * The Flow sync group of GitHub Project settings
 * (`../../specifications/ui/SET-project-settings.md` SET-FR-ZJRC, SET-FR-UFXD,
 * SET-FR-PQWW).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { FlowSyncSettingsGroup } from "./FlowSyncSettings";
import { resetLogBufferForTest } from "../logging";
import type { FlowTrackerSettings, MetadataList, PublicationIssueType } from "../types";

const DEFAULTS: FlowTrackerSettings = {
  deliverableType: "Feature",
  epicType: "Feature",
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

let stored: FlowTrackerSettings;
let typeList: () => Promise<MetadataList<PublicationIssueType>>;
let setImpl: (args: FlowTrackerSettings) => Promise<FlowTrackerSettings>;
const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);
const select = (label: string) => screen.getByLabelText(label) as HTMLSelectElement;

beforeEach(() => {
  resetLogBufferForTest();
  stored = { ...DEFAULTS };
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
        return Promise.resolve(stored);
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
    await waitFor(() => expect(select("Deliverable").value).toBe("Feature"));
    expect(select("Epic").value).toBe("Feature");
    expect(select("Feature").value).toBe("Feature");
    expect(select("Task").value).toBe("Task");
    expect(select("Defect").value).toBe("Bug");
    await waitFor(() =>
      expect([...select("Task").options].map((o) => o.value)).toEqual(["Task", "Feature", "Bug", "Epic"]),
    );
    expect(screen.queryByLabelText("Milestone")).toBeNull();
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

  it("SET-FR-PQWW: a change persists at once with all five Types", async () => {
    render(<FlowSyncSettingsGroup />);
    await waitFor(() => expect(select("Epic").options.length).toBe(4));
    await userEvent.selectOptions(select("Epic"), "Epic");
    await waitFor(() => expect(select("Epic").value).toBe("Epic"));
    expect(calls("set_flow_tracker_settings")[0][1]).toEqual({ ...DEFAULTS, epicType: "Epic" });
  });

  it("SET-FR-PQWW: a refused write keeps the previous value and shows the error", async () => {
    setImpl = () => Promise.reject("invalid_flow_tracker_settings");
    render(<FlowSyncSettingsGroup />);
    await waitFor(() => expect(select("Task").options.length).toBe(4));
    await userEvent.selectOptions(select("Task"), "Bug");
    await screen.findByTestId("flow-sync-save-error");
    expect(select("Task").value).toBe("Task");
  });
});
