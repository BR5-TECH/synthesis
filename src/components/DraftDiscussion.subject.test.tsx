/**
 * The subject strip of a hunk discussion
 * (`../../specifications/ui/DDS-draft-discussion.md` DDS-FR-WKTQ, DDS-FR-PBXN;
 * `CMT-comments.md` CMT-FR-48).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { HunkSubject } from "./DraftDiscussion/HunkSubject";
import * as logging from "../logging";
import { resetLogBufferForTest } from "../logging";
import {
  noteProposalChanged,
  resetDraftProposals,
  reviewingProposal,
} from "../state/draftProposals";
import { draftDiscussionState, resetDraftDiscussions } from "../state/draftDiscussion";
import { resetProposalHunks } from "../state/proposalHunks";
import {
  DRAFT,
  hunkReading,
  hunkReference,
  proposalHunk,
  proposalOf,
} from "../test/draftDiscussionFixtures";
import type { HunkState, ProposalHunk } from "../types";

/** The backend: the hunk reading this case seeds, and a refusal of anything else. */
function backend(hunks: ProposalHunk[] | "fail") {
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "append_log_records") return Promise.resolve();
    if (cmd === "list_draft_change_proposals") return Promise.resolve([]);
    if (cmd === "load_draft_change_proposal_hunks") {
      return hunks === "fail"
        ? Promise.reject(new Error("proposals_unreadable"))
        : Promise.resolve(hunkReading(hunks));
    }
    return Promise.reject(new Error(`unexpected command ${cmd}`));
  });
}

function seed(states: HunkState[], state: "pending" | "accepted" | "rejected" = "pending") {
  act(() => noteProposalChanged({ draftId: DRAFT, proposal: proposalOf(states, state) }));
}

/**
 * jsdom lays nothing out, so the boxes report the layout a case needs: a text
 * `cut` reports more height than it shows.
 */
function layOut(cut: (el: HTMLElement) => boolean) {
  vi.spyOn(HTMLElement.prototype, "clientHeight", "get").mockReturnValue(40);
  vi.spyOn(HTMLElement.prototype, "scrollHeight", "get").mockImplementation(function (
    this: HTMLElement,
  ) {
    return cut(this) ? 120 : 40;
  });
}

const THREE = [proposalHunk("h1"), proposalHunk("h2"), proposalHunk("h3")];

