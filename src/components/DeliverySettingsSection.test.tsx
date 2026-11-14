/**
 * The Delivery section of Project settings
 * (`../../specifications/ui/SET-project-settings.md` SET-FR-03, SET-FR-WDYJ,
 * SET-FR-XUKD, SET-FR-HFKK, SET-FR-RUPL, on the terms of SET-FR-XFNA and
 * SET-FR-LJYT).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
const listeners: Record<string, (() => void)[]> = {};
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (name: string, handler: () => void) => {
    (listeners[name] ??= []).push(handler);
    return () => {
      listeners[name] = (listeners[name] ?? []).filter((h) => h !== handler);
    };
  }),
}));

import { DeliverySettingsSection, describeSource, flowFiles, nextReadType } from "./DeliverySettingsSection";
import { deliveryErrorCode, deliveryErrorMessage } from "../state/deliveryErrors";
import { resetLogBufferForTest } from "../logging";
import { resetSettingsSections, settingsSectionsPending } from "../state/settingsSweep";
import { Settings } from "../test/settingsHarness";
import type {
  DeliverySettings,
  DeliverySource,
  MetadataList,
  PublicationIssueType,
  TreeNode,
} from "../types";

/** DSR-FR-ZJMC: what the backend reads for unset values. */
const DEFAULTS: DeliverySettings = {
  source: { kind: "automatic" },
  createTypes: { epic: "Epic", feature: "Feature", task: "Task", defect: "Bug" },
  readEntries: [],
  untyped: null,
};

const GITHUB_SOURCE: DeliverySource = {
  kind: "github",
  project: { nodeId: "PVT_1", title: "Payments", ownerLogin: "acme", number: 7, url: "" },
  repository: { host: "github.com", owner: "acme", name: "payments" },
  configuration: { state: "valid", errorCode: null, error: null, missingStatus: null, projectTitle: "Payments" },
};

const types = (...names: string[]): MetadataList<PublicationIssueType> => ({
  state: "loaded",
  items: names.map((name) => ({ name })),
  errorCode: null,
  error: null,
});

const file = (path: string, artifactType?: TreeNode["artifactType"]): TreeNode => ({
  id: path,
  name: path.split("/").pop()!,
  path,
  nodeKind: "file",
  artifactType,
});
const TREE: TreeNode = {
  id: "",
  name: "p",
  path: "",
  nodeKind: "folder",
  children: [
    {
      id: "flows",
      name: "flows",
      path: "flows",
      nodeKind: "folder",
      children: [file("flows/release.flow.md", "flow"), file("flows/notes.md", "doc")],
    },
    file("roadmap.flow.md", "flow"),
  ],
};

let stored: DeliverySettings;
let source: DeliverySource;
let typeList: () => Promise<MetadataList<PublicationIssueType>>;
let setImpl: (settings: DeliverySettings) => Promise<DeliverySettings>;
const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);
const writes = () => calls("set_delivery_settings").map((c) => (c[1] as { settings: DeliverySettings }).settings);
const select = (label: string) => screen.getByLabelText(label) as HTMLSelectElement;
const radio = (name: string) => screen.getByRole("radio", { name: new RegExp(`^${name}`) }) as HTMLInputElement;

beforeEach(() => {
  resetLogBufferForTest();
  resetSettingsSections();
  for (const k in listeners) delete listeners[k];
  stored = structuredClone(DEFAULTS);
  source = GITHUB_SOURCE;
  typeList = async () => types("Epic", "Feature", "Task", "Bug", "Spike", "Chore");
  setImpl = async (settings) => {
    stored = settings;
    if (settings.source.kind === "file") {
      source = { kind: "file", flowId: settings.source.flowId, path: settings.source.flowId };
    }
    return stored;
  };
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string, args?: unknown) => {
    switch (cmd) {
      case "append_log_records":
        return Promise.resolve();
      case "get_delivery_settings":
        return Promise.resolve(structuredClone(stored));
      case "get_delivery_source":
        return Promise.resolve(source);
      case "set_delivery_settings":
        return setImpl((args as { settings: DeliverySettings }).settings);
      case "list_github_issue_types":
        return typeList();
      case "load_project_tree":
        return Promise.resolve(TREE);
      default:
        return Promise.reject(new Error(`unexpected command ${cmd}`));
    }
  });
});
afterEach(cleanup);

