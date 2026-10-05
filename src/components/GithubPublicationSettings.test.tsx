import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { GithubPublicationSettingsGroup } from "./GithubPublicationSettings";
import type {
  GithubPublicationSettings,
  MetadataList,
  PublicationIssueType,
} from "../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const DEFAULTS: GithubPublicationSettings = {
  parentIssueTypes: ["Feature"],
  subIssueType: "Task",
  subIssueMilestonePolicy: "inherit_parent",
};

const types = (...names: string[]): MetadataList<PublicationIssueType> => ({
  state: "loaded",
  items: names.map((name) => ({ name })),
  errorCode: null,
  error: null,
});

let stored: GithubPublicationSettings;
let typeList: () => Promise<MetadataList<PublicationIssueType>>;
let setImpl: (args: GithubPublicationSettings) => Promise<GithubPublicationSettings>;

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

beforeEach(() => {
  stored = { ...DEFAULTS };
  typeList = async () => types("Feature", "Task", "Bug", "Epic");
  setImpl = async (args) => {
    stored = args;
    return stored;
  };
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
    switch (cmd) {
      case "get_github_publication_settings":
        return stored;
      case "list_github_issue_types":
        return typeList();
      case "set_github_publication_settings":
        return setImpl(args as GithubPublicationSettings);
      default:
        return undefined;
    }
  });
});
afterEach(cleanup);

const subType = () => screen.getByLabelText("Sub-issue Type") as HTMLSelectElement;
const parentGroup = () => screen.getByRole("group", { name: "Parent issue Types" });

