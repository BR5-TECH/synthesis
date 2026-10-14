/**
 * The publication chooser over the backend's metadata cache: the cached lists
 * at once, the one background refresh, the change event, and what a change
 * does to the choices the author made
 * (`../../specifications/ui/NAW-new-artifact.md` NAW-FR-HNVR, NAW-FR-ASYN,
 * NAW-FR-WMEP, NAW-FR-RBTE, NAW-FR-NDDO, NAW-FR-HRXE, NAW-FR-VOTL,
 * NAW-FR-LETN, served by `../../specifications/core/GHP-github-publication.md`
 * GHP-FR-JTYL, GHP-FR-SIMK, GHP-FR-JIAU).
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
  listen: vi.fn(async (event: string, cb: (e: { payload: unknown }) => void) => {
    const set = listeners.get(event) ?? new Set();
    set.add(cb);
    listeners.set(event, set);
    return () => set.delete(cb);
  }),
}));

import {
  chooseParent,
  loadedList,
  makeStubs,
  openActions,
  publicationChooser,
  publicationMetadata,
  renderWorkspace,
} from "../test/newArtifactFixtures";
import { resetDraftDiscussions } from "../state/draftDiscussion";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { resetDiscussionFocus } from "../state/discussionFocus";
import { resetToasts, visibleToasts } from "../state/toasts";
import { resetLogBufferForTest } from "../logging";
import { PUBLICATION_METADATA_CHANGED } from "../events";
import type {
  PublicationMetadata,
  PublicationMetadataRefresh,
  PublicationParentIssue,
} from "../types";

const { stub } = makeStubs(invokeMock);

const calls = (name: string) => invokeMock.mock.calls.filter(([cmd]) => cmd === name);

function parent(number: number, title: string): PublicationParentIssue {
  return {
    number,
    title,
    issueType: "Feature",
    url: `https://github.com/acme/widgets/issues/${number}`,
    milestone: null,
  };
}

/** The cached lists: the default metadata. */
const CACHED = publicationMetadata();

/** A refresh the test answers by hand, so its outstanding state is visible. */
function pendingRefresh() {
  let answer: (value: PublicationMetadataRefresh) => void = () => {};
  let refuse: (error: unknown) => void = () => {};
  const refresh = () =>
    new Promise<PublicationMetadataRefresh>((resolve, reject) => {
      answer = resolve;
      refuse = reject;
    });
  return {
    refresh,
    answer: (value: PublicationMetadataRefresh) => act(async () => answer(value)),
    refuse: (error: unknown) => act(async () => refuse(error)),
  };
}

function refreshed(
  metadata: PublicationMetadata,
  outcome: PublicationMetadataRefresh["outcome"] = "changed",
): PublicationMetadataRefresh {
  return { outcome, errorCode: null, error: null, remoteName: "origin", metadata };
}

function emitChanged(remoteName: string | null, metadata: PublicationMetadata | null) {
  return act(async () => {
    for (const cb of listeners.get(PUBLICATION_METADATA_CHANGED) ?? []) {
      cb({ payload: { remoteName, metadata } });
    }
  });
}

async function openChooser() {
  renderWorkspace();
  const menu = await openActions();
  await userEvent.click(within(menu).getByRole("menuitem", { name: /Publish to GitHub/ }));
  return publicationChooser();
}

const publishButton = (dialog: HTMLElement) =>
  within(dialog).getByRole("button", { name: "Publish" });

beforeEach(() => {
  invokeMock.mockReset();
  listeners.clear();
  resetDraftDiscussions();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  resetToasts();
  resetLogBufferForTest();
});

afterEach(cleanup);

