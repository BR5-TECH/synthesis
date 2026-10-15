/**
 * The publication chooser: root and sub-issue publication, the Type and
 * milestone controls, the states of the three lists, cancellation, retry that
 * keeps the saved choice, and the recovery choice
 * (`../../specifications/ui/NAW-new-artifact.md` NAW-FR-HNVR … NAW-FR-QEZG,
 * served by `../../specifications/core/GHP-github-publication.md`).
 *
 * Driven through the real tab, like its sibling `NewArtifactWorkspace.publication`,
 * because what the requirements state is what the author reaches.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { act, cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
const listeners = new Map<string, Set<(e: { payload: unknown }) => void>>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    async (event: string, cb: (e: { payload: unknown }) => void) => {
      const set = listeners.get(event) ?? new Set();
      set.add(cb);
      listeners.set(event, set);
      return () => set.delete(cb);
    },
  ),
}));

import {
  chooseParent,
  failedList,
  fourRemotes,
  loadedList,
  makeStubs,
  noPublication,
  openActions,
  publicationChooser,
  publicationMetadata,
  publicationRecord,
  renderWorkspace,
} from "../test/newArtifactFixtures";
import { resetDraftDiscussions } from "../state/draftDiscussion";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { resetDiscussionFocus } from "../state/discussionFocus";
import type {
  DraftPublicationView,
  PublicationChoice,
  PublicationMetadata,
  PublicationOutcome,
} from "../types";

const { stub } = makeStubs(invokeMock);

const calls = (name: string) =>
  invokeMock.mock.calls.filter(([cmd]) => cmd === name);

async function publishEntry() {
  const menu = await openActions();
  return within(menu).getByRole("menuitem", { name: /Publish to GitHub/ });
}

/** Open the chooser from the action control, for a project with one remote. */
async function openChooser() {
  renderWorkspace();
  await userEvent.click(await publishEntry());
  return publicationChooser();
}

/** Wait for the chooser's read to land: no list states that it loads. */
async function loaded(dialog: HTMLElement) {
  await waitFor(() => expect(dialog).not.toHaveTextContent(/Loading/));
}

/** NAW-FR-ASYN: select the root option, which a loaded parent list does not. */
async function selectRoot(dialog: HTMLElement) {
  await userEvent.click(within(dialog).getByRole("radio", { name: /Publish as a root issue/ }));
}

const SUB_ISSUE_CHOICE: PublicationChoice = {
  kind: "sub_issue",
  parentRepositoryOwner: "acme",
  parentRepositoryName: "widgets",
  parentIssueNumber: 412,
  issueType: "Task",
  milestonePolicy: "inherit_parent",
  milestoneNumber: 7,
  milestoneTitle: "v1.2",
};

function attempt(
  state: "open" | "awaiting_choice" = "open",
  choice?: PublicationChoice,
) {
  return {
    marker: "pub-1",
    remoteName: "origin",
    remoteUrl: "github.com/acme/widgets",
    repositoryOwner: "acme",
    repositoryName: "widgets",
    state,
    startedAt: "2026-09-12T10:00:00.000Z",
    updatedAt: "2026-09-12T10:00:00.000Z",
    ...(choice ? { choice } : {}),
  } as const;
}

function withMetadata(overrides: Partial<PublicationMetadata>) {
  stub({ metadata: publicationMetadata(overrides) });
}

beforeEach(() => {
  invokeMock.mockReset();
  listeners.clear();
  resetDraftDiscussions();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  stub();
});

afterEach(cleanup);

