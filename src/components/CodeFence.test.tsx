/**
 * The fenced code blocks and the rich body of the issue view
 * (`../../specifications/ui/MIV-map-issue-view.md` MIV-FR-RNVP .. MIV-FR-CTJU).
 */
import hljs from "highlight.js/lib/common";
import { afterEach, beforeEach, describe, expect, it, onTestFinished, vi } from "vitest";
import { cleanup, render, screen } from "@testing-library/react";

vi.mock("mermaid", async () => (await import("../test/mermaidMock")).mermaidModule);
vi.mock("../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

import { holdNextRender, overrideNextRender, resetMermaidMock } from "../test/mermaidMock";
import { DIAGRAM_ERROR, resetMermaidForTest } from "./markdownMermaidRender";
import { CodeFence } from "./CodeFence";
import { DocumentMarkdown } from "./DocumentViewer/DocumentMarkdown";
import { IssueMarkdown } from "./Flow/issueView/IssueMarkdown";

beforeEach(() => {
  resetMermaidForTest();
  resetMermaidMock();
});

afterEach(() => {
  cleanup();
});

const fence = () => document.querySelector("pre.code-fence") as HTMLElement;
const label = () => document.querySelector(".code-fence-box > .code-fence__lang");
const code = () => fence().querySelector("code") as HTMLElement;
const roleOf = (text: string) =>
  [...code().querySelectorAll(".hl")].find((s) => s.textContent === text)?.className ?? "";

describe("a fenced code block (MIV-FR-DDAF, MIV-FR-ALPJ, MIV-FR-IVGV, MIV-FR-GZTZ)", () => {
  it("MIV-FR-DDAF, MIV-FR-GZTZ: a known language is highlighted with the token roles, and the text is the source character for character", () => {
    const source = "def pay(order):\n\n    return 'ok'  # done\n";
    render(<CodeFence text={source} language="python" />);
    expect(code().textContent).toBe(source);
    expect(roleOf("def")).toBe("hl hl--keyword");
    expect(roleOf("'ok'")).toBe("hl hl--string");
    expect(label()).toHaveTextContent("python");
    // MIV-FR-DDAF: the label is outside the block that scrolls.
    expect(fence()).not.toContainElement(label() as HTMLElement);
  });

  it.each([
    ["{.python}", "python", "python"],
    [".rust", "rust", "rust"],
    ["language-ts", "ts", "ts"],
    ["TypeScript title=x", "typescript", "TypeScript"],
  ])("MIV-FR-ALPJ: the info string %s highlights as %s and shows the label %s", (info, language, text) => {
    render(<CodeFence text="let x = 1;" language={info} />);
    expect(fence()).toHaveAttribute("data-highlight", language);
    expect(label()).toHaveTextContent(text);
  });

  it("MIV-FR-ALPJ, MIV-FR-IVGV: an unknown or empty language is plain, keeps its label, and is never put to autodetection", () => {
    const auto = vi.spyOn(hljs, "highlightAuto");
    onTestFinished(() => auto.mockRestore());
    const source = "function pay() { return 1; }";
    render(<CodeFence text={source} language="pseudocode" />);
    expect(fence()).not.toHaveAttribute("data-highlight");
    expect(code().querySelector(".hl")).toBeNull();
    expect(label()).toHaveTextContent("pseudocode");
    cleanup();
    render(<CodeFence text={source} language={undefined} />);
    expect(code().querySelector(".hl")).toBeNull();
    expect(label()).toBeNull();
    expect(auto).not.toHaveBeenCalled();
  });
});

describe("Gherkin (MIV-FR-EDFS)", () => {
  const scenario = [
    "@payments",
    "Feature: Card capture",
    "  # A comment",
    "  Scenario: A valid card",
    '    Given a cart with "one" item',
    "    When the buyer pays",
    "    Then the order is paid",
    "    And no card is stored",
    "    But a token is",
  ].join("\n");

  it.each(["gherkin", "feature"])("MIV-FR-EDFS: a %s fence colours the keywords, the tag, the string, and the comment", (info) => {
    render(<CodeFence text={scenario} language={info} />);
    expect(fence().getAttribute("data-highlight")).toBe(info);
    expect(code().textContent).toBe(scenario);
    for (const word of ["Given", "When", "Then", "And", "But"]) expect(roleOf(word)).toBe("hl hl--keyword");
    const keywords = [...code().querySelectorAll(".hl--keyword")].map((s) => s.textContent).join(" ");
    expect(keywords).toContain("Feature");
    expect(keywords).toContain("Scenario");
    const all = [...code().querySelectorAll(".hl")];
    const tag = all.find((s) => s.textContent === "@payments");
    expect(tag?.className).toBe("hl hl--meta");
    expect(all.find((s) => s.textContent === '"one"')?.className).toBe("hl hl--string");
    expect(all.find((s) => s.textContent?.includes("# A comment"))?.className).toBe("hl hl--comment");
  });
});

describe("a Mermaid fence (MIV-FR-CTJU)", () => {
  it("MIV-FR-CTJU: the source shows while the diagram renders, then the inert diagram with no controls", async () => {
    const held = holdNextRender();
    render(<CodeFence text={"graph TD\nA-->B"} language="mermaid" />);
    expect(code().textContent).toBe("graph TD\nA-->B");
    held.release();
    const figure = await screen.findByRole("img", { name: "Mermaid diagram" });
    expect(figure.querySelector("svg")).not.toBeNull();
    expect(screen.queryByRole("button")).toBeNull();
    expect(document.querySelector("pre.code-fence")).toBeNull();
  });

  it("MIV-FR-CTJU: a diagram that fails shows its source and one error line", async () => {
    overrideNextRender(async () => {
      throw new Error("INVALID");
    });
    render(<CodeFence text="graph TD" language="Mermaid" />);
    expect(await screen.findByText(`Diagram error: ${DIAGRAM_ERROR.draw}`)).toBeInTheDocument();
    expect(code().textContent).toBe("graph TD");
    expect(document.querySelectorAll(".mermaid-error__summary")).toHaveLength(1);
  });
});

describe("the rich body (MIV-FR-RNVP)", () => {
  const body = [
    "## Acceptance",
    "",
    "- one",
    "  - nested",
    "",
    "| a | b |",
    "| - | - |",
    "| 1 | 2 |",
    "",
    "> a quote with **strong** and `code`",
    "",
    "<script>alert(1)</script> and <b>bold</b>",
    "",
    "[a link](https://example.com) ![a picture](https://example.com/x.png)",
    "",
    "```ts",
    "const a = 1;",
    "```",
  ].join("\n");

  it("MIV-FR-RNVP: headings, nested lists, tables, quotes, and code render; no HTML, no navigation, no remote image", () => {
    render(<IssueMarkdown body={body} testId="body" />);
    const root = screen.getByTestId("body");
    expect(root.querySelector("h2")).toHaveTextContent("Acceptance");
    expect(root.querySelector("ul ul li")).toHaveTextContent("nested");
    expect(root.querySelector("table td")).toHaveTextContent("1");
    expect(root.querySelector("blockquote strong")).toHaveTextContent("strong");
    expect(root.querySelector("script, b")).toBeNull();
    expect(root).toHaveTextContent("<script>alert(1)</script>");
    expect(root.querySelector("a[href]")).toBeNull();
    expect(root.querySelector('img[src^="http"]')).toBeNull();
    expect(root.querySelector("pre.code-fence")).toHaveAttribute("data-highlight", "ts");
  });

  it("MIV-FR-DDAF, DTV-FR-SSQI: outside the issue view a fenced block stays the plain block of the document view", () => {
    const { container } = render(<DocumentMarkdown text={"```ts\nconst a = 1;\n```"} />);
    const pre = container.querySelector("pre") as HTMLElement;
    expect(pre).toHaveAttribute("data-language", "ts");
    expect(pre).not.toHaveClass("code-fence");
    expect(pre.querySelector(".hl")).toBeNull();
  });
});
