/**
 * The tool-list disclosure and the activity row (`CTA-comment-agent-turns.md`
 * CTA-FR-GGYJ, CTA-FR-BJWN, CTA-FR-FPNP, CTA-FR-QMWO).
 */
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

import { ActivityRows, ToolDisclosure } from "./ToolActivity";
import type { TurnToolCall } from "../../types";

afterEach(cleanup);

const calls: TurnToolCall[] = [
  { id: "call-1", tool: "search_specifications", activationSeq: 1, state: "done", keyInput: "**tabs** <b>bold</b>" },
  { id: "call-2", tool: "list_skills", activationSeq: 2, state: "failed", keyInput: null },
  { id: "call-3", tool: "openrouter:web_fetch", activationSeq: 3, state: "running", keyInput: "https://example.com/a" },
];

describe("CTA-FR-GGYJ: the disclosure control", () => {
  it("is collapsed when it first renders and toggles by click, and is a native button for Enter and Space", () => {
    render(<ToolDisclosure label="Searching…" calls={calls} testId="toggle" />);
    const toggle = screen.getByTestId("toggle");
    expect(toggle.tagName).toBe("BUTTON");
    expect(toggle).toHaveAttribute("type", "button");
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(toggle).toHaveAttribute("aria-controls", screen.getByTestId("tool-call-list").parentElement!.id);
    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByTestId("tool-call-list")).toBeNull();
  });
});

describe("CTA-FR-BJWN / CTA-FR-FPNP: the entries", () => {
  it("show the tool, the key input as plain text, and the state as a word", () => {
    render(<ToolDisclosure label="Working…" calls={calls} testId="toggle" />);
    fireEvent.click(screen.getByTestId("toggle"));
    const entries = screen.getAllByTestId("tool-call-entry");
    expect(entries).toHaveLength(3);
    // Neither Markdown nor HTML: the text stands as written.
    expect(screen.getAllByTestId("tool-call-input")[0].textContent).toBe("**tabs** <b>bold</b>");
    expect(entries[0].querySelector("b")).toBeNull();
    expect(entries[0].querySelector("strong")).toBeNull();
    // A call with no key input shows its name and state alone.
    expect(entries[1].textContent).toBe("list_skillsfailed");
    expect(screen.getAllByTestId("tool-call-state").map((s) => s.textContent)).toEqual([
      "done",
      "failed",
      "running",
    ]);
  });
});

describe("CTA-FR-QMWO: an activity row", () => {
  it("reads the agent and the count, and carries no timestamp, cancel, or Retry", () => {
    render(<ActivityRows rows={[{ turnId: "t1", agentId: "a", nickname: "arch", calls }]} />);
    const row = screen.getByTestId("activity-row");
    expect(row).toHaveTextContent("@arch called 3 tools");
    expect(row.querySelectorAll("button")).toHaveLength(1);
    expect(row.querySelector("time, .comment__time")).toBeNull();
    expect(screen.queryByRole("button", { name: /retry|cancel/i })).toBeNull();
  });

  it("renders nothing where there is no row", () => {
    const { container } = render(<ActivityRows rows={[]} />);
    expect(container).toBeEmptyDOMElement();
  });
});
