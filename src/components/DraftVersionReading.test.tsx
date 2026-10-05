import { describe, expect, it, afterEach } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

import {
  DraftVersionReading,
  splitReadingFrontmatter,
} from "./DraftVersionReading";

/**
 * Reading a past version of a draft's prompt
 * (`../../specifications/ui/NAW-new-artifact.md` NAW-FR-09).
 *
 * The claim under test is that a version is read **the way the prompt is read**:
 * the same two renderings, the same page, the same frontmatter region, and
 * read-only in both — rather than a slab of monospace that happens to hold the
 * same characters.
 */

const LABEL = "Read-only past version of spec.md — Original, Original";

afterEach(cleanup);

describe("DraftVersionReading (NAW-FR-09)", () => {
  it("EDT-FR-18: splits the leading frontmatter off before the body is parsed", () => {
    // Without the split the fences render as a horizontal rule with the YAML as
    // a paragraph between them — which is not what the author sees when they
    // open the same text as the live prompt.
    expect(
      splitReadingFrontmatter("---\ntitle: Spec\n---\n\n# Heading\n"),
    ).toEqual({ frontmatter: "title: Spec", body: "\n# Heading\n" });

    // A document with none is all body, and `---` that is not a leading fence
    // is content like any other.
    expect(splitReadingFrontmatter("# Heading\n")).toEqual({
      frontmatter: null,
      body: "# Heading\n",
    });
    expect(splitReadingFrontmatter("# H\n\n---\n\nmore\n").frontmatter).toBeNull();
  });

  it("renders the rich surface on the Editor's own page, read-only", () => {
    render(
      <DraftVersionReading
        text={"# Worktree cleanup\n\nThe opening paragraph.\n"}
        mode="wysiwyg"
        label={LABEL}
      />,
    );

    const reading = screen.getByTestId("draft-version-reading");
    // EDT-FR-62: the typography an Editor tab's WYSIWYG surface carries.
    expect(reading).toHaveClass("doc", "editor__prose");
    // EDT-FR-63: set on the Editor's own page rather than beside it.
    expect(reading.closest(".editor")).not.toBeNull();
    // NAW-FR-09: rendered as the document rather than as its source — the
    // heading is a heading, and the `#` is not in the text.
    expect(reading.querySelector("h1")?.textContent).toBe("Worktree cleanup");
    expect(reading.textContent).not.toContain("#");
    // …and nothing in it accepts a keystroke, announced in semantics rather
    // than by presentation.
    expect(reading).toHaveAttribute("contenteditable", "false");
    expect(reading).toHaveAttribute("aria-readonly", "true");
    expect(reading).toHaveAttribute("role", "document");
    expect(reading).toHaveAttribute("aria-label", LABEL);
  });

  it("EDT-FR-18: renders frontmatter in its own region above the rich body", () => {
    render(
      <DraftVersionReading
        text={"---\ntitle: Worktree cleanup\ntype: spec\n---\n\n# Body\n"}
        mode="wysiwyg"
        label={LABEL}
      />,
    );

    const region = document.querySelector(".editor__frontmatter") as HTMLElement;
    expect(region).not.toBeNull();
    expect(region.textContent).toContain("title: Worktree cleanup");
    // EDT-FR-63: the page gives up its top inset to the region, which the tab
    // declares rather than the region asserting it for itself.
    expect(
      document.querySelector(".editor-tab")?.getAttribute("data-frontmatter"),
    ).toBe("on");
    // The YAML is a reading, not a field: there is nothing here to type into.
    expect(region.querySelector("textarea")).toBeNull();
    // …and the body below it holds the body alone.
    const reading = screen.getByTestId("draft-version-reading");
    expect(reading.textContent).not.toContain("title:");
    expect(reading.querySelector("h1")?.textContent).toBe("Body");
  });

  it("renders the raw Markdown surface on the same sheet, whole and read-only", () => {
    const text = "---\ntitle: Spec\n---\n\n# Body\n\nA line.\n";
    render(<DraftVersionReading text={text} mode="text" label={LABEL} />);

    const reading = screen.getByTestId("draft-version-reading");
    // EDT-FR-17 / EDT-FR-63: the source surface's own sheet and Source role.
    expect(reading).toHaveClass("editor__source");
    expect(reading.closest(".editor__source-wrap")).not.toBeNull();
    // The whole file, frontmatter included — the raw surface has no region.
    expect(reading.textContent).toBe(text);
    expect(document.querySelector(".editor__frontmatter")).toBeNull();
    expect(reading).toHaveAttribute("aria-readonly", "true");
    expect(reading).toHaveAttribute("role", "document");
    // A `<pre>` rather than a disabled field: there is nothing to type into,
    // and a disabled control reads as one that has been taken away.
    expect(reading.tagName).toBe("PRE");
  });

  it("adopts a different version's text without remounting the surface", () => {
    const { rerender } = render(
      <DraftVersionReading text="first version" mode="wysiwyg" label={LABEL} />,
    );
    const before = screen.getByTestId("draft-version-reading");
    expect(before).toHaveTextContent("first version");

    rerender(
      <DraftVersionReading text="second version" mode="wysiwyg" label={LABEL} />,
    );

    const after = screen.getByTestId("draft-version-reading");
    expect(after).toHaveTextContent("second version");
    expect(after).not.toHaveTextContent("first version");
    expect(after).toBe(before);
  });
});
