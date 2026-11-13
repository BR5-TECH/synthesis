/**
 * Deciding the edits an agent proposed to an OpenSpec change, in the Source of
 * its files (`../../../specifications/ui/OCW-openspec-change-workspace.md`
 * OCW-FR-ABUR, OCW-FR-COLR, OCW-FR-RRXG, OCW-FR-DTRX, OCW-FR-TAZJ,
 * OCW-FR-MLXJ, OCW-FR-UOKM).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, screen, waitFor, within } from "@testing-library/react";
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

/**
 * PCR chips as a stand-in. The real chip is placed from the rendered box of
 * its hunk, and jsdom renders no box. The stand-in calls the same handlers.
 * The real chip has no edit control: the edit is the field of the proposed
 * text in Source (`../Editor/SourceReview.tsx`), which is not a stand-in.
 */
vi.mock("../Editor/ReviewOverlay", async () => {
  const { createElement } = await import("react");
  type Review = import("../Editor/props").EditorReview;
  return {
    ReviewOverlay: ({ review }: { review: Review | undefined }) => {
      if (review === undefined || review.focused === null) return null;
      if (!review.hunks.some((h) => h.id === review.focused)) return null;
      const id = review.focused;
      return createElement(
        "div",
        { role: "group", "aria-label": `Actions for ${id}` },
        createElement("button", { type: "button", disabled: review.busy, onClick: () => review.onAccept(id) }, "Accept"),
        createElement("button", { type: "button", disabled: review.busy, onClick: () => review.onReject(id) }, "Reject"),
        createElement("button", { type: "button", disabled: review.busy, onClick: () => review.onDiscuss(id) }, "Discuss"),
      );
    },
  };
});

import { visibleToasts } from "../../state/toasts";
import { AUTOSAVE_DELAY_MS } from "../../state/writeSchedule";
import type { ChangeDecisionOutcome, GraduationRun } from "../../types";
import {
  AFTER,
  BEFORE,
  CHANGE,
  NEW_PATH,
  NEW_TEXT,
  PROPOSAL_TEXT,
  STREAM_ID,
  changeBackend,
  changeProposal,
  drawChange,
  resetChangeWorkspaceStores,
  wireChangeBackend,
  type ChangeBackend,
} from "../../test/changeWorkspaceFixtures";

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

function emit(event: string, payload: unknown) {
  for (const cb of listeners.get(event) ?? []) cb({ payload });
}

let backend: ChangeBackend;

beforeEach(() => {
  resetChangeWorkspaceStores();
  listeners.clear();
  invokeMock.mockReset();
  backend = changeBackend();
  wireChangeBackend(invokeMock, backend);
});
afterEach(() => {
  cleanup();
  expect(backend.unknown).toEqual([]);
});

const artifactButton = (id: string) =>
  screen.getAllByTestId("openspec-artifact").find((b) => b.textContent?.startsWith(id))!;
const fileArticle = (path: string) =>
  screen.getAllByTestId("openspec-file").find((a) => a.getAttribute("data-path") === path)!;

/** An outcome of a decision on proposal p1. */
function outcome(over: Partial<ChangeDecisionOutcome> = {}): ChangeDecisionOutcome {
  return {
    proposal: changeProposal(["accepted", "pending"]),
    committed: false,
    resolved: false,
    originKind: "change_discussion",
    ...over,
  };
}

/** Open the change at proposal.md, and wait until its hunk is drawn. */
async function openAtProposal() {
  drawChange({ file: "proposal.md" });
  await screen.findByTestId("source-review");
  return screen.findByRole("group", { name: "Actions for h1" });
}

