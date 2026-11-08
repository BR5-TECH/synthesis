import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { makeRun } from "../../test/graduationFixtures";
import type { GraduationDecision, GraduationRun } from "../../types";
import { EscalationsLog } from "./escalationsLog";

afterEach(cleanup);

const decision = (over: Partial<GraduationDecision> = {}): GraduationDecision => ({
  question: "Which limit stands?",
  answer: "The base limit.",
  summary: "Base",
  turn: "work",
  pass: 1,
  ...over,
});

const runWith = (decisions?: GraduationDecision[], id = "r1"): GraduationRun =>
  makeRun(id, "working", { checkpoint: { decisions } });

describe("Escalations log", () => {
  it("GRU-FR-NQLG: shows nothing while the ledger is empty or absent", () => {
    const { rerender } = render(<EscalationsLog run={runWith([])} />);
    expect(screen.queryByTestId("graduation-escalations-log")).toBeNull();
    rerender(<EscalationsLog run={runWith(undefined)} />);
    expect(screen.queryByTestId("graduation-escalations-log")).toBeNull();
  });

  it("GRU-FR-WMTH: is collapsed by default, counts the decisions and expands and collapses", async () => {
    const user = userEvent.setup();
    render(<EscalationsLog run={runWith([decision(), decision({ pass: 2 })])} />);
    const toggle = screen.getByTestId("graduation-escalations-log-toggle");
    expect(toggle).toHaveTextContent("Escalations log · 2 decisions");
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByTestId("graduation-escalations-rows")).toBeNull();

    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(screen.getByTestId("graduation-escalations-rows")).toBeInTheDocument();

    await user.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByTestId("graduation-escalations-rows")).toBeNull();
  });

  it("GRU-FR-WMTH: says one decision in the singular", () => {
    render(<EscalationsLog run={runWith([decision()])} />);
    expect(screen.getByTestId("graduation-escalations-log-toggle")).toHaveTextContent(
      "Escalations log · 1 decision",
    );
  });

  it("GRU-FR-DRPW: shows each row in ledger order, a repeated question twice, and no control that changes it", async () => {
    const user = userEvent.setup();
    render(
      <EscalationsLog
        run={runWith([
          decision({ question: "First?", answer: "A1", turn: "work", pass: 1 }),
          decision({
            question: "Second?",
            answer: "A2",
            note: "Because of the audit.",
            turn: "merge_review",
            pass: 2,
          }),
          decision({ question: "First?", answer: "A3", turn: "review", pass: 3 }),
        ])}
      />,
    );
    await user.click(screen.getByTestId("graduation-escalations-log-toggle"));
    const rows = screen.getAllByTestId("graduation-escalations-row");
    expect(rows).toHaveLength(3);
    expect(within(rows[0]).getByText("First?")).toBeInTheDocument();
    expect(within(rows[0]).getByText("A1")).toBeInTheDocument();
    expect(within(rows[0]).getByText("Work turn · pass 1")).toBeInTheDocument();
    expect(within(rows[1]).getByText("Second?")).toBeInTheDocument();
    expect(within(rows[1]).getByText("Note: Because of the audit.")).toBeInTheDocument();
    expect(within(rows[1]).getByText("Merge review turn · pass 2")).toBeInTheDocument();
    expect(within(rows[2]).getByText("First?")).toBeInTheDocument();
    expect(within(rows[2]).getByText("A3")).toBeInTheDocument();
    expect(within(rows[2]).getByText("Review turn · pass 3")).toBeInTheDocument();
    expect(within(screen.getByTestId("graduation-escalations-log")).queryAllByRole("button"))
      .toHaveLength(1);
  });

  it("GRU-FR-DRPW: renders the text of a decision as text and not as markup", async () => {
    const user = userEvent.setup();
    render(
      <EscalationsLog run={runWith([decision({ answer: "<b>bold</b> **x**" })])} />,
    );
    await user.click(screen.getByTestId("graduation-escalations-log-toggle"));
    expect(screen.getByText("<b>bold</b> **x**")).toBeInTheDocument();
  });

  it("GRU-FR-DRPW: updates when the run state is published", async () => {
    const user = userEvent.setup();
    const { rerender } = render(<EscalationsLog run={runWith([decision()])} />);
    await user.click(screen.getByTestId("graduation-escalations-log-toggle"));
    rerender(<EscalationsLog run={runWith([decision(), decision({ question: "New?" })])} />);
    expect(screen.getAllByTestId("graduation-escalations-row")).toHaveLength(2);
    expect(screen.getByTestId("graduation-escalations-log-toggle")).toHaveTextContent("2 decisions");
  });
});