beforeEach(() => {
  invokeMock.mockReset();
  backend(THREE);
  resetDraftProposals();
  resetProposalHunks();
  resetDraftDiscussions();
  resetLogBufferForTest();
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("DDS-FR-WKTQ: what the strip names", () => {
  it("DDS-FR-WKTQ: names the change by its place, its file and its state", async () => {
    seed(["pending", "rejected", "pending"]);
    render(<HunkSubject reference={hunkReference("h2")} />);

    const strip = screen.getByTestId("discussion-subject");
    expect(strip).toHaveTextContent("Change 2 of 3");
    expect(strip).toHaveTextContent("Spec.md");
    const state = screen.getByTestId("discussion-subject-state");
    expect(state).toHaveTextContent("rejected");
    expect(state).toHaveAttribute("data-state", "rejected");
    await screen.findByTestId("discussion-subject-after");
  });

  it("DDS-FR-WKTQ: the state follows each decision, in the same strip", async () => {
    seed(["pending", "pending", "pending"]);
    render(<HunkSubject reference={hunkReference("h2")} />);
    const state = screen.getByTestId("discussion-subject-state");
    expect(state).toHaveTextContent("undecided");

    seed(["pending", "accepted", "pending"]);
    expect(screen.getByTestId("discussion-subject-state")).toBe(state);
    expect(state).toHaveTextContent("accepted");
    await screen.findByTestId("discussion-subject-after");
  });

  it("DDS-FR-WKTQ, DDS-FR-RHKW: before the proposal is read, the strip names the place the composer names", async () => {
    render(<HunkSubject reference={hunkReference("h2")} place={{ position: 2, total: 3 }} />);
    expect(screen.getByTestId("discussion-subject")).toHaveTextContent("Change 2 of 3");
    await screen.findByTestId("discussion-subject-after");
  });

  it("DDS-FR-WKTQ: once the proposal is read, the strip names the change by the proposal's own count", async () => {
    seed(["pending", "pending", "pending"]);
    render(<HunkSubject reference={hunkReference("h2")} place={{ position: 1, total: 1 }} />);
    expect(screen.getByTestId("discussion-subject")).toHaveTextContent("Change 2 of 3");
    await screen.findByTestId("discussion-subject-after");
  });

  it("DDS-FR-WKTQ: a proposal that is gone is said in words, with nowhere to go", async () => {
    // The draft's proposals were read and `p1` is not among them, and the
    // further reading agrees: the reference is gone.
    render(<HunkSubject reference={hunkReference("h2")} place={{ position: 2, total: 3 }} />);
    const state = await screen.findByTestId("discussion-subject-state");
    expect(state).toHaveTextContent("no longer available");
    expect(state).toHaveAttribute("data-state", "missing");
    expect(screen.queryByRole("button", { name: "Show in document" })).toBeNull();
    // The place the composer named still stands.
    expect(screen.getByTestId("discussion-subject")).toHaveTextContent("Change 2 of 3");
  });

  it("DDS-FR-WKTQ: a change its proposal's reading does not hold is still named, and the reason is logged", async () => {
    const warn = vi.spyOn(logging, "logWarn");
    backend([proposalHunk("h1")]);
    seed(["pending", "pending"]);
    render(<HunkSubject reference={hunkReference("h2")} />);

    await waitFor(() =>
      expect(warn).toHaveBeenCalledWith(
        ["frontend"],
        "a discussed change is not in its proposal's reading",
        { draftId: DRAFT, proposalId: "p1", hunkId: "h2" },
      ),
    );
    expect(screen.getByTestId("discussion-subject")).toHaveTextContent("Change 2 of 2");
    expect(screen.queryByTestId("discussion-subject-after")).toBeNull();
  });
});

describe("DDS-FR-PBXN: what the strip shows of the change", () => {
  it("DDS-FR-PBXN: shows the agent's note, the text removed and the text added, each signed", async () => {
    backend([
      proposalHunk("h1", { note: "Shorter.", before: "A long sentence.", after: "A sentence." }),
    ]);
    seed(["pending"]);
    render(<HunkSubject reference={hunkReference("h1")} />);

    expect(await screen.findByTestId("discussion-subject-note")).toHaveTextContent("Shorter.");
    expect(screen.getByTestId("discussion-subject-before")).toHaveTextContent("A long sentence.");
    expect(screen.getByTestId("discussion-subject-after")).toHaveTextContent("A sentence.");
    expect(screen.getByTestId("discussion-subject-before")).toHaveTextContent(/Removes: A long sentence\./);
    expect(screen.getByTestId("discussion-subject-after")).toHaveTextContent(/Adds: A sentence\./);
  });

  it("DDS-FR-PBXN: an insertion shows no removed text", async () => {
    backend([proposalHunk("h1", { kind: "add", before: null, after: "New text." })]);
    seed(["pending"]);
    render(<HunkSubject reference={hunkReference("h1")} />);
    await screen.findByTestId("discussion-subject-after");
    expect(screen.queryByTestId("discussion-subject-before")).toBeNull();
  });

  it("DDS-FR-PBXN: Show more stands while the strip cuts a text", async () => {
    layOut((el) => el.textContent === "cut text");
    backend([proposalHunk("h1", { before: "short", after: "cut text" })]);
    seed(["pending"]);
    render(<HunkSubject reference={hunkReference("h1")} />);
    expect(await screen.findByRole("button", { name: "Show more" })).toBeInTheDocument();
  });

  it("DDS-FR-PBXN: Show more stands also for a cut note", async () => {
    layOut((el) => el.textContent === "a long note");
    backend([proposalHunk("h1", { note: "a long note" })]);
    seed(["pending"]);
    render(<HunkSubject reference={hunkReference("h1")} />);
    expect(await screen.findByRole("button", { name: "Show more" })).toBeInTheDocument();
  });

  it("DDS-FR-PBXN: no Show more stands while every text shows whole, however long it is", async () => {
    layOut(() => false);
    backend([proposalHunk("h1", { note: "n".repeat(400), after: "x".repeat(400) })]);
    seed(["pending"]);
    render(<HunkSubject reference={hunkReference("h1")} />);
    await screen.findByTestId("discussion-subject-after");
    expect(screen.queryByRole("button", { name: "Show more" })).toBeNull();
  });

  it("DDS-FR-PBXN: Show more shows the whole text and Show less cuts it again", async () => {
    layOut(() => true);
    backend([proposalHunk("h1")]);
    seed(["pending"]);
    render(<HunkSubject reference={hunkReference("h1")} />);
    const more = await screen.findByRole("button", { name: "Show more" });
    const strip = screen.getByTestId("discussion-subject");
    expect(more).toHaveAttribute("aria-expanded", "false");
    expect(strip).toHaveAttribute("data-expanded", "false");

    fireEvent.click(more);
    expect(strip).toHaveAttribute("data-expanded", "true");
    const less = screen.getByRole("button", { name: "Show less" });
    expect(less).toHaveAttribute("aria-expanded", "true");

    fireEvent.click(less);
    expect(strip).toHaveAttribute("data-expanded", "false");
    expect(screen.getByRole("button", { name: "Show more" })).toBeInTheDocument();
  });

  it("DDS-FR-PBXN: another change starts cut again", async () => {
    layOut(() => true);
    backend([proposalHunk("h1"), proposalHunk("h2")]);
    seed(["pending", "pending"]);
    const { rerender } = render(<HunkSubject reference={hunkReference("h1")} />);
    fireEvent.click(await screen.findByRole("button", { name: "Show more" }));
    expect(screen.getByTestId("discussion-subject")).toHaveAttribute("data-expanded", "true");

    rerender(<HunkSubject reference={hunkReference("h2")} />);
    expect(screen.getByTestId("discussion-subject")).toHaveAttribute("data-expanded", "false");
  });

  it("DDS-FR-PBXN: a reading that failed says so, and still names the change", async () => {
    backend("fail");
    seed(["pending", "pending"]);
    render(<HunkSubject reference={hunkReference("h2")} />);
    expect(await screen.findByRole("status")).toHaveTextContent(
      "The text of this change could not be read.",
    );
    expect(screen.getByTestId("discussion-subject")).toHaveTextContent("Change 2 of 2");
    expect(screen.queryByTestId("discussion-subject-after")).toBeNull();
  });
});

describe("DDS-FR-PBXN, CMT-FR-48: Show in document", () => {
  it("DDS-FR-PBXN, CMT-FR-48: while the proposal is open, it puts the review on the change and asks the document to show it, each time", async () => {
    seed(["pending", "pending"]);
    render(<HunkSubject reference={hunkReference("h2")} />);
    const go = screen.getByRole("button", { name: "Show in document" });
    expect(reviewingProposal()).toBeNull();

    fireEvent.click(go);
    expect(draftDiscussionState(DRAFT).focusedHunkId).toBe("h2");
    expect(draftDiscussionState(DRAFT).hunkReveal).toBe(1);
    expect(reviewingProposal()).toBe("p1");

    // The review is on the change already. A second ask still shows it, since
    // the author may have scrolled away from it.
    fireEvent.click(go);
    expect(draftDiscussionState(DRAFT).hunkReveal).toBe(2);
    await screen.findByTestId("discussion-subject-after");
  });

  it("DDS-FR-PBXN: a rejected change of an open proposal is still shown in the document", async () => {
    seed(["pending", "rejected"]);
    render(<HunkSubject reference={hunkReference("h2")} />);
    expect(screen.getByRole("button", { name: "Show in document" })).toBeInTheDocument();
    await screen.findByTestId("discussion-subject-after");
  });

  it("DDS-FR-PBXN, DCR-FR-07: an accepted change has no decoration to go to", async () => {
    seed(["pending", "accepted"]);
    render(<HunkSubject reference={hunkReference("h2")} />);
    expect(screen.queryByRole("button", { name: "Show in document" })).toBeNull();
    await screen.findByTestId("discussion-subject-after");
  });

  it("DDS-FR-PBXN, DCR-FR-17: once the proposal closes there is nothing to go to, and the state is still said", async () => {
    seed(["accepted", "rejected"], "accepted");
    render(<HunkSubject reference={hunkReference("h2")} />);
    expect(screen.queryByRole("button", { name: "Show in document" })).toBeNull();
    expect(screen.getByTestId("discussion-subject-state")).toHaveTextContent("rejected");
    await screen.findByTestId("discussion-subject-after");
  });
});