describe("Publication chooser — opening and root publication", () => {
  it("NAW-FR-HNVR, NAW-FR-TCQB, NAW-FR-YLKD, NAW-FR-RBTE: the chooser reads the metadata once, lists the sub-issue option first and selects it, and lists each parent with its facts", async () => {
    const dialog = await openChooser();
    await loaded(dialog);

    expect(calls("load_publication_metadata")).toHaveLength(1);
    expect(calls("publish_draft_to_github")).toHaveLength(0);
    expect(dialog).toHaveTextContent("acme/widgets");
    const radios = within(dialog).getAllByRole("radio");
    expect(radios.map((r) => (r as HTMLInputElement).value)).toEqual(["sub", "root"]);
    expect(radios[0]).toHaveAccessibleName("Publish as a sub-issue of");
    expect(radios[0]).toBeChecked();
    expect(radios[1]).toHaveAccessibleName("Publish as a root issue");
    expect(radios[1]).toBeEnabled();
    // NAW-FR-RBTE: the sub-issue option with no parent chosen cannot publish.
    expect(within(dialog).getByRole("button", { name: "Publish" })).toBeDisabled();
    // NAW-FR-YLKD: one combobox in place of a row for each parent.
    const combobox = within(dialog).getByRole("combobox", { name: "Parent issue" });
    expect(combobox).toHaveTextContent("Choose a parent issue");
    await userEvent.click(combobox);
    const list = within(dialog).getByRole("listbox", { name: "Parent issue" });
    const options = within(list).getAllByRole("option");
    expect(options).toHaveLength(2);
    expect(options[0]).toHaveTextContent("#412 Window chrome");
    expect(options[0]).toHaveTextContent("Feature · v1.2");
    expect(options[1]).toHaveTextContent("#398 Release pipeline");
    expect(options[1]).toHaveTextContent("Feature · No milestone");
    expect(dialog).not.toHaveTextContent("https://github.com/acme/widgets/issues/412");
  });

  it("NAW-FR-ZOAS, NAW-FR-FGUI, NAW-FR-RBTE: the root option sends the selected Type and milestone", async () => {
    const dialog = await openChooser();
    await loaded(dialog);
    await selectRoot(dialog);

    await userEvent.selectOptions(within(dialog).getByLabelText("Type"), "Bug");
    await userEvent.selectOptions(within(dialog).getByLabelText("Milestone"), "v1.3");
    await userEvent.click(within(dialog).getByRole("button", { name: "Publish" }));

    await waitFor(() => expect(calls("publish_draft_to_github")).toHaveLength(1));
    expect(calls("publish_draft_to_github")[0][1]).toEqual({
      draftId: "d1",
      remoteName: "origin",
      persistRemote: false,
      publicationChoice: { parentIssueNumber: null, issueType: "Bug", milestoneNumber: 8 },
    });
  });

  it("NAW-FR-ZOAS, NAW-FR-FGUI: the Type and milestone controls offer a way to publish without either", async () => {
    const dialog = await openChooser();
    await loaded(dialog);
    await selectRoot(dialog);

    expect(within(dialog).getByRole("option", { name: "No Type" })).toBeInTheDocument();
    expect(within(dialog).getByRole("option", { name: "No milestone" })).toBeInTheDocument();
    expect(within(dialog).getByLabelText("Type")).toHaveValue("");
    expect(within(dialog).getByLabelText("Milestone")).toHaveValue("");
  });

  it("NAW-FR-ZOAS, SET-FR-ZNLF: the Type control opens on the stored root issue Type, and the author can change it", async () => {
    withMetadata({ rootIssueType: { name: "bug", resolved: "Bug" } });
    const dialog = await openChooser();
    await loaded(dialog);
    await selectRoot(dialog);

    expect(within(dialog).getByLabelText("Type")).toHaveValue("Bug");
    await userEvent.click(within(dialog).getByRole("button", { name: "Publish" }));
    await waitFor(() => expect(calls("publish_draft_to_github")).toHaveLength(1));
    expect(calls("publish_draft_to_github")[0][1]).toMatchObject({
      publicationChoice: { parentIssueNumber: null, issueType: "Bug" },
    });
  });

  it("NAW-FR-ZOAS: the author changes the pre-filled Type for one publication", async () => {
    withMetadata({ rootIssueType: { name: "Bug", resolved: "Bug" } });
    const dialog = await openChooser();
    await loaded(dialog);
    await selectRoot(dialog);

    await userEvent.selectOptions(within(dialog).getByLabelText("Type"), "No Type");
    await userEvent.click(within(dialog).getByRole("button", { name: "Publish" }));
    await waitFor(() => expect(calls("publish_draft_to_github")).toHaveLength(1));
    expect(calls("publish_draft_to_github")[0][1]).toMatchObject({
      publicationChoice: { parentIssueNumber: null, issueType: null },
    });
  });

  it("NAW-FR-ZOAS, GHP-FR-AFEX: a stored root issue Type the Type list does not hold pre-fills No Type and publishes without a Type", async () => {
    withMetadata({ rootIssueType: { name: "Retired", resolved: null } });
    const dialog = await openChooser();
    await loaded(dialog);
    await selectRoot(dialog);

    expect(within(dialog).getByLabelText("Type")).toHaveValue("");
    await userEvent.click(within(dialog).getByRole("button", { name: "Publish" }));
    await waitFor(() => expect(calls("publish_draft_to_github")).toHaveLength(1));
    expect(calls("publish_draft_to_github")[0][1]).toMatchObject({
      publicationChoice: { parentIssueNumber: null, issueType: null },
    });
  });

  it("NAW-FR-ASYN: a root option the author selects during the read stays selected when the parents load", async () => {
    let release: (m: PublicationMetadata) => void = () => {};
    stub({
      metadata: () =>
        new Promise<PublicationMetadata>((resolve) => {
          release = resolve;
        }),
    });
    const dialog = await openChooser();
    const sub = within(dialog).getByRole("radio", { name: /Publish as a sub-issue of/ });
    const root = within(dialog).getByRole("radio", { name: /Publish as a root issue/ });
    expect(sub).toBeDisabled();
    expect(root).toBeChecked();

    await userEvent.click(root);
    await act(async () => release(publicationMetadata()));
    await loaded(dialog);
    expect(sub).toBeEnabled();
    expect(root).toBeChecked();
    expect(within(dialog).getByRole("button", { name: "Publish" })).toBeEnabled();
  });

  it("NAW-FR-RBTE: Publish is disabled while the read is outstanding, stays disabled for the sub-issue option with no parent, and enables for the root option", async () => {
    let release: (m: PublicationMetadata) => void = () => {};
    stub({
      metadata: () =>
        new Promise<PublicationMetadata>((resolve) => {
          release = resolve;
        }),
    });
    const dialog = await openChooser();

    expect(within(dialog).getByRole("button", { name: "Publish" })).toBeDisabled();
    expect(within(dialog).getByText("Loading parent issues…")).toBeInTheDocument();
    expect(within(dialog).getByRole("radio", { name: /Publish as a root issue/ })).toBeEnabled();

    await act(async () => release(publicationMetadata()));
    await loaded(dialog);
    expect(within(dialog).queryByText("Loading parent issues…")).toBeNull();
    expect(within(dialog).getByRole("button", { name: "Publish" })).toBeDisabled();
    await selectRoot(dialog);
    expect(within(dialog).getByRole("button", { name: "Publish" })).toBeEnabled();
  });

  it("NAW-FR-RBTE, NAW-FR-JBHV: confirming closes the chooser while the held call runs, and the call settles into the tag", async () => {
    let release: (value: PublicationOutcome) => void = () => {};
    const held = new Promise<PublicationOutcome>((resolve) => {
      release = resolve;
    });
    const base = invokeMock.getMockImplementation() as (c: string, a?: unknown) => Promise<unknown>;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "publish_draft_to_github") return held;
      return base(cmd, args);
    });
    const dialog = await openChooser();
    await loaded(dialog);
    await selectRoot(dialog);
    await userEvent.click(within(dialog).getByRole("button", { name: "Publish" }));
    await waitFor(() => expect(calls("publish_draft_to_github")).toHaveLength(1));

    await act(async () =>
      release({ kind: "published", record: publicationRecord(418, "pub-1") }),
    );
    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Publish to GitHub" })).toBeNull());
  });

  it("NAW-FR-JBHV: two activations of Publish are one attempt", async () => {
    const dialog = await openChooser();
    await loaded(dialog);
    await selectRoot(dialog);
    const publish = within(dialog).getByRole("button", { name: "Publish" });

    await act(async () => {
      publish.click();
      publish.click();
    });

    await waitFor(() => expect(calls("publish_draft_to_github")).toHaveLength(1));
  });
});

