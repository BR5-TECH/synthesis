/**
 * The store of the split of each change
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-BGDN).
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { resetLogBufferForTest } from "../../logging";
import { newChangeProposal } from "../../test/changeWorkspaceFixtures";
import type { NewChangeProposalList } from "../../types";
import {
  discussNewChange,
  newChangesOf,
  noteNewChangeChanged,
  registerNewChangeDiscussRoute,
  rereadNewChanges,
  resetNewChangeProposals,
} from "./newChangeProposals";

/** Each list call waits until the test answers it. */
let answers: Array<(list: NewChangeProposalList) => void>;

beforeEach(() => {
  resetLogBufferForTest();
  resetNewChangeProposals();
  answers = [];
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "append_log_records") return Promise.resolve(null);
    if (cmd === "list_new_change_proposals") {
      return new Promise((resolve) => answers.push(resolve));
    }
    return Promise.reject(new Error(`unexpected invoke ${cmd}`));
  });
});

describe("the split store", () => {
  it("OCW-FR-BGDN: an event during a read keeps its proposal over the read's answer", async () => {
    const read = rereadNewChanges("s-1", "c");
    noteNewChangeChanged({ streamId: "s-1", change: "c", proposal: newChangeProposal({ state: "accepted" }), cause: "decided" });
    answers[0]({ proposals: [newChangeProposal()], split: null });
    await read;
    expect(newChangesOf("s-1", "c").proposals.map((p) => p.state)).toEqual(["accepted"]);
  });

  it("OCW-FR-BGDN: an older answer that lands after a newer one is dropped", async () => {
    const first = rereadNewChanges("s-1", "c");
    const second = rereadNewChanges("s-1", "c");
    answers[1]({ proposals: [], split: { setId: "x", accepted: ["a"], confirmedAt: "2026-01-01T00:00:00Z" } });
    await second;
    answers[0]({ proposals: [], split: { setId: "x", accepted: [] } });
    await first;
    expect(newChangesOf("s-1", "c").split?.confirmedAt).toBe("2026-01-01T00:00:00Z");
  });

  it("OCW-FR-BGDN: Discuss asked for before the tab is open waits for its route, and a route removes only itself", () => {
    const proposal = newChangeProposal();
    expect(discussNewChange(proposal)).toBe(false);
    const route = vi.fn();
    const stop = registerNewChangeDiscussRoute(proposal.streamId, proposal.change, route);
    expect(route).toHaveBeenCalledWith(proposal);
    expect(discussNewChange(proposal)).toBe(true);
    expect(route).toHaveBeenCalledTimes(2);
    const other = vi.fn();
    const stopOther = registerNewChangeDiscussRoute(proposal.streamId, proposal.change, other);
    stop();
    expect(discussNewChange(proposal)).toBe(true);
    expect(other).toHaveBeenCalledTimes(1);
    stopOther();
  });
});
