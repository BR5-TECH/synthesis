/**
 * The split of a change into new changes, in its tab
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-BGDN,
 * OCW-FR-ANKT, OCW-FR-TZRT, OCW-FR-IJAI).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
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

import { render } from "@testing-library/react";

import { CommentAttachmentList } from "../CommentAttachments";
import { configureOpenSpecChangeRoutes } from "../../state/openspec/changeRoutes";
import { visibleToasts } from "../../state/toasts";
import type { GraduationRun, NewChangeDecisionOutcome } from "../../types";
import {
  CHANGE,
  STREAM_ID,
  changeBackend,
  drawChange,
  newChangeProposal,
  resetChangeWorkspaceStores,
  splitDiscussion,
  wireChangeBackend,
  type ChangeBackend,
} from "../../test/changeWorkspaceFixtures";

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

function emit(event: string, payload: unknown) {
  for (const cb of listeners.get(event) ?? []) cb({ payload });
}

let backend: ChangeBackend;
const open = vi.fn();

beforeEach(() => {
  resetChangeWorkspaceStores();
  listeners.clear();
  invokeMock.mockReset();
  open.mockReset();
  configureOpenSpecChangeRoutes({ open });
  backend = changeBackend({
    proposals: [],
    discussions: [splitDiscussion()],
    newChanges: [newChangeProposal()],
    split: { setId: "set-1", accepted: [] },
  });
  wireChangeBackend(invokeMock, backend);
});
afterEach(() => {
  cleanup();
  configureOpenSpecChangeRoutes(null);
  expect(backend.unknown).toEqual([]);
});

/** An outcome of a decision on proposal n1. */
function outcome(over: Partial<NewChangeDecisionOutcome> = {}): NewChangeDecisionOutcome {
  return {
    proposal: newChangeProposal({ state: "accepted", createdChange: "add-logout-ui", decidedAt: "2026-01-03T00:00:00Z" }),
    committed: true,
    commentId: "t3-c3",
    originKind: "change_discussion",
    ...over,
  };
}

async function card() {
  drawChange();
  return screen.findByTestId("new-change-card");
}