describe("Publication chooser — sub-issue publication", () => {
  it("NAW-FR-JXDN, NAW-FR-RBTE: a parent row shows the configured Type and the inherited milestone, and sends only the parent", async () => {
    const dialog = await openChooser();
    await loaded(dialog);

    await chooseParent(dialog, 412);
    expect(dialog).toHaveTextContent("Type: Task (set in Project settings)");
    expect(dialog).toHaveTextContent("Milestone: inherited from the parent, v1.2");
    // The author cannot override the configured Type.
    expect(within(dialog).queryByLabelText("Type")).toBeNull();
    expect(within(dialog).queryByLabelText("Milestone")).toBeNull();

    await userEvent.click(within(dialog).getByRole("button", { name: "Publish" }));
    await waitFor(() => expect(calls("publish_draft_to_github")).toHaveLength(1));
    expect(calls("publish_draft_to_github")[0][1]).toMatchObject({
      publicationChoice: { parentIssueNumber: 412, issueType: null, milestoneNumber: null },
    });
  });

  it("NAW-FR-JXDN: an inheriting policy over a parent without a milestone says the issue has none", async () => {
    const dialog = await openChooser();
    await loaded(dialog);

    await chooseParent(dialog, 398);
    expect(dialog).toHaveTextContent("The parent has no milestone, so the issue publishes without one.");
  });

  it("NAW-FR-JXDN: the no-milestone policy states it and offers no milestone control", async () => {
    withMetadata({
      settings: {
        parentIssueTypes: ["Feature"],
        subIssueType: "Task",
        subIssueMilestonePolicy: "no_milestone",
        rootIssueType: null,
        noteIssueType: "Feature",
      },
    });
    const dialog = await openChooser();
    await loaded(dialog);

    await chooseParent(dialog, 412);
    expect(dialog).toHaveTextContent("Milestone: none (set in Project settings)");
    expect(within(dialog).queryByLabelText("Milestone")).toBeNull();
    await userEvent.click(within(dialog).getByRole("button", { name: "Publish" }));
    await waitFor(() => expect(calls("publish_draft_to_github")).toHaveLength(1));
    expect(calls("publish_draft_to_github")[0][1]).toMatchObject({
      publicationChoice: { parentIssueNumber: 412, milestoneNumber: null },
    });
  });

  it("NAW-FR-JXDN, NAW-FR-FGUI, NAW-FR-RBTE: the author-selectable policy offers the open milestones and sends the one chosen", async () => {
    withMetadata({
      settings: {
        parentIssueTypes: ["Feature"],
        subIssueType: "Task",
        subIssueMilestonePolicy: "author_selected",
        rootIssueType: null,
        noteIssueType: "Feature",
      },
    });
    const dialog = await openChooser();
    await loaded(dialog);

    await chooseParent(dialog, 412);
    await userEvent.selectOptions(within(dialog).getByLabelText("Milestone"), "v1.3");
    await userEvent.click(within(dialog).getByRole("button", { name: "Publish" }));

    await waitFor(() => expect(calls("publish_draft_to_github")).toHaveLength(1));
    expect(calls("publish_draft_to_github")[0][1]).toMatchObject({
      publicationChoice: { parentIssueNumber: 412, issueType: null, milestoneNumber: 8 },
    });
  });

  it("NAW-FR-JXDN: a configured Type the repository does not offer is stated and the issue publishes without it", async () => {
    withMetadata({ subIssueType: { name: "Chore", resolved: null } });
    const dialog = await openChooser();
    await loaded(dialog);

    await chooseParent(dialog, 412);
    expect(dialog).toHaveTextContent("The configured Type “Chore” is unavailable in this repository.");
    expect(dialog).toHaveTextContent("publishes without a Type");
    expect(within(dialog).getByRole("button", { name: "Publish" })).toBeEnabled();
  });

  it("NAW-FR-TCQB: selecting the root option again drops the sub-issue controls", async () => {
    const dialog = await openChooser();
    await loaded(dialog);

    await chooseParent(dialog, 412);
    await userEvent.click(within(dialog).getByRole("radio", { name: /Publish as a root issue/ }));
    expect(dialog).not.toHaveTextContent("set in Project settings");
    expect(within(dialog).getByLabelText("Type")).toBeInTheDocument();
  });
});

