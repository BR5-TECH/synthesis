import { afterEach, describe, expect, it } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

import { CommentMarkdown } from "./CommentMarkdown";

/**
 * CMT-FR-09: a comment is authored as Markdown source and rendered as rich text.
 *
 * What matters here is that the rendering is *faithful* — the syntax disappears
 * and the meaning survives. A renderer that dropped a construct silently would
 * lose part of what someone wrote, which is worse than showing them the raw
 * asterisks.
 */
afterEach(() => cleanup());

describe("CommentMarkdown", () => {
  it("renders emphasis and code spans as themselves, not as syntax", () => {
    const { container } = render(
      <CommentMarkdown body="Say **which** one, in *this* file, via `run()`." />,
    );
    expect(container.querySelector("strong")?.textContent).toBe("which");
    expect(container.querySelector("em")?.textContent).toBe("this");
    expect(container.querySelector("code")?.textContent).toBe("run()");
    // None of the syntax leaks through as literal text.
    expect(container.textContent).not.toContain("**");
    expect(container.textContent).not.toContain("`");
  });

  it("renders a fenced code block verbatim", () => {
    const { container } = render(
      <CommentMarkdown body={"Try:\n\n```\nconst x = 1;\n```\n"} />,
    );
    const pre = container.querySelector("pre code");
    expect(pre?.textContent).toContain("const x = 1;");
    // The fence itself is structure, not content.
    expect(container.textContent).not.toContain("```");
  });

  it("renders a blockquote and a horizontal rule", () => {
    const { container } = render(
      <CommentMarkdown body={"> quoted line\n\n---\n\nafter\n"} />,
    );
    expect(container.querySelector("blockquote")?.textContent).toContain("quoted line");
    expect(container.querySelector("hr")).not.toBeNull();
    expect(container.textContent).toContain("after");
  });

  it("renders list items with a marker and preserves their text", () => {
    const { container } = render(
      <CommentMarkdown body={"- alpha\n- beta\n"} />,
    );
    const items = container.querySelectorAll(".comment__body-list");
    expect(items).toHaveLength(2);
    expect(items[0].textContent).toContain("alpha");
    expect(items[1].textContent).toContain("beta");
  });

  it("renders a link as its text and leaves it inert", () => {
    // Matching the diff viewer's treatment: a comment body is content someone
    // else wrote, and making it navigable is a decision this surface does not
    // make.
    const { container } = render(
      <CommentMarkdown body="see [the spec](https://example.com/spec)" />,
    );
    const a = container.querySelector("a");
    expect(a?.textContent).toBe("the spec");
    expect(a?.getAttribute("href")).toBeNull();
    expect(a?.getAttribute("aria-disabled")).not.toBeNull();
    expect(container.textContent).not.toContain("https://example.com/spec");
  });

  it("renders a heading at the rail's scale rather than the document's", () => {
    // The rail is a narrow column beside the artifact, not a second document —
    // an `<h1>` here would dwarf the comment it belongs to.
    const { container } = render(<CommentMarkdown body={"# Big\n\nbody\n"} />);
    expect(container.querySelector("h1")).toBeNull();
    expect(container.querySelector(".comment__body-heading")?.textContent).toBe("Big");
  });

  it("renders an empty body without crashing and shows nothing", () => {
    const { container } = render(<CommentMarkdown body="" />);
    expect(container.textContent).toBe("");
  });

  it("keeps a table row's cells rather than dropping them", () => {
    // A real table is unreadable in a narrow rail, but nothing the author wrote
    // may disappear.
    render(<CommentMarkdown body={"| a | b |\n| - | - |\n| 1 | 2 |\n"} />);
    expect(screen.getByText(/a/)).toBeInTheDocument();
    expect(document.body.textContent).toContain("1");
    expect(document.body.textContent).toContain("2");
  });
});

// ---------------------------------------------------------------------------
// AGT-FR-29 / AGT-FR-38: which tags are bold
// ---------------------------------------------------------------------------

