import { describe, expect, it, vi, beforeEach } from "vitest";
import { renderHook } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import {
  closeReview,
  ensureLoaded,
  noteProposalChanged,
  openReview,
  pendingOf,
  proposalsOf,
  readingStatusOf,
  referenceTo,
  requestProposalReading,
  resetDraftProposals,
  reviewingProposal,
  useDraftProposals,
} from "./draftProposals";
import type { DraftChangeProposal } from "../types";

/** Every reading this store has asked the backend for. */
const reads = () =>
  invokeMock.mock.calls.filter((c) => c[0] === "list_draft_change_proposals");

function proposal(over: Partial<DraftChangeProposal> = {}): DraftChangeProposal {
  return {
    id: "p1",
    draftId: "d1",
    path: "ui/spec.md",
    agent: { kind: "agent", agentId: "a1", handle: "arch", model: "m" },
    rationale: "why",
    threadId: "t1",
    commentId: "c1",
    state: "pending",
    candidateEdited: false,
    legacy: false,
    hunkCount: 1,
    counts: { pending: 1, accepted: 0, rejected: 0, discussing: 0 },
    ledger: [
      { id: "h1", kind: "replace", state: "pending", edited: false, revision: 0 },
    ],
    createdAt: "2026-01-01T00:00:00Z",
    ...over,
  };
}

/** Let the load's promise settle. */
const settle = () => new Promise((r) => setTimeout(r, 0));

/**
 * A draft's tab, as far as this store is concerned: one mounted consumer.
 *
 * DCR-FR-03 only opens a review on a proposal that has somewhere to arrive, and
 * "somewhere" is a mounted consumer of this store — so a test about arrival has
 * to have one, exactly as the running application does.
 */
function mountTabOn(draftId: string): { unmount: () => void } {
  const { unmount } = renderHook(() => useDraftProposals(draftId));
  return { unmount };
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockResolvedValue([]);
  resetDraftProposals();
});