describe("Publication chooser — loading, empty, unavailable, and failed states", () => {
  it("NAW-FR-WMEP, NAW-FR-TCQB: a parent list that cannot be read is stated and root publication still goes through", async () => {
    withMetadata({
      parents: failedList("parent_issues_unreadable", "The open issues of this repository could not be read."),
    });
    const dialog = await openChooser();
    await loaded(dialog);

    // NAW-FR-WMEP / NAW-FR-ASYN: the notice stands in place of the combobox.
    expect(within(dialog).getByRole("radio", { name: "Publish as a sub-issue of" })).toBeDisabled();
    expect(within(dialog).queryByRole("combobox", { name: "Parent issue" })).toBeNull();
    expect(dialog).toHaveTextContent("Parent issues are unavailable: The open issues of this repository could not be read.");
    expect(dialog).toHaveTextContent("Root publication is still available.");
    // The Type and milestone controls are not taken down with the parents.
    expect(within(dialog).getByLabelText("Type")).toBeInTheDocument();

    await userEvent.click(within(dialog).getByRole("button", { name: "Publish" }));
    await waitFor(() => expect(calls("publish_draft_to_github")).toHaveLength(1));
    expect(calls("publish_draft_to_github")[0][1]).toMatchObject({
      publicationChoice: { parentIssueNumber: null },
    });
  });

  it("NAW-FR-WMEP, NAW-FR-ASYN: an empty parent list names the configured Types and selects root", async () => {
    withMetadata({ parents: loadedList([]) });
    const dialog = await openChooser();
    await loaded(dialog);

    expect(within(dialog).getByRole("radio", { name: "Publish as a sub-issue of" })).toBeDisabled();
    expect(within(dialog).queryByRole("combobox", { name: "Parent issue" })).toBeNull();
    expect(dialog).toHaveTextContent("No open issue of Feature was found.");
    expect(within(dialog).getByRole("radio", { name: /Publish as a root issue/ })).toBeEnabled();
    expect(within(dialog).getByRole("radio", { name: /Publish as a root issue/ })).toBeChecked();
    expect(within(dialog).getByRole("button", { name: "Publish" })).toBeEnabled();
  });

  it("NAW-FR-HQRY: the chooser renders no label list and sends no label, though the metadata holds one", async () => {
    const dialog = await openChooser();
    await loaded(dialog);
    expect(within(dialog).queryByText("Labels")).toBeNull();
    expect(within(dialog).queryByRole("group", { name: "Labels" })).toBeNull();
    expect(within(dialog).queryByText("bug")).toBeNull();
    await userEvent.click(within(dialog).getByRole("radio", { name: /Publish as a root issue/ }));
    await userEvent.click(within(dialog).getByRole("button", { name: "Publish" }));
    await waitFor(() => expect(calls("publish_draft_to_github")).toHaveLength(1));
    expect(
      (calls("publish_draft_to_github")[0][1] as { publicationChoice: object }).publicationChoice,
    ).not.toHaveProperty("labels");
  });

  it("NAW-FR-WMEP, NAW-FR-ZOAS: an empty Type list renders a notice instead of an empty selector", async () => {
    withMetadata({ issueTypes: loadedList([]) });
    const dialog = await openChooser();
    await loaded(dialog);
    await selectRoot(dialog);

    expect(within(dialog).queryByLabelText("Type")).toBeNull();
    expect(dialog).toHaveTextContent("No issue Type is available. The issue publishes without a Type.");
    await userEvent.click(within(dialog).getByRole("button", { name: "Publish" }));
    await waitFor(() => expect(calls("publish_draft_to_github")).toHaveLength(1));
    expect(calls("publish_draft_to_github")[0][1]).toMatchObject({
      publicationChoice: { issueType: null },
    });
  });

  it("NAW-FR-WMEP, NAW-FR-ZOAS: a failed Type list is stated with its error and blocks nothing", async () => {
    withMetadata({
      issueTypes: failedList("issue_types_unreadable", "The issue Types of this repository could not be read."),
    });
    const dialog = await openChooser();
    await loaded(dialog);
    await selectRoot(dialog);

    expect(within(dialog).queryByLabelText("Type")).toBeNull();
    expect(dialog).toHaveTextContent("Issue Types are unavailable: The issue Types of this repository could not be read.");
    expect(within(dialog).getByRole("button", { name: "Publish" })).toBeEnabled();
  });

  it("NAW-FR-WMEP, NAW-FR-FGUI: an empty and a failed milestone list each render a notice and no selector", async () => {
    withMetadata({ milestones: loadedList([]) });
    let dialog = await openChooser();
    await loaded(dialog);
    await selectRoot(dialog);
    expect(within(dialog).queryByLabelText("Milestone")).toBeNull();
    expect(dialog).toHaveTextContent("There is no open milestone. The issue publishes without a milestone.");
    cleanup();

    invokeMock.mockReset();
    stub({
      metadata: publicationMetadata({
        milestones: failedList("milestones_unreadable", "The milestones of this repository could not be read."),
      }),
    });
    dialog = await openChooser();
    await loaded(dialog);
    await selectRoot(dialog);
    expect(within(dialog).queryByLabelText("Milestone")).toBeNull();
    expect(dialog).toHaveTextContent("Milestones are unavailable: The milestones of this repository could not be read.");
  });

  it("NAW-FR-WMEP, NAW-FR-TCQB: a read that fails outright leaves every list unavailable and root publication enabled", async () => {
    stub({
      metadata: async () => {
        throw new Error("token_unavailable");
      },
    });
    const dialog = await openChooser();
    await waitFor(() =>
      expect(dialog).toHaveTextContent("Parent issues are unavailable"),
    );
    // The refusal reads as a sentence rather than as its bare code.
    expect(dialog).toHaveTextContent("No GitHub token is available for this project.");

    expect(dialog).toHaveTextContent("Issue Types are unavailable");
    expect(dialog).toHaveTextContent("Milestones are unavailable");
    expect(within(dialog).getByRole("button", { name: "Publish" })).toBeEnabled();
    await userEvent.click(within(dialog).getByRole("button", { name: "Publish" }));
    await waitFor(() => expect(calls("publish_draft_to_github")).toHaveLength(1));
    expect(calls("publish_draft_to_github")[0][1]).toMatchObject({
      publicationChoice: { parentIssueNumber: null, issueType: null, milestoneNumber: null },
    });
  });
});

