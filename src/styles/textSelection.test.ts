import { readdirSync, readFileSync, statSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { blocksFor, decl, sheet } from "../test/cssRules";

/**
 * SNV-FR-TLXS: the text of the window chrome cannot be selected, while editable
 * fields and reading content can.
 *
 * Vitest runs with `css: false`, so jsdom applies no stylesheet and no
 * rendering test can see a selection rule. These rules are the only place the
 * requirement lives, so this guard reads them as text.
 */

const SRC = join(__dirname, "..");

/** Every component source file, as one string, to find the class names in. */
function componentSources(): string {
  const out: string[] = [];
  const walk = (dir: string) => {
    for (const name of readdirSync(dir)) {
      const path = join(dir, name);
      if (statSync(path).isDirectory()) walk(path);
      else if (/\.tsx?$/.test(name) && !/\.test\.tsx?$/.test(name)) out.push(readFileSync(path, "utf8"));
    }
  };
  walk(join(SRC, "components"));
  return out.join("\n");
}

const EDITABLE = ["input", "textarea", '[contenteditable]:not([contenteditable="false"])'];

const CONTENT = [
  ".doc",
  ".document-viewer__source",
  ".dds-message__body",
  ".comment__body",
  ".agent-surface__text",
  ".logs__msg",
  ".logs__json",
  ".logs__error",
  ".glw__viewport",
  ".diff-line__text",
  ".diff-sbs__cell",
  ".pp-detail__scroll",
  ".miv-desc",
  ".miv-error",
  ".git-info__message",
];

/** The surfaces that keep a selectable rule of their own. */
const OWN_RULES: [string, string][] = [
  ["components/diagram-editor.css", ".diagram-hints"],
  ["components/diagram-editor.css", ".diagram-comparison__source"],
  ["components/diagram-editor.css", ".diagram-editor__preview .mermaid-error.diagram-editor__error"],
  ["components/diagram-editor.css", ".diagram-comparison__failed .mermaid-error"],
  ["components/diagram-hunk.css", ".hunk__diagram-source"],
  ["components/calm-diagram.css", ".calm-details"],
];

/** Every rule of a sheet, as selector list and block. */
function rules(css: string): { selectors: string[]; block: string }[] {
  return [...css.matchAll(/([^{}]+)\{([^{}]*)\}/g)].map((m) => ({
    selectors: m[1].split(",").map((s) => s.trim().replace(/\s+/g, " ")),
    block: m[2],
  }));
}

describe("text selection (SNV-FR-TLXS)", () => {
  const base = sheet("colors_and_type.css");

  it("SNV-FR-TLXS: the document is not selectable and shows the arrow", () => {
    const blocks = blocksFor(base, "body");
    const block = blocks.find((b) => decl(b, "user-select") !== null);
    expect(block, "html, body sets user-select").toBeDefined();
    expect(decl(block!, "user-select")).toBe("none");
    expect(decl(block!, "-webkit-user-select")).toBe("none");
    expect(decl(block!, "cursor")).toBe("default");
    expect(blocksFor(base, "html")).toContain(block);
  });

  it.each([...EDITABLE, ...CONTENT])("SNV-FR-TLXS: %s is selectable", (selector) => {
    const blocks = blocksFor(base, selector).filter((b) => decl(b, "user-select") !== null);
    expect(blocks.length, `${selector} has a selection rule`).toBeGreaterThan(0);
    const block = blocks[blocks.length - 1];
    expect(decl(block, "user-select")).toBe("text");
    expect(decl(block, "-webkit-user-select")).toBe("text");
    expect(decl(block, "cursor")).toBe("auto");
  });

  it("SNV-FR-TLXS: every reading-content class is in use, so a rename fails here", () => {
    const sources = componentSources();
    for (const selector of CONTENT) {
      const name = selector.slice(1);
      // A whole class token, so `doc` is not found inside `document`.
      const token = new RegExp(`(^|[\\s"'\`])${name}([\\s"'\`]|$)`, "m");
      expect(token.test(sources), `${name} is used by a component`).toBe(true);
    }
  });

  it("SNV-FR-TLXS: the base sheet loads first in both windows, so local rules still win", () => {
    for (const file of ["App.tsx", "SettingsWindowApp.tsx"]) {
      const text = readFileSync(join(SRC, file), "utf8");
      const imports = [...text.matchAll(/import "\.\/styles\/([^"]+)";/g)].map((m) => m[1]);
      expect(imports[0], file).toBe("colors_and_type.css");
      expect(imports).toEqual(expect.arrayContaining(["components.css", "kit.css"]));
    }
  });

  it.each(OWN_RULES)("SNV-FR-TLXS: %s keeps %s selectable", (file, selector) => {
    const block = blocksFor(sheet(file), selector).find((b) => decl(b, "user-select") !== null);
    expect(block && decl(block, "user-select"), selector).toBe("text");
    expect(block && decl(block, "-webkit-user-select"), selector).toBe("text");
  });

  it("SNV-FR-TLXS: a field that `all: unset` resets is selectable again in the same rule", () => {
    const found: string[] = [];
    for (const file of ["components.css", "kit.css"]) {
      for (const { selectors, block } of rules(sheet(file))) {
        if (decl(block, "all") !== "unset") continue;
        const fields = selectors.filter((s) => /(^|[\s>+~])(input|textarea)(\[[^\]]*\])?$/.test(s));
        if (fields.length === 0) continue;
        found.push(...fields);
        expect(decl(block, "user-select"), fields.join(", ")).toBe("text");
        expect(decl(block, "-webkit-user-select"), fields.join(", ")).toBe("text");
      }
    }
    expect(found).toEqual(expect.arrayContaining([".search-input input", ".editor__find-input input"]));
  });

  it("SNV-FR-TLXS, SNV-FR-WCQH: both windows install the context menu guard", () => {
    const main = readFileSync(join(SRC, "main.tsx"), "utf8");
    const line = main.split("\n").find((l) => l.includes("installContextMenuGuard(window)"));
    expect(line, "main.tsx installs the guard").toBeDefined();
    expect(line!.trim()).toBe("installContextMenuGuard(window);");
  });

  it("SNV-FR-TLXS: the parts of reading content that are not its text stay unselectable", () => {
    const proposals = sheet("kit/proposals.css");
    for (const selector of [".pp-b__g", ".pp-code__lang", ".pp-code__g"]) {
      const block = blocksFor(proposals, selector).find((b) => decl(b, "user-select") !== null);
      expect(block && decl(block, "user-select"), selector).toBe("none");
    }
    const diff = sheet("components/graduation-cont-2.css");
    const sign = blocksFor(diff, ".diff-line__sign").find((b) => decl(b, "user-select") !== null);
    expect(sign && decl(sign, "user-select")).toBe("none");
    const area = sheet("components/diagram-editor.css");
    const viewer = blocksFor(area, ".diagram-viewer__area.doc")[0];
    expect(decl(viewer, "user-select")).toBe("none");
  });
});