describe("loading (DCR-FR-03, DCR-FR-32)", () => {
  it("reads a draft's proposals once per session", async () => {
    invokeMock.mockResolvedValue([proposal()]);
    ensureLoaded("d1");
    ensureLoaded("d1");
    ensureLoaded("d1");
    await settle();

    expect(reads()).toHaveLength(1);
    expect(pendingOf("d1")?.id).toBe("p1");
  });

  it("keeps one ask in flight per draft, however many surfaces want it", async () => {
    // DCR-FR-32: three surfaces showing one draft cost one call.
    let release: (v: unknown[]) => void = () => {};
    invokeMock.mockReturnValue(new Promise((r) => (release = r)));
    ensureLoaded("d1");
    ensureLoaded("d1");
    requestProposalReading("d1", "p1");
    expect(reads()).toHaveLength(1);
    expect(readingStatusOf("d1")).toBe("asking");

    release([proposal()]);
    await settle();
    expect(readingStatusOf("d1")).toBe("read");
  });

  it("allows a retry when the read failed, rather than staying empty for good", async () => {
    invokeMock.mockRejectedValueOnce(new Error("nope"));
    ensureLoaded("d1");
    await settle();
    expect(proposalsOf("d1")).toEqual([]);
    // A reading that failed says so rather than saying the draft holds nothing:
    // no surface may report a proposal absent from a reading that never
    // completed (DCR-FR-32).
    expect(readingStatusOf("d1")).toBe("failed");

    invokeMock.mockResolvedValue([proposal()]);
    ensureLoaded("d1");
    await settle();
    expect(pendingOf("d1")?.id).toBe("p1");
    expect(readingStatusOf("d1")).toBe("read");
  });

  it("stops asking after a bounded run of failures", async () => {
    invokeMock.mockRejectedValue(new Error("nope"));
    for (let i = 0; i < 8; i += 1) {
      ensureLoaded("d1");
      await settle();
    }
    // CTA-FR-XVNC: asked again whenever one failed, but never forever.
    expect(reads()).toHaveLength(3);
    expect(readingStatusOf("d1")).toBe("failed");
  });

  it("answers every reference at once when the draft itself is gone", async () => {
    // CTA-FR-SACG's two routes — the draft graduated or deleted, and a committed
    // log read from outside the draft it belonged to — are both a refused read
    // rather than an empty list: the draft took its proposals with it
    // (DCP-FR-02), so there is nothing a further ask could heal.
    invokeMock.mockRejectedValue("draft_not_found");
    ensureLoaded("d1");
    await settle();

    expect(readingStatusOf("d1")).toBe("absent");
    expect(referenceTo("d1", "p1")).toEqual({ kind: "gone" });
    expect(referenceTo("d1", "p2")).toEqual({ kind: "gone" });

    // Settled: neither the ordinary ask nor a reference's further one re-asks
    // after a draft that is not there.
    ensureLoaded("d1");
    requestProposalReading("d1", "p1");
    expect(reads()).toHaveLength(1);
  });

  it("tells a draft that is gone apart from a read that failed", async () => {
    invokeMock.mockRejectedValue(new Error("no project open"));
    ensureLoaded("d1");
    await settle();
    // Not "gone": nobody has managed to look yet.
    expect(readingStatusOf("d1")).toBe("failed");
    expect(referenceTo("d1", "p1")).toEqual({ kind: "unresolved" });
  });

  it("drops a reading that lands after the content root changed", async () => {
    // DCR-FR-33: a promise cannot be cancelled, so a reading issued against the
    // outgoing tree must not fold itself into the incoming session — and must
    // not leave the draft marked read, which would suppress the fresh reading
    // that was supposed to replace it.
    let release: (v: unknown[]) => void = () => {};
    invokeMock.mockReturnValue(new Promise((r) => (release = r)));
    ensureLoaded("d1");

    resetDraftProposals();
    release([proposal({ id: "outgoing" })]);
    await settle();

    expect(proposalsOf("d1")).toEqual([]);
    expect(readingStatusOf("d1")).toBe("unread");

    invokeMock.mockResolvedValue([proposal({ id: "incoming" })]);
    ensureLoaded("d1");
    await settle();
    expect(reads()).toHaveLength(2);
    expect(proposalsOf("d1").map((p) => p.id)).toEqual(["incoming"]);
  });

  it("drops a reading that fails after the content root changed", async () => {
    let reject: (e: unknown) => void = () => {};
    invokeMock.mockReturnValue(new Promise((_, r) => (reject = r)));
    ensureLoaded("d1");

    resetDraftProposals();
    reject(new Error("outgoing"));
    await settle();

    // Neither the failure count nor the status of the tree the window has left
    // reaches the one it is now reading.
    expect(readingStatusOf("d1")).toBe("unread");
  });

  it("gives a draft its asks back when an event proves the backend answers", async () => {
    // DCR-FR-32, CTA-FR-XVNC: a tab whose reading failed still shows the proposal the event
    // announces, and the review still opens on it — the reading having failed
    // is not allowed to cost the author the arrival.
    const tab = mountTabOn("no-such-draft");
    invokeMock.mockRejectedValue(new Error("nope"));
    for (let i = 0; i < 5; i += 1) {
      ensureLoaded("d1");
      await settle();
    }
    const exhausted = reads().length;

    const arriving = mountTabOn("d1");
    noteProposalChanged({ draftId: "d1", proposal: proposal() });
    expect(pendingOf("d1")?.id).toBe("p1");
    expect(reviewingProposal()).toBe("p1");
    closeReview();

    invokeMock.mockResolvedValue([proposal()]);
    ensureLoaded("d1");
    await settle();

    expect(reads().length).toBe(exhausted + 1);
    expect(readingStatusOf("d1")).toBe("read");
    arriving.unmount();
    tab.unmount();
  });

  it("does not treat an event as a reading of the draft", async () => {
    // DCR-FR-32: an event tells the store about one proposal. A draft known
    // only through events has not been read, so a reference to any *other*
    // proposal of it must not be judged against a list nobody ever took.
    noteProposalChanged({ draftId: "d1", proposal: proposal() });
    expect(readingStatusOf("d1")).toBe("unread");
    expect(referenceTo("d1", "p9")).toEqual({ kind: "unresolved" });

    invokeMock.mockResolvedValue([proposal(), proposal({ id: "p9" })]);
    ensureLoaded("d1");
    await settle();
    expect(referenceTo("d1", "p9")).toMatchObject({ kind: "known" });
  });

  it("answers an unloaded draft with a stable empty array", () => {
    // `useSyncExternalStore` compares snapshots by identity and loops forever on
    // a getSnapshot that returns a fresh value each call.
    expect(proposalsOf("never-loaded")).toBe(proposalsOf("never-loaded"));
  });
});

