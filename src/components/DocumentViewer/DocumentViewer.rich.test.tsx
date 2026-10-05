// The rich view of the Document text viewer (`DTV-document-text-viewer.md`
// DTV-FR-SSQI): Markdown renders read-only, in the Rich Markdown role, and
// never runs a script, never renders raw HTML, and never loads a remote
// resource.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, within } from "@testing-library/react";

import { resetViewStateForTest } from "../../state/documentViewState";
import { sheet, blocksFor, decl } from "../../test/cssRules";
import { DocumentTextViewer } from ".";
import { ID, documentText, readReturns, resetFixtures } from "./viewerFixtures";

vi.mock("@tauri-apps/api/core", async () => ({
  invoke: (await import("./viewerFixtures")).invokeMock,
}));
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("./viewerFixtures")).listenMock,
}));
vi.mock("../../logging", async () => (await import("./viewerFixtures")).loggingMock);

beforeEach(() => {
  resetFixtures();
  resetViewStateForTest();
});
afterEach(cleanup);

async function rich(markdown: string) {
  readReturns(documentText(markdown));
  const view = render(<DocumentTextViewer documentId={ID} />);
  const region = await screen.findByRole("region", { name: "Document text" });
  return { region, view };
}

describe("DTV-FR-SSQI: the elements of Markdown", () => {
  it("DTV-FR-SSQI: shows headings, lists, emphasis, code, quotes, and a table as Markdown shows them", async () => {
    const { region } = await rich(
      [
        "# One",
        "",
        "## Two",
        "",
        "Some *soft* and **strong** and `inline` text.",
        "",
        "- first",
        "- second",
        "",
        "1. alpha",
        "2. beta",
        "",
        "> quoted words",
        "",
        "```ts",
        "const x = 1;",
        "```",
        "",
        "| a | b |",
        "|---|---|",
        "| 1 | 2 |",
      ].join("\n"),
    );
    expect(within(region).getByRole("heading", { level: 1, name: "One" })).toBeInTheDocument();
    expect(within(region).getByRole("heading", { level: 2, name: "Two" })).toBeInTheDocument();
    expect(region.querySelector("em")?.textContent).toBe("soft");
    expect(region.querySelector("strong")?.textContent).toBe("strong");
    expect(region.querySelector("p code")?.textContent).toBe("inline");
    expect(region.querySelectorAll("ul > li")).toHaveLength(2);
    expect(region.querySelectorAll("ol > li")).toHaveLength(2);
    expect(region.querySelector("blockquote")?.textContent).toBe("quoted words");
    expect(region.querySelector("pre code")?.textContent).toBe("const x = 1;");
    const table = within(region).getByRole("table");
    expect(within(table).getAllByRole("columnheader").map((c) => c.textContent)).toEqual(["a", "b"]);
    expect(within(table).getAllByRole("cell").map((c) => c.textContent)).toEqual(["1", "2"]);
  });

  it("DTV-FR-SSQI: shows nested lists and a leading YAML region as source text", async () => {
    const { region } = await rich("---\ntitle: x\n---\n\n- top\n  - nested\n");
    expect(region.querySelector(".doc-frontmatter")?.textContent).toBe("---\ntitle: x\n---");
    expect(region.querySelectorAll("ul ul > li")).toHaveLength(1);
  });
});

describe("DTV-FR-SSQI: nothing runs and nothing loads", () => {
  it("DTV-FR-SSQI: raw HTML shows as text and never becomes an element, and no script runs", async () => {
    const hostile = "<script>window.__ran = 1</script>\n\n<img src=x onerror=\"window.__ran = 2\">\n\n<b>bold</b> <iframe src=\"https://example.com\"></iframe>";
    const { region } = await rich(hostile);
    expect(region.querySelector("script, img, b, iframe")).toBeNull();
    expect(region.textContent).toContain("<script>window.__ran = 1</script>");
    expect(region.textContent).toContain("<b>bold</b>");
    expect((window as unknown as { __ran?: number }).__ran).toBeUndefined();
  });

  it("DTV-FR-SSQI: a link renders as text and does not navigate", async () => {
    const { region } = await rich(
      "[a site](https://example.com) and [bad](javascript:alert(1)) and [mail](mailto:a@b.c)",
    );
    expect(region.querySelector("a")).toBeNull();
    expect(region.querySelector("[href]")).toBeNull();
    expect(region.textContent).toContain("a site");
    expect(region.textContent).toContain("bad");
    // A script address never reaches an attribute.
    expect(region.innerHTML).not.toContain("javascript:");
  });

  it("DTV-FR-SSQI: a remote image loads nothing and shows a placeholder with its alt text", async () => {
    const { region } = await rich(
      "![a remote chart](https://example.com/x.png) and ![a file](file:///etc/passwd) and ![rel](./local.png)",
    );
    expect(region.querySelector("img")).toBeNull();
    expect(region.innerHTML).not.toContain("example.com/x.png");
    expect(region.innerHTML).not.toContain("/etc/passwd");
    const pictures = within(region).getAllByRole("img");
    expect(pictures.map((p) => p.getAttribute("aria-label"))).toEqual([
      "a remote chart",
      "a file",
      "rel",
    ]);
  });

  it("DTV-FR-SSQI: an image that holds its own bytes is shown, because it loads nothing from the network", async () => {
    const png = "data:image/png;base64,iVBORw0KGgo=";
    const { region } = await rich(`![inline pic](${png})`);
    const img = region.querySelector("img");
    expect(img).not.toBeNull();
    expect(img!.getAttribute("src")).toBe(png);
    expect(img!.getAttribute("alt")).toBe("inline pic");
  });

  it("DTV-FR-SSQI: a data source that is not a raster image is not shown as an image", async () => {
    const { region } = await rich("![x](data:text/html;base64,PHNjcmlwdD4=)");
    expect(region.querySelector("img")).toBeNull();
  });

  it("DTV-FR-SSQI: the view uses no dangerouslySetInnerHTML and no remote resource", async () => {
    const { readFileSync } = await import("node:fs");
    const { resolve } = await import("node:path");
    for (const name of ["DocumentMarkdown.tsx", "index.tsx", "ViewSwitch.tsx"]) {
      const code = readFileSync(resolve(process.cwd(), "src/components/DocumentViewer", name), "utf8")
        .replace(/\/\*[\s\S]*?\*\//g, "")
        .replace(/^\s*\/\/.*$/gm, "");
      expect(code, name).not.toContain("dangerouslySetInnerHTML");
      expect(code, name).not.toMatch(/https?:\/\//);
      expect(code, name).not.toMatch(/\b(fetch|XMLHttpRequest)\b/);
    }
  });
});

describe("DTV-FR-SSQI: the typographic roles", () => {
  it("DTV-FR-SSQI, DTV-FR-CIWP, OVW-FR-13: the rich view is read in the Rich Markdown role and the source view in the Source code role", async () => {
    const css = sheet("components.css");
    const sourceBlock = blocksFor(css, ".document-viewer__source");
    expect(sourceBlock).toHaveLength(1);
    expect(decl(sourceBlock[0], "font-family")).toBe("var(--font-source-family)");
    expect(decl(sourceBlock[0], "white-space")).toBe("pre");
    const { region } = await rich("# x");
    // The rich surface carries the shared Rich Markdown class of the Editor.
    expect(region.querySelector(".doc.document-viewer__rich")).not.toBeNull();
    const base = sheet("colors_and_type.css");
    const doc = blocksFor(base, ".doc");
    expect(doc).toHaveLength(1);
    expect(decl(doc[0], "font-family")).toBe("var(--font-rich-family)");
  });
});
