import { describe, expect, it } from "vitest";
import {
  anchorIndex,
  classifyLink,
  folderOf,
  headingAnchors,
  headingSlug,
  joinProjectPath,
  schemeOfLink,
  type LinkBase,
} from "./markdownLinks";

const ROOT = "/Users/me/project";
const IN_DOCS: LinkBase = { fileDir: "docs/guide", roots: [ROOT] };
const NO_FILE: LinkBase = { fileDir: null, roots: [ROOT] };

describe("classifyLink", () => {
  it("EDT-FR-UXYY: http, https, and mailto open outside the app", () => {
    for (const url of ["https://example.com/a?b#c", "http://x.org", "mailto:a@b.c", "HTTPS://X.COM"]) {
      expect(classifyLink(url, IN_DOCS)).toEqual({ kind: "external", url });
    }
    // Also where the host resolves no file.
    expect(classifyLink("https://example.com", NO_FILE).kind).toBe("external");
  });

  it("EDT-FR-HMJI: a fragment alone names a heading of the same document", () => {
    expect(classifyLink("#setup", IN_DOCS)).toEqual({ kind: "fragment", fragment: "setup" });
    expect(classifyLink("#caf%C3%A9", NO_FILE)).toEqual({ kind: "fragment", fragment: "café" });
    expect(classifyLink("?q=1#a", IN_DOCS)).toEqual({ kind: "fragment", fragment: "a" });
  });

  it("an empty destination, `#` alone, and blank text go nowhere", () => {
    expect(classifyLink("", IN_DOCS)).toEqual({ kind: "none" });
    expect(classifyLink("   ", IN_DOCS)).toEqual({ kind: "none" });
    expect(classifyLink("#", IN_DOCS)).toEqual({ kind: "none" });
  });

  it("EDT-FR-RFCU: a relative path resolves against the folder of the file", () => {
    expect(classifyLink("other.md", IN_DOCS)).toEqual({ kind: "file", path: "docs/guide/other.md", fragment: null });
    expect(classifyLink("./other.md#install", IN_DOCS)).toEqual({
      kind: "file",
      path: "docs/guide/other.md",
      fragment: "install",
    });
    expect(classifyLink("../../README.md", IN_DOCS)).toEqual({ kind: "file", path: "README.md", fragment: null });
    expect(classifyLink("sub\\x.md", IN_DOCS)).toEqual({ kind: "file", path: "docs/guide/sub/x.md", fragment: null });
    expect(classifyLink("a/./b/../c.md", IN_DOCS)).toEqual({ kind: "file", path: "docs/guide/a/c.md", fragment: null });
  });

  it("EDT-FR-RFCU: percent-escapes are decoded and a query is ignored", () => {
    expect(classifyLink("My%20File.md?raw=1#Part%20Two", IN_DOCS)).toEqual({
      kind: "file",
      path: "docs/guide/My File.md",
      fragment: "Part Two",
    });
    // A malformed escape is kept as written.
    expect(classifyLink("100%.md", IN_DOCS)).toEqual({ kind: "file", path: "docs/guide/100%.md", fragment: null });
  });

  it("EDT-FR-RFCU: a path that starts with / is relative to the project root", () => {
    expect(classifyLink("/specs/x.md#a", IN_DOCS)).toEqual({ kind: "file", path: "specs/x.md", fragment: "a" });
    expect(classifyLink("/x.md", { fileDir: "", roots: [] })).toEqual({ kind: "file", path: "x.md", fragment: null });
  });

  it("EDT-FR-AWFH: a filesystem path inside the project root names that file", () => {
    expect(classifyLink(`${ROOT}/docs/x.md#b`, IN_DOCS)).toEqual({ kind: "file", path: "docs/x.md", fragment: "b" });
    // The second root, for example the project folder beside an active worktree.
    const two: LinkBase = { fileDir: "", roots: ["/w/tree", ROOT] };
    expect(classifyLink(`${ROOT}/a.md`, two)).toEqual({ kind: "file", path: "a.md", fragment: null });
    // A Windows root, with either separator and no regard to the case of the drive path.
    const win: LinkBase = { fileDir: "", roots: ["C:\\Work\\Proj"] };
    expect(classifyLink("c:\\work\\proj\\docs\\x.md", win)).toEqual({ kind: "file", path: "docs/x.md", fragment: null });
    expect(classifyLink("C:/Work/Proj/a.md", win)).toEqual({ kind: "file", path: "a.md", fragment: null });
  });

  it("EDT-FR-AWFH: a path that goes outside the project root is refused", () => {
    const outside = { kind: "refused", reason: "outside" };
    expect(classifyLink("../../../x.md", IN_DOCS)).toEqual(outside);
    expect(classifyLink("/../x.md", IN_DOCS)).toEqual(outside);
    expect(classifyLink(`${ROOT}/../other/x.md`, IN_DOCS)).toEqual(outside);
    expect(classifyLink("/Users/someone/else.md", IN_DOCS)).toEqual(outside);
    expect(classifyLink("D:\\other\\x.md", { fileDir: "", roots: ["C:\\Work"] })).toEqual(outside);
    // A backslash separates segments, so it cannot hide a step above the root.
    expect(classifyLink("..\\..\\..\\x.md", IN_DOCS)).toEqual(outside);
    expect(classifyLink("..%5C..%5C..%5Cx.md", IN_DOCS)).toEqual(outside);
    expect(classifyLink("//server/share/x.md", IN_DOCS)).toEqual(outside);
  });

  it("EDT-FR-AWFH: a destination with another scheme is refused", () => {
    for (const href of ["file:///etc/passwd", "javascript:alert(1)", "data:text/html,x", "ftp://x", "javascript%3Aalert(1)"]) {
      expect(classifyLink(href, IN_DOCS)).toEqual({ kind: "refused", reason: "scheme" });
    }
  });

  it("EDT-FR-QBIE: a host that resolves no file refuses each file link", () => {
    for (const href of ["other.md", "/x.md", `${ROOT}/x.md`, "./x.md#a"]) {
      expect(classifyLink(href, NO_FILE)).toEqual({ kind: "refused", reason: "no-file" });
    }
  });
});