describe("AGT-FR-29: a live tag is bold and nothing else is", () => {
  const READY = { nicknames: ["arch", "sec"], ready: ["arch", "sec"] };

  /** The text of every marked tag, in order. */
  const tags = () =>
    screen.queryAllByTestId("comment-tag").map((e) => e.textContent);

  it("marks a nickname and the @all handle, and leaves every other @ alone", () => {
    // AGT-FR-29, AGT-FR-28, AGT-FR-38: an address, a `@media` rule and a `@nobody` all keep the weight
    // of the prose around them (AGT-FR-28) — bolding text that addressed no one
    // would promise a reader a participant who was never in the conversation.
    render(
      <CommentMarkdown
        body="@all and @arch, plus me@example.com, the @media rule and @nobody"
        agentRoster={READY}
      />,
    );
    expect(tags()).toEqual(["@all", "@arch"]);
    expect(document.body.textContent).toContain("me@example.com");
    expect(document.body.textContent).toContain("@media");
    expect(document.body.textContent).toContain("@nobody");
  });

  it("renders the handle as the text @all rather than the names it stands for", () => {
    // AGT-FR-40: a reader sees the question asked of everyone, not a list.
    render(<CommentMarkdown body="@all is this two specs?" agentRoster={READY} />);
    expect(tags()).toEqual(["@all"]);
    expect(document.body.textContent).not.toContain("@arch");
  });

  it("leaves @all as prose where no enrolled agent can answer", () => {
    // AGT-FR-37, AGT-FR-29 / AGT-FR-38. Both shapes of "nobody can answer": nothing
    // enrolled, and everything enrolled but degraded.
    for (const roster of [
      { nicknames: [], ready: [] },
      { nicknames: ["arch"], ready: [] },
    ]) {
      const { unmount } = render(
        <CommentMarkdown body="@all take a look" agentRoster={roster} />,
      );
      expect(tags()).toEqual([]);
      expect(document.body.textContent).toContain("@all take a look");
      unmount();
    }
  });

  it("marks the same body once an agent is enrolled, the body unchanged", () => {
    // AGT-FR-37, AGT-FR-38, AGT-FR-29's second half: enrolling is what makes the text live, nothing
    // about the message having to change.
    const body = "@all take a look";
    const { unmount } = render(
      <CommentMarkdown body={body} agentRoster={{ nicknames: [], ready: [] }} />,
    );
    expect(tags()).toEqual([]);
    unmount();
    render(<CommentMarkdown body={body} agentRoster={READY} />);
    expect(tags()).toEqual(["@all"]);
  });

  it("never marks a tag inside a code span", () => {
    // A tag in a code sample is a code sample. The dispatch rule and the marking
    // rule have to agree, or a message would summon an agent without showing it.
    render(<CommentMarkdown body="write `@all` to reach everyone" agentRoster={READY} />);
    expect(tags()).toEqual([]);
  });

  it("marks tags in a list item and in a quote, not only in a paragraph", () => {
    // AGT-FR-29: wherever a message body is rendered.
    render(
      <CommentMarkdown body={"- ask @arch\n\n> and @all\n"} agentRoster={READY} />,
    );
    expect(tags()).toEqual(["@arch", "@all"]);
  });

  it("CMT-FR-AWIE: hides provider citation markers in a paragraph, a list, and a quote", () => {
    const span = "\uE200cite\uE202turn0search1\uE201";
    const body = `Route one ${span}.\n\n- an item ${span}\n\n> quoted ${span}`;
    const { container } = render(<CommentMarkdown body={body} />);
    const text = container.textContent ?? "";
    expect(text).not.toMatch(/[\uE200-\uE2FF]/);
    expect(text).not.toContain("turn0search1");
    expect(text).toContain("Route one.");
    expect(text).toContain("an item");
    expect(text).toContain("quoted");
  });
});