describe("Rich and Source", () => {
  it("OCW-FR-ABUR: each file offers Rich and Source, Source is the text as stored, and the tab keeps the choice", async () => {
    backend.proposals = [];
    const first = drawChange({ file: "proposal.md" });
    const article = await waitFor(() => fileArticle("proposal.md"));
    const rich = within(article).getByRole("button", { name: "Rich" });
    const source = within(article).getByRole("button", { name: "Source" });
    // A file with no hunk opens in Rich.
    expect(rich).toHaveAttribute("aria-pressed", "true");
    expect(within(article).queryByTestId("openspec-source")).not.toBeInTheDocument();
    await userEvent.click(source);
    expect(within(article).getByTestId("openspec-source-text").textContent).toBe(PROPOSAL_TEXT);
    // The choice belongs to the tab, so a remount keeps it.
    first.view.unmount();
    drawChange({ file: "proposal.md" });
    const again = await waitFor(() => fileArticle("proposal.md"));
    expect(within(again).getByRole("button", { name: "Source" })).toHaveAttribute("aria-pressed", "true");
    expect(within(again).getByTestId("openspec-source-text").textContent).toBe(PROPOSAL_TEXT);
  });

  it("OCW-FR-ABUR: a delta file of specs offers Rich and Source in its capability head", async () => {
    backend.proposals = [];
    drawChange({ file: "specs/auth/spec.md" });
    const group = (await screen.findAllByTestId("openspec-capability"))[0];
    expect(within(group).getAllByTestId("openspec-delta").length).toBeGreaterThan(0);
    await userEvent.click(within(group).getByRole("button", { name: "Source" }));
    expect(within(group).queryByTestId("openspec-delta")).not.toBeInTheDocument();
    expect(within(group).getByTestId("openspec-source-text").textContent).toBe("## MODIFIED Requirements");
  });

  it("OCW-FR-UOKM: on a stream the Source text takes no edit, and no file offers Open in editor", async () => {
    backend.proposals = [];
    drawChange({ file: "proposal.md" });
    const article = await waitFor(() => fileArticle("proposal.md"));
    await userEvent.click(within(article).getByRole("button", { name: "Source" }));
    expect(within(article).queryByRole("textbox")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Open in editor" })).not.toBeInTheDocument();
  });
});

describe("the hunks of a pending proposal", () => {
  it("OCW-FR-COLR: a file with a hunk opens in Source with the hunk decorated, the chip offers accept, reject, and discuss, and the proposed text is a field to edit", async () => {
    const chip = await openAtProposal();
    const article = fileArticle("proposal.md");
    expect(within(article).getByRole("button", { name: "Source" })).toHaveAttribute("aria-pressed", "true");
    const struck = article.querySelector('.hunk--del[data-hunk="h1"]');
    expect(struck?.textContent).toBe(BEFORE);
    // Edit: the proposed text is a field of its own.
    expect(within(article).getByRole("textbox", { name: /Proposed text of Change 1 of 2/ })).toHaveValue(AFTER);
    for (const name of ["Accept", "Reject", "Discuss"]) {
      expect(within(chip).getByRole("button", { name })).toBeEnabled();
    }
    expect(calls("load_change_edit_proposal_hunks")[0][1]).toEqual({ proposalId: "p1" });
  });

  it("OCW-FR-COLR, CEP-FR-UCVO: an edit of the proposed text writes through edit change edit hunk after the rest, and writes nothing into the file", async () => {
    await openAtProposal();
    const field = within(fileArticle("proposal.md")).getByRole("textbox", {
      name: /Proposed text of Change 1 of 2/,
    });
    const edited = "Users cannot sign off.";
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    try {
      fireEvent.change(field, { target: { value: edited } });
      // The write rests first, so a burst of typing is one write.
      expect(calls("edit_change_edit_hunk")).toHaveLength(0);
      await act(async () => {
        await vi.advanceTimersByTimeAsync(AUTOSAVE_DELAY_MS);
      });
    } finally {
      vi.useRealTimers();
    }
    await waitFor(() => expect(calls("edit_change_edit_hunk")).toHaveLength(1));
    expect(calls("edit_change_edit_hunk")[0][1]).toEqual({
      proposalId: "p1",
      hunkId: "h1",
      after: edited,
      baselineChecksum: "ck-1",
    });
    expect(field).toHaveValue(edited);
    expect(calls("accept_change_edit_hunk")).toHaveLength(0);
    expect(calls("reject_change_edit_hunk")).toHaveLength(0);
  });

  it("OCW-FR-RRXG: the artifact list marks the undecided hunks of each artifact, and a new file shows under its artifact marked new", async () => {
    drawChange({ file: "proposal.md" });
    await screen.findByTestId("source-review");
    expect(within(artifactButton("proposal")).getByTestId("openspec-artifact-hunks")).toHaveTextContent("1");
    expect(within(artifactButton("specs")).getByTestId("openspec-artifact-hunks")).toHaveTextContent("1");
    expect(within(artifactButton("design")).queryByTestId("openspec-artifact-hunks")).not.toBeInTheDocument();
    await userEvent.click(artifactButton("specs"));
    const created = await waitFor(() => fileArticle(NEW_PATH));
    expect(within(created).getByTestId("openspec-file-new")).toHaveTextContent("new");
    // A new file is not on disk, so it shows in Source alone, with its text.
    expect(within(created).getByRole("button", { name: "Rich" })).toBeDisabled();
    expect(within(created).getByRole("textbox", { name: /Proposed text of Change 2 of 2/ })).toHaveValue(NEW_TEXT);
  });

  it("OCW-FR-RRXG: a decided hunk is not counted, and an accepted new file is not marked new", async () => {
    backend.proposals = [changeProposal(["pending", "accepted"])];
    backend.detail.artifacts[1].files.push({ path: NEW_PATH, text: NEW_TEXT, unreadable: false });
    drawChange({ file: "proposal.md" });
    await screen.findByTestId("source-review");
    expect(within(artifactButton("specs")).queryByTestId("openspec-artifact-hunks")).not.toBeInTheDocument();
    await userEvent.click(artifactButton("specs"));
    const file = await waitFor(() => fileArticle(NEW_PATH));
    expect(within(file).queryByTestId("openspec-file-new")).not.toBeInTheDocument();
  });
});

describe("decisions", () => {
  it("OCW-FR-DTRX: Accept invokes accept change edit hunk, then reads the change and the proposals again", async () => {
    backend.decide = () => outcome();
    const chip = await openAtProposal();
    const reads = calls("read_openspec_change").length;
    const lists = calls("list_change_edit_proposals").length;
    await userEvent.click(within(chip).getByRole("button", { name: "Accept" }));
    await waitFor(() => expect(calls("accept_change_edit_hunk")).toHaveLength(1));
    expect(calls("accept_change_edit_hunk")[0][1]).toEqual({ proposalId: "p1", hunkId: "h1" });
    await waitFor(() => expect(calls("read_openspec_change").length).toBeGreaterThan(reads));
    await waitFor(() => expect(calls("list_change_edit_proposals").length).toBeGreaterThan(lists));
  });

  it("OCW-FR-DTRX: Reject invokes reject change edit hunk, and writes nothing else", async () => {
    backend.decide = () => outcome({ proposal: changeProposal(["rejected", "pending"]) });
    const chip = await openAtProposal();
    await userEvent.click(within(chip).getByRole("button", { name: "Reject" }));
    await waitFor(() => expect(calls("reject_change_edit_hunk")).toHaveLength(1));
    expect(calls("reject_change_edit_hunk")[0][1]).toEqual({ proposalId: "p1", hunkId: "h1" });
    expect(calls("accept_change_edit_hunk")).toHaveLength(0);
  });

  it("OCW-FR-DTRX: stream_busy is stated at the hunk with the name of the run that holds the stream", async () => {
    backend.decide = () => ({ reject: "stream_busy" });
    backend.runs = [
      { id: "g-1", streamId: "other", state: "working", input: { draftName: "Elsewhere" } },
      { id: "g-2", streamId: STREAM_ID, state: "completed", input: { draftName: "Done before" } },
      { id: "g-3", streamId: STREAM_ID, state: "reviewing", input: { draftName: "Tighten auth" } },
    ] as unknown as GraduationRun[];
    const chip = await openAtProposal();
    const reads = calls("read_openspec_change").length;
    const lists = calls("list_change_edit_proposals").length;
    await userEvent.click(within(chip).getByRole("button", { name: "Accept" }));
    const refusal = await within(fileArticle("proposal.md")).findByTestId("openspec-hunk-refusal");
    expect(refusal).toHaveTextContent("The run “Tighten auth” holds this stream");
    // The hunk stays undecided and nothing was read again for a decision.
    expect(fileArticle("proposal.md").querySelector('.hunk--del[data-hunk="h1"]')).not.toBeNull();
    expect(calls("read_openspec_change")).toHaveLength(reads);
    expect(calls("list_change_edit_proposals")).toHaveLength(lists);
  });

  it("OCW-FR-DTRX: stream_busy with no run to name states the refusal without a name", async () => {
    backend.decide = () => ({ reject: "stream_busy" });
    const chip = await openAtProposal();
    await userEvent.click(within(chip).getByRole("button", { name: "Accept" }));
    const refusal = await within(fileArticle("proposal.md")).findByTestId("openspec-hunk-refusal");
    expect(refusal).toHaveTextContent("A graduation run holds this stream");
  });

  it("OCW-FR-DTRX: another refusal is stated at the hunk in words", async () => {
    backend.decide = () => ({ reject: "anchor_lost" });
    const chip = await openAtProposal();
    await userEvent.click(within(chip).getByRole("button", { name: "Accept" }));
    const refusal = await within(fileArticle("proposal.md")).findByTestId("openspec-hunk-refusal");
    expect(refusal).toHaveTextContent("no longer in the file");
  });
});

describe("the proposals follow the backend", () => {
  it("OCW-FR-TAZJ: the event of its stream and change reads the proposals again, and the event of another change does not", async () => {
    await openAtProposal();
    const lists = calls("list_change_edit_proposals").length;
    backend.proposals = [changeProposal(["rejected", "pending"])];
    // The event of another change comes first, then the event of this change.
    // Only the second reads the proposals again.
    act(() => {
      emit("change-edit-proposals-changed", {
        streamId: STREAM_ID,
        change: "another-change",
        proposal: changeProposal(),
        cause: "recorded",
      });
      emit("change-edit-proposals-changed", {
        streamId: STREAM_ID,
        change: CHANGE,
        proposal: changeProposal(["rejected", "pending"]),
        cause: "decided",
      });
    });
    // The rejected hunk is no longer counted as undecided.
    await waitFor(() =>
      expect(within(artifactButton("proposal")).queryByTestId("openspec-artifact-hunks")).not.toBeInTheDocument(),
    );
    expect(calls("list_change_edit_proposals")).toHaveLength(lists + 1);
  });
});

describe("the close of a proposal", () => {
  it("OCW-FR-MLXJ: a close with a commit raises an Info toast that names the change", async () => {
    backend.decide = () =>
      outcome({
        proposal: changeProposal(["accepted", "accepted"], { state: "accepted" }),
        committed: true,
        resolved: true,
      });
    const chip = await openAtProposal();
    await userEvent.click(within(chip).getByRole("button", { name: "Accept" }));
    await waitFor(() => expect(visibleToasts()).toHaveLength(1));
    expect(visibleToasts()[0]).toMatchObject({ level: "Info" });
    expect(visibleToasts()[0].title).toContain(CHANGE);
  });

  it("OCW-FR-MLXJ: a decision that does not close the proposal raises no toast", async () => {
    backend.decide = () => outcome();
    const chip = await openAtProposal();
    await userEvent.click(within(chip).getByRole("button", { name: "Accept" }));
    await waitFor(() => expect(calls("accept_change_edit_hunk")).toHaveLength(1));
    await waitFor(() => expect(calls("read_openspec_change").length).toBeGreaterThan(1));
    expect(visibleToasts()).toHaveLength(0);
  });

  it("OCW-FR-MLXJ: a close whose commit fails raises an Error toast, and Retry invokes complete change edit decision", async () => {
    backend.decide = () =>
      outcome({
        proposal: changeProposal(["accepted", "rejected"], { state: "accepted", commitOwed: true }),
        committed: false,
        resolved: true,
      });
    backend.complete = () =>
      outcome({
        proposal: changeProposal(["accepted", "rejected"], { state: "accepted", commitOwed: false }),
        committed: true,
        resolved: false,
      });
    const chip = await openAtProposal();
    await userEvent.click(within(chip).getByRole("button", { name: "Accept" }));
    await waitFor(() => expect(visibleToasts()).toHaveLength(1));
    const failed = visibleToasts()[0];
    expect(failed).toMatchObject({ level: "Error" });
    expect(failed.title).toContain(CHANGE);
    expect(failed.link?.label).toBe("Retry");
    act(() => failed.link?.onActivate());
    await waitFor(() => expect(calls("complete_change_edit_decision")).toHaveLength(1));
    expect(calls("complete_change_edit_decision")[0][1]).toEqual({ proposalId: "p1" });
    // The commit landed, so the toast says so in place of the failure.
    await waitFor(() => expect(visibleToasts()[0]).toMatchObject({ level: "Info" }));
    expect(visibleToasts()).toHaveLength(1);
  });
});
