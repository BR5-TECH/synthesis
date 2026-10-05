import { describe, expect, it } from "vitest";
import {
  activityStatus,
  THINKING,
  toolActivityStatus,
  TOOL_ACTIVITY_STATUS,
  WORKING,
} from "./agentActivity";
import type { ActiveToolCall } from "../types";

function call(tool: string, activationSeq: number): ActiveToolCall {
  return { id: `call-${activationSeq}`, tool, activationSeq };
}

describe("CTA-FR-HEYB: the activity vocabulary", () => {
  it("reads Thinking… while the turn waits on a model response", () => {
    // CTA-FR-FBJR, CTA-FR-IHOB's first clause: a turn with no tool call active in it.
    expect(activityStatus({ activeToolCalls: [] })).toBe("Thinking…");
    expect(THINKING).toBe("Thinking…");
  });

  it("gives every tool a conversation turn is lent its own fixed wording", () => {
    // CTA-FR-FBJR, CTA-FR-IHOB: the whole of the vocabulary, and never an empty line.
    const vocabulary: Array<[string, string]> = [
      ["search_specifications", "Searching related specifications…"],
      ["read_file", "Reading a project file…"],
      ["search_drafts", "Searching drafts…"],
      ["read_draft", "Reading a draft…"],
      ["search_notes", "Searching notes…"],
      ["search_skills", "Searching available skills…"],
      ["list_skills", "Listing available skills…"],
      ["load_skill", "Loading skill instructions…"],
      ["ask_user_comment", "Asking a clarifying question…"],
      ["ask_discussion_questions", "Preparing questions for you…"],
      ["propose_draft_changes", "Preparing draft changes…"],
      ["propose_prompt_changes", "Preparing prompt changes…"],
      ["openrouter:web_search", "Searching the web…"],
      ["openrouter:web_fetch", "Fetching from the web…"],
    ];
    for (const [tool, status] of vocabulary) {
      expect(activityStatus({ activeToolCalls: [call(tool, 1)] })).toBe(status);
      expect(status).not.toBe("");
    }
    // Exactly this and no more: a tool no conversation turn carries has no
    // status here, the graduation loop's own tools among them.
    expect(Object.keys(TOOL_ACTIVITY_STATUS).sort()).toEqual(
      vocabulary.map(([tool]) => tool).sort(),
    );
    expect(Object.keys(TOOL_ACTIVITY_STATUS)).toHaveLength(14);
  });

  it("reads Working… for a tool the vocabulary does not name", () => {
    // CTA-FR-FBJR, CTA-FR-IHOB's last clause / TLC-FR-27, CTA-FR-VSIM: non-empty, and it names neither the
    // tool nor any argument of the call.
    const status = activityStatus({
      activeToolCalls: [call("a_tool_this_surface_was_never_taught", 1)],
    });
    expect(status).toBe("Working…");
    expect(status).toBe(WORKING);
    expect(status).not.toContain("a_tool_this_surface_was_never_taught");
    expect(toolActivityStatus("a_tool_nobody_has_written_yet")).toBe("Working…");
  });

  it("takes its wording from this surface rather than from the tool", () => {
    // TLC-FR-27, CTA-FR-VSIM: the status a call reads is fixed here — nothing about the call
    // itself can compose one, the table being keyed by the tool's name alone.
    expect(toolActivityStatus("read_file")).toBe("Reading a project file…");
    expect(
      activityStatus({
        activeToolCalls: [
          // A call carrying nothing but a name, an id, and an order — which is
          // all a turn ever carries (AGC-FR-33).
          { id: "call-9", tool: "read_file", activationSeq: 9 },
        ],
      }),
    ).toBe("Reading a project file…");
  });
});

describe("CTA-FR-IWOJ: which of several active calls is spoken for", () => {
  it("reads the most recently activated of them", () => {
    // CTA-FR-IWOJ, CTA-FR-KWOF: one model response activated `read_file` and then
    // `search_skills`, both still active.
    expect(
      activityStatus({
        activeToolCalls: [call("read_file", 1), call("search_skills", 2)],
      }),
    ).toBe("Searching available skills…");
  });

  it("reads the turn's own activation order rather than the arrival order", () => {
    // CTA-FR-IWOJ, CTA-FR-KWOF's second clause: two events announcing those calls reaching
    // the rail in the opposite order settle on the same status either way.
    expect(
      activityStatus({
        activeToolCalls: [call("search_skills", 2), call("read_file", 1)],
      }),
    ).toBe("Searching available skills…");
  });

  it("hands the line to the latest call still active when one finishes", () => {
    // CTA-FR-IVNG, CTA-FR-JQUU: the web search finishes, and the contribution reads the
    // earlier call's status at once rather than being cleared or held.
    const both = [call("read_file", 1), call("openrouter:web_search", 2)];
    expect(activityStatus({ activeToolCalls: both })).toBe("Searching the web…");
    const afterSearch = both.filter((c) => c.tool !== "openrouter:web_search");
    expect(activityStatus({ activeToolCalls: afterSearch })).toBe(
      "Reading a project file…",
    );
    // And when the `read_file` call then refuses, the turn is waiting on its
    // next model response again.
    expect(activityStatus({ activeToolCalls: [] })).toBe("Thinking…");
  });

  it("lands on exactly one non-empty status at every transition", () => {
    // CTA-FR-JQUU: there is no moment at which a pending contribution reads
    // nothing — an unrecognised tool reads Working… rather than an empty line.
    const transitions: ActiveToolCall[][] = [
      [],
      [call("read_file", 1)],
      [call("read_file", 1), call("a_tool_nobody_has_written_yet", 2)],
      [call("a_tool_nobody_has_written_yet", 2)],
      [],
    ];
    for (const activeToolCalls of transitions) {
      const status = activityStatus({ activeToolCalls });
      expect(status).not.toBe("");
      expect(status.trim()).not.toBe("");
    }
    expect(
      transitions.map((activeToolCalls) => activityStatus({ activeToolCalls })),
    ).toEqual([
      "Thinking…",
      "Reading a project file…",
      "Working…",
      "Working…",
      "Thinking…",
    ]);
  });

  it("survives a turn carrying no list at all", () => {
    // A payload from a build that predates AGC-FR-33 reads as a turn with
    // nothing active rather than throwing inside a render.
    expect(
      activityStatus({
        activeToolCalls: undefined as unknown as ActiveToolCall[],
      }),
    ).toBe("Thinking…");
  });
});
