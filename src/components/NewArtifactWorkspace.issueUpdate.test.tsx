/**
 * **Update on GitHub** from the New Artifact tab: the entry that takes the
 * place of **Publish to GitHub** once the draft holds a current record, the
 * closed-issue dialog, and the toast of a failed update
 * (`../../specifications/ui/NAW-new-artifact.md` NAW-FR-KMLN … NAW-FR-PTYO,
 * `../../specifications/ui/ACT-action-control.md` ACT-FR-SSHS,
 * `../../specifications/ui/NTF-notifications.md` NTF-FR-RMJT).
 */
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { act, cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import {
  editSource,
  makeStubs,
  noPublication,
  openActions,
  publicationRecord,
  publicationRefused,
  renderWorkspace,
} from "../test/newArtifactFixtures";
import { resetDraftDiscussions } from "../state/draftDiscussion";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { resetDiscussionFocus } from "../state/discussionFocus";
import { resetToasts, visibleToasts } from "../state/toasts";
import { resetLogBufferForTest } from "../logging";
import {
  UPDATE_FAILED_KEY,
  UPDATE_FAILED_TITLE,
} from "../hooks/useDraftPublication";
import type {
  DraftPublicationView,
  IssueUpdateOutcome,
  PublicationOutcome,
} from "../types";

const { stub } = makeStubs(invokeMock);

const calls = (name: string) =>
  invokeMock.mock.calls.filter(([cmd]) => cmd === name);

const RECORD = publicationRecord(418, "pub-c");

/** A draft whose current record names issue #418. */
function published(overrides: Partial<DraftPublicationView> = {}): DraftPublicationView {
  return { ...noPublication(), current: RECORD, history: [RECORD], ...overrides };
}

/** Answer `update_draft_on_github` with `answer`, and the rest as stubbed. */
function onUpdate(answer: (updateClosed: boolean) => Promise<IssueUpdateOutcome>) {
  const base = invokeMock.getMockImplementation() as (
    cmd: string,
    args?: unknown,
  ) => Promise<unknown>;
  invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
    if (cmd === "update_draft_on_github") {
      return answer((args as { updateClosed: boolean }).updateClosed);
    }
    return base(cmd, args);
  });
}

const updated = (): Promise<IssueUpdateOutcome> =>
  Promise.resolve({
    kind: "updated",
    record: { ...RECORD, updatedAt: "2026-10-14T09:00:00.000Z" },
  });

async function updateEntry() {
  const menu = await openActions();
  return within(menu).getByRole("menuitem", { name: /Update on GitHub/ });
}

beforeEach(() => {
  invokeMock.mockReset();
  resetDraftDiscussions();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  resetToasts();
  resetLogBufferForTest();
});

afterEach(cleanup);