describe("Publication chooser — the metadata cache", () => {
  it("NAW-FR-HNVR, NAW-FR-WMEP, NAW-FR-ASYN: cached lists render at once, with the sub-issue option selected and no read of its own", async () => {
    const pending = pendingRefresh();
    stub({ cache: CACHED, refresh: pending.refresh });
    const dialog = await openChooser();

    await waitFor(() =>
      expect(within(dialog).getByRole("radio", { name: /sub-issue/ })).toBeChecked(),
    );
    expect(dialog).not.toHaveTextContent(/Loading/);
    expect(calls("get_publication_metadata_cache")[0][1]).toEqual({ remoteName: "origin" });
    expect(calls("load_publication_metadata")).toHaveLength(0);
    await userEvent.click(within(dialog).getByRole("combobox", { name: "Parent issue" }));
    expect(within(dialog).getAllByRole("option")).toHaveLength(2);
  });

  it("NAW-FR-HNVR, NAW-FR-NDDO: where the cache holds no lists, the chooser reads them itself and requests no refresh", async () => {
    stub({ cache: null });
    const dialog = await openChooser();

    await waitFor(() => expect(calls("load_publication_metadata")).toHaveLength(1));
    await waitFor(() => expect(dialog).not.toHaveTextContent(/Loading/));
    expect(calls("refresh_publication_metadata")).toHaveLength(0);
  });

  it("NAW-FR-NDDO, NAW-FR-RBTE, NAW-FR-WMEP: one refresh starts at once; Publish waits for it while the lists stay usable", async () => {
    const pending = pendingRefresh();
    stub({ cache: CACHED, refresh: pending.refresh });
    const dialog = await openChooser();
    await waitFor(() => expect(calls("refresh_publication_metadata")).toHaveLength(1));

    await chooseParent(dialog, 412);
    expect(within(dialog).getByRole("combobox", { name: "Parent issue" })).toHaveTextContent(
      "#412 Window chrome",
    );
    expect(publishButton(dialog)).toBeDisabled();

    await pending.answer(refreshed(CACHED, "unchanged"));
    await waitFor(() => expect(publishButton(dialog)).toBeEnabled());
    expect(calls("refresh_publication_metadata")).toHaveLength(1);
  });

  it("NAW-FR-NDDO: a failed refresh keeps the cached lists, raises no toast, and enables Publish", async () => {
    const pending = pendingRefresh();
    stub({ cache: CACHED, refresh: pending.refresh });
    const dialog = await openChooser();
    await waitFor(() => expect(calls("refresh_publication_metadata")).toHaveLength(1));
    await chooseParent(dialog, 398);

    await pending.answer({
      outcome: "failed",
      errorCode: "github_unreachable",
      error: "GitHub could not be reached.",
      remoteName: "origin",
      metadata: CACHED,
    });

    await waitFor(() => expect(publishButton(dialog)).toBeEnabled());
    expect(dialog).not.toHaveTextContent("GitHub could not be reached.");
    expect(visibleToasts()).toHaveLength(0);
    await userEvent.click(publishButton(dialog));
    await waitFor(() => expect(calls("publish_draft_to_github")).toHaveLength(1));
    expect(calls("publish_draft_to_github")[0][1]).toMatchObject({
      publicationChoice: { parentIssueNumber: 398 },
    });
  });

  it("NAW-FR-NDDO: a refresh call that is refused enables Publish and keeps the lists", async () => {
    const pending = pendingRefresh();
    stub({ cache: CACHED, refresh: pending.refresh });
    const dialog = await openChooser();
    await waitFor(() => expect(calls("refresh_publication_metadata")).toHaveLength(1));
    await chooseParent(dialog, 412);

    await pending.refuse("no_project_open");

    await waitFor(() => expect(publishButton(dialog)).toBeEnabled());
    expect(visibleToasts()).toHaveLength(0);
  });

  it("NAW-FR-HRXE: a change event updates the open dropdown and keeps its filter text and its scroll position", async () => {
    const pending = pendingRefresh();
    stub({ cache: CACHED, refresh: pending.refresh });
    const dialog = await openChooser();
    await waitFor(() => expect(calls("refresh_publication_metadata")).toHaveLength(1));
    await userEvent.click(within(dialog).getByRole("combobox", { name: "Parent issue" }));
    await userEvent.type(within(dialog).getByTestId("publication-parent-filter"), "re");
    const list = within(dialog).getByRole("listbox", { name: "Parent issue" });
    list.scrollTop = 40;

    await emitChanged(
      "origin",
      publicationMetadata({
        parents: loadedList([parent(398, "Release pipeline"), parent(500, "Reporting")]),
      }),
    );

    expect(within(dialog).getByTestId("publication-parent-filter")).toHaveValue("re");
    expect(within(dialog).getByRole("listbox", { name: "Parent issue" })).toBe(list);
    expect(list.scrollTop).toBe(40);
    expect(within(dialog).queryByTestId("publication-parent-option-412")).toBeNull();
    expect(within(dialog).getByTestId("publication-parent-option-500")).toBeInTheDocument();
    expect(within(dialog).getByTestId("publication-parent-option-398")).toBeInTheDocument();
  });

  it("NAW-FR-HRXE: a change event for another remote changes nothing", async () => {
    const pending = pendingRefresh();
    stub({ cache: CACHED, refresh: pending.refresh });
    const dialog = await openChooser();
    await waitFor(() => expect(calls("refresh_publication_metadata")).toHaveLength(1));

    await emitChanged("upstream", publicationMetadata({ parents: loadedList([]) }));

    expect(within(dialog).getByRole("radio", { name: /sub-issue/ })).toBeEnabled();
    expect(within(dialog).getByRole("combobox", { name: "Parent issue" })).toBeInTheDocument();
  });

  it("NAW-FR-VOTL, NAW-FR-RBTE: a refresh without the chosen parent clears it, and Publish stays disabled until the author chooses again or selects the root option", async () => {
    const pending = pendingRefresh();
    stub({ cache: CACHED, refresh: pending.refresh });
    const dialog = await openChooser();
    await waitFor(() => expect(calls("refresh_publication_metadata")).toHaveLength(1));
    await chooseParent(dialog, 412);

    await pending.answer(
      refreshed(publicationMetadata({ parents: loadedList([parent(398, "Release pipeline")]) })),
    );

    const combobox = within(dialog).getByRole("combobox", { name: "Parent issue" });
    await waitFor(() => expect(combobox).toHaveTextContent("Choose a parent issue"));
    expect(within(dialog).getByRole("radio", { name: /sub-issue/ })).toBeChecked();
    expect(publishButton(dialog)).toBeDisabled();

    // The choice is cleared, not hidden: the parent coming back later does
    // not choose it again.
    await emitChanged("origin", CACHED);
    expect(combobox).toHaveTextContent("Choose a parent issue");
    expect(publishButton(dialog)).toBeDisabled();

    await userEvent.click(within(dialog).getByRole("radio", { name: /root issue/ }));
    expect(publishButton(dialog)).toBeEnabled();
  });

  it("NAW-FR-VOTL: a refresh that still holds the chosen parent keeps it and enables Publish", async () => {
    const pending = pendingRefresh();
    stub({ cache: CACHED, refresh: pending.refresh });
    const dialog = await openChooser();
    await waitFor(() => expect(calls("refresh_publication_metadata")).toHaveLength(1));
    await chooseParent(dialog, 398);

    await pending.answer(
      refreshed(
        publicationMetadata({
          parents: loadedList([parent(398, "Release pipeline"), parent(600, "New")]),
        }),
      ),
    );

    await waitFor(() => expect(publishButton(dialog)).toBeEnabled());
    expect(within(dialog).getByRole("combobox", { name: "Parent issue" })).toHaveTextContent(
      "#398 Release pipeline",
    );
  });

  it("NAW-FR-LETN: a chosen Type or milestone the new list no longer holds resets to No Type or No milestone", async () => {
    const pending = pendingRefresh();
    stub({ cache: CACHED, refresh: pending.refresh });
    const dialog = await openChooser();
    await waitFor(() => expect(calls("refresh_publication_metadata")).toHaveLength(1));
    await userEvent.click(within(dialog).getByRole("radio", { name: /root issue/ }));
    await userEvent.selectOptions(within(dialog).getByLabelText("Type"), "Bug");
    await userEvent.selectOptions(within(dialog).getByLabelText("Milestone"), "v1.3");

    await pending.answer(
      refreshed(
        publicationMetadata({
          issueTypes: loadedList([{ name: "Feature" }, { name: "Task" }]),
          milestones: loadedList([{ number: 7, title: "v1.2" }]),
        }),
      ),
    );

    await waitFor(() => expect(within(dialog).getByLabelText("Type")).toHaveValue(""));
    expect(within(dialog).getByLabelText("Milestone")).toHaveValue("");
    await userEvent.click(publishButton(dialog));
    await waitFor(() => expect(calls("publish_draft_to_github")).toHaveLength(1));
    expect(calls("publish_draft_to_github")[0][1]).toMatchObject({
      publicationChoice: { parentIssueNumber: null, issueType: null, milestoneNumber: null },
    });
  });

  it("NAW-FR-LETN: a Type and a milestone the new lists still hold stay chosen", async () => {
    const pending = pendingRefresh();
    stub({ cache: CACHED, refresh: pending.refresh });
    const dialog = await openChooser();
    await waitFor(() => expect(calls("refresh_publication_metadata")).toHaveLength(1));
    await userEvent.click(within(dialog).getByRole("radio", { name: /root issue/ }));
    await userEvent.selectOptions(within(dialog).getByLabelText("Type"), "Task");
    await userEvent.selectOptions(within(dialog).getByLabelText("Milestone"), "v1.2");

    await pending.answer(
      refreshed(publicationMetadata({ milestones: loadedList([{ number: 7, title: "v1.2" }]) })),
    );

    await waitFor(() => expect(publishButton(dialog)).toBeEnabled());
    expect(within(dialog).getByLabelText("Type")).toHaveValue("Task");
    expect(within(dialog).getByLabelText("Milestone")).toHaveValue("7");
  });

  it("NAW-FR-TCQB, NAW-FR-ASYN: a refresh that leaves no parent selects the root option, which stays available", async () => {
    const pending = pendingRefresh();
    stub({ cache: CACHED, refresh: pending.refresh });
    const dialog = await openChooser();
    await waitFor(() => expect(calls("refresh_publication_metadata")).toHaveLength(1));

    await pending.answer(refreshed(publicationMetadata({ parents: loadedList([]) })));

    await waitFor(() =>
      expect(within(dialog).getByRole("radio", { name: /root issue/ })).toBeChecked(),
    );
    expect(within(dialog).getByRole("radio", { name: /sub-issue/ })).toBeDisabled();
    expect(publishButton(dialog)).toBeEnabled();
  });

  it("NAW-FR-VUCK: an answer that lands after the chooser closed reopens nothing", async () => {
    const pending = pendingRefresh();
    stub({ cache: CACHED, refresh: pending.refresh });
    const dialog = await openChooser();
    await waitFor(() => expect(calls("refresh_publication_metadata")).toHaveLength(1));
    await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));

    await pending.answer(refreshed(CACHED));

    expect(screen.queryByRole("radio", { name: /root issue/ })).toBeNull();
  });
});