describe("folding an event in (DCP-FR-16)", () => {
  it("replaces a record in place rather than re-reading the draft", async () => {
    invokeMock.mockResolvedValue([proposal()]);
    ensureLoaded("d1");
    await settle();
    const before = invokeMock.mock.calls.length;

    noteProposalChanged({
      draftId: "d1",
      proposal: proposal({ state: "accepted", decidedAt: "2026-01-02T00:00:00Z" }),
    });

    expect(proposalsOf("d1")).toHaveLength(1);
    expect(proposalsOf("d1")[0].state).toBe("accepted");
    expect(pendingOf("d1")).toBeUndefined();
    expect(invokeMock.mock.calls.length).toBe(before);
  });

  it("adds a proposal for a draft it had never loaded", () => {
    noteProposalChanged({ draftId: "d2", proposal: proposal({ draftId: "d2" }) });
    expect(pendingOf("d2")?.id).toBe("p1");
  });

  it("keeps at most one pending, whichever order the events arrive in", () => {
    // DCP-FR-04 is enforced in the backend; the store just has to not invent a
    // second pending record out of two events about one proposal.
    noteProposalChanged({ draftId: "d1", proposal: proposal() });
    noteProposalChanged({ draftId: "d1", proposal: proposal({ state: "rejected" }) });
    noteProposalChanged({ draftId: "d1", proposal: proposal({ id: "p2" }) });

    expect(proposalsOf("d1")).toHaveLength(2);
    expect(pendingOf("d1")?.id).toBe("p2");
  });
});