describe("the new-change card (OCW-FR-BGDN)", () => {
  it("OCW-FR-BGDN: a comment with a new_change_proposal renders a card with the name, the position and total, the paths, and the state", async () => {
    const shown = await card();
    expect(shown).toHaveTextContent("Part 1 of 2 · add-logout-ui");
    expect(shown).toHaveTextContent("Waiting for you");
    const paths = within(shown).getByRole("list", { name: "Proposed files" });
    expect(within(paths).getAllByRole("listitem").map((li) => li.textContent)).toEqual([
      "proposal.md",
      "specs/ui/spec.md",
    ]);
    expect(calls("list_new_change_proposals")[0][1]).toEqual({ streamId: STREAM_ID, change: CHANGE });
    // The reference is drawn as the card alone, not as a chip of the comment.
    expect(screen.queryByTestId("comment-attachment-new-change")).toBeNull();
  });

  it("OCW-FR-BGDN: Accept invokes accept new change proposal with the feedback, and the card reads the decision", async () => {
    const user = userEvent.setup();
    backend.decideNew = () => outcome();
    const shown = await card();
    await user.type(within(shown).getByRole("textbox", { name: "Feedback for the agent (optional)" }), "Good split");
    await user.click(within(shown).getByRole("button", { name: "Accept" }));
    await waitFor(() => expect(calls("accept_new_change_proposal")).toHaveLength(1));
    expect(calls("accept_new_change_proposal")[0][1]).toEqual({ proposalId: "n1", feedback: "Good split" });
    await waitFor(() => expect(screen.getByTestId("new-change-card")).toHaveAttribute("data-state", "accepted"));
    expect(screen.getByTestId("new-change-card")).toHaveTextContent("Accepted");
    expect(within(screen.getByTestId("new-change-card")).queryByRole("button", { name: "Accept" })).toBeNull();
  });

  it("OCW-FR-BGDN: Reject invokes reject new change proposal with no feedback", async () => {
    const user = userEvent.setup();
    backend.decideNew = () =>
      outcome({ proposal: newChangeProposal({ state: "rejected" }), committed: false });
    const shown = await card();
    await user.click(within(shown).getByRole("button", { name: "Reject" }));
    await waitFor(() => expect(calls("reject_new_change_proposal")).toHaveLength(1));
    expect(calls("reject_new_change_proposal")[0][1]).toEqual({ proposalId: "n1", feedback: null });
    await waitFor(() => expect(screen.getByTestId("new-change-card")).toHaveTextContent("Rejected"));
    expect(calls("accept_new_change_proposal")).toHaveLength(0);
  });

  it("OCW-FR-BGDN: Discuss opens a discussion whose opening comment carries one new_change_subject", async () => {
    const user = userEvent.setup();
    const shown = await card();
    await user.click(within(shown).getByRole("button", { name: "Discuss →" }));
    expect(await screen.findByText("This begins a discussion about part 1 of 2 of the split.")).toBeInTheDocument();
    const composer = screen.getByRole("textbox", { name: "Discuss this change" });
    expect((composer as HTMLTextAreaElement).value).toBe("@arch ");
    await user.type(composer, "is the UI part complete?");
    await user.keyboard("{Control>}{Enter}{/Control}");
    await waitFor(() => expect(calls("open_discussion")).toHaveLength(1));
    expect(calls("open_discussion")[0][1]).toMatchObject({
      attachments: [{ kind: "newChangeSubject", proposalId: "n1", streamId: STREAM_ID, change: CHANGE }],
    });
    // Opening a discussion decides nothing.
    expect(calls("accept_new_change_proposal")).toHaveLength(0);
    expect(calls("reject_new_change_proposal")).toHaveLength(0);
  });

  it("OCW-FR-BGDN: View files shows the proposed files from load new change proposal files, read-only", async () => {
    const user = userEvent.setup();
    const shown = await card();
    await user.click(within(shown).getByRole("button", { name: "View files" }));
    const files = await screen.findByTestId("new-change-files");
    expect(calls("load_new_change_proposal_files")[0][1]).toEqual({ proposalId: "n1" });
    expect(files).toHaveTextContent("proposal.md");
    expect(files).toHaveTextContent("The UI part.");
    expect(within(files).queryByRole("textbox")).toBeNull();
    await user.click(within(shown).getByRole("button", { name: "Hide files" }));
    expect(screen.queryByTestId("new-change-files")).toBeNull();
  });

  it("OCW-FR-BGDN: a refusal is stated on the card and the proposal stays pending", async () => {
    const user = userEvent.setup();
    backend.decideNew = () => ({ reject: "openspec_change_exists" });
    const shown = await card();
    await user.click(within(shown).getByRole("button", { name: "Accept" }));
    expect(await screen.findByTestId("new-change-refusal")).toHaveTextContent(
      "A change of this name is already on the stream",
    );
    expect(screen.getByTestId("new-change-card")).toHaveAttribute("data-state", "pending");
  });

  it("OCW-FR-BGDN: the card follows new change proposals changed for its change", async () => {
    await card();
    await waitFor(() => expect(listeners.get("new-change-proposals-changed")?.size ?? 0).toBeGreaterThan(0));
    backend.newChanges = [newChangeProposal({ state: "withdrawn" })];
    act(() => {
      emit("new-change-proposals-changed", {
        streamId: STREAM_ID,
        change: CHANGE,
        proposal: newChangeProposal({ state: "withdrawn" }),
        cause: "updated",
      });
    });
    await waitFor(() => expect(screen.getByTestId("new-change-card")).toHaveTextContent("Withdrawn"));
  });
});

