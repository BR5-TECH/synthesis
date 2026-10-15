// Publish to GitHub on a note row (`specifications/ui/NTS-notes.md`
// NTS-FR-FYUJ, NTS-FR-EZEA, NTS-FR-ZDLS) and the note publish modal
// (`specifications/ui/NIP-note-issue-publication.md`). The backend is mocked
// at `invoke`: every assertion is about what the panel and the modal render
// and which operation they invoke.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import type {
  NoteListItem,
  NotesPanelState,
  PublicationMetadata,
  PublicationMetadataCacheChanged,
} from "../types";
import { Harness, openMenu, projectNote, rows } from "../test/notesFixtures";
import { resetToasts, visibleToasts } from "../state/toasts";
import { resetLogBufferForTest } from "../logging";
import { PUBLICATION_METADATA_CHANGED } from "../events";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

/** The event listeners the modal installs, so a test can deliver a payload. */
const listeners = new Map<string, Set<(payload: unknown) => void>>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: (event: string, handler: (ev: { payload: unknown }) => void) => {
    const fn = (payload: unknown) => handler({ payload });
    const set = listeners.get(event) ?? new Set();
    set.add(fn);
    listeners.set(event, set);
    return Promise.resolve(() => set.delete(fn));
  },
  emit: () => Promise.resolve(),
}));

function emit(event: string, payload: unknown): void {
  for (const fn of [...(listeners.get(event) ?? [])]) fn(payload);
}

function metadata(overrides: Partial<PublicationMetadata> = {}): PublicationMetadata {
  return {
    repositoryOwner: "acme",
    repositoryName: "widgets",
    settings: {
      parentIssueTypes: ["Feature"],
      subIssueType: "Task",
      subIssueMilestonePolicy: "inherit_parent",
      rootIssueType: null,
      noteIssueType: "Feature",
    },
    parents: {
      state: "loaded",
      items: [
        {
          number: 12,
          title: "Epic work",
          issueType: "Feature",
          url: "https://github.com/acme/widgets/issues/12",
          milestone: { number: 7, title: "v1.2" },
        },
      ],
      errorCode: null,
      error: null,
    },
    issueTypes: {
      state: "loaded",
      items: [{ name: "Feature" }, { name: "Bug" }],
      errorCode: null,
      error: null,
    },
    milestones: {
      state: "loaded",
      items: [{ number: 7, title: "v1.2" }],
      errorCode: null,
      error: null,
    },
    labels: {
      state: "loaded",
      items: [
        { name: "bug", color: "d73a4a", description: null },
        { name: "ui", color: "0e8a16", description: "User interface" },
      ],
      errorCode: null,
      error: null,
    },
    subIssueType: { name: "Task", resolved: "Task" },
    rootIssueType: null,
    noteIssueType: { name: "Feature", resolved: "Feature" },
    ...overrides,
  };
}

const AVAILABLE = {
  available: true,
  reasonCode: null,
  reason: null,
  remoteName: "origin",
  repositoryHost: "github.com",
  repositoryOwner: "acme",
  repositoryName: "widgets",
};

/** What the mocked backend serves for the test being run. */
let served: {
  project: NoteListItem[];
  panelState: NotesPanelState;
  availability: unknown;
  cache: PublicationMetadata | null;
  refresh: () => Promise<unknown>;
  publish: (args: Record<string, unknown>) => Promise<unknown>;
};