describe("a proposal that arrives shows itself (DCR-FR-03)", () => {
  it("opens the review on a newly arrived pending proposal", () => {
    // The author asked an agent a question and is waiting for the answer.
    // Making them find and click an indication to read what came back is a step
    // with nothing in it.
    const tab = mountTabOn("d1");
    expect(reviewingProposal()).toBeNull();
    noteProposalChanged({ draftId: "d1", proposal: proposal() });
    expect(reviewingProposal()).toBe("p1");
    closeReview();
    tab.unmount();
  });

  it("opens one that arrives for a draft whose list has already been read", async () => {
    // The ordinary case: the tab is open, so its proposals were listed on mount
    // and the draft is already in the store holding nothing.
    invokeMock.mockResolvedValue([]);
    const tab = mountTabOn("d1");
    await settle();
    expect(proposalsOf("d1")).toEqual([]);

    noteProposalChanged({ draftId: "d1", proposal: proposal() });
    expect(reviewingProposal()).toBe("p1");
    closeReview();
    tab.unmount();
  });

  it("does not reopen a proposal the author has dismissed", () => {
    const tab = mountTabOn("d1");
    noteProposalChanged({ draftId: "d1", proposal: proposal() });
    closeReview();
    expect(reviewingProposal()).toBeNull();

    // A later event about the *same* proposal is not an arrival. Reopening on
    // one would make the modal impossible to get rid of.
    noteProposalChanged({ draftId: "d1", proposal: proposal() });
    expect(reviewingProposal()).toBeNull();
    tab.unmount();
  });

  it("opens on nothing a proposal that was decided rather than made", () => {
    const tab = mountTabOn("d1");
    noteProposalChanged({
      draftId: "d1",
      proposal: proposal({ state: "accepted", decidedAt: "2026-01-02T00:00:00Z" }),
    });
    expect(reviewingProposal()).toBeNull();
    // A decline arriving from another window is a decision too.
    noteProposalChanged({
      draftId: "d1",
      proposal: proposal({ id: "p2", state: "rejected" }),
    });
    expect(reviewingProposal()).toBeNull();
    tab.unmount();
  });

  it("never replaces a review the author is already reading", () => {
    // DCR-FR-02: whatever is open is the one being decided, and swapping it out
    // under the author's pointer would decide it by accident.
    const first = mountTabOn("d1");
    const second = mountTabOn("d2");
    noteProposalChanged({ draftId: "d1", proposal: proposal() });
    expect(reviewingProposal()).toBe("p1");
    noteProposalChanged({
      draftId: "d2",
      proposal: proposal({ id: "p2", draftId: "d2" }),
    });
    expect(reviewingProposal()).toBe("p1");
    closeReview();
    first.unmount();
    second.unmount();
  });

  it("opens nothing for a draft with no tab to open it in", () => {
    // DCR-FR-19. DCR-FR-03 / DCR-FR-18: the listener is the shell's and hears about every
    // draft in the worktree. A proposal for a closed tab must not take the open
    // slot — it would surface nowhere, block an arrival for the draft the author
    // IS looking at, and then ambush them the next time they opened that draft
    // for a reason of their own.
    noteProposalChanged({ draftId: "closed", proposal: proposal({ draftId: "closed" }) });
    expect(pendingOf("closed")?.id).toBe("p1");
    expect(reviewingProposal()).toBeNull();

    // …and the slot is still free for the draft that does have a tab.
    const tab = mountTabOn("d1");
    noteProposalChanged({ draftId: "d1", proposal: proposal({ id: "p2" }) });
    expect(reviewingProposal()).toBe("p2");
    closeReview();
    tab.unmount();
  });

  it("stops opening once the last consumer of a draft has gone", () => {
    const tab = mountTabOn("d1");
    tab.unmount();
    noteProposalChanged({ draftId: "d1", proposal: proposal() });
    expect(reviewingProposal()).toBeNull();
  });

  it("leaves a proposal already standing when a draft is loaded alone", async () => {
    // DCR-FR-03. A proposal that was already there when the tab opened is the
    // tab's own indication to offer, not a modal to interrupt with.
    invokeMock.mockResolvedValue([proposal()]);
    const tab = mountTabOn("d1");
    await settle();
    expect(pendingOf("d1")?.id).toBe("p1");
    expect(reviewingProposal()).toBeNull();
    tab.unmount();
  });

  it("keeps a proposal that arrived while the draft's list was still loading", async () => {
    // The read was taken before the proposal existed, so it comes back without
    // it. Overwriting would drop a proposal the author has just been notified
    // about, leaving no marker and an open slot pointing at a record the store
    // no longer holds — and every later arrival suppressed by that slot.
    let release: (v: unknown[]) => void = () => {};
    invokeMock.mockReturnValue(new Promise((r) => (release = r)));
    const tab = mountTabOn("d1");

    noteProposalChanged({ draftId: "d1", proposal: proposal() });
    expect(reviewingProposal()).toBe("p1");

    release([proposal({ id: "p0", state: "accepted" })]);
    await settle();

    expect(proposalsOf("d1").map((p) => p.id).sort()).toEqual(["p0", "p1"]);
    expect(pendingOf("d1")?.id).toBe("p1");
    expect(reviewingProposal()).toBe("p1");
    closeReview();
    tab.unmount();
  });
});