describe("an acceptance (OCW-FR-IJAI)", () => {
  it("OCW-FR-IJAI: an accepted proposal's card names the created change and opens its tab", async () => {
    const user = userEvent.setup();
    backend.newChanges = [newChangeProposal({ state: "accepted", createdChange: "add-logout-ui" })];
    const shown = await card();
    expect(shown).toHaveTextContent("Created change add-logout-ui");
    await user.click(within(shown).getByRole("button", { name: "add-logout-ui" }));
    expect(open).toHaveBeenCalledWith({ kind: "stream", streamId: STREAM_ID }, "add-logout-ui", {});
  });

  it("OCW-FR-IJAI: an acceptance whose commit fails raises an Error toast whose Retry invokes complete new change decision", async () => {
    const user = userEvent.setup();
    backend.decideNew = (cmd) =>
      cmd === "complete_new_change_decision"
        ? outcome()
        : outcome({
            committed: false,
            proposal: newChangeProposal({ state: "accepted", createdChange: "add-logout-ui", commitOwed: true }),
          });
    const shown = await card();
    await user.click(within(shown).getByRole("button", { name: "Accept" }));
    await waitFor(() =>
      expect(visibleToasts().some((t) => t.level === "Error" && t.title.includes("add-logout-ui"))).toBe(true),
    );
    const toast = visibleToasts().find((t) => t.level === "Error")!;
    expect(toast.link?.label).toBe("Retry");
    act(() => toast.link!.onActivate());
    await waitFor(() => expect(calls("complete_new_change_decision")).toHaveLength(1));
    expect(calls("complete_new_change_decision")[0][1]).toEqual({ proposalId: "n1" });
    await waitFor(() => expect(visibleToasts().some((t) => t.level === "Info")).toBe(true));
  });

  it("OCW-FR-IJAI: a refusal stream_busy is stated on the card with the name of the run that holds the stream", async () => {
    const user = userEvent.setup();
    backend.decideNew = () => ({ reject: "stream_busy" });
    backend.runs = [
      { id: "g-9", streamId: STREAM_ID, state: "working", input: { draftName: "Logout run" } },
    ] as unknown as GraduationRun[];
    const shown = await card();
    await user.click(within(shown).getByRole("button", { name: "Accept" }));
    const refusal = await screen.findByTestId("new-change-refusal");
    expect(refusal).toHaveTextContent("holds this stream, so the change was not created");
    expect(refusal.textContent).toMatch(/The run “.+”/);
  });
});

describe("Confirm split (OCW-FR-ANKT)", () => {
  it("OCW-FR-ANKT: no Confirm split while a proposal is pending, or while none is accepted", async () => {
    await card();
    expect(screen.queryByRole("button", { name: "Confirm split" })).toBeNull();
    cleanup();
    resetChangeWorkspaceStores();
    backend.newChanges = [newChangeProposal({ state: "rejected" })];
    drawChange();
    await screen.findByTestId("new-change-card");
    await waitFor(() => expect(screen.getByTestId("new-change-card")).toHaveTextContent("Rejected"));
    expect(screen.queryByRole("button", { name: "Confirm split" })).toBeNull();
  });

  it("OCW-FR-ANKT: one accepted proposal and one pending proposal show no Confirm split", async () => {
    backend.newChanges = [
      newChangeProposal({ state: "accepted", createdChange: "add-logout-ui", decidedAt: "2026-01-03T00:00:00Z" }),
      newChangeProposal({ id: "n2", name: "add-logout-api", position: 2, createdAt: "2026-01-03T00:00:00Z" }),
    ];
    backend.split = { setId: "set-1", accepted: ["add-logout-ui"] };
    drawChange();
    // The card of n1 reads the same list that holds the pending n2, so the
    // bar had its data when the card shows the acceptance.
    const shown = await screen.findByTestId("new-change-card");
    await waitFor(() => expect(shown).toHaveAttribute("data-state", "accepted"));
    expect(calls("list_new_change_proposals").length).toBeGreaterThan(0);
    expect(screen.queryByTestId("openspec-split-bar")).toBeNull();
    expect(screen.queryByRole("button", { name: "Confirm split" })).toBeNull();
  });

  it("OCW-FR-ANKT: with an accepted proposal and none pending, Confirm split asks for a confirmation that names the accepted changes, invokes confirm change split, and gives the focus to the reading pane", async () => {
    const user = userEvent.setup();
    backend.newChanges = [
      newChangeProposal({ state: "accepted", createdChange: "add-logout-ui", decidedAt: "2026-01-03T00:00:00Z" }),
      newChangeProposal({ id: "n2", name: "add-logout-api", position: 2, state: "accepted", createdChange: "add-logout-api", decidedAt: "2026-01-04T00:00:00Z", createdAt: "2026-01-03T00:00:00Z" }),
    ];
    backend.split = { setId: "set-1", accepted: ["add-logout-ui", "add-logout-api"] };
    backend.confirm = () => {
      backend.split = { setId: "set-1", accepted: ["add-logout-ui", "add-logout-api"], confirmedAt: "2026-01-05T00:00:00Z" };
      return backend.split;
    };
    drawChange();
    const bar = await screen.findByTestId("openspec-split-bar");
    await user.click(within(bar).getByRole("button", { name: "Confirm split" }));
    const dialog = screen.getByRole("dialog", { name: "Confirm split" });
    expect(within(dialog).getByTestId("openspec-confirm-split-statement")).toHaveTextContent(
      "add-logout-ui, add-logout-api",
    );
    expect(calls("confirm_change_split")).toHaveLength(0);
    await user.click(within(dialog).getByRole("button", { name: "Confirm split" }));
    await waitFor(() => expect(calls("confirm_change_split")).toHaveLength(1));
    expect(calls("confirm_change_split")[0][1]).toEqual({ streamId: STREAM_ID, change: CHANGE });
    await waitFor(() => expect(screen.queryByTestId("openspec-split-bar")).toBeNull());
    expect(screen.getByTestId("openspec-split-marker")).toBeInTheDocument();
    // The bar went away with the focus, so the focus goes to the reading pane.
    await waitFor(() => expect(document.activeElement).toBe(screen.getByTestId("openspec-reading")));
  });

  it("OCW-FR-ANKT: Cancel invokes nothing, and a refused confirmation is stated with the focus back on Confirm split", async () => {
    const user = userEvent.setup();
    backend.newChanges = [newChangeProposal({ state: "accepted", createdChange: "add-logout-ui" })];
    backend.split = { setId: "set-1", accepted: ["add-logout-ui"] };
    backend.confirm = () => ({ reject: "new_change_proposal_pending" });
    drawChange();
    const bar = await screen.findByTestId("openspec-split-bar");
    await user.click(within(bar).getByRole("button", { name: "Confirm split" }));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(calls("confirm_change_split")).toHaveLength(0);
    await user.click(within(bar).getByRole("button", { name: "Confirm split" }));
    await user.click(within(screen.getByRole("dialog")).getByRole("button", { name: "Confirm split" }));
    expect(await within(bar).findByRole("alert")).toHaveTextContent("still waiting for a decision");
    await waitFor(() =>
      expect(document.activeElement).toBe(within(bar).getByRole("button", { name: "Confirm split" })),
    );
  });
});

