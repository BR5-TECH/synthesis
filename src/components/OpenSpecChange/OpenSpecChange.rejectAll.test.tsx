/**
 * The review bar of a change and its Reject all
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-EJGH).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import type { GraduationRun } from "../../types";
import {
  changeBackend,
  changeProposal,
  drawChange,
  resetChangeWorkspaceStores,
  wireChangeBackend,
  type ChangeBackend,
} from "../../test/changeWorkspaceFixtures";

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

let backend: ChangeBackend;

beforeEach(() => {
  resetChangeWorkspaceStores();
  invokeMock.mockReset();
  backend = changeBackend();
  wireChangeBackend(invokeMock, backend);
});
afterEach(() => {
  cleanup();
  expect(backend.unknown).toEqual([]);
});

const bar = () => screen.findByTestId("openspec-review-bar");

describe("the review bar (OCW-FR-EJGH)", () => {
  it("OCW-FR-EJGH: while a proposal is pending, the bar above the reading pane states the undecided hunks across the change", async () => {
    drawChange();
    const shown = await bar();
    // One hunk on proposal.md and one on a new spec file: two artifacts.
    expect(within(shown).getByTestId("openspec-review-count")).toHaveTextContent("2 undecided changes in this change.");
    const reading = screen.getByTestId("openspec-reading");
    expect(shown.compareDocumentPosition(reading) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });

  it("OCW-FR-EJGH: a held hunk is still undecided and a decided one is not counted", async () => {
    backend.proposals = [changeProposal(["discussing", "accepted"])];
    drawChange();
    expect(within(await bar()).getByTestId("openspec-review-count")).toHaveTextContent("1 undecided change in this change.");
  });

  it("OCW-FR-EJGH: a change with no pending proposal shows no bar", async () => {
    backend.proposals = [changeProposal(["accepted", "rejected"], { state: "accepted" })];
    drawChange();
    await screen.findAllByTestId("openspec-artifact");
    await waitFor(() => expect(calls("list_change_edit_proposals").length).toBeGreaterThan(0));
    expect(screen.queryByTestId("openspec-review-bar")).toBeNull();
  });

  it("OCW-FR-EJGH: Reject all asks for a confirmation that names the count, then declines and reads the change and the proposals again", async () => {
    const user = userEvent.setup();
    backend.decide = () => {
      backend.proposals = [changeProposal(["rejected", "rejected"], { state: "rejected" })];
      return {
        proposal: backend.proposals[0],
        committed: false,
        resolved: true,
        originKind: "change_discussion",
      };
    };
    drawChange();
    const shown = await bar();
    await waitFor(() => expect(calls("read_openspec_change")).toHaveLength(1));
    const proposalsRead = calls("list_change_edit_proposals").length;
    await user.click(within(shown).getByRole("button", { name: "Reject all" }));
    const dialog = screen.getByRole("dialog", { name: "Reject all" });
    expect(within(dialog).getByTestId("openspec-reject-all-statement")).toHaveTextContent(
      "Reject the 2 undecided changes of this proposal?",
    );
    expect(calls("decline_change_edit_proposal")).toHaveLength(0);
    await user.click(within(dialog).getByRole("button", { name: "Reject all" }));
    await waitFor(() => expect(calls("decline_change_edit_proposal")).toHaveLength(1));
    expect(calls("decline_change_edit_proposal")[0][1]).toEqual({ proposalId: "p1" });
    await waitFor(() => expect(calls("read_openspec_change")).toHaveLength(2));
    await waitFor(() => expect(calls("list_change_edit_proposals").length).toBeGreaterThan(proposalsRead));
    await waitFor(() => expect(screen.queryByTestId("openspec-review-bar")).toBeNull());
  });

  it("OCW-FR-EJGH: after a confirmed Reject all the bar goes away and the focus goes to the reading pane, not to the document body", async () => {
    const user = userEvent.setup();
    backend.decide = () => {
      backend.proposals = [changeProposal(["rejected", "rejected"], { state: "rejected" })];
      return {
        proposal: backend.proposals[0],
        committed: false,
        resolved: true,
        originKind: "change_discussion",
      };
    };
    drawChange();
    const shown = await bar();
    await user.click(within(shown).getByRole("button", { name: "Reject all" }));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Reject all" }));
    await waitFor(() => expect(screen.queryByTestId("openspec-review-bar")).toBeNull());
    await waitFor(() => expect(document.activeElement).toBe(screen.getByTestId("openspec-reading")));
  });

  it("OCW-FR-EJGH: Cancel declines nothing", async () => {
    const user = userEvent.setup();
    drawChange();
    const shown = await bar();
    await user.click(within(shown).getByRole("button", { name: "Reject all" }));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(calls("decline_change_edit_proposal")).toHaveLength(0);
    expect(within(shown).getByRole("button", { name: "Reject all" })).toBeEnabled();
  });

  it("OCW-FR-EJGH: a refused Reject all is stated in the bar, with the run that holds the stream, and the focus goes back to Reject all", async () => {
    const user = userEvent.setup();
    backend.decide = () => ({ reject: "stream_busy" });
    backend.runs = [
      { id: "g-9", streamId: "s-1", state: "reviewing", input: { draftName: "Logout run" } },
    ] as unknown as GraduationRun[];
    drawChange();
    const shown = await bar();
    await user.click(within(shown).getByRole("button", { name: "Reject all" }));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Reject all" }));
    expect(await within(shown).findByRole("alert")).toHaveTextContent(/The run “.+” holds this stream/);
    expect(screen.getByTestId("openspec-review-bar")).toBeInTheDocument();
    // The bar stays, so the focus goes back to its Reject all.
    await waitFor(() =>
      expect(document.activeElement).toBe(within(shown).getByRole("button", { name: "Reject all" })),
    );
  });
});
