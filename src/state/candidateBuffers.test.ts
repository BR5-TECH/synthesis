import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { CandidateBufferStore, hunkKey, parseHunkKey } from "./candidateBuffers";
import { PROPOSAL_ERRORS } from "../types";
import { editDraftChangeHunk } from "../api";

/**
 * Tests for the candidate buffer of one proposed change
 * (`DCR-draft-change-review.md` DCR-FR-25, DCR-FR-26, DCR-FR-29; `../../
 * specifications/core/DCP-draft-change-proposals.md` DCP-FR-25, DCP-FR-27).
 *
 * Driven directly rather than through the modal, because the cases that matter
 * most here are the ones a component test cannot stage: two writes overlapping,
 * an edit arriving while a write is in flight, and a load landing on a buffer
 * that already holds the author's own text.
 */

const AGENT = "the agent's own text\n";

let store: CandidateBufferStore;

/** The one `edit_draft_change_hunk` call the store makes. */
const saves = () =>
  invokeMock.mock.calls.filter((c) => c[0] === "edit_draft_change_hunk");

beforeEach(() => {
  vi.useFakeTimers();
  invokeMock.mockReset();
  invokeMock.mockImplementation(async () => ({ checksum: "next" }));
  // The store is parametrised by which command writes a candidate through and
  // what a stale refusal reads as, the two proposal modules sharing neither
  // (DCP-FR-25, PCP-FR-29). These are the draft's, which is what this file
  // exercises.
  store = new CandidateBufferStore({
    // DCP-FR-XDRV: a draft's proposal is a list of changes and each is edited
    // on its own, so the key names the proposal AND the change. The writer is
    // the one thing that knows the key's shape; the store never takes it apart.
    save: (key, content, baseline) => {
      const { proposalId, hunkId } = parseHunkKey(key);
      return editDraftChangeHunk(proposalId, hunkId, content, baseline);
    },
    staleError: PROPOSAL_ERRORS.candidateStale,
  });
});

afterEach(() => {
  store.clear();
  vi.useRealTimers();
});

describe("adopting what storage holds (DCR-FR-08 / DCR-FR-25)", () => {
  it("takes the load when it holds no buffer for the proposal", () => {
    const buffer = store.adopt("p1", AGENT, "c1");
    expect(buffer.text).toBe(AGENT);
    expect(buffer.origin).toBe(AGENT);
    expect(buffer.baseline).toBe("c1");
    expect(buffer.dirty).toBe(false);
    expect(store.isEdited("p1")).toBe(false);
  });

  it("keeps the author's own text over a later load of the agent's", () => {
    // The promise DCR-FR-25 makes: no route back into a review restores what
    // the agent first composed over what the author wrote.
    store.adopt("p1", AGENT, "c1");
    store.edit("p1", "mine\n");

    store.adopt("p1", AGENT, "c1");

    expect(store.get("p1")?.text).toBe("mine\n");
    expect(store.isEdited("p1")).toBe(true);
  });

  it("takes a moved candidate when the buffer holds no edits", () => {
    store.adopt("p1", AGENT, "c1");
    store.adopt("p1", "rewritten elsewhere\n", "c2");

    expect(store.get("p1")?.text).toBe("rewritten elsewhere\n");
    expect(store.get("p1")?.origin).toBe("rewritten elsewhere\n");
    expect(store.get("p1")?.conflict).toBeNull();
  });

  it("raises a conflict when a moved candidate meets an edited buffer", () => {
    // DCR-FR-29: neither side is discarded, and neither is chosen for the
    // author.
    store.adopt("p1", AGENT, "c1");
    store.edit("p1", "mine\n");

    store.adopt("p1", "rewritten elsewhere\n", "c2");

    expect(store.get("p1")?.conflict).toEqual({ kind: "candidate" });
    expect(store.get("p1")?.text).toBe("mine\n");
  });
});

