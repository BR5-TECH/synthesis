/** The transcript of a translated voice comment (`CMT-comments.md` CMT-FR-HCVB). */
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";

import type { Comment } from "../../types";
import { SpokenOriginal } from "./SpokenOriginal";

function comment(over: Partial<Comment> = {}): Comment {
  return {
    id: "c1",
    author: { kind: "human", login: "me" },
    body: "Look at the second part.",
    quotes: [],
    attachments: [],
    createdAt: "2026-11-04T10:00:00Z",
    ...over,
  };
}

afterEach(cleanup);

describe("the Original disclosure", () => {
  it("CMT-FR-HCVB: a comment with a transcript shows it collapsed, and it opens", () => {
    render(<SpokenOriginal comment={comment({ spokenOriginal: "Посмотри вторую часть." })} />);
    const disclosure = screen.getByTestId("comment-spoken-original");
    expect(disclosure).not.toHaveAttribute("open");
    expect(screen.getByText("Original")).toBeInTheDocument();
    fireEvent.click(screen.getByText("Original"));
    expect(disclosure).toHaveAttribute("open");
    expect(screen.getByText("Посмотри вторую часть.")).toBeInTheDocument();
  });

  it("CMT-FR-HCVB: a comment with no transcript shows nothing", () => {
    const { container } = render(<SpokenOriginal comment={comment()} />);
    expect(container).toBeEmptyDOMElement();
    cleanup();
    const blank = render(<SpokenOriginal comment={comment({ spokenOriginal: "  " })} />);
    expect(blank.container).toBeEmptyDOMElement();
  });

  it("CMT-FR-HCVB: the transcript is plain text, and no Markdown renders", () => {
    render(<SpokenOriginal comment={comment({ spokenOriginal: "**bold** <b>x</b>" })} />);
    expect(screen.getByText("**bold** <b>x</b>")).toBeInTheDocument();
  });
});