describe("what a reference resolves to (CTA-FR-UKIG, CTA-FR-XVNC)", () => {
  it("is unresolved — never gone — until a reading has completed", async () => {
    let release: (v: unknown[]) => void = () => {};
    invokeMock.mockReturnValue(new Promise((r) => (release = r)));

    // Nothing read yet.
    expect(referenceTo("d1", "p1")).toEqual({ kind: "unresolved" });
    requestProposalReading("d1", "p1");
    // In flight.
    expect(referenceTo("d1", "p1")).toEqual({ kind: "unresolved" });

    release([proposal()]);
    await settle();
    expect(referenceTo("d1", "p1")).toMatchObject({
      kind: "known",
      proposal: { id: "p1", state: "pending" },
    });
  });

  it("stays unresolved when the reading failed", async () => {
    invokeMock.mockRejectedValue(new Error("nope"));
    requestProposalReading("d1", "p1");
    await settle();
    // A reading that failed is not evidence that a proposal is gone: saying so
    // would take the review away from an author whose proposal is standing.
    expect(referenceTo("d1", "p1")).toEqual({ kind: "unresolved" });
  });

  it("asks once more when a completed reading did not return it, then settles", async () => {
    // The reading was taken before the proposal existed and the event that
    // announced it never arrived — the case that wedges a conversation.
    invokeMock.mockResolvedValue([]);
    requestProposalReading("d1", "p1");
    await settle();
    expect(reads()).toHaveLength(1);
    expect(referenceTo("d1", "p1")).toEqual({ kind: "unresolved" });

    invokeMock.mockResolvedValue([proposal()]);
    requestProposalReading("d1", "p1");
    await settle();
    expect(reads()).toHaveLength(2);
    expect(referenceTo("d1", "p1")).toMatchObject({ kind: "known" });

    // …and having resolved, it asks for nothing further.
    requestProposalReading("d1", "p1");
    expect(reads()).toHaveLength(2);
  });

  it("says a proposal is gone once the further reading agreed", async () => {
    // Its draft has been graduated or deleted, which takes its proposals with
    // it (DCP-FR-02), while the committed comment log outlives them.
    invokeMock.mockResolvedValue([]);
    requestProposalReading("d1", "p1");
    await settle();
    requestProposalReading("d1", "p1");
    await settle();

    expect(referenceTo("d1", "p1")).toEqual({ kind: "gone" });
    // Settled: the further ask is made once per reference, so a control whose
    // proposal is genuinely gone stops asking after it.
    requestProposalReading("d1", "p1");
    expect(reads()).toHaveLength(2);
  });

  it("gives each reference its own further ask", async () => {
    invokeMock.mockResolvedValue([]);
    requestProposalReading("d1", "p1");
    await settle();
    requestProposalReading("d1", "p1");
    await settle();
    expect(referenceTo("d1", "p1")).toEqual({ kind: "gone" });

    // A second reference inherits none of the first's exhaustion: it has not
    // had its own further ask, so it is unresolved rather than gone…
    expect(referenceTo("d1", "p2")).toEqual({ kind: "unresolved" });
    requestProposalReading("d1", "p2");
    await settle();
    // …and it is that ask, not the first reference's, that settles it.
    expect(reads()).toHaveLength(3);
    expect(referenceTo("d1", "p2")).toEqual({ kind: "gone" });
  });

  it("spends a reference's further ask once for the session, not once per mount", async () => {
    // CTA-FR-LOOE's bound is per reference rather than per rendering of it: a
    // control that has settled on saying the change is gone does not start
    // asking again because the card it sits in was scrolled out and back.
    invokeMock.mockResolvedValue([]);
    requestProposalReading("d1", "p1");
    await settle();
    requestProposalReading("d1", "p1");
    await settle();
    expect(referenceTo("d1", "p1")).toEqual({ kind: "gone" });

    invokeMock.mockResolvedValue([proposal()]);
    requestProposalReading("d1", "p1");
    await settle();
    expect(reads()).toHaveLength(2);
    expect(referenceTo("d1", "p1")).toEqual({ kind: "gone" });
  });

  it("answers unresolved for a reference carrying no draft", () => {
    expect(referenceTo(undefined, "p1")).toEqual({ kind: "unresolved" });
  });

  it("answers with the same value each time, so a caller may hold it", async () => {
    // `useSyncExternalStore` and every dependency list downstream compare by
    // identity, so the two answers that carry nothing are one value each.
    expect(referenceTo("d1", "p1")).toBe(referenceTo("d1", "p1"));

    invokeMock.mockResolvedValue([]);
    requestProposalReading("d1", "p1");
    await settle();
    requestProposalReading("d1", "p1");
    await settle();
    expect(referenceTo("d1", "p1")).toBe(referenceTo("d1", "p1"));
  });

  it("keeps a decision an event carried over a reading that predates it", async () => {
    // The reading was taken while the proposal was still pending and lands
    // after the author's decision has already been folded in. Letting the list
    // win would un-decide a decided proposal.
    let release: (v: unknown[]) => void = () => {};
    invokeMock.mockReturnValue(new Promise((r) => (release = r)));
    ensureLoaded("d1");
    noteProposalChanged({
      draftId: "d1",
      proposal: proposal({ state: "accepted", decidedAt: "2026-01-02T00:00:00Z" }),
    });

    release([proposal({ state: "pending" })]);
    await settle();

    expect(proposalsOf("d1")).toHaveLength(1);
    expect(referenceTo("d1", "p1")).toMatchObject({
      kind: "known",
      proposal: { state: "accepted" },
    });
  });
});