describe("writing the candidate (DCR-FR-26 / DCP-FR-25)", () => {
  it("writes once at the end of a burst, against the baseline it holds", async () => {
    store.adopt("p1", AGENT, "c1");
    store.edit("p1", "a\n");
    store.edit("p1", "ab\n");
    store.edit("p1", "abc\n");
    expect(saves()).toHaveLength(0);

    await vi.advanceTimersByTimeAsync(store.delay + 10);

    expect(saves()).toHaveLength(1);
    // The key is taken apart by the writer, so the operation names the change
    // rather than a key only this store understands.
    expect(saves()[0][1]).toEqual({
      proposalId: "p1",
      hunkId: "",
      after: "abc\n",
      baselineChecksum: "c1",
    });
    expect(store.get("p1")?.dirty).toBe(false);
    expect(store.get("p1")?.baseline).toBe("next");
  });

  it("keeps an edit made while the write was in flight, and schedules the next", async () => {
    // Clearing the dirty flag for bytes that were never written would leave the
    // edit unwritten with nothing coming to write it.
    let release!: (value: { checksum: string }) => void;
    invokeMock.mockImplementation(
      async () => new Promise<{ checksum: string }>((r) => (release = r)),
    );
    store.adopt("p1", AGENT, "c1");
    store.edit("p1", "first\n");
    await vi.advanceTimersByTimeAsync(store.delay + 10);
    expect(store.get("p1")?.saving).toBe(true);

    store.edit("p1", "second\n");
    release({ checksum: "c2" });
    await vi.advanceTimersByTimeAsync(0);

    expect(store.get("p1")?.dirty).toBe(true);
    expect(store.get("p1")?.text).toBe("second\n");
    invokeMock.mockImplementation(async () => ({ checksum: "c3" }));
    await vi.advanceTimersByTimeAsync(store.delay + 10);
    expect(saves()).toHaveLength(2);
    expect(store.get("p1")?.dirty).toBe(false);
  });

  it("never overlaps two writes of one candidate", async () => {
    // Two writes in flight at once can complete out of order: the older buffer
    // then lands last, and its checksum becomes the baseline with the record
    // clean — so the edit that was actually lost looks saved.
    const inFlight: Array<(value: { checksum: string }) => void> = [];
    let concurrent = 0;
    let peak = 0;
    invokeMock.mockImplementation(async () => {
      concurrent += 1;
      peak = Math.max(peak, concurrent);
      return new Promise<{ checksum: string }>((resolve) =>
        inFlight.push((v) => {
          concurrent -= 1;
          resolve(v);
        }),
      );
    });
    store.adopt("p1", AGENT, "c1");
    store.edit("p1", "one\n");
    const first = store.flush("p1");
    // Let the first write actually start before the second is asked for.
    await vi.advanceTimersByTimeAsync(0);
    expect(inFlight).toHaveLength(1);

    store.edit("p1", "two\n");
    const second = store.flush("p1");
    await vi.advanceTimersByTimeAsync(0);
    // The second is queued behind the first rather than started beside it.
    expect(inFlight).toHaveLength(1);

    inFlight[0]({ checksum: "c2" });
    await vi.advanceTimersByTimeAsync(0);
    expect(inFlight).toHaveLength(2);
    inFlight[1]({ checksum: "c3" });
    await Promise.all([first, second]);

    expect(peak).toBe(1);
    // And the second write carried the baseline the first established rather
    // than racing it with the stale one.
    expect(saves()).toHaveLength(2);
    expect(saves()[1][1]).toEqual(
      expect.objectContaining({ after: "two\n", baselineChecksum: "c2" }),
    );
  });

  it("keeps every edit when the write fails, and retries on the next one", async () => {
    invokeMock.mockImplementation(async () => {
      throw "write_failed";
    });
    store.adopt("p1", AGENT, "c1");
    store.edit("p1", "mine\n");
    await vi.advanceTimersByTimeAsync(store.delay + 10);

    expect(store.get("p1")?.error).toContain("write_failed");
    expect(store.get("p1")?.text).toBe("mine\n");
    expect(store.get("p1")?.dirty).toBe(true);
    // Nothing is retried on a timer.
    await vi.advanceTimersByTimeAsync(store.delay * 5);
    expect(saves()).toHaveLength(1);

    // The next edit clears the error and schedules the retry — without which
    // both decisions stay disabled for good (DCR-FR-11).
    invokeMock.mockImplementation(async () => ({ checksum: "c2" }));
    store.edit("p1", "mine, corrected\n");
    expect(store.get("p1")?.error).toBeNull();
    await vi.advanceTimersByTimeAsync(store.delay + 10);
    expect(store.get("p1")?.dirty).toBe(false);
  });

  it("raises a conflict rather than an error when storage has moved on", async () => {
    // DCP-FR-27: a stale baseline is not a failure to report and retry — it is
    // two candidates, and which survives is the author's to say.
    invokeMock.mockImplementation(async () => {
      throw "candidate_stale";
    });
    store.adopt("p1", AGENT, "c1");
    store.edit("p1", "mine\n");
    await vi.advanceTimersByTimeAsync(store.delay + 10);

    expect(store.get("p1")?.conflict).toEqual({ kind: "candidate" });
    expect(store.get("p1")?.error).toBeNull();
    // And a flush over an unresolved conflict writes nothing and reports the
    // candidate unsafe to decide over.
    expect(await store.flush("p1")).toBe(false);
    expect(saves()).toHaveLength(1);
  });
});