describe("the split marker (OCW-FR-TZRT)", () => {
  it("OCW-FR-TZRT: a confirmed split shows a header marker naming its changes, and each name opens that change on the same stream", async () => {
    const user = userEvent.setup();
    backend.newChanges = [newChangeProposal({ state: "accepted", createdChange: "add-logout-ui" })];
    backend.split = { setId: "set-1", accepted: ["add-logout-ui", "add-logout-api"], confirmedAt: "2026-01-05T00:00:00Z" };
    drawChange();
    const marker = await screen.findByTestId("openspec-split-marker");
    expect(marker.closest("header")).not.toBeNull();
    expect(marker).toHaveTextContent("Split into add-logout-ui, add-logout-api");
    await user.click(within(marker).getByRole("button", { name: "add-logout-api" }));
    expect(open).toHaveBeenCalledWith({ kind: "stream", streamId: STREAM_ID }, "add-logout-api", {});
    // A confirmed split offers no Confirm split and no decision.
    expect(screen.queryByTestId("openspec-split-bar")).toBeNull();
  });

  it("OCW-FR-TZRT: a split that is not confirmed shows no marker", async () => {
    await card();
    expect(screen.queryByTestId("openspec-split-marker")).toBeNull();
  });
});

describe("a reference outside the change (OCW-FR-BGDN)", () => {
  it("OCW-FR-BGDN: a new_change_proposal on a comment elsewhere is a control that names the proposal and opens the change", async () => {
    const user = userEvent.setup();
    render(
      <CommentAttachmentList
        threadId="t3"
        attachments={[{ kind: "newChangeProposal", proposalId: "n1", streamId: STREAM_ID, change: CHANGE }]}
      />,
    );
    const control = await screen.findByTestId("comment-attachment-new-change");
    await waitFor(() => expect(control).toHaveTextContent("Part 1 of 2 · add-logout-ui"));
    await user.click(control);
    expect(open).toHaveBeenCalledWith({ kind: "stream", streamId: STREAM_ID }, CHANGE, {});
  });
});