beforeEach(() => {
  invokeMock.mockReset();
  resetToasts();
  resetLogBufferForTest();
  listeners.clear();
  served = {
    project: [
      {
        ...projectNote("1", "Check the **FR** numbering.", "2026-05-01T09:00:00Z"),
      },
    ],
    panelState: { scopePosition: "project", textFilter: "" },
    availability: AVAILABLE,
    cache: metadata(),
    refresh: async () => ({
      outcome: "unchanged",
      errorCode: null,
      error: null,
      remoteName: "origin",
      metadata: served.cache,
    }),
    publish: async () => ({
      kind: "published",
      issueNumber: 41,
      issueUrl: "https://github.com/acme/widgets/issues/41",
    }),
  };
  invokeMock.mockImplementation((cmd: string, args: Record<string, unknown>) => {
    switch (cmd) {
      case "append_log_records":
      case "save_notes_panel_state":
        return Promise.resolve(undefined);
      case "load_notes_panel_state":
        return Promise.resolve(served.panelState);
      case "list_project_notes":
      case "list_all_notes":
        return Promise.resolve(served.project);
      case "get_note_publication_availability":
        return Promise.resolve(served.availability);
      case "get_publication_metadata_cache":
        return Promise.resolve(served.cache);
      case "refresh_publication_metadata":
        return served.refresh();
      case "publish_note_to_github":
        return served.publish(args);
      case "open_publication_repository_issue":
        return Promise.resolve(undefined);
      default:
        return Promise.reject(new Error(`unexpected command ${cmd}`));
    }
  });
});

afterEach(() => {
  cleanup();
});

function calls(cmd: string): unknown[][] {
  return invokeMock.mock.calls.filter((c) => c[0] === cmd);
}

async function openModal(onNoteDeleted?: (id: string) => void) {
  render(<Harness entity={null} onNoteDeleted={onNoteDeleted} />);
  await screen.findByText("FR");
  const menu = await openMenu("numbering");
  const entry = within(menu).getByText("Publish to GitHub");
  await waitFor(() => expect(entry).not.toHaveAttribute("aria-disabled"));
  await userEvent.click(within(document.querySelector(".menu") as HTMLElement).getByText("Publish to GitHub"));
  const dialog = await screen.findByRole("dialog", { name: "Publish to GitHub" });
  // Wait for a state, not for time: with a cache served, the lists render first.
  if (served.cache) await within(dialog).findByLabelText("Type");
  return dialog;
}

function publishButton(dialog: HTMLElement) {
  return within(dialog).getByRole("button", { name: "Publish" });
}

describe("the Publish to GitHub entry (NTS-FR-FYUJ, NTS-FR-EZEA)", () => {
  it("NTS-FR-FYUJ: is enabled where the availability read answers available", async () => {
    render(<Harness entity={null} />);
    await screen.findByText("FR");
    const menu = await openMenu("numbering");
    await waitFor(() =>
      expect(within(menu).getByText("Publish to GitHub")).not.toHaveAttribute("aria-disabled"),
    );
    expect(calls("get_note_publication_availability")).toHaveLength(1);
  });

  it("NTS-FR-FYUJ: is disabled with the reason when no GitHub Project is selected", async () => {
    served.availability = {
      ...AVAILABLE,
      available: false,
      reasonCode: "github_project_not_selected",
      reason: "No GitHub Project is selected in Project settings.",
      remoteName: null,
    };
    render(<Harness entity={null} />);
    await screen.findByText("FR");
    const menu = await openMenu("numbering");
    const entry = within(menu).getByText("Publish to GitHub");
    await waitFor(() =>
      expect(entry).toHaveAttribute("title", "No GitHub Project is selected in Project settings."),
    );
    expect(entry).toHaveAttribute("aria-disabled", "true");
    expect(entry).toHaveAccessibleDescription("No GitHub Project is selected in Project settings.");
    await userEvent.click(entry);
    expect(screen.queryByRole("dialog")).toBeNull();
  });

  it("NTS-FR-FYUJ: states the remote or token refusal, and is disabled while the read is outstanding", async () => {
    let answer: (value: unknown) => void = () => {};
    served.availability = new Promise((resolve) => (answer = resolve));
    render(<Harness entity={null} />);
    await screen.findByText("FR");
    const menu = await openMenu("numbering");
    const entry = within(menu).getByText("Publish to GitHub");
    expect(entry).toHaveAttribute("aria-disabled", "true");
    await act(async () =>
      answer({
        ...AVAILABLE,
        available: false,
        reasonCode: "token_unavailable",
        reason: "This project has no GitHub token.",
        remoteName: null,
      }),
    );
    await waitFor(() => expect(entry).toHaveAttribute("title", "This project has no GitHub token."));
  });

  it("NTS-FR-EZEA, NIP-FR-TVNN, NIP-FR-HYUJ: opens the modal naming the repository, and reads the cache and one refresh", async () => {
    const dialog = await openModal();
    expect(within(dialog).getByTestId("note-publication-repository")).toHaveTextContent(
      "acme/widgets",
    );
    await waitFor(() => expect(calls("refresh_publication_metadata")).toHaveLength(1));
    expect(calls("get_publication_metadata_cache")).toEqual([
      ["get_publication_metadata_cache", { remoteName: "origin" }],
    ]);
    expect(document.querySelector(".menu")).toBeNull();
  });
});