describe("resolving a conflict (DCR-FR-29)", () => {
  it("keeps what is on screen, against the baseline the refusal was about", async () => {
    store.adopt("p1", AGENT, "c1");
    store.edit("p1", "mine\n");
    store.get("p1")!.conflict = { kind: "candidate" };

    store.resolveCandidate("p1", "keep", {
      content: "theirs\n",
      checksum: "c9",
    });
    await vi.advanceTimersByTimeAsync(0);

    expect(store.get("p1")?.conflict).toBeNull();
    expect(store.get("p1")?.text).toBe("mine\n");
    expect(saves()[0][1]).toEqual(
      expect.objectContaining({ after: "mine\n", baselineChecksum: "c9" }),
    );
  });

  it("takes the stored version, discarding the edits with the conflict", () => {
    store.adopt("p1", AGENT, "c1");
    store.edit("p1", "mine\n");
    store.get("p1")!.conflict = { kind: "candidate" };

    store.resolveCandidate("p1", "take", {
      content: "theirs\n",
      checksum: "c9",
    });

    expect(store.get("p1")?.text).toBe("theirs\n");
    expect(store.get("p1")?.origin).toBe("theirs\n");
    expect(store.get("p1")?.dirty).toBe(false);
    expect(store.isEdited("p1")).toBe(false);
    // The history's floor moved with the text: one undo must not reinstate the
    // candidate the author chose to discard.
    store.traverse("p1", "undo");
    expect(store.get("p1")?.text).toBe("theirs\n");
  });

  it("raises a base conflict only for a candidate the author has edited", () => {
    store.adopt("p1", AGENT, "c1");
    store.raiseBaseConflict("p1");
    expect(store.get("p1")?.conflict).toBeNull();

    store.edit("p1", "mine\n");
    store.raiseBaseConflict("p1");
    expect(store.get("p1")?.conflict).toEqual({ kind: "base" });

    store.resolveBase("p1");
    expect(store.get("p1")?.conflict).toBeNull();
    expect(store.get("p1")?.text).toBe("mine\n");
  });
});

describe("the candidate's own history (DCR-FR-24)", () => {
  it("traverses back and forward without touching what storage holds", async () => {
    const { sealBurst } = await import("./editHistory");
    store.adopt("p1", AGENT, "c1");
    store.edit("p1", "one\n");
    sealBurst(store.get("p1")!.history);
    store.edit("p1", "two\n");
    sealBurst(store.get("p1")!.history);

    store.traverse("p1", "undo");
    expect(store.get("p1")?.text).toBe("one\n");
    store.traverse("p1", "redo");
    expect(store.get("p1")?.text).toBe("two\n");

    // A traversal is an edit like any other: unwritten, and scheduled.
    expect(store.get("p1")?.dirty).toBe(true);
    await vi.advanceTimersByTimeAsync(store.delay + 10);
    expect(saves()).toHaveLength(1);
  });

  it("is inert while a conflict stands", () => {
    store.adopt("p1", AGENT, "c1");
    store.edit("p1", "one\n");
    store.get("p1")!.conflict = { kind: "base" };
    store.traverse("p1", "undo");
    expect(store.get("p1")?.text).toBe("one\n");
  });
});

describe("dropping a buffer (DCR-FR-25)", () => {
  it("cancels the write it had scheduled", async () => {
    store.adopt("p1", AGENT, "c1");
    store.edit("p1", "mine\n");
    expect(store.hasPendingWrite("p1")).toBe(true);

    store.drop("p1");
    await vi.advanceTimersByTimeAsync(store.delay * 3);

    expect(store.get("p1")).toBeUndefined();
    expect(saves()).toHaveLength(0);
  });

  it("names every candidate a teardown still has to write", () => {
    store.adopt("p1", AGENT, "c1");
    store.adopt("p2", AGENT, "c1");
    store.edit("p2", "mine\n");
    expect(store.pendingKeys()).toEqual(["p2"]);
  });
});

// ---------------------------------------------------------------------------
// What a buffer is keyed by (DCP-FR-XDRV, DCR-FR-25)
// ---------------------------------------------------------------------------

describe("the key one change is buffered under", () => {
  it("DCR-FR-25: names the proposal and the change, and both come back out", () => {
    // Two changes of one proposal are two buffers, and a change of one proposal
    // is not a change of another — which is the whole reason the key is a pair.
    const a = hunkKey("p1", "h1");
    const b = hunkKey("p1", "h2");
    expect(a).not.toBe(b);
    expect(parseHunkKey(a)).toEqual({ proposalId: "p1", hunkId: "h1" });
    expect(parseHunkKey(b)).toEqual({ proposalId: "p1", hunkId: "h2" });
    expect(parseHunkKey(hunkKey("p2", "h1")).proposalId).toBe("p2");
  });

  it("DCR-FR-25: every buffer of one proposal is dropped together when it is decided", () => {
    store.adopt(hunkKey("p1", "h1"), AGENT, "c1");
    store.adopt(hunkKey("p1", "h2"), AGENT, "c1");
    store.adopt(hunkKey("p2", "h1"), AGENT, "c1");

    store.dropProposal("p1");

    expect(store.get(hunkKey("p1", "h1"))).toBeUndefined();
    expect(store.get(hunkKey("p1", "h2"))).toBeUndefined();
    // Another proposal's changes are untouched: a decision is about one
    // proposal, and dropping more than that would lose an author's unwritten
    // rewrite of a change nobody decided.
    expect(store.get(hunkKey("p2", "h1"))).toBeDefined();
  });
});