describe("paths", () => {
  it("EDT-FR-RFCU: joinProjectPath resolves segments and stops at the root", () => {
    expect(joinProjectPath("", "a/b")).toBe("a/b");
    expect(joinProjectPath("a", "../b")).toBe("b");
    expect(joinProjectPath("a", "../../b")).toBeNull();
    expect(joinProjectPath("a", "./")).toBe("a");
  });

  it("folderOf gives the folder of a project-relative path", () => {
    expect(folderOf("a/b/c.md")).toBe("a/b");
    expect(folderOf("c.md")).toBe("");
  });

  it("EDT-FR-AWFH: schemeOfLink names the scheme, never the address", () => {
    expect(schemeOfLink("javascript:alert(1)")).toBe("javascript");
    expect(schemeOfLink("./a.md")).toBe("none");
  });
});

describe("heading anchors", () => {
  it("EDT-FR-BJFP: the slug is lower case, keeps letters, digits, -, and _, and has - for each space", () => {
    expect(headingSlug("Getting Started")).toBe("getting-started");
    expect(headingSlug("What's new? (v2.0)")).toBe("whats-new-v20");
    expect(headingSlug("snake_case and-dash")).toBe("snake_case-and-dash");
    expect(headingSlug("Café Ünïcode 日本")).toBe("café-ünïcode-日本");
    expect(headingSlug("A  B")).toBe("a--b");
    expect(headingSlug("")).toBe("");
    // A mark that belongs to a letter stays with it.
    expect(headingSlug("हिन्दी")).toBe("हिन्दी");
  });

  it("EDT-FR-BJFP: a repeated slug gets -1, -2, and so on", () => {
    expect(headingAnchors(["Usage", "Usage", "Usage"])).toEqual(["usage", "usage-1", "usage-2"]);
    // A heading whose own slug is a suffixed one does not take that anchor twice.
    expect(headingAnchors(["Foo", "Foo 1", "Foo"])).toEqual(["foo", "foo-1", "foo-2"]);
  });

  it("EDT-FR-BJFP: a fragment matches with no regard to case", () => {
    const texts = ["Intro", "Setup", "Setup"];
    expect(anchorIndex(texts, "setup")).toBe(1);
    expect(anchorIndex(texts, "SETUP-1")).toBe(2);
    expect(anchorIndex(texts, "missing")).toBe(-1);
  });
});
