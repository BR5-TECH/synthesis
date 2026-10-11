/**
 * A proposal whose changes quote Markdown is drawn in the draft's document, and
 * the review bar says "not shown" only for a change the document cannot hold
 * (`../../specifications/ui/DCR-draft-change-review.md` DCR-FR-LGHZ,
 * DCR-FR-TSNW, DCR-FR-31).
 *
 * Driven through the whole tab, because the defect lived in the seam: the
 * placement found nothing, the bar reported it, and each half on its own looked
 * right.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, screen, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

import {
  PROMPT,
  draft,
  freshHistory,
  makeStubs,
  renderWorkspace,
} from "../test/newArtifactFixtures";
import { otherCommand } from "../test/invokeMocks";
import { hunkCandidateBuffers } from "../state/candidateBuffers";
import { resetDraftProposals } from "../state/draftProposals";
import { resetDraftDiscussions, setFocusedHunk } from "../state/draftDiscussion";
import { resetProposalHunks } from "../state/proposalHunks";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { resetDiscussionFocus } from "../state/discussionFocus";
import { resetLogBufferForTest } from "../logging";

const { stub } = makeStubs(invokeMock);

/** The shape of the proposal an author reported, with invented text. */
const BODY = [
  "## Purpose",
  "",
  "The tool should talk to every mirror.",
  "",
  "## Steps",
  "",
  "- User types a mirror, with a placeholder of [mirror.org](http://mirror.org) ",
  "",
  "## Rules",
  "",
  "- [mirror.org](http://mirror.org) stays the default. Hosts like \\*.mirror.net are an option.",
  "- Saved mirrors keep working since [mirror.org](http://mirror.org) is a default.",
].join("\n");

const AGENT = { kind: "agent", agentId: "a1", handle: "arch", model: "m" };

/** An insertion after `lead`, as the backend records one. */
function insertion(id: string, lead: string) {
  const end = BODY.indexOf(lead) + lead.length;
  return {
    id,
    kind: "add",
    revision: 0,
    before: "",
    after: `Added by ${id}.`,
    anchor: { lead, trail: "", hint_start: end, hint_end: end },
  };
}

const HUNKS = [
  insertion("h1", "## Purpose\n\nThe tool should talk to every mirror."),
  insertion(
    "h2",
    "## Steps\n\n- User types a mirror, with a placeholder of [mirror.org](http://mirror.org) ",
  ),
  // Cut inside a link destination, as the backend cuts a lead at a fixed length.
  insertion("h3", BODY.slice(BODY.indexOf("rror.org) stays the default."))),
];

/** A replacement whose text quotes a link and an escape. */
const LINKED = {
  id: "h4",
  kind: "replace",
  revision: 0,
  before: "[mirror.org](http://mirror.org) stays the default. Hosts like \\*.mirror.net are an option.",
  after: "Only mirror.org is the default.",
  anchor: { lead: "", trail: "", hint_start: 0, hint_end: 0 },
};

/** A change whose lead the prompt does not hold. */
const ABSENT = {
  ...insertion("h5", "## Purpose"),
  anchor: { lead: "A sentence the prompt never held.", trail: "", hint_start: 0, hint_end: 0 },
};

function backend(hunks: { id: string }[]) {
  const proposal = {
    id: "p1",
    draftId: "d1",
    path: PROMPT,
    agent: AGENT,
    rationale: "Record the decisions.",
    threadId: "t1",
    commentId: "c1",
    state: "pending",
    candidateEdited: false,
    legacy: false,
    hunkCount: hunks.length,
    counts: { pending: hunks.length, accepted: 0, rejected: 0, discussing: 0 },
    ledger: hunks.map((h) => ({
      id: h.id,
      kind: (h as { kind?: string }).kind ?? "add",
      state: "pending",
      edited: false,
      revision: 0,
    })),
    createdAt: "2026-01-01T00:00:00Z",
  };
  invokeMock.mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case "open_draft":
        return draft();
      case "list_draft_history":
        return freshHistory();
      case "load_draft_file_contents":
        return { body: BODY, checksum: "c1" };
      case "save_draft_file_contents":
        return { checksum: "c2" };
      case "list_draft_change_proposals":
        return [proposal];
      case "load_draft_change_proposal_hunks":
        return {
          hunks,
          resolutions: hunks.map((_, i) => ({ kind: "resolved", start: i, end: i })),
          checksum: "hc-1",
          legacy: false,
        };
      default:
        return otherCommand(cmd);
    }
  });
}

beforeEach(() => {
  invokeMock.mockReset();
  hunkCandidateBuffers.clear();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  resetDraftProposals();
  resetDraftDiscussions();
  resetProposalHunks();
  resetLogBufferForTest();
  stub();
});

afterEach(cleanup);

/** Wait until every change named in `ids` is drawn in the document. */
async function drawn(ids: string[]) {
  await screen.findByRole("region", { name: "Proposed changes" });
  await waitFor(() => {
    for (const id of ids) {
      expect(document.querySelector(`[data-hunk="${id}"]`)).not.toBeNull();
    }
  });
}

describe("a proposal whose changes quote Markdown (DCR-FR-LGHZ, DCR-FR-TSNW)", () => {
  it("DCR-FR-LGHZ, DCR-FR-TSNW, DCR-FR-31: every change is drawn and none is reported as not shown", async () => {
    backend([...HUNKS, LINKED]);
    renderWorkspace();
    await drawn(["h1", "h2", "h3", "h4"]);
    for (const id of ["h1", "h2", "h3", "h4"]) {
      act(() => setFocusedHunk("d1", id));
      await screen.findByText(new RegExp(`reviewing change ${id.slice(1)} of 4`, "i"));
      expect(screen.queryByText(/not shown in the document/i)).toBeNull();
    }
  });

  it("DCR-FR-31: a change the document does not hold is still reported as not shown", async () => {
    backend([...HUNKS, ABSENT]);
    renderWorkspace();
    await drawn(["h1", "h2", "h3"]);
    act(() => setFocusedHunk("d1", "h5"));
    expect(await screen.findByText(/not shown in the document/i)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /accept all/i })).toBeEnabled();
    expect(screen.getByRole("button", { name: /reject all/i })).toBeEnabled();
  });
});
