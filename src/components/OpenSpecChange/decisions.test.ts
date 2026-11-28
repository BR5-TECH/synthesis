/**
 * What a decision on an OpenSpec change says to the author, in words
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-DTRX,
 * OCW-FR-BGDN, OCW-FR-IJAI; `../../../specifications/core/PVC-product-vocabulary.md`
 * PVC-FR-VBPY, PVC-FR-NEXL).
 */
import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (cmd: string, args?: unknown) => invokeMock(cmd, args),
}));

import { changeDecisionMessage, streamBusyMessage } from "./decisions";
import { newChangeBusyMessage, newChangeMessage } from "./newChangeDecisions";
import { CHANGE_PROPOSAL_ERRORS, NEW_CHANGE_ERRORS } from "../../types";

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "append_log_records") return undefined;
    // No run holds the stream, so the statement names no run.
    if (cmd === "list_graduation_queue") return { runs: [] };
    throw new Error(`unexpected command ${cmd}`);
  });
});

describe("PVC-FR-VBPY: a refusal at a hunk names the hunk an edit", () => {
  it("OCW-FR-DTRX: every refusal of a decision calls the hunk an edit, never a change", () => {
    const E = CHANGE_PROPOSAL_ERRORS;
    expect(changeDecisionMessage(E.hunkAlreadyDecided)).toBe("This edit has already been decided.");
    expect(changeDecisionMessage(E.acceptanceInProgress)).toBe(
      "Another edit to this proposal is still being applied. Try again in a moment.",
    );
    expect(changeDecisionMessage(E.changeNotFound)).toBe(
      "The OpenSpec change is no longer on its stream, so this edit cannot be applied.",
    );
    for (const code of [...Object.values(E), "something_else"]) {
      const text = changeDecisionMessage(code);
      // PVC-FR-NEXL: a bare "change" is never the word for a hunk here.
      expect(text.replace(/OpenSpec change/g, "")).not.toMatch(/\bchange\b/);
    }
  });

  it("OCW-FR-DTRX, CEP-FR-ESQR: stream_busy says the edit was not applied", async () => {
    expect(await streamBusyMessage("s1")).toBe(
      "An implementation run holds this stream, so this edit was not applied. Try again when the run stops working.",
    );
  });
});

describe("PVC-FR-NEXL: a refusal of a new-change decision names the OpenSpec change", () => {
  it("OCW-FR-BGDN: each statement that names the change says OpenSpec change", () => {
    const E = NEW_CHANGE_ERRORS;
    expect(newChangeMessage(E.alreadyDecided)).toBe(
      "This proposed OpenSpec change has already been decided.",
    );
    expect(newChangeMessage(E.pending)).toBe(
      "A proposed OpenSpec change is still waiting for a decision. Decide it first.",
    );
    for (const code of [...Object.values(E), "something_else"]) {
      const text = newChangeMessage(code);
      expect(text.replace(/OpenSpec change/g, "")).not.toMatch(/\bchange\b/);
    }
  });

  it("OCW-FR-IJAI: stream_busy says the OpenSpec change was not created", async () => {
    expect(await newChangeBusyMessage("s1")).toBe(
      "An implementation run holds this stream, so the OpenSpec change was not created. Try again when the run stops working.",
    );
  });
});
