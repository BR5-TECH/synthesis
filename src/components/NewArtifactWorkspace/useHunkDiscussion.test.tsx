/**
 * **Discuss** on one change of a proposal (`DCR-draft-change-review.md`
 * DCR-FR-MTFD, DCR-FR-JTXE, DCR-FR-VBQN; `DDS-draft-discussion.md` DDS-FR-RHKW;
 * `DCP-draft-change-proposals.md` DCP-FR-FHWN).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, renderHook, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import {
  seededBody,
  useHunkDiscussion,
} from "./useHunkDiscussion";
import { hunkDiscussionOf, openingHunkOf } from "../DraftDiscussion/hunkDiscussion";
import {
  draftDiscussionState,
  resetDraftDiscussions,
  setDraftDiscussionHidden,
} from "../../state/draftDiscussion";
import {
  clearAllDiscussionSessions,
  getDiscussionSession,
  setComposerBody,
} from "../../state/discussionSession";
import { resetProposalHunks } from "../../state/proposalHunks";
import { resetLogBufferForTest } from "../../logging";
import type {
  Attachment,
  Discussion,
  DraftChangeProposal,
  HunkState,
  ProposalHunkReference,
} from "../../types";

const KEY = "draft:d1";

function proposal(states: HunkState[] = ["pending", "rejected", "pending"]): DraftChangeProposal {
  return {
    id: "p1",
    draftId: "d1",
    path: "Spec.md",
    agent: { kind: "agent", agentId: "a1", handle: "arch", model: "m" },
    rationale: "why",
    threadId: "t1",
    commentId: "c1",
    state: "pending",
    candidateEdited: false,
    legacy: false,
    hunkCount: states.length,
    counts: { pending: 0, accepted: 0, rejected: 0, discussing: 0 },
    ledger: states.map((state, at) => ({
      id: `h${at + 1}`,
      kind: "replace",
      state,
      edited: false,
      revision: 0,
    })),
    createdAt: "2026-01-01T00:00:00Z",
  };
}

const reference = (hunkId: string): ProposalHunkReference => ({
  kind: "proposalHunk",
  proposalId: "p1",
  hunkId,
  draftId: "d1",
  path: "Spec.md",
});

function discussion(id: string, opening: Attachment[], later: Attachment[] = []): Discussion {
  const comment = (cid: string, attachments: Attachment[]) => ({
    id: cid,
    author: { kind: "human" as const, login: "raver119" },
    body: "about this",
    quotes: [],
    attachments,
    createdAt: "2026-01-01T00:00:00Z",
  });
  return {
    id,
    target: { kind: "draft", draftId: "d1" },
    fragmentTarget: null,
    comments: [comment(`${id}-c1`, opening), comment(`${id}-c2`, later)],
    resolved: false,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
  };
}

const callsTo = (cmd: string) =>
  invokeMock.mock.calls.filter((c) => c[0] === cmd).map((c) => c[1]);

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "append_log_records") return Promise.resolve();
    if (cmd === "set_draft_change_hunk_discussing") return Promise.resolve(proposal());
    if (cmd === "set_prompt_change_hunk_discussing") return Promise.resolve(proposal());
    if (cmd === "load_prompt_change_proposal_hunks") {
      return Promise.resolve({ hunks: [], resolutions: [], checksum: "c", legacy: false });
    }
    if (cmd === "load_draft_change_proposal_hunks") {
      return Promise.resolve({ hunks: [], resolutions: [], checksum: "c", legacy: false });
    }
    return Promise.reject(new Error(`unexpected command ${cmd}`));
  });
  resetDraftDiscussions();
  clearAllDiscussionSessions();
  resetProposalHunks();
  resetLogBufferForTest();
});

afterEach(() => {
  resetLogBufferForTest();
});

function mount(discussions: Discussion[] = [], p: DraftChangeProposal = proposal()) {
  const selectDiscussion = vi.fn();
  const focusComposer = vi.fn();
  const hook = renderHook(() =>
    useHunkDiscussion({ draftId: "d1", proposal: p, discussions, selectDiscussion, focusComposer }),
  );
  return { hook, selectDiscussion, focusComposer };
}

describe("DCR-FR-VBQN, DCP-FR-FHWN: which discussion is a change's", () => {
  it("DCR-FR-VBQN, DCP-FR-FHWN: the discussion whose opening comment names the change", () => {
    const own = discussion("d-own", [reference("h2")]);
    expect(hunkDiscussionOf([own], "p1", "h2")).toBe(own);
    expect(openingHunkOf(own)).toEqual(reference("h2"));
  });

  it("DCR-FR-VBQN: a later comment naming the change does not make it that change's discussion", () => {
    const other = discussion("d-other", [], [reference("h2")]);
    expect(hunkDiscussionOf([other], "p1", "h2")).toBeUndefined();
    expect(openingHunkOf(other)).toBeUndefined();
  });

  it("DCR-FR-VBQN: a change of another proposal is another change", () => {
    const own = discussion("d-own", [{ ...reference("h2"), proposalId: "p0" }]);
    expect(hunkDiscussionOf([own], "p1", "h2")).toBeUndefined();
  });
});

describe("DCR-FR-MTFD: the opening text starts with the proposing agent's tag", () => {
  it("DCR-FR-MTFD: an empty composer takes the tag alone", () => {
    expect(seededBody("arch", "")).toBe("@arch ");
  });

  it("DCR-FR-MTFD: text already typed is kept after the tag, and a tag already there is not doubled", () => {
    expect(seededBody("arch", "why this?")).toBe("@arch why this?");
    expect(seededBody("arch", "@arch why this?")).toBe("@arch why this?");
  });
});

describe("DCR-FR-MTFD, DDS-FR-RHKW: Discuss on a change", () => {
  it("DCR-FR-MTFD, DDS-FR-RHKW: opens the composer of a new discussion about that change", () => {
    act(() => setDraftDiscussionHidden("d1", true));
    const { hook, selectDiscussion, focusComposer } = mount();

    act(() => hook.result.current.discuss("h2"));

    expect(hook.result.current.opening).toEqual({
      reference: reference("h2"),
      position: 2,
      total: 3,
    });
    // DDS-FR-RHKW: the column comes back and the caret goes into the composer.
    expect(draftDiscussionState("d1").hidden).toBe(false);
    expect(focusComposer).toHaveBeenCalled();
    expect(selectDiscussion).not.toHaveBeenCalled();
    // DCR-FR-MTFD: the text starts with the proposing agent's tag.
    expect(getDiscussionSession(KEY).body).toBe("@arch ");
    // DCR-FR-JTXE: Discuss itself decides nothing and holds nothing.
    expect(callsTo("set_draft_change_hunk_discussing")).toHaveLength(0);
    expect(draftDiscussionState("d1").focusedHunkId).toBe("h2");
  });

  it("DCR-FR-MTFD: keeps what the author already typed in the opening composer", () => {
    setComposerBody(KEY, "half a thought");
    const { hook } = mount();
    act(() => hook.result.current.discuss("h1"));
    expect(getDiscussionSession(KEY).body).toBe("@arch half a thought");
  });

  it("DCR-FR-VBQN: a change that has a discussion shows that one instead of opening a second", () => {
    const own = discussion("d-own", [reference("h2")]);
    const { hook, selectDiscussion, focusComposer } = mount([own]);

    act(() => hook.result.current.discuss("h2"));

    expect(selectDiscussion).toHaveBeenCalledWith("d-own");
    expect(focusComposer).toHaveBeenCalled();
    expect(hook.result.current.opening).toBeNull();
  });

  it("DDS-FR-RHKW: discarding the composer drops the opening", () => {
    const { hook } = mount();
    act(() => hook.result.current.discuss("h1"));
    act(() => hook.result.current.cancel());
    expect(hook.result.current.opening).toBeNull();
  });
});

describe("DCR-FR-JTXE: once the discussion exists", () => {
  it("DCR-FR-JTXE: an undecided change is held for its discussion", async () => {
    const { hook } = mount();
    act(() => hook.result.current.discuss("h1"));
    act(() => hook.result.current.opened(discussion("d-new", [reference("h1")])));

    expect(hook.result.current.opening).toBeNull();
    await waitFor(() =>
      expect(callsTo("set_draft_change_hunk_discussing")).toEqual([
        { proposalId: "p1", hunkId: "h1", discussing: true },
      ]),
    );
  });

  it("DCR-FR-JTXE: a rejected change stays rejected, so nothing is held", async () => {
    const { hook } = mount();
    act(() => hook.result.current.discuss("h2"));
    act(() => hook.result.current.opened(discussion("d-new", [reference("h2")])));
    // Let anything the call might have started settle before asserting it never started.
    await act(async () => {});
    expect(callsTo("set_draft_change_hunk_discussing")).toHaveLength(0);
  });

  it("DCR-FR-JTXE: a discussion that is about no change holds nothing", async () => {
    const { hook } = mount();
    act(() => hook.result.current.opened(discussion("d-plain", [])));
    await act(async () => {});
    expect(callsTo("set_draft_change_hunk_discussing")).toHaveLength(0);
  });
});

describe("PCR-FR-HDMC, DDS-FR-EHDK: Discuss on a change in a file's Editor tab", () => {
  const ARTIFACT = "docs/guide.md";
  function mountArtifact(discussions: Discussion[] = []) {
    const selectDiscussion = vi.fn();
    const focusComposer = vi.fn();
    const showColumn = vi.fn();
    const p = { ...proposal(), path: ARTIFACT };
    const hook = renderHook(() =>
      useHunkDiscussion({
        artifactId: ARTIFACT,
        proposal: p,
        discussions,
        selectDiscussion,
        focusComposer,
        showColumn,
      }),
    );
    return { hook, selectDiscussion, focusComposer, showColumn };
  }

  it("PCR-FR-HDMC, CMS-FR-TQVB: opens the composer of a discussion of the file, seeded with the agent tag, naming the change by artifact", () => {
    const { hook, showColumn, focusComposer } = mountArtifact();
    act(() => hook.result.current.discuss("h3"));
    expect(showColumn).toHaveBeenCalled();
    expect(focusComposer).toHaveBeenCalled();
    expect(hook.result.current.opening?.reference).toEqual({
      kind: "proposalHunk",
      proposalId: "p1",
      hunkId: "h3",
      artifactId: ARTIFACT,
      path: ARTIFACT,
    });
    expect(hook.result.current.opening?.reference.draftId).toBeUndefined();
    expect(getDiscussionSession(`artifact:${ARTIFACT}`).body).toBe("@arch ");
  });

  it("PCR-FR-HDMC: posting holds the change through the prompt module's own command", async () => {
    const { hook } = mountArtifact();
    act(() => hook.result.current.discuss("h1"));
    const opening = hook.result.current.opening!.reference;
    act(() => hook.result.current.opened(discussion("d-new", [opening])));
    await waitFor(() =>
      expect(callsTo("set_prompt_change_hunk_discussing")).toEqual([
        { proposalId: "p1", hunkId: "h1", discussing: true },
      ]),
    );
    expect(callsTo("set_draft_change_hunk_discussing")).toHaveLength(0);
  });

  it("PCR-FR-HDMC: a change that has a discussion shows that one instead of opening a second", () => {
    const existing = discussion("d-old", [
      { kind: "proposalHunk", proposalId: "p1", hunkId: "h1", artifactId: ARTIFACT, path: ARTIFACT },
    ]);
    const { hook, selectDiscussion } = mountArtifact([existing]);
    act(() => hook.result.current.discuss("h1"));
    expect(selectDiscussion).toHaveBeenCalledWith("d-old");
    expect(hook.result.current.opening).toBeNull();
  });
});
