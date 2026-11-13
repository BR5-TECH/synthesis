/**
 * CMT-FR-48 / OCW-FR-COLR: the control a `change_proposal` reference renders,
 * on a card and as a row of the discussion column.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { CommentAttachmentList } from "./CommentAttachments";
import { ChangeRow } from "./DraftDiscussion/stream/ChangeRow";
import { configureOpenSpecChangeRoutes } from "../state/openspec/changeRoutes";
import { draftDiscussionState } from "../state/draftDiscussion";
import { changeReviewKey } from "../state/openspec/changeProposals";
import {
  CHANGE,
  STREAM_ID,
  changeProposal,
  resetChangeWorkspaceStores,
} from "../test/changeWorkspaceFixtures";
import type { Attachment, ChangeEditProposal } from "../types";

const REFERENCE: Extract<Attachment, { kind: "changeProposal" }> = {
  kind: "changeProposal",
  proposalId: "p1",
  streamId: STREAM_ID,
  change: CHANGE,
};

let proposals: ChangeEditProposal[];

beforeEach(() => {
  resetChangeWorkspaceStores();
  proposals = [changeProposal()];
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "append_log_records") return undefined;
    if (cmd === "list_change_edit_proposals") return proposals;
    throw new Error(`unexpected command ${cmd}`);
  });
});
afterEach(() => {
  cleanup();
  configureOpenSpecChangeRoutes(null);
});

describe("a change_proposal reference", () => {
  it("CMT-FR-48, OCW-FR-COLR: renders a control that opens the change on its stream at the first undecided hunk", async () => {
    const open = vi.fn();
    configureOpenSpecChangeRoutes({ open });
    render(<CommentAttachmentList threadId="t1" attachments={[REFERENCE]} />);
    const control = await screen.findByTestId("comment-attachment-change-proposal");
    await waitFor(() => expect(control).toHaveAttribute("data-state", "pending"));
    expect(control).toHaveTextContent(CHANGE);
    expect(control).toHaveTextContent("2 changes");
    expect(control).toHaveTextContent("Review proposed change");
    await userEvent.click(control);
    expect(open).toHaveBeenCalledWith({ kind: "stream", streamId: STREAM_ID }, CHANGE, {
      file: "proposal.md",
    });
    // The review of the change is put on that hunk, and asked to show it.
    expect(draftDiscussionState(changeReviewKey(STREAM_ID, CHANGE))).toMatchObject({
      focusedHunkId: "h1",
      hunkReveal: 1,
    });
  });

  it("CTA-FR-UKIG: a reference whose proposal two readings do not hold says it is gone", async () => {
    proposals = [];
    render(<CommentAttachmentList threadId="t1" attachments={[REFERENCE]} />);
    const control = await screen.findByTestId("comment-attachment-change-proposal");
    await waitFor(() => expect(control).toHaveAttribute("data-state", "missing"));
    expect(control).toBeDisabled();
    expect(invokeMock.mock.calls.filter((c) => c[0] === "list_change_edit_proposals")).toHaveLength(2);
  });

  it("DDS-FR-XHRB: as a row of the column it names the change, the count, and the state", async () => {
    render(<ChangeRow reference={REFERENCE} createdAt="2026-01-01T00:00:00Z" />);
    const row = await screen.findByTestId("dds-change-row");
    await waitFor(() => expect(row).toHaveAttribute("data-state", "pending"));
    expect(row).toHaveAccessibleName(`${CHANGE}, 2 changes, pending`);
  });
});