describe("Publication chooser — cancellation, focus, and accessibility", () => {
  it("NAW-FR-VUCK: Cancel invokes nothing, starts no attempt, and returns focus to the action control", async () => {
    const dialog = await openChooser();
    await loaded(dialog);

    await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));

    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Publish to GitHub" })).toBeNull());
    expect(calls("publish_draft_to_github")).toHaveLength(0);
    expect(calls("retry_draft_publication")).toHaveLength(0);
    expect(screen.getByRole("button", { name: "Draft actions" })).toHaveFocus();
  });

  it("NAW-FR-VUCK: Escape dismisses the chooser on the same terms", async () => {
    const dialog = await openChooser();
    await loaded(dialog);

    await userEvent.keyboard("{Escape}");

    await waitFor(() => expect(screen.queryByRole("dialog", { name: "Publish to GitHub" })).toBeNull());
    expect(calls("publish_draft_to_github")).toHaveLength(0);
    expect(dialog.isConnected).toBe(false);
  });

  it("NAW-FR-VUCK: a read that lands after the chooser was dismissed does not reopen it", async () => {
    let release: (m: PublicationMetadata) => void = () => {};
    stub({
      metadata: () =>
        new Promise<PublicationMetadata>((resolve) => {
          release = resolve;
        }),
    });
    const dialog = await openChooser();
    await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await act(async () => release(publicationMetadata()));

    expect(screen.queryByRole("dialog", { name: "Publish to GitHub" })).toBeNull();
    expect(calls("publish_draft_to_github")).toHaveLength(0);
  });

  it("NAW-FR-PSMO: the chooser is a named modal dialog, focus starts on the first enabled option, and its choices form one labelled radio group", async () => {
    // The read is held, so the chooser opens with the sub-issue option
    // disabled and the root option is the first enabled option.
    let release: (m: PublicationMetadata) => void = () => {};
    stub({
      metadata: () =>
        new Promise<PublicationMetadata>((resolve) => {
          release = resolve;
        }),
    });
    const dialog = await openChooser();

    expect(dialog).toHaveAttribute("aria-modal", "true");
    expect(dialog).toHaveAccessibleName("Publish to GitHub");
    expect(within(dialog).getByRole("radiogroup", { name: "Publish as" })).toBeInTheDocument();
    expect(within(dialog).getByRole("radio", { name: /Publish as a sub-issue of/ })).toBeDisabled();
    expect(within(dialog).getByRole("radio", { name: /Publish as a root issue/ })).toHaveFocus();
    await act(async () => release(publicationMetadata()));
    await loaded(dialog);
  });

  it("NAW-FR-PSMO: Tab stays inside the chooser, and the radio group moves by arrow key", async () => {
    const dialog = await openChooser();
    await loaded(dialog);

    const publish = within(dialog).getByRole("button", { name: "Publish" });
    publish.focus();
    await userEvent.tab();
    expect(dialog.contains(document.activeElement)).toBe(true);
    await userEvent.tab({ shift: true });
    expect(dialog.contains(document.activeElement)).toBe(true);

    // The sub-issue option is checked when the parents load (NAW-FR-ASYN),
    // so the arrow key must move the selection to the root option.
    const sub = within(dialog).getByRole("radio", { name: "Publish as a sub-issue of" });
    expect(sub).toBeChecked();
    sub.focus();
    await userEvent.keyboard("{ArrowDown}");
    expect(within(dialog).getByRole("radio", { name: /Publish as a root issue/ })).toBeChecked();
  });

  it("NAW-FR-PSMO: each notice is announced as a status", async () => {
    withMetadata({ parents: loadedList([]), milestones: loadedList([]) });
    const dialog = await openChooser();
    await loaded(dialog);

    const statuses = within(dialog).getAllByRole("status").map((n) => n.textContent);
    expect(statuses.some((t) => t?.includes("No open issue of Feature"))).toBe(true);
    expect(statuses.some((t) => t?.includes("There is no open milestone"))).toBe(true);
  });
});