async function renderSection() {
  render(<DeliverySettingsSection />);
  await waitFor(() => expect(select("Epic").value).toBe("Epic"));
}

describe("the Source choice (SET-FR-WDYJ / SET-FR-XUKD)", () => {
  it("SET-FR-WDYJ: offers Automatic, GitHub Project, and Flow file, and reads the stored choice and the resolved source on mount", async () => {
    await renderSection();
    expect(radio("Automatic").checked).toBe(true);
    expect(radio("GitHub Project").checked).toBe(false);
    expect(radio("Flow file").checked).toBe(false);
    expect(await screen.findByTestId("delivery-resolved-source")).toHaveTextContent(
      "The delivery is the GitHub Project “Payments” (acme #7).",
    );
    expect(calls("get_delivery_settings")).toHaveLength(1);
    expect(calls("get_delivery_source")).toHaveLength(1);
    expect(screen.queryByLabelText("Flow file", { selector: "select" })).toBeNull();
  });

  it("SET-FR-WDYJ: states each resolved source in words", () => {
    expect(describeSource({ kind: "file", flowId: "f", path: "flows/release.flow.md" })).toBe(
      "The delivery is the Flow file flows/release.flow.md.",
    );
    expect(describeSource({ kind: "none", reason: "no_source" })).toMatch(/^The project has no delivery\./);
    expect(describeSource({ kind: "none", reason: "github_project_unset" })).toMatch(/no GitHub Project is selected/);
    expect(describeSource({ kind: "none", reason: "delivery_file_missing", flowId: "f" })).toMatch(
      /chosen Flow file is missing/,
    );
  });

  it("SET-FR-WDYJ: the picker lists the project's flow-typed files alone", () => {
    expect(flowFiles(TREE)).toEqual([
      { id: "flows/release.flow.md", path: "flows/release.flow.md" },
      { id: "roadmap.flow.md", path: "roadmap.flow.md" },
    ]);
    expect(flowFiles(null)).toEqual([]);
  });

  it("SET-FR-XUKD: a source change persists at once and the resolved source is read again", async () => {
    await renderSection();
    await userEvent.click(radio("GitHub Project"));
    await waitFor(() => expect(writes()).toHaveLength(1));
    expect(writes()[0]).toEqual({ ...DEFAULTS, source: { kind: "github" } });
    await waitFor(() => expect(radio("GitHub Project").checked).toBe(true));
    await waitFor(() => expect(calls("get_delivery_source")).toHaveLength(2));
  });

  it("SET-FR-WDYJ: Flow file shows the picker and writes nothing until a file is picked", async () => {
    await renderSection();
    await userEvent.click(radio("Flow file"));
    const picker = (await screen.findByLabelText("Flow file", { selector: "select" })) as HTMLSelectElement;
    expect(writes()).toHaveLength(0);
    await waitFor(() =>
      expect([...picker.options].map((o) => o.textContent)).toEqual([
        "Choose a Flow file…",
        "flows/release.flow.md",
        "roadmap.flow.md",
      ]),
    );
    expect(picker.options[0].disabled).toBe(true);
    await userEvent.selectOptions(picker, "roadmap.flow.md");
    await waitFor(() => expect(writes()).toHaveLength(1));
    expect(writes()[0].source).toEqual({ kind: "file", flowId: "roadmap.flow.md" });
    expect(await screen.findByTestId("delivery-resolved-source")).toHaveTextContent(
      "The delivery is the Flow file roadmap.flow.md.",
    );
  });

  it("SET-FR-WDYJ: a stored Flow file that the tree does not hold stays offered and selected", async () => {
    stored = { ...DEFAULTS, source: { kind: "file", flowId: "gone.flow.md" } };
    source = { kind: "none", reason: "delivery_file_missing", flowId: "gone.flow.md" };
    await renderSection();
    expect(radio("Flow file").checked).toBe(true);
    const picker = screen.getByLabelText("Flow file", { selector: "select" }) as HTMLSelectElement;
    await waitFor(() =>
      expect([...picker.options].map((o) => o.value)).toEqual([
        "gone.flow.md",
        "flows/release.flow.md",
        "roadmap.flow.md",
      ]),
    );
    expect(picker.value).toBe("gone.flow.md");
    expect(await screen.findByTestId("delivery-resolved-source")).toHaveTextContent(/missing/);
  });

  it("SET-FR-XUKD: a refused write keeps the previous value and shows the error beside the control", async () => {
    setImpl = () => Promise.reject({ code: "no_project_open", message: "No project is open." });
    await renderSection();
    await userEvent.click(radio("GitHub Project"));
    expect(await screen.findByTestId("delivery-save-error-source")).toHaveTextContent("✗ No project is open.");
    await waitFor(() => expect(radio("Automatic")).toBeEnabled());
    expect(radio("Automatic").checked).toBe(true);
    expect(screen.queryByTestId("delivery-save-error-create")).toBeNull();
  });

  it("SET-FR-WDYJ: a change made elsewhere reads the settings and the source again", async () => {
    await renderSection();
    await waitFor(() => expect((listeners["delivery-source-changed"] ?? []).length).toBe(1));
    stored = { ...DEFAULTS, source: { kind: "github" } };
    listeners["delivery-source-changed"][0]();
    await waitFor(() => expect(radio("GitHub Project").checked).toBe(true));
    expect(calls("get_delivery_source")).toHaveLength(2);
  });
});