describe("the note publish modal (NIP)", () => {
  it("NIP-FR-DZWQ, NIP-FR-UMKW, NIP-FR-SHMC, NIP-FR-DKXP, NIP-FR-FDAN, NIP-FR-DIXC, NIP-FR-YQFL: opens on its defaults", async () => {
    const dialog = await openModal();
    expect(within(dialog).getByRole("radio", { name: "Publish as a root issue" })).toBeChecked();
    expect(within(dialog).getByRole("radio", { name: "Publish as a sub-issue of" })).not.toBeChecked();
    expect(within(dialog).getByRole("radiogroup", { name: "Parent" })).toBeInTheDocument();
    await waitFor(() => expect(within(dialog).getByLabelText("Type")).toHaveValue("Feature"));
    expect(within(dialog).getByLabelText("Milestone")).toHaveValue("");
    for (const box of within(within(dialog).getByRole("group", { name: "Labels" })).getAllByRole("checkbox")) {
      expect(box).not.toBeChecked();
    }
    expect(within(dialog).getByLabelText("Title")).toHaveValue("");
    expect(within(dialog).getByLabelText("Description")).toHaveValue("Check the **FR** numbering.");
    // NAW-FR-PSMO: focus starts on the first enabled option. The sub-issue
    // option is disabled while the lists load, so it is the root option.
    expect(document.activeElement).toBe(
      within(dialog).getByRole("radio", { name: "Publish as a root issue" }),
    );
  });

  it("NIP-FR-UMKW: a stored Type the list does not hold opens on No Type with a notice", async () => {
    served.cache = metadata({ noteIssueType: { name: "Epic", resolved: null } });
    const dialog = await openModal();
    await waitFor(() => expect(within(dialog).getByLabelText("Type")).toHaveValue(""));
    expect(within(dialog).getByText(/“Epic” is not available/)).toBeInTheDocument();
  });

  it("NIP-FR-UMKW: a stored No Type opens on No Type with no notice", async () => {
    served.cache = metadata({ noteIssueType: null });
    const dialog = await openModal();
    await waitFor(() => expect(within(dialog).getByLabelText("Type")).toHaveValue(""));
    expect(within(dialog).queryByText(/is not available/)).toBeNull();
  });

  it("NIP-FR-FDAN, NIP-FR-IJAL: Publish is disabled while the trimmed title is empty", async () => {
    const dialog = await openModal();
    expect(publishButton(dialog)).toBeDisabled();
    await userEvent.type(within(dialog).getByLabelText("Title"), "   ");
    expect(publishButton(dialog)).toBeDisabled();
    await userEvent.type(within(dialog).getByLabelText("Title"), "Fix numbering");
    expect(publishButton(dialog)).toBeEnabled();
  });

  it("NIP-FR-FDAN: the title holds at most 256 characters", async () => {
    const dialog = await openModal();
    const title = within(dialog).getByLabelText("Title");
    fireEvent.change(title, { target: { value: "é".repeat(300) } });
    expect((title as HTMLInputElement).value).toHaveLength(256);
  });

  it("NIP-FR-DIXC: a description over 65,536 characters is refused inline", async () => {
    const dialog = await openModal();
    fireEvent.change(within(dialog).getByLabelText("Title"), { target: { value: "T" } });
    fireEvent.change(within(dialog).getByLabelText("Description"), {
      target: { value: "x".repeat(65_537) },
    });
    expect(within(dialog).getByRole("alert")).toHaveTextContent("65,536");
    expect(publishButton(dialog)).toBeDisabled();
  });

  it("NIP-FR-ITUR: warns that the discussion is deleted and states the reminder goes", async () => {
    served.project = [
      {
        ...projectNote("1", "Check the **FR** numbering.", "2026-05-01T09:00:00Z"),
        discussionThreadId: "t-1",
      },
    ];
    served.project[0].note.reminder = "2030-01-01T09:00:00Z";
    const dialog = await openModal();
    expect(within(dialog).getByText(/The discussion is deleted with the note/)).toBeInTheDocument();
    expect(within(dialog).getByText(/The reminder of this note is removed too/)).toBeInTheDocument();
  });

  it("NIP-FR-ITUR: shows neither statement for a note with neither", async () => {
    const dialog = await openModal();
    expect(within(dialog).queryByText(/discussion is deleted/)).toBeNull();
    expect(within(dialog).queryByText(/reminder of this note/)).toBeNull();
  });

  it("NIP-FR-SJGY, NIP-FR-MTGO, NTS-FR-ZDLS, NTF-FR-AGRZ, NTF-FR-XNCG: a success closes the modal, removes the row, and shows the Info toast with its link", async () => {
    const deleted = vi.fn();
    const dialog = await openModal(deleted);
    fireEvent.change(within(dialog).getByLabelText("Title"), { target: { value: "  Fix numbering  " } });
    await userEvent.click(within(within(dialog).getByRole("group", { name: "Labels" })).getByLabelText("ui"));
    await userEvent.selectOptions(within(dialog).getByLabelText("Milestone"), "7");
    served.project = [];
    await userEvent.click(publishButton(dialog));

    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(calls("publish_note_to_github")).toEqual([
      [
        "publish_note_to_github",
        {
          noteId: "1",
          title: "Fix numbering",
          body: "Check the **FR** numbering.",
          choice: { parentIssueNumber: null, issueType: "Feature", milestoneNumber: 7, labels: ["ui"] },
        },
      ],
    ]);
    expect(deleted).toHaveBeenCalledWith("1");
    await waitFor(() => expect(rows()).toHaveLength(0));
    const toast = visibleToasts().find((t) => t.key === "note-published");
    expect(toast?.level).toBe("Info");
    expect(toast?.title).toBe("Issue created");
    expect(toast?.body).toContain("#41");
    expect(toast?.address).toBeNull();
    expect(toast?.link?.label).toBe("Open issue #41");
    toast?.link?.onActivate();
    await waitFor(() =>
      expect(calls("open_publication_repository_issue")).toEqual([
        ["open_publication_repository_issue", { url: "https://github.com/acme/widgets/issues/41" }],
      ]),
    );
  });

  it("NIP-FR-NWGA, NIP-FR-YEKZ, NIP-FR-SHMC, NIP-FR-WVQP: a sub-issue sends the parent, the Type, and No milestone however the parent is scheduled", async () => {
    const dialog = await openModal();
    fireEvent.change(within(dialog).getByLabelText("Title"), { target: { value: "Sub" } });
    await userEvent.click(within(dialog).getByRole("radio", { name: "Publish as a sub-issue of" }));
    expect(publishButton(dialog)).toBeDisabled();
    // NIP-FR-WVQP / NAW-FR-YELA: the parent combobox filters on the number.
    await userEvent.click(within(dialog).getByTestId("note-publication-parent"));
    await userEvent.type(screen.getByLabelText("Filter parent issue"), "#99");
    expect(await screen.findByText("No matching issue")).toBeInTheDocument();
    await userEvent.clear(screen.getByLabelText("Filter parent issue"));
    await userEvent.click(await screen.findByRole("option", { name: /#12 Epic work/ }));
    expect(within(dialog).getByRole("radio", { name: "Publish as a sub-issue of" })).toBeChecked();
    await userEvent.selectOptions(within(dialog).getByLabelText("Type"), "Bug");
    expect(within(dialog).getByLabelText("Milestone")).toHaveValue("");
    await userEvent.click(publishButton(dialog));
    await waitFor(() => expect(calls("publish_note_to_github")).toHaveLength(1));
    expect((calls("publish_note_to_github")[0][1] as { choice: unknown }).choice).toEqual({
      parentIssueNumber: 12,
      issueType: "Bug",
      milestoneNumber: null,
      labels: [],
    });
  });

  it("NIP-FR-YEKZ: selecting the root option keeps the chosen parent but sends none", async () => {
    const dialog = await openModal();
    fireEvent.change(within(dialog).getByLabelText("Title"), { target: { value: "T" } });
    await userEvent.click(within(dialog).getByTestId("note-publication-parent"));
    await userEvent.click(await screen.findByRole("option", { name: /#12 Epic work/ }));
    await userEvent.click(within(dialog).getByRole("radio", { name: "Publish as a root issue" }));
    await userEvent.click(within(dialog).getByRole("radio", { name: "Publish as a sub-issue of" }));
    expect(within(dialog).getByTestId("note-publication-parent")).toHaveTextContent("#12");
    expect(publishButton(dialog)).toBeEnabled();
    await userEvent.click(within(dialog).getByRole("radio", { name: "Publish as a root issue" }));
    await userEvent.click(publishButton(dialog));
    await waitFor(() => expect(calls("publish_note_to_github")).toHaveLength(1));
    expect(
      (calls("publish_note_to_github")[0][1] as { choice: { parentIssueNumber: unknown } }).choice
        .parentIssueNumber,
    ).toBeNull();
  });

  it("NIP-FR-YEKZ, NIP-FR-VLFR: an empty parent list disables the sub-issue option, and empty lists render notices", async () => {
    served.cache = metadata({
      parents: { state: "loaded", items: [], errorCode: null, error: null },
      labels: { state: "failed", items: [], errorCode: "labels_unreadable", error: "The labels of this repository could not be read." },
      milestones: { state: "loaded", items: [], errorCode: null, error: null },
    });
    const dialog = await openModal();
    expect(within(dialog).getByRole("radio", { name: "Publish as a sub-issue of" })).toBeDisabled();
    expect(within(dialog).getByText(/No open issue of Feature was found/)).toBeInTheDocument();
    expect(within(dialog).getByText(/Labels are unavailable: The labels of this repository could not be read/)).toBeInTheDocument();
    expect(within(dialog).getByText(/There are no open milestones/)).toBeInTheDocument();
    fireEvent.change(within(dialog).getByLabelText("Title"), { target: { value: "T" } });
    expect(publishButton(dialog)).toBeEnabled();
  });

  it("NIP-FR-GPEO, NIP-FR-IJAL: with no cache, the lists load, and a failed refresh makes each unavailable", async () => {
    served.cache = null;
    let answer: (value: unknown) => void = () => {};
    served.refresh = () => new Promise((resolve) => (answer = resolve));
    const dialog = await openModal();
    expect(within(dialog).getByText("Loading parent issues…")).toBeInTheDocument();
    fireEvent.change(within(dialog).getByLabelText("Title"), { target: { value: "T" } });
    expect(publishButton(dialog)).toBeDisabled();
    await act(async () =>
      answer({
        outcome: "failed",
        errorCode: "github_unreachable",
        error: "GitHub could not be reached.",
        remoteName: null,
        metadata: null,
      }),
    );
    expect(within(dialog).getByText(/Parent issues are unavailable: GitHub could not be reached/)).toBeInTheDocument();
    expect(within(dialog).getByText(/Issue Types are unavailable/)).toBeInTheDocument();
    expect(publishButton(dialog)).toBeEnabled();
  });

  it("NIP-FR-PWYE, NIP-FR-ONHC, NIP-FR-PWAB: a changed list renders, and a choice it no longer holds resets", async () => {
    const dialog = await openModal();
    await waitFor(() => expect(calls("refresh_publication_metadata")).toHaveLength(1));
    fireEvent.change(within(dialog).getByLabelText("Title"), { target: { value: "T" } });
    await userEvent.click(within(dialog).getByTestId("note-publication-parent"));
    await userEvent.click(await screen.findByRole("option", { name: /#12 Epic work/ }));
    await userEvent.selectOptions(within(dialog).getByLabelText("Type"), "Bug");
    await userEvent.selectOptions(within(dialog).getByLabelText("Milestone"), "7");
    await userEvent.click(within(within(dialog).getByRole("group", { name: "Labels" })).getByLabelText("bug"));
    // The new lists hold none of the choices.
    const without: PublicationMetadataCacheChanged = {
      remoteName: "origin",
      metadata: metadata({
        parents: { state: "loaded", items: [{ number: 13, title: "Other", issueType: "Feature", url: "u", milestone: null }], errorCode: null, error: null },
        issueTypes: { state: "loaded", items: [{ name: "Feature" }], errorCode: null, error: null },
        milestones: { state: "loaded", items: [{ number: 8, title: "v1.3" }], errorCode: null, error: null },
        labels: { state: "loaded", items: [{ name: "ui", color: "0e8a16", description: null }], errorCode: null, error: null },
      }),
    };
    act(() => emit(PUBLICATION_METADATA_CHANGED, without));
    await waitFor(() => expect(within(dialog).getByLabelText("Type")).toHaveValue(""));
    await waitFor(() => expect(within(dialog).getByLabelText("Milestone")).toHaveValue(""));
    // The old lists come back. A choice that was reset stays reset.
    act(() => emit(PUBLICATION_METADATA_CHANGED, { remoteName: "origin", metadata: metadata() }));
    await waitFor(() => expect(within(dialog).getByLabelText("bug")).toBeInTheDocument());
    expect(within(dialog).getByLabelText("bug")).not.toBeChecked();
    expect(within(dialog).getByLabelText("Milestone")).toHaveValue("");
    expect(within(dialog).getByLabelText("Type")).toHaveValue("");
    // NIP-FR-ONHC: the sub-issue option stays selected with no parent.
    expect(within(dialog).getByTestId("note-publication-parent")).not.toHaveTextContent("#12");
    expect(publishButton(dialog)).toBeDisabled();
    await userEvent.click(within(dialog).getByRole("radio", { name: "Publish as a root issue" }));
    await userEvent.click(publishButton(dialog));
    await waitFor(() => expect(calls("publish_note_to_github")).toHaveLength(1));
    expect((calls("publish_note_to_github")[0][1] as { choice: unknown }).choice).toEqual({
      parentIssueNumber: null,
      issueType: null,
      milestoneNumber: null,
      labels: [],
    });
  });

  it("NIP-FR-ISQT, NTF-FR-KBBO: a create failure keeps the modal and its values, renders the error, and raises the Error toast", async () => {
    served.publish = () => Promise.reject("issue_create_failed");
    const dialog = await openModal();
    fireEvent.change(within(dialog).getByLabelText("Title"), { target: { value: "Keep me" } });
    await userEvent.click(publishButton(dialog));
    expect(await within(dialog).findByTestId("note-publication-error")).toHaveTextContent(
      "GitHub did not create the issue.",
    );
    expect(within(dialog).getByLabelText("Title")).toHaveValue("Keep me");
    expect(rows()).toHaveLength(1);
    const toast = visibleToasts().find((t) => t.key === "note-publish-failed");
    expect(toast?.level).toBe("Error");
    expect(toast?.title).toBe("Note not published");
    expect(toast?.address).toBeNull();
    expect(publishButton(dialog)).toBeEnabled();
  });

  it("NIP-FR-SNLZ: a failed sub-issue link keeps the modal and names the created issue", async () => {
    served.publish = async () => ({
      kind: "sub_issue_link_failed",
      issueNumber: 42,
      issueUrl: "https://github.com/acme/widgets/issues/42",
    });
    const dialog = await openModal();
    fireEvent.change(within(dialog).getByLabelText("Title"), { target: { value: "T" } });
    await userEvent.click(publishButton(dialog));
    expect(await within(dialog).findByTestId("note-publication-error")).toHaveTextContent(
      "Issue #42 was created",
    );
    expect(visibleToasts().find((t) => t.key === "note-publish-failed")?.body).toContain("#42");
    expect(rows()).toHaveLength(1);
  });

  it("NIP-FR-GQVQ, NTF-FR-IWAR: a deletion failure closes the modal, keeps the note, and raises the Error toast with the link", async () => {
    const deleted = vi.fn();
    served.publish = async () => ({
      kind: "note_not_removed",
      issueNumber: 43,
      issueUrl: "https://github.com/acme/widgets/issues/43",
      errorCode: "discussion_cleanup_failed",
    });
    const dialog = await openModal(deleted);
    fireEvent.change(within(dialog).getByLabelText("Title"), { target: { value: "T" } });
    await userEvent.click(publishButton(dialog));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(deleted).not.toHaveBeenCalled();
    expect(rows()).toHaveLength(1);
    const toast = visibleToasts().find((t) => t.key === "note-not-removed");
    expect(toast?.level).toBe("Error");
    expect(toast?.title).toBe("Note not removed");
    expect(toast?.body).toContain("#43");
    expect(toast?.link?.label).toBe("Open issue #43");
  });

  it("NIP-FR-IJAL, NIP-FR-WUID: one click is one call, and no dismissal closes the modal while it runs", async () => {
    let finish: (value: unknown) => void = () => {};
    served.publish = () => new Promise((resolve) => (finish = resolve));
    const dialog = await openModal();
    fireEvent.change(within(dialog).getByLabelText("Title"), { target: { value: "T" } });
    await userEvent.click(publishButton(dialog));
    expect(publishButton(dialog)).toBeDisabled();
    fireEvent.click(publishButton(dialog));
    await userEvent.keyboard("{Escape}");
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(calls("publish_note_to_github")).toHaveLength(1);
    await act(async () =>
      finish({ kind: "published", issueNumber: 1, issueUrl: "https://github.com/acme/widgets/issues/1" }),
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("NIP-FR-WUID: Escape, Cancel, and the close control close it and invoke nothing; an outside click does not", async () => {
    let dialog = await openModal();
    fireEvent.click(dialog.parentElement as HTMLElement);
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(document.activeElement).toHaveAccessibleName("Note actions");

    const menu = await openMenu("numbering");
    await waitFor(() =>
      expect(within(menu).getByText("Publish to GitHub")).not.toHaveAttribute("aria-disabled"),
    );
    await userEvent.click(within(menu).getByText("Publish to GitHub"));
    dialog = await screen.findByRole("dialog", { name: "Publish to GitHub" });
    await userEvent.click(within(dialog).getByRole("button", { name: "Close" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    expect(calls("publish_note_to_github")).toHaveLength(0);
  });

  it("NIP-FR-DQQN: reopening after a failure starts from the defaults", async () => {
    served.publish = () => Promise.reject("issue_create_failed");
    const dialog = await openModal();
    fireEvent.change(within(dialog).getByLabelText("Title"), { target: { value: "Typed" } });
    await userEvent.click(publishButton(dialog));
    await within(dialog).findByTestId("note-publication-error");
    await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
    const menu = await openMenu("numbering");
    await waitFor(() =>
      expect(within(menu).getByText("Publish to GitHub")).not.toHaveAttribute("aria-disabled"),
    );
    await userEvent.click(within(menu).getByText("Publish to GitHub"));
    const again = await screen.findByRole("dialog", { name: "Publish to GitHub" });
    expect(within(again).getByLabelText("Title")).toHaveValue("");
    expect(within(again).queryByTestId("note-publication-error")).toBeNull();
  });
});
