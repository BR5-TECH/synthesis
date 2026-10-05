/**
 * The sentences the update resolution window and the stream row are both
 * written from (`../../../specifications/ui/WSS-work-stream-selector.md`
 * WSS-FR-ZMPC, WSS-FR-GTQL).
 *
 * Pure functions, tested directly: each of the record's seven states reads back
 * differently, and each pair of side changes names what really happened to that
 * path. Rendering covers only the states a row can reach, so a sentence that
 * silently became another sentence would otherwise go unnoticed.
 */

import { describe, expect, it } from "vitest";

import { fateSentence, updateStateSentence } from "./index";
import { updateRecord } from "../../test/streamFixtures";
import type { StreamUpdateState } from "../../types";

describe("what an update rests on, in words (WSS-FR-ZMPC)", () => {
  it("WSS-FR-ZMPC: every state reads back as its own sentence", () => {
    const said = (
      [
        "running",
        "updated",
        "nothing_to_update",
        "conflicted",
        "escalated",
        "cancelled",
        "failed",
      ] as StreamUpdateState[]
    ).map((state) => updateStateSentence(updateRecord(state)));

    // No two states say the same thing, which is what makes the sentence the
    // whole of what the row and the window report.
    expect(new Set(said).size).toBe(said.length);
    for (const sentence of said) expect(sentence.length).toBeGreaterThan(0);
  });

  it("WSS-FR-ZMPC: an update that stopped says nothing was written", () => {
    const conflicted = updateStateSentence(
      updateRecord("conflicted", {
        conflicts: [{ path: "a.ts", baseChange: "updated", streamChange: "updated" }],
      }),
    );
    expect(conflicted).toMatch(/1 path\b/);
    expect(conflicted).toMatch(/Nothing was written/);

    const asked = updateStateSentence(
      updateRecord("escalated", {
        escalation: {
          reason: "r",
          raisedAt: "2026-05-01T00:01:00Z",
          origin: "semantic_merge",
          questions: [
            { position: 1, question: "q1", options: [] },
            { position: 2, question: "q2", options: [] },
          ],
        },
      }),
    );
    expect(asked).toMatch(/asked you 2 questions/);
    expect(asked).toMatch(/Nothing was written/);
  });

  it("WSS-FR-ZMPC: a stream with nothing to update names its base branch", () => {
    expect(
      updateStateSentence(updateRecord("nothing_to_update", { baseBranch: "dev" })),
    ).toMatch(/already holds everything dev does/);
  });

  it("WSS-FR-NPXC: a failed update reads its typed refusal as a sentence", () => {
    expect(
      updateStateSentence(updateRecord("failed", { failure: "stream_dirty" })),
    ).toMatch(/holds work that is not committed/);
  });

  it("WSS-FR-ZMPC: a cancelled update says neither branch changed", () => {
    expect(updateStateSentence(updateRecord("cancelled"))).toMatch(
      /Neither branch was changed/,
    );
  });
});

describe("what either side did to one path (GEA-FR-ZRGP)", () => {
  const fate = (baseChange: string, streamChange: string) =>
    fateSentence({ path: "a.ts", baseChange, streamChange });

  it("GEA-FR-ZRGP: a deletion on either side is named as one", () => {
    expect(fate("deleted", "updated")).toBe(
      "the base branch deleted it; the stream changed it",
    );
    expect(fate("updated", "deleted")).toBe(
      "the stream deleted it; the base branch changed it",
    );
  });

  it("GEA-FR-ZRGP: two sides that each made the path say so", () => {
    expect(fate("created", "created")).toBe("both sides created it");
  });

  it("GEA-FR-ZRGP: the ordinary case is both sides changing it", () => {
    expect(fate("updated", "updated")).toBe("both sides changed it");
  });

  it("GEA-FR-ZRGP: a record that names no side change says nothing rather than guessing", () => {
    // An attempt that could not be planned reports its paths with no sides, so
    // the row and the window name the path alone.
    expect(fate("", "")).toBe("");
  });
});