describe("the Type mapping group (SET-FR-HFKK / SET-FR-RUPL)", () => {
  it("SET-FR-HFKK: one create Type selector per kind, on the defaults, with the Type list as options", async () => {
    await renderSection();
    expect(select("Feature").value).toBe("Feature");
    expect(select("Task").value).toBe("Task");
    expect(select("Defect").value).toBe("Bug");
    await waitFor(() =>
      expect([...select("Task").options].map((o) => o.value)).toEqual(["Epic", "Feature", "Task", "Bug", "Spike", "Chore"]),
    );
  });

  it("SET-FR-HFKK: a create Type change persists at once", async () => {
    await renderSection();
    await waitFor(() => expect(select("Defect").options.length).toBe(6));
    await userEvent.selectOptions(select("Defect"), "Chore");
    await waitFor(() => expect(writes()).toHaveLength(1));
    expect(writes()[0].createTypes).toEqual({ ...DEFAULTS.createTypes, defect: "Chore" });
    await waitFor(() => expect(select("Defect").value).toBe("Chore"));
  });

  it("SET-FR-HFKK, SET-FR-LJYT: a stored Type the list does not hold is selected, marked unavailable, and not written", async () => {
    stored = { ...DEFAULTS, createTypes: { ...DEFAULTS.createTypes, defect: "Incident" } };
    await renderSection();
    await waitFor(() => expect(select("Defect").selectedOptions[0].textContent).toBe("Incident (unavailable)"));
    expect(select("Defect").value).toBe("Incident");
    expect(writes()).toHaveLength(0);
  });

  it("SET-FR-HFKK, SET-FR-LJYT: a Type list that cannot be read keeps every value, marks nothing, and offers Retry", async () => {
    typeList = async () =>
      ({ state: "failed", items: [], errorCode: null, error: "The issue Types could not be read." }) as MetadataList<PublicationIssueType>;
    stored = { ...DEFAULTS, createTypes: { ...DEFAULTS.createTypes, defect: "Incident" } };
    await renderSection();
    expect(await screen.findByTestId("delivery-types-error")).toHaveTextContent("Your saved choices are kept.");
    expect(select("Defect").selectedOptions[0].textContent).toBe("Incident");
    expect(screen.getByRole("button", { name: "Add" })).toBeDisabled();
    typeList = async () => types("Epic", "Feature", "Task", "Bug");
    await userEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(screen.queryByTestId("delivery-types-error")).toBeNull());
    expect(calls("list_github_issue_types")).toHaveLength(2);
    expect(writes()).toHaveLength(0);
  });

  it("SET-FR-LJYT: an empty Type list renders a notice with Retry", async () => {
    typeList = async () => types();
    await renderSection();
    expect(await screen.findByTestId("delivery-types-empty")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
  });

  it("SET-FR-RUPL: lists the read entries, each with a Type and a kind selector of the four kinds and Leave out", async () => {
    stored = { ...DEFAULTS, readEntries: [{ type: "Spike", kind: "task" }, { type: "Chore", kind: "exclude" }] };
    await renderSection();
    expect(screen.getAllByTestId("delivery-read-entry")).toHaveLength(2);
    expect(select("Type of read entry 1").value).toBe("Spike");
    expect(select("Kind of read entry 2").value).toBe("exclude");
    expect([...select("Kind of read entry 1").options].map((o) => o.textContent)).toEqual([
      "Epic",
      "Feature",
      "Task",
      "Defect",
      "Leave out",
    ]);
  });

  it("SET-FR-RUPL: a kind change and a Remove persist at once", async () => {
    stored = { ...DEFAULTS, readEntries: [{ type: "Spike", kind: "task" }, { type: "Chore", kind: "task" }] };
    await renderSection();
    await userEvent.selectOptions(select("Kind of read entry 2"), "exclude");
    await waitFor(() => expect(writes()).toHaveLength(1));
    expect(writes()[0].readEntries).toEqual([{ type: "Spike", kind: "task" }, { type: "Chore", kind: "exclude" }]);
    await waitFor(() => expect(screen.getByRole("button", { name: "Remove read entry 1" })).toBeEnabled());
    await userEvent.click(screen.getByRole("button", { name: "Remove read entry 1" }));
    await waitFor(() => expect(writes()).toHaveLength(2));
    expect(writes()[1].readEntries).toEqual([{ type: "Chore", kind: "exclude" }]);
  });

  it("SET-FR-RUPL: Add appends the first free Type of the list", async () => {
    await renderSection();
    await waitFor(() => expect(screen.getByRole("button", { name: "Add" })).toBeEnabled());
    await userEvent.click(screen.getByRole("button", { name: "Add" }));
    await waitFor(() => expect(writes()).toHaveLength(1));
    expect(writes()[0].readEntries).toEqual([{ type: "Spike", kind: "task" }]);
  });

  it("SET-FR-RUPL: Add takes no Type that is a create Type or a read entry already", () => {
    const settings = { ...DEFAULTS, readEntries: [{ type: "Spike", kind: "task" as const }] };
    expect(nextReadType(settings, ["Epic", "Spike", "Chore"])).toBe("Chore");
    expect(nextReadType(settings, ["Epic", "Spike"])).toBeNull();
    expect(nextReadType(settings, [])).toBeNull();
  });

  it("SET-FR-RUPL: Add is disabled when every listed Type is mapped", async () => {
    typeList = async () => types("Epic", "Feature", "Task", "Bug");
    await renderSection();
    await waitFor(() => expect(select("Task").options.length).toBe(4));
    expect(screen.getByRole("button", { name: "Add" })).toBeDisabled();
  });

  it("SET-FR-RUPL: Issues with no Type offers Not set, the four kinds, and Leave out, and Not set writes null", async () => {
    stored = { ...DEFAULTS, untyped: "task" };
    await renderSection();
    expect(select("Issues with no Type").value).toBe("task");
    expect([...select("Issues with no Type").options].map((o) => o.textContent)).toEqual([
      "Not set",
      "Epic",
      "Feature",
      "Task",
      "Defect",
      "Leave out",
    ]);
    await userEvent.selectOptions(select("Issues with no Type"), "Not set");
    await waitFor(() => expect(writes()).toHaveLength(1));
    expect(writes()[0].untyped).toBeNull();
  });

  it("SET-FR-XUKD: a refused Type mapping write shows its error beside that control and keeps the value", async () => {
    setImpl = () => Promise.reject("invalid_delivery_settings");
    await renderSection();
    await waitFor(() => expect(select("Epic").options.length).toBe(6));
    await userEvent.selectOptions(select("Epic"), "Feature");
    expect(await screen.findByTestId("delivery-save-error-create")).toHaveTextContent(
      "✗ Each Type name must not be empty, and one Type can map to one kind only.",
    );
    await waitFor(() => expect(select("Epic")).toBeEnabled());
    expect(select("Epic").value).toBe("Epic");
    expect(screen.queryByTestId("delivery-save-error-source")).toBeNull();
  });
});