describe("the Publication group of GitHub Project settings", () => {
  it("SET-FR-XFNA, SET-FR-DWRC, SET-FR-HPKV, SET-FR-ZHCT: reads the stored defaults and the available Types on mount", async () => {
    render(<GithubPublicationSettingsGroup />);

    await waitFor(() => expect(subType().value).toBe("Task"));
    expect(calls("get_github_publication_settings")).toHaveLength(1);
    expect(calls("list_github_issue_types")).toHaveLength(1);
    expect(within(parentGroup()).getByRole("checkbox", { name: "Feature" })).toBeChecked();
    expect(within(parentGroup()).getByRole("checkbox", { name: "Bug" })).not.toBeChecked();
    expect(screen.getByRole("radio", { name: /Inherit parent milestone/ })).toBeChecked();
    expect(Array.from(subType().options).map((o) => o.textContent)).toEqual([
      "Feature",
      "Task",
      "Bug",
      "Epic",
    ]);
  });

  it("SET-FR-ZHCT: the milestone policy is exactly three exclusive choices", async () => {
    render(<GithubPublicationSettingsGroup />);
    await waitFor(() => expect(subType().value).toBe("Task"));

    const group = screen.getByRole("radiogroup", { name: "Sub-issue milestone" });
    expect(
      within(group)
        .getAllByRole("radio")
        .map((r) => (r as HTMLInputElement).value),
    ).toEqual(["inherit_parent", "no_milestone", "author_selected"]);
    expect(within(group).getByText("Inherit parent milestone")).toBeInTheDocument();
    expect(within(group).getByText("No milestone")).toBeInTheDocument();
    expect(within(group).getByText("Author-selectable milestone")).toBeInTheDocument();
  });

  it("SET-FR-MQUE, SET-FR-DWRC: choosing another parent Type persists at once, keeping the others, with no Save control", async () => {
    render(<GithubPublicationSettingsGroup />);
    await waitFor(() => expect(subType().value).toBe("Task"));

    await userEvent.click(within(parentGroup()).getByRole("checkbox", { name: "Epic" }));

    await waitFor(() => expect(calls("set_github_publication_settings")).toHaveLength(1));
    expect(calls("set_github_publication_settings")[0][1]).toEqual({
      parentIssueTypes: ["Feature", "Epic"],
      subIssueType: "Task",
      subIssueMilestonePolicy: "inherit_parent",
    });
    expect(screen.queryByRole("button", { name: /^Save/ })).toBeNull();
    await waitFor(() =>
      expect(within(parentGroup()).getByRole("checkbox", { name: "Epic" })).toBeChecked(),
    );
  });

  it("SET-FR-DWRC: the last selected parent Type cannot be cleared", async () => {
    render(<GithubPublicationSettingsGroup />);
    await waitFor(() => expect(subType().value).toBe("Task"));

    const last = within(parentGroup()).getByRole("checkbox", { name: "Feature" });
    expect(last).toBeDisabled();
    await userEvent.click(last);
    expect(calls("set_github_publication_settings")).toHaveLength(0);
  });

  it("SET-FR-MQUE, SET-FR-HPKV: the sub-issue Type and the milestone policy each persist at once", async () => {
    render(<GithubPublicationSettingsGroup />);
    await waitFor(() => expect(subType().value).toBe("Task"));

    await userEvent.selectOptions(subType(), "Bug");
    await waitFor(() => expect(calls("set_github_publication_settings")).toHaveLength(1));
    expect(calls("set_github_publication_settings")[0][1]).toMatchObject({ subIssueType: "Bug" });

    await userEvent.click(screen.getByRole("radio", { name: /Author-selectable milestone/ }));
    await waitFor(() => expect(calls("set_github_publication_settings")).toHaveLength(2));
    expect(calls("set_github_publication_settings")[1][1]).toEqual({
      parentIssueTypes: ["Feature"],
      subIssueType: "Bug",
      subIssueMilestonePolicy: "author_selected",
    });
  });

  it("SET-FR-MQUE: a refused write keeps the previous values and renders the error inline", async () => {
    setImpl = () => Promise.reject("invalid_publication_settings");
    render(<GithubPublicationSettingsGroup />);
    await waitFor(() => expect(subType().value).toBe("Task"));

    await userEvent.selectOptions(subType(), "Bug");

    expect(await screen.findByTestId("github-publication-save-error")).toHaveTextContent(
      "Keep at least one parent issue Type",
    );
    expect(subType().value).toBe("Task");
  });

  it("SET-FR-LJYT: a stored Type that GitHub does not list is shown as unavailable and is neither replaced nor written again", async () => {
    stored = {
      parentIssueTypes: ["Retired", "Feature"],
      subIssueType: "Gone",
      subIssueMilestonePolicy: "no_milestone",
    };
    render(<GithubPublicationSettingsGroup />);

    await waitFor(() => expect(subType().value).toBe("Gone"));
    expect(subType().selectedOptions[0].textContent).toBe("Gone (unavailable)");
    const retired = within(parentGroup()).getByRole("checkbox", { name: /Retired/ });
    expect(retired).toBeChecked();
    expect(screen.getAllByTestId("github-type-unavailable")).toHaveLength(1);
    expect(screen.getByRole("radio", { name: /No milestone/ })).toBeChecked();
    expect(calls("set_github_publication_settings")).toHaveLength(0);
  });

  it("SET-FR-LJYT, SET-FR-XFNA: a Type list that cannot be read states the error with Retry, keeps every stored value, and writes nothing", async () => {
    typeList = () => Promise.reject("github_unreachable");
    stored = { ...DEFAULTS, parentIssueTypes: ["Retired"], subIssueType: "Gone" };
    render(<GithubPublicationSettingsGroup />);

    expect(await screen.findByTestId("github-types-error")).toHaveTextContent(
      "GitHub could not be reached",
    );
    expect(subType().value).toBe("Gone");
    // The list failed, so nothing is claimed about availability either way.
    expect(screen.queryAllByTestId("github-type-unavailable")).toHaveLength(0);
    expect(within(parentGroup()).getByRole("checkbox", { name: "Retired" })).toBeChecked();
    expect(calls("set_github_publication_settings")).toHaveLength(0);

    typeList = async () => types("Retired", "Gone");
    await userEvent.click(screen.getByRole("button", { name: "Retry" }));
    await waitFor(() => expect(screen.queryByTestId("github-types-error")).toBeNull());
    expect(calls("list_github_issue_types")).toHaveLength(2);
    expect(subType().value).toBe("Gone");
  });

  it("SET-FR-LJYT: a repository refusal reads as an unavailable list with its text", async () => {
    typeList = () => Promise.reject("no_remote_configured");
    render(<GithubPublicationSettingsGroup />);

    expect(await screen.findByTestId("github-types-error")).toHaveTextContent(
      "no remote",
    );
    expect(subType().value).toBe("Task");
  });

  it("SET-FR-LJYT: an empty Type list renders a notice with Retry and marks the stored Types unavailable", async () => {
    typeList = async () => types();
    render(<GithubPublicationSettingsGroup />);

    expect(await screen.findByTestId("github-types-empty")).toHaveTextContent(
      "lists no issue Type",
    );
    expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
    expect(subType().value).toBe("Task");
    expect(screen.getAllByTestId("github-type-unavailable").length).toBeGreaterThan(0);
  });

  it("SET-FR-XFNA: while the Type read is outstanding the controls show a loading state and keep the stored values", async () => {
    let release: (l: MetadataList<PublicationIssueType>) => void = () => {};
    typeList = () =>
      new Promise((resolve) => {
        release = resolve;
      });
    render(<GithubPublicationSettingsGroup />);

    expect(await screen.findByTestId("github-types-loading")).toBeInTheDocument();
    await waitFor(() => expect(subType().value).toBe("Task"));
    expect(screen.getByRole("radio", { name: /Inherit parent milestone/ })).toBeChecked();

    release(types("Feature", "Task"));
    await waitFor(() => expect(screen.queryByTestId("github-types-loading")).toBeNull());
  });
});
