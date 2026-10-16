/**
 * The draft column's part in a discussion about one change, and the menu its
 * header holds (`../../specifications/ui/DDS-draft-discussion.md` DDS-FR-RHKW,
 * DDS-FR-MZQJ; `DCR-draft-change-review.md` DCR-FR-MTFD).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import { resetDraftDiscussions } from "../state/draftDiscussion";
import { resetLayoutPreferencesCache } from "../state/layoutPreferences";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { forgetThreads, heldThreads } from "../state/conversationThreads";
import { resetLogBufferForTest } from "../logging";
import { DRAFT, columnElement, discussionSaying } from "../test/draftDiscussionFixtures";
import type { ProposalHunkReference } from "../types";

const reference: ProposalHunkReference = {
  kind: "proposalHunk",
  proposalId: "p1",
  hunkId: "h2",
  draftId: DRAFT,
  path: "Spec.md",
};

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) =>
    cmd === "append_log_records" ? Promise.resolve() : Promise.resolve([]),
  );
  resetDraftDiscussions();
  resetLayoutPreferencesCache();
  clearAllDiscussionSessions();
  resetLogBufferForTest();
  for (const d of heldThreads()) forgetThreads([d.id]);
});

afterEach(cleanup);

describe("DDS-FR-RHKW: the composer of a discussion about one change", () => {
  it("DDS-FR-RHKW, DCR-FR-MTFD: names the change and posts a new discussion carrying its reference", async () => {
    const onOpenDiscussion = vi.fn(async () => discussionSaying("disc-9", ["x"]));
    render(
      columnElement({
        onOpenDiscussion,
        openingHunk: { reference, position: 2, total: 3 },
      }),
    );

    // DDS-FR-RHKW: the composer names its change, whichever discussion the
    // column was reading.
    expect(screen.getByTestId("draft-open-discussion")).toHaveTextContent(
      "This begins a discussion about change 2 of 3.",
    );
    const field = await screen.findByRole("textbox", { name: "Discuss this draft" });
    fireEvent.change(field, { target: { value: "@arch why this word?" } });
    await userEvent.click(screen.getByRole("button", { name: "Post" }));

    // DCR-FR-MTFD: a new discussion record, whole-target, with one
    // `proposal_hunk` attachment naming the change.
    await waitFor(() => expect(onOpenDiscussion).toHaveBeenCalledTimes(1));
    expect(onOpenDiscussion).toHaveBeenCalledWith({
      target: { kind: "draft", draftId: DRAFT },
      fragmentTarget: null,
      body: "@arch why this word?",
      attachments: [reference],
    });
  });

  it("DDS-FR-RHKW: discarding the composer gives the opening back", async () => {
    const onCancelHunk = vi.fn();
    render(
      columnElement({
        openingHunk: { reference, position: 2, total: 3 },
        onCancelHunk,
      }),
    );
    await userEvent.click(await screen.findByRole("button", { name: "Cancel" }));
    expect(onCancelHunk).toHaveBeenCalledTimes(1);
  });
});

describe("DDS-FR-MZQJ: the discussion's menu in the column header", () => {
  it("DDS-FR-MZQJ, CMT-FR-15: the header holds the one entry, and no message holds a menu", async () => {
    const onSetResolved = vi.fn(async () => {});
    render(columnElement({ onSetResolved }));
    const header = screen.getByTestId("discussion-header-actions");
    const menu = await within(header).findByRole("button", { name: "Thread actions for disc-1" });
    expect(screen.getAllByRole("button", { name: "Thread actions for disc-1" })).toHaveLength(1);

    await userEvent.click(menu);
    await userEvent.click(screen.getByRole("menuitem", { name: "Mark resolved" }));
    expect(onSetResolved).toHaveBeenCalledWith("disc-1", true);
  });

  it("DDS-FR-MZQJ, CMT-FR-16: a resolved discussion says so in the header, offers Reopen, and has no composer", async () => {
    const settled = { ...discussionSaying("disc-1", ["opening"]), resolved: true };
    render(columnElement({ discussions: [settled], selectedThreadId: "disc-1" }));

    expect(await screen.findByTestId("discussion-resolved-state")).toHaveTextContent("resolved");
    expect(screen.queryByRole("textbox", { name: "Reply to thread disc-1" })).toBeNull();
    const header = screen.getByTestId("discussion-header-actions");
    await userEvent.click(
      await within(header).findByRole("button", { name: "Thread actions for disc-1" }),
    );
    expect(screen.getAllByRole("menuitem").map((i) => i.textContent)).toEqual([
      "Reopen discussion",
    ]);
  });
});