describe("reads that fail or wait (SET-FR-WDYJ / SET-FR-XFNA / SET-FR-XUKD)", () => {
  it("SET-FR-WDYJ: a refused settings read shows the error, offers no control, and writes nothing", async () => {
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "get_delivery_settings"
        ? Promise.reject("no_project_open")
        : cmd === "append_log_records" || cmd === "load_project_tree"
          ? Promise.resolve(cmd === "load_project_tree" ? TREE : undefined)
          : cmd === "list_github_issue_types"
            ? typeList()
            : cmd === "get_delivery_source"
              ? Promise.resolve(source)
              : Promise.reject(new Error(`unexpected command ${cmd}`)),
    );
    render(<DeliverySettingsSection />);
    expect(await screen.findByRole("alert")).toHaveTextContent("✗ No project is open.");
    expect(screen.queryByRole("radio")).toBeNull();
    expect(screen.queryByRole("combobox")).toBeNull();
    expect(writes()).toHaveLength(0);
  });

  it("SET-FR-XFNA: while the Type read is outstanding, a loading state shows and the stored values stay", async () => {
    let answerTypes!: (list: MetadataList<PublicationIssueType>) => void;
    typeList = () => new Promise((res) => (answerTypes = res));
    stored = { ...DEFAULTS, createTypes: { ...DEFAULTS.createTypes, task: "Story" } };
    await renderSection();
    expect(screen.getByTestId("delivery-types-loading")).toBeInTheDocument();
    expect(select("Task").value).toBe("Story");
    answerTypes(types("Epic", "Feature", "Bug"));
    await waitFor(() => expect(screen.queryByTestId("delivery-types-loading")).toBeNull());
    expect(select("Task").selectedOptions[0].textContent).toBe("Story (unavailable)");
  });

  it("SET-FR-LJYT: a Type read that is refused shows the notice with Retry", async () => {
    typeList = () => Promise.reject("github_unreachable");
    await renderSection();
    expect(await screen.findByTestId("delivery-types-error")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
  });

  it("SET-FR-XUKD: the controls are disabled during a write, and a later write that succeeds clears the error", async () => {
    let settle!: (s: DeliverySettings) => void;
    let refuse = true;
    setImpl = (settings) => {
      if (refuse) return Promise.reject("invalid_delivery_settings");
      return new Promise((res) => (settle = () => res(settings)));
    };
    await renderSection();
    await userEvent.click(radio("GitHub Project"));
    expect(await screen.findByTestId("delivery-save-error-source")).toBeInTheDocument();
    refuse = false;
    await waitFor(() => expect(radio("GitHub Project")).toBeEnabled());
    await userEvent.click(radio("GitHub Project"));
    await waitFor(() => expect(radio("Automatic")).toBeDisabled());
    expect(select("Epic")).toBeDisabled();
    expect(screen.queryByTestId("delivery-save-error-source")).toBeNull();
    settle(stored);
    await waitFor(() => expect(radio("GitHub Project").checked).toBe(true));
    expect(writes()).toHaveLength(2);
  });
});