describe("Publication chooser — remotes", () => {
  it("NAW-FR-DWKA, NAW-FR-JBHV: with several remotes the picker comes first and the chooser reads the chosen remote", async () => {
    stub({ remotes: fourRemotes() });
    renderWorkspace();
    await userEvent.click(await publishEntry());
    const picker = await screen.findByRole("dialog", { name: "Publish to GitHub" });
    expect(calls("load_publication_metadata")).toHaveLength(0);

    await userEvent.click(within(picker).getByRole("button", { name: "Publish" }));
    const dialog = await publicationChooser();
    await loaded(dialog);

    expect(calls("load_publication_metadata")[0][1]).toEqual({ draftId: "d1", remoteName: "origin" });
    expect(calls("publish_draft_to_github")).toHaveLength(0);
  });
});

describe("Publication chooser — retry, failure, and recovery keep the saved choice", () => {
  /** A draft whose attempt stands with `choice`; `outcomes` answers each call. */
  function standing(
    state: "open" | "awaiting_choice",
    choice: PublicationChoice | undefined,
    answers: Record<string, () => unknown> = {},
  ) {
    const view: DraftPublicationView = {
      ...noPublication(),
      attempt: attempt(state, choice),
      eligibility: { ...noPublication().eligibility, publishable: false, reasonCode: "attempt_in_progress", reason: "A publication attempt for this draft is already in progress." },
    };
    stub({ publication: view });
    const base = invokeMock.getMockImplementation() as (c: string, a?: unknown) => Promise<unknown>;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (answers[cmd]) return answers[cmd]();
      return base(cmd, args);
    });
  }

  it("NAW-FR-NHAY: the band states the saved choice in words", async () => {
    standing("open", SUB_ISSUE_CHOICE);
    renderWorkspace();

    const band = await screen.findByTestId("draft-publication-band");
    expect(within(band).getByTestId("draft-publication-choice")).toHaveTextContent(
      "Sub-issue of #412 in acme/widgets · Type Task · Milestone v1.2.",
    );
  });

  it("NAW-FR-NHAY: an attempt that holds no choice reads as a root issue", async () => {
    standing("open", undefined);
    renderWorkspace();

    const band = await screen.findByTestId("draft-publication-band");
    expect(within(band).getByTestId("draft-publication-choice")).toHaveTextContent("Root issue.");
  });

  it("NAW-FR-LQAF, NAW-FR-NHAY: Retry invokes the retry alone and never reopens the chooser or reads the metadata", async () => {
    standing("open", SUB_ISSUE_CHOICE, {
      retry_draft_publication: () =>
        ({ kind: "published", record: publicationRecord(418, "pub-1") }) satisfies PublicationOutcome,
    });
    renderWorkspace();
    const band = await screen.findByTestId("draft-publication-band");

    await userEvent.click(within(band).getByRole("button", { name: "Retry" }));

    await waitFor(() => expect(calls("retry_draft_publication")).toHaveLength(1));
    expect(calls("retry_draft_publication")[0][1]).toEqual({ draftId: "d1" });
    expect(calls("load_publication_metadata")).toHaveLength(0);
    expect(calls("publish_draft_to_github")).toHaveLength(0);
    expect(screen.queryByRole("radio", { name: /Publish as a root issue/ })).toBeNull();
  });

  it("NAW-FR-HZSW, NAW-FR-NHAY: parent_issue_unavailable is stated in the band with Retry and Abandon in place", async () => {
    standing("open", SUB_ISSUE_CHOICE, {
      retry_draft_publication: () => {
        throw new Error("parent_issue_unavailable");
      },
    });
    renderWorkspace();
    const band = await screen.findByTestId("draft-publication-band");

    await userEvent.click(within(band).getByRole("button", { name: "Retry" }));

    await waitFor(() =>
      expect(within(band).getByRole("alert")).toHaveTextContent("parent_issue_unavailable"),
    );
    expect(within(band).getByRole("alert")).toHaveTextContent("missing, closed, or no longer usable");
    expect(within(band).getByRole("button", { name: "Retry" })).toBeInTheDocument();
    expect(within(band).getByRole("button", { name: "Abandon" })).toBeInTheDocument();
    // No root issue was offered in its place.
    expect(calls("publish_draft_to_github")).toHaveLength(0);
  });

  it("NAW-FR-HZSW: a sub-issue publication that fails on the link says so", async () => {
    stub();
    const base = invokeMock.getMockImplementation() as (c: string, a?: unknown) => Promise<unknown>;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "publish_draft_to_github") throw new Error("sub_issue_link_failed");
      return base(cmd, args);
    });
    const dialog = await openChooser();
    await loaded(dialog);
    await chooseParent(dialog, 412);
    await userEvent.click(within(dialog).getByRole("button", { name: "Publish" }));

    const band = await screen.findByTestId("draft-publication-band");
    expect(within(band).getByRole("alert")).toHaveTextContent("sub_issue_link_failed");
    expect(within(band).getByRole("alert")).toHaveTextContent("did not link it to its parent");
  });

  it("NAW-FR-EOTB: the recovery choice names what differs, including the parent, Type, and milestone", async () => {
    standing("awaiting_choice", SUB_ISSUE_CHOICE, {
      retry_draft_publication: () =>
        ({
          kind: "recoveryRequired",
          issueNumber: 418,
          issueUrl: "https://github.com/acme/widgets/issues/418",
          marker: "pub-1",
          mismatches: ["parent", "type", "milestone"],
        }) satisfies PublicationOutcome,
    });
    renderWorkspace();
    const band = await screen.findByTestId("draft-publication-band");

    await userEvent.click(within(band).getByRole("button", { name: "Recover" }));

    const dialog = await screen.findByRole("dialog", { name: "Recover publication" });
    expect(dialog).toHaveTextContent("Issue #418 already carries this attempt's marker");
    expect(dialog).toHaveTextContent("parent, Type and milestone differ");
    expect(calls("load_publication_metadata")).toHaveLength(0);
  });

  it("NAW-FR-ZGDM, NAW-FR-EOTB, NAW-FR-HZSW: a recovery answer that fails states its error inside the dialog and leaves it open", async () => {
    standing("awaiting_choice", SUB_ISSUE_CHOICE, {
      retry_draft_publication: () =>
        ({
          kind: "recoveryRequired",
          issueNumber: 418,
          issueUrl: "https://github.com/acme/widgets/issues/418",
          marker: "pub-1",
          mismatches: ["parent"],
        }) satisfies PublicationOutcome,
      resolve_draft_publication_conflict: () => {
        throw new Error("sub_issue_link_failed");
      },
    });
    renderWorkspace();
    const band = await screen.findByTestId("draft-publication-band");
    await userEvent.click(within(band).getByRole("button", { name: "Recover" }));
    const dialog = await screen.findByRole("dialog", { name: "Recover publication" });

    await userEvent.click(within(dialog).getByRole("button", { name: "Update existing issue" }));

    await waitFor(() =>
      expect(within(dialog).getByRole("alert")).toHaveTextContent("sub_issue_link_failed"),
    );
    expect(screen.getByRole("dialog", { name: "Recover publication" })).toBeInTheDocument();
  });

  it("NAW-FR-EOTB, NAW-FR-NHAY: Publish as a new issue sends no choice, the saved one being kept by the backend", async () => {
    standing("awaiting_choice", SUB_ISSUE_CHOICE, {
      retry_draft_publication: () =>
        ({
          kind: "recoveryRequired",
          issueNumber: 418,
          issueUrl: "https://github.com/acme/widgets/issues/418",
          marker: "pub-1",
          mismatches: ["parent"],
        }) satisfies PublicationOutcome,
      resolve_draft_publication_conflict: () =>
        ({ kind: "published", record: publicationRecord(419, "pub-2") }) satisfies PublicationOutcome,
    });
    renderWorkspace();
    const band = await screen.findByTestId("draft-publication-band");
    await userEvent.click(within(band).getByRole("button", { name: "Recover" }));
    const dialog = await screen.findByRole("dialog", { name: "Recover publication" });

    await userEvent.click(within(dialog).getByRole("button", { name: "Publish as a new issue" }));

    await waitFor(() => expect(calls("resolve_draft_publication_conflict")).toHaveLength(1));
    expect(calls("resolve_draft_publication_conflict")[0][1]).toEqual({
      draftId: "d1",
      choice: "publish_new",
    });
    expect(calls("load_publication_metadata")).toHaveLength(0);
  });
});
