import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { AdvisorsListView, SynthesisLabel, isSynthesis } from "./GroupMarks";
import type { AdvisorsList, Attachment } from "../../types";

afterEach(cleanup);

const advisors: AdvisorsList = {
  kind: "advisors",
  advisors: [
    {
      handle: "sec",
      state: "finished",
      points: ["Key rotation is missing"],
      questions: ["Who owns the keys?"],
    },
    { handle: "ops", state: "skipped", reason: "timed_out" },
    { handle: "dev", state: "skipped", reason: "failed" },
  ],
};

describe("Synthesis label and Advisors list", () => {
  it("CVP-FR-OMAS: a comment with the synthesis attachment shows the Synthesis label", () => {
    const attachments: Attachment[] = [{ kind: "synthesis" }];
    expect(isSynthesis(attachments)).toBe(true);
    render(<SynthesisLabel attachments={attachments} />);
    expect(screen.getByTestId("comment-synthesis-label")).toHaveTextContent("Synthesis");
  });

  it("CVP-FR-OMAS: an ordinary comment shows no label", () => {
    render(<SynthesisLabel attachments={[]} />);
    expect(screen.queryByTestId("comment-synthesis-label")).toBeNull();
  });

  it("CVP-FR-HRGH: the Advisors list is collapsed and shows how many advisors it holds", () => {
    render(<AdvisorsListView attachment={advisors} />);
    const toggle = screen.getByTestId("advisors-toggle");
    expect(toggle).toHaveTextContent("Advisors (3)");
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByTestId("advisor-entry")).toBeNull();
  });

  it("CVP-FR-HRGH, CVP-FR-VPKO: the keyboard expands it, and each advisor shows points, questions, or the reason it was skipped", async () => {
    const user = userEvent.setup();
    render(<AdvisorsListView attachment={advisors} />);
    screen.getByTestId("advisors-toggle").focus();
    await user.keyboard("{Enter}");
    expect(screen.getByTestId("advisors-toggle")).toHaveAttribute("aria-expanded", "true");
    const entries = screen.getAllByTestId("advisor-entry");
    expect(entries).toHaveLength(3);
    expect(entries[0]).toHaveTextContent("@sec");
    expect(entries[0]).toHaveTextContent("Key rotation is missing");
    expect(entries[0]).toHaveTextContent("Who owns the keys?");
    expect(entries[1]).toHaveTextContent("@ops");
    expect(entries[1]).toHaveTextContent("skipped: timed out");
    expect(entries[2]).toHaveTextContent("skipped: failed");
    await user.keyboard("{Enter}");
    expect(screen.queryByTestId("advisor-entry")).toBeNull();
  });
});