describe("resetting (DCR-FR-22, DCR-FR-33)", () => {
  it("drops every proposal and any open review", async () => {
    invokeMock.mockResolvedValue([proposal()]);
    ensureLoaded("d1");
    await settle();
    openReview("p1");

    // A proposal names a draft of the outgoing content root, and a modal left
    // open over one would be reviewing a change to a file the application is no
    // longer reading.
    resetDraftProposals();

    expect(proposalsOf("d1")).toEqual([]);
    expect(pendingOf("d1")).toBeUndefined();
    // The review goes with them: one left open would be deciding a change to a
    // file the application is no longer reading.
    expect(reviewingProposal()).toBeNull();

    // And the draft can be loaded afresh against the incoming root.
    invokeMock.mockResolvedValue([proposal({ id: "p9" })]);
    ensureLoaded("d1");
    await settle();
    expect(pendingOf("d1")?.id).toBe("p9");
    closeReview();
  });

  it("drops the readings with them, so the incoming tree is read afresh", async () => {
    // DCR-FR-33: a reading of the outgoing tree would otherwise answer for a
    // draft of the incoming one — and answer wrongly.
    invokeMock.mockResolvedValue([]);
    requestProposalReading("d1", "p1");
    await settle();
    requestProposalReading("d1", "p1");
    await settle();
    expect(referenceTo("d1", "p1")).toEqual({ kind: "gone" });

    resetDraftProposals();
    expect(readingStatusOf("d1")).toBe("unread");
    expect(referenceTo("d1", "p1")).toEqual({ kind: "unresolved" });

    invokeMock.mockResolvedValue([proposal()]);
    requestProposalReading("d1", "p1");
    await settle();
    expect(referenceTo("d1", "p1")).toMatchObject({ kind: "known" });
  });

  it("leaves the mounted consumers alone, so an arrival still has somewhere to go", () => {
    // `watching` is ref-counted against components that are still mounted, and
    // clearing it here would leave those counts unbalanced when they unmount —
    // and a proposal arriving in the incoming tree with nowhere to arrive.
    const tab = mountTabOn("d1");
    resetDraftProposals();
    noteProposalChanged({ draftId: "d1", proposal: proposal() });
    expect(reviewingProposal()).toBe("p1");
    closeReview();
    tab.unmount();
  });
});