describe("New Artifact workspace — Update on GitHub", () => {
  it("ACT-FR-SSHS, NAW-FR-KMLN: a draft with a current record offers Update on GitHub in the place of Publish to GitHub", async () => {
    stub({ publication: published() });
    renderWorkspace();
    await screen.findByTestId("draft-publication-tag");

    const menu = await openActions();
    const labels = within(menu)
      .getAllByRole("menuitem")
      .map((item) => item.textContent?.trim());
    expect(labels).toEqual([
      "Discuss",
      "Graduate",
      "Copy as Source",
      "Update on GitHub",
      "Archive",
    ]);
  });

  it("ACT-FR-SSHS: a draft with no record keeps Publish to GitHub", async () => {
    stub();
    renderWorkspace();
    const menu = await openActions();
    expect(within(menu).getByRole("menuitem", { name: /Publish to GitHub/ })).toBeInTheDocument();
    expect(within(menu).queryByRole("menuitem", { name: /Update on GitHub/ })).toBeNull();
  });

  it("NAW-FR-KMLN, NAW-FR-PTYO: activating it invokes the update alone, opens no picker or chooser, and keeps the tag and the status", async () => {
    const after = { ...RECORD, updatedAt: "2026-10-14T09:00:00.000Z" };
    let view = published();
    stub({ publication: () => view });
    onUpdate(async () => {
      view = published({ current: after, history: [after, RECORD] });
      return { kind: "updated", record: after };
    });
    renderWorkspace();
    await screen.findByTestId("draft-publication-tag");
    const readsBefore = calls("get_draft_publication").length;

    await userEvent.click(await updateEntry());

    await waitFor(() => expect(calls("update_draft_on_github")).toHaveLength(1));
    expect(calls("update_draft_on_github")[0][1]).toEqual({ draftId: "d1", updateClosed: false });
    expect(calls("list_publication_remotes")).toHaveLength(0);
    expect(calls("load_publication_metadata")).toHaveLength(0);
    expect(calls("publish_draft_to_github")).toHaveLength(0);
    expect(screen.queryByRole("dialog")).toBeNull();
    await waitFor(() =>
      expect(calls("get_draft_publication").length).toBeGreaterThan(readsBefore),
    );
    await waitFor(() =>
      expect(screen.getByTestId("draft-publication-tag")).toHaveAttribute(
        "title",
        expect.stringContaining("#418"),
      ),
    );
    expect(screen.getByTestId("draft-publication-tag")).toHaveTextContent("Published #418");
    expect(screen.queryByTestId("draft-publication-band")).toBeNull();
    expect(calls("set_draft_status")).toHaveLength(0);
    expect(visibleToasts()).toHaveLength(0);
  });

  it("NAW-FR-PTYO: while the update runs, the tag names the same issue", async () => {
    stub({ publication: published() });
    let release: (o: IssueUpdateOutcome) => void = () => {};
    onUpdate(
      () =>
        new Promise<IssueUpdateOutcome>((resolve) => {
          release = resolve;
        }),
    );
    renderWorkspace();
    await screen.findByTestId("draft-publication-tag");

    await userEvent.click(await updateEntry());
    await waitFor(() => expect(calls("update_draft_on_github")).toHaveLength(1));
    const tag = screen.getByTestId("draft-publication-tag");
    expect(tag).toHaveAttribute("data-state", "published");
    expect(tag).toHaveTextContent("Published #418");

    await act(async () => release(await updated()));
  });

  it("NAW-FR-KMLN: an unsaved prompt is written before the update", async () => {
    stub({ publication: published() });
    onUpdate(updated);
    // Every write is held, so the update can start only after the write that
    // the action itself makes has landed, whatever an autosave does.
    const withUpdate = invokeMock.getMockImplementation() as (
      cmd: string,
      args?: unknown,
    ) => Promise<unknown>;
    const pending: (() => void)[] = [];
    invokeMock.mockImplementation((cmd: string, args?: unknown) => {
      if (cmd === "save_draft_file_contents") {
        return new Promise((resolve) => pending.push(() => resolve({ checksum: "c2" })));
      }
      return withUpdate(cmd, args);
    });
    renderWorkspace();
    await screen.findByTestId("draft-publication-tag");
    await editSource("A new paragraph.");

    await userEvent.click(await updateEntry());

    await waitFor(() => expect(calls("save_draft_file_contents").length).toBeGreaterThan(0));
    expect(calls("update_draft_on_github")).toHaveLength(0);
    await act(async () => {
      while (pending.length > 0) pending.shift()?.();
    });
    await waitFor(() => expect(calls("update_draft_on_github")).toHaveLength(1));
  });

  it("NAW-FR-HKYL: the entry is disabled with the read's reason while the draft is not publishable or an attempt stands", async () => {
    stub({
      publication: {
        ...publicationRefused(
          "attempt_in_progress",
          "A publication attempt for this draft is already in progress.",
        ),
        current: RECORD,
        history: [RECORD],
      },
    });
    renderWorkspace();
    await screen.findByTestId("draft-publication-tag");

    const entry = await updateEntry();
    expect(entry).toBeDisabled();
    expect(entry).toHaveAttribute(
      "title",
      "A publication attempt for this draft is already in progress.",
    );
  });

  it("NAW-FR-HKYL: the entry is disabled while its call is in flight, so one gesture is one update", async () => {
    stub({ publication: published() });
    let release: (o: IssueUpdateOutcome) => void = () => {};
    onUpdate(
      () =>
        new Promise<IssueUpdateOutcome>((resolve) => {
          release = resolve;
        }),
    );
    renderWorkspace();
    await screen.findByTestId("draft-publication-tag");

    const entry = await updateEntry();
    await act(async () => {
      entry.click();
      entry.click();
    });
    await waitFor(() => expect(calls("update_draft_on_github")).toHaveLength(1));
    const again = await updateEntry();
    expect(again).toBeDisabled();
    await userEvent.click(again);
    expect(calls("update_draft_on_github")).toHaveLength(1);

    await act(async () => release(await updated()));
  });

  it("NAW-FR-AQBE: a closed issue opens the dialog; Update closed issue invokes the update again with updateClosed true", async () => {
    stub({ publication: published() });
    onUpdate((updateClosed) =>
      updateClosed
        ? updated()
        : Promise.resolve({
            kind: "issueClosed",
            issueNumber: 418,
            issueUrl: RECORD.issueUrl,
          }),
    );
    renderWorkspace();
    await screen.findByTestId("draft-publication-tag");

    await userEvent.click(await updateEntry());
    const dialog = await screen.findByRole("dialog", { name: "Update closed issue" });
    expect(dialog).toHaveTextContent("Issue #418 in acme/widgets is closed.");
    expect(calls("update_draft_on_github")).toHaveLength(1);

    await userEvent.click(within(dialog).getByRole("button", { name: "Update closed issue" }));
    await waitFor(() => expect(calls("update_draft_on_github")).toHaveLength(2));
    expect(calls("update_draft_on_github")[1][1]).toEqual({ draftId: "d1", updateClosed: true });
    await waitFor(() =>
      expect(screen.queryByRole("dialog", { name: "Update closed issue" })).toBeNull(),
    );
    expect(visibleToasts()).toHaveLength(0);
  });

  it("NAW-FR-AQBE: Cancel, Escape, and the close control each dismiss the dialog, invoke nothing, and show no toast", async () => {
    stub({ publication: published() });
    onUpdate(() =>
      Promise.resolve({ kind: "issueClosed", issueNumber: 418, issueUrl: RECORD.issueUrl }),
    );
    renderWorkspace();
    await screen.findByTestId("draft-publication-tag");

    const dismissals: ((dialog: HTMLElement) => Promise<unknown>)[] = [
      (d) => userEvent.click(within(d).getByRole("button", { name: "Cancel" })),
      () => userEvent.keyboard("{Escape}"),
      (d) => userEvent.click(within(d).getByRole("button", { name: "Close" })),
    ];
    for (const [index, dismiss] of dismissals.entries()) {
      await userEvent.click(await updateEntry());
      const dialog = await screen.findByRole("dialog", { name: "Update closed issue" });
      await dismiss(dialog);
      await waitFor(() =>
        expect(screen.queryByRole("dialog", { name: "Update closed issue" })).toBeNull(),
      );
      expect(calls("update_draft_on_github")).toHaveLength(index + 1);
    }
    expect(calls("update_draft_on_github").every(([, a]) => !a.updateClosed)).toBe(true);
    expect(visibleToasts()).toHaveLength(0);
  });

  it("NAW-FR-KMFT, NTF-FR-RMJT: a failed update raises the Error toast with the displayable error, and the tag and the band do not change", async () => {
    stub({ publication: published() });
    onUpdate(() => Promise.reject(new Error("issue_not_found")));
    renderWorkspace();
    await screen.findByTestId("draft-publication-tag");

    await userEvent.click(await updateEntry());

    await waitFor(() => expect(visibleToasts()).toHaveLength(1));
    const [toast] = visibleToasts();
    expect(toast).toMatchObject({
      key: UPDATE_FAILED_KEY,
      level: "Error",
      title: UPDATE_FAILED_TITLE,
      address: null,
    });
    expect(toast.title).toBe("Issue not updated");
    expect(toast.body).toContain("GitHub does not find the published issue.");
    expect(screen.getByTestId("draft-publication-tag")).toHaveAttribute("data-state", "published");
    expect(screen.queryByTestId("draft-publication-band")).toBeNull();
  });

  it("NTF-FR-RMJT: a second failed update replaces the toast that shows", async () => {
    stub({ publication: published() });
    let code = "issue_moved";
    onUpdate(() => Promise.reject(new Error(code)));
    renderWorkspace();
    await screen.findByTestId("draft-publication-tag");

    await userEvent.click(await updateEntry());
    await waitFor(() => expect(visibleToasts()).toHaveLength(1));
    code = "github_unreachable";
    await userEvent.click(await updateEntry());
    await waitFor(() => expect(visibleToasts()[0]?.body).toContain("GitHub could not be reached."));
    expect(visibleToasts()).toHaveLength(1);
  });

  it("NAW-FR-UDPM, NAW-FR-TSQE: a deliberate re-publication through the recovery choice leaves the tag naming the new record", async () => {
    const first = publicationRecord(402, "pub-a", "2026-09-04T16:20:00.000Z");
    const second = publicationRecord(418, "pub-b", "2026-09-12T10:04:00.000Z");
    let view: DraftPublicationView = {
      current: first,
      history: [first],
      attempt: {
        marker: "pub-b",
        remoteName: "origin",
        remoteUrl: "github.com/acme/widgets",
        repositoryOwner: "acme",
        repositoryName: "widgets",
        state: "awaiting_choice",
        startedAt: "2026-09-12T10:00:00.000Z",
        updatedAt: "2026-09-12T10:00:00.000Z",
      },
      eligibility: noPublication().eligibility,
    };
    stub({ publication: () => view });
    const base = invokeMock.getMockImplementation() as (
      cmd: string,
      args?: unknown,
    ) => Promise<unknown>;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "retry_draft_publication") {
        return {
          kind: "recoveryRequired",
          issueNumber: 417,
          issueUrl: "https://github.com/acme/widgets/issues/417",
          marker: "pub-b",
          mismatches: ["body"],
        } satisfies PublicationOutcome;
      }
      if (cmd === "resolve_draft_publication_conflict") {
        view = { ...view, current: second, history: [second, first], attempt: null };
        return { kind: "published", record: second } satisfies PublicationOutcome;
      }
      return base(cmd, args);
    });

    renderWorkspace();
    // NAW-FR-UDPM: the action control offers no second publication.
    const menu = await openActions();
    expect(within(menu).queryByRole("menuitem", { name: /Publish to GitHub/ })).toBeNull();
    expect(within(menu).getByRole("menuitem", { name: /Update on GitHub/ })).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");

    const band = await screen.findByTestId("draft-publication-band");
    await userEvent.click(within(band).getByRole("button", { name: "Recover" }));
    const dialog = await screen.findByRole("dialog", { name: "Recover publication" });
    await userEvent.click(within(dialog).getByRole("button", { name: "Publish as a new issue" }));
    await waitFor(() => expect(calls("resolve_draft_publication_conflict")).toHaveLength(1));

    // NAW-FR-TSQE: the tag follows the one read rather than the tab holding a
    // copy of what it published.
    await waitFor(() =>
      expect(screen.getByTestId("draft-publication-tag")).toHaveTextContent(
        "Published #418",
      ),
    );
    // The earlier record is read in Draft Information; the tag names the
    // current publication alone.
    expect(screen.queryByText(/#402/)).toBeNull();
    expect(calls("publish_draft_to_github")).toHaveLength(0);
  });
});