describe("the refusal text", () => {
  it("SET-FR-XUKD: a typed error shows its message, a bare code its text, and the log gets the code alone", () => {
    expect(deliveryErrorMessage({ code: "not_a_flow", message: "That file is not a Flow." })).toBe(
      "That file is not a Flow.",
    );
    expect(deliveryErrorMessage("not_a_flow")).toBe("The chosen file is not a Flow file of this project.");
    expect(deliveryErrorMessage("strange_code")).toBe("The delivery operation failed (strange_code).");
    expect(deliveryErrorCode({ code: "not_a_flow", message: "x" })).toBe("not_a_flow");
    expect(deliveryErrorCode(new Error("free text with a name"))).toBe("unknown");
  });
});

describe("the Delivery section in the Settings window (SET-FR-03, SET-FR-08)", () => {
  it("SET-FR-03, SET-FR-XUKD: the section is reached as project/delivery, and a change leaves the dirty state and Save untouched", async () => {
    render(
      <Settings
        contentRoot="/p#0"
        lineEndings="lf"
        onSelectLineEndings={vi.fn()}
        sectionRequest={{ section: "project/delivery", nonce: 1 }}
      />,
    );
    const section = await screen.findByTestId("settings-delivery");
    await waitFor(() => expect(within(section).getByLabelText("Epic")).toBeInTheDocument());
    const pendingBefore = settingsSectionsPending();
    expect(screen.queryByRole("button", { name: /^Save/ })).toBeNull();
    await userEvent.click(within(section).getByRole("radio", { name: /^GitHub Project/ }));
    await waitFor(() => expect(writes()).toHaveLength(1));
    expect(settingsSectionsPending()).toEqual(pendingBefore);
    expect(screen.queryByRole("button", { name: /^Save/ })).toBeNull();
  });
});
