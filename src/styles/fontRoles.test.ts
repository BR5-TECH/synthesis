import { readFileSync, readdirSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { readStylesheet } from "../test/readStylesheet";

/**
 * The three typographic roles, asserted against the real stylesheet.
 *
 * `vitest.config.ts` sets `css: false` — jsdom loads no stylesheet, so
 * `getComputedStyle` sees nothing and a rendering test could not fail if these
 * rules regressed. The roles are almost entirely stylesheet behaviour (the
 * applier only writes three custom properties per role), so this is where
 * OVW-FR-13's partition, EDT-FR-62, and DFV-FR-37 are actually enforced. The
 * approach is the one `DiffView.test.tsx` already uses for the diff viewer's
 * layout.
 */
const tokens = readStylesheet("colors_and_type.css");
const kit = readStylesheet("kit.css");
const components = readStylesheet("components.css");

const rule = (css: string, selector: string) => {
  const match = css.match(
    new RegExp(
      `(^|\\n)\\s*${selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\s*\\{([^}]*)\\}`,
    ),
  );
  expect(match, `expected \`${selector} { … }\` in the stylesheet`).not.toBeNull();
  return match![2];
};

const ROLES = ["ui", "rich", "source"] as const;

describe("typographic role tokens (OVW-FR-13)", () => {
  it("declares a family, a size, and a line height for each of the three roles", () => {
    // These nine custom properties are the whole contract between the applier
    // and the stylesheet. A missing one is a role the Appearance section can
    // set and nothing reads.
    for (const role of ROLES) {
      expect(tokens).toMatch(
        new RegExp(`--font-${role}-family\\s*:\\s*\\S`),
      );
      expect(tokens).toMatch(new RegExp(`--font-${role}-size\\s*:\\s*\\S`));
      expect(tokens).toMatch(
        new RegExp(`--font-${role}-line-height\\s*:\\s*\\S`),
      );
    }
  });

  it("expresses every step of the type scale as a ratio of its role's base size", () => {
    // A literal px anywhere in the scale is a step that would not move when the
    // role's size does — the whole reason the ladder is expressed in calc().
    const scale = tokens.match(/--fs-[a-z0-9-]+\s*:\s*[^;]+;/g) ?? [];
    expect(scale.length).toBeGreaterThan(10);
    for (const decl of scale) {
      expect(decl, `\`${decl.trim()}\` is a literal, so a role's size cannot move it`).toMatch(
        /var\(--font-(ui|rich|source)-size\)/,
      );
    }
  });

  it("leaves no literal font size anywhere — stylesheet or JSX", () => {
    // Every size belongs to a role; a hard-coded size is text that belongs to
    // none, which OVW-FR-13's total partition does not allow, and which would
    // sit still while the rest of its role moved (GLS-FR-20's "applies to the
    // whole application").
    for (const [name, css] of [
      ["colors_and_type.css", tokens],
      ["kit.css", kit],
      ["components.css", components],
    ] as const) {
      // `em` is deliberately not banned: it is a ratio of the run it sits in,
      // so it moves with whatever role that run belongs to. `.doc code`'s
      // `0.92em` is the case — inline code takes the Source face but stays
      // proportional to the prose around it, so a `code` span inside an h1 is
      // not set two steps below the heading. Absolute units are the problem.
      const literals = css.match(/font-size\s*:\s*\d+(\.\d+)?(px|rem|pt|%)/g);
      expect(literals, `${name} still sizes text outside the three roles`).toBeNull();
    }

    // The stylesheet is not the whole surface: an inline `style={{ fontSize }}`
    // is just as fixed, and there is no rule anywhere that would move it. Both
    // halves have to be swept or the claim above is only half true.
    const skip = new Set([
      // The specimens are the one legitimate exception: each is deliberately
      // set in the value its own role currently carries, including one the
      // section has not committed yet (GLS-FR-21), so it cannot read a token.
      "FontRoleControls.tsx",
    ]);
    // The sweep recurses, because a component that grew past the file-size
    // ceiling is a directory of parts rather than one `.tsx`, and a walk that
    // read only the top level would stop guarding it without saying so.
    const sources: string[] = [];
    const walk = (dir: string) => {
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const path = `${dir}/${entry.name}`;
        if (entry.isDirectory()) walk(path);
        else if (
          entry.name.endsWith(".tsx") &&
          !entry.name.includes(".test.") &&
          !skip.has(entry.name)
        ) {
          sources.push(path);
        }
      }
    };
    walk("src/components");
    for (const path of sources) {
      const source = readFileSync(path, "utf8");
      const literals = source.match(/fontSize:\s*(\d|["'`]\d)/g);
      expect(
        literals,
        `${path} sets an inline font size that no role can move`,
      ).toBeNull();
    }
  });

  it("gives every role-bearing rule a line height that moves with its role", () => {
    // The class of bug this catches: a rule tokenised for family and size but
    // left with a literal line height, so the surface silently stops tracking
    // the role while every other surface on it keeps up. Sweeping every rule
    // that names a role's family is what makes the next one fail here rather
    // than in someone's editor.
    const ROLE_LINE_HEIGHTS = [
      "--font-ui-line-height",
      "--font-rich-line-height",
      "--font-source-line-height",
      // Still a ratio of the Source role's line height, so the knob reaches it.
      "--lh-source-compact",
    ];
    for (const css of [tokens, kit, components]) {
      for (const [, selector, body] of css.matchAll(
        /(^|\n)([^{}@\n][^{}]*?)\{([^}]*)\}/g,
      )) {
        if (!/font-family\s*:\s*var\(--font-(ui|rich|source)-family\)/.test(body)) {
          continue;
        }
        const lineHeight = body.match(/line-height\s*:\s*([^;]+);/);
        if (!lineHeight) continue; // inheriting is fine — it tracks too
        expect(
          ROLE_LINE_HEIGHTS.some((token) => lineHeight[1].includes(token)),
          `\`${selector.trim()}\` names a role's family but sets line-height: ${lineHeight[1].trim()}`,
        ).toBe(true);
      }
    }
  });

  it("routes the chrome family aliases onto the UI role and leaves the mono accent alone", () => {
    // Existing rules read `--font-sans` by the hundred; the alias is what makes
    // them follow the user's choice without a rewrite.
    expect(tokens).toMatch(/--font-sans\s*:\s*var\(--font-ui-family\)/);
    expect(tokens).toMatch(/--font-display\s*:\s*var\(--font-ui-family\)/);

    // `--font-mono` must NOT alias onto the Source role. It dresses commit
    // hashes, diffstats, paths and kbd hints — UI-role text by OVW-FR-13's
    // "everything else" clause, and OVW-FR-13 names the status bar as UI
    // outright. Aliasing it would re-typeset the whole chrome the moment
    // someone picked a proportional face for code, which GLS-FR-18 explicitly
    // permits.
    const mono = tokens.match(/--font-mono\s*:\s*([^;]+);/)![1];
    expect(mono).not.toMatch(/var\(--font-(source|rich)-family\)/);
    expect(mono).toMatch(/monospace\s*$/);
  });

  it("names the Source role directly on every surface that belongs to it", () => {
    // The corollary of the rule above: with `--font-mono` decoupled, a source
    // surface that still reached for it would silently stop following the
    // user's coding face. Each of these is an enumerated Source surface
    // (OVW-FR-13), so each must name the role itself.
    const sourceSurfaces: [string, string][] = [
      [".editor__source", kit],
      [".editor__source-hl", kit],
      [".editor__frontmatter-preview", kit],
      [".editor__frontmatter-input,\n.editor__frontmatter-hl", kit],
      [".doc code, .t-code", tokens],
      [".doc pre", tokens],
      [".diff", components],
    ];
    for (const [selector, css] of sourceSurfaces) {
      const decl = rule(css, selector);
      expect(
        decl,
        `\`${selector}\` is a Source surface and must name the role, not the mono accent`,
      ).toMatch(/font-family\s*:\s*var\(--font-source-family\)/);
      expect(decl).not.toMatch(/font-family\s*:\s*var\(--font-mono\)/);
    }
  });

  it("OVW-FR-13: the UI role is what text falls through to", () => {
    // The document root carries the UI role, so a surface that names no role
    // renders correctly with no typographic decision of its own.
    const body = rule(tokens, "html, body");
    expect(body).toMatch(/font-family\s*:\s*var\(--font-ui-family\)/);
    expect(body).toMatch(/line-height\s*:\s*var\(--font-ui-line-height\)/);
  });

  it("OVW-FR-14: a chosen family falls back to the built-in face", () => {
    // The applier writes the chosen family in front of the stack declared
    // here, so a font uninstalled since it was chosen falls back on its own
    // (GLS-FR-22). A single-family default would leave it nothing to reach.
    for (const role of ROLES) {
      const decl = tokens.match(
        new RegExp(`--font-${role}-family\\s*:\\s*([^;]+);`),
      )![1];
      expect(decl.split(",").length).toBeGreaterThan(2);
    }
    expect(
      tokens.match(/--font-source-family\s*:\s*([^;]+);/)![1],
    ).toMatch(/monospace\s*$/);
  });
});

describe("OVW-FR-13: the partition across the three roles", () => {
  it("sets the Editor's WYSIWYG prose in the Rich Markdown role (EDT-FR-62)", () => {
    // Declared on `.doc` itself rather than inherited, so the page carries the
    // role wherever it is mounted — the Editor's canvas and a Diff tab's rich
    // rendering alike.
    expect(rule(tokens, ".doc")).toMatch(
      /font-family\s*:\s*var\(--font-rich-family\)/,
    );
    expect(rule(tokens, ".doc p,  .t-p")).toMatch(
      /line-height\s*:\s*var\(--font-rich-line-height\)/,
    );
  });

  it("sets the Editor's raw-text surface in the Source code role (EDT-FR-62)", () => {
    const source = rule(kit, ".editor__source");
    expect(source).toMatch(/font-family\s*:\s*var\(--font-source-family\)/);
    expect(source).toMatch(/font-size\s*:\s*var\(--fs-src-md\)/);
    expect(source).toMatch(/line-height\s*:\s*var\(--font-source-line-height\)/);
  });

  it("EFR-FR-DOQR: the find-highlight layer keeps the raw surface's exact metrics", () => {
    // The two are stacked and their glyphs have to register; a role change
    // reaching only one of them would knock the highlight out of position.
    const surface = rule(kit, ".editor__source");
    const layer = rule(kit, ".editor__source-hl");
    for (const property of ["font-family", "font-size", "line-height"]) {
      const value = (decl: string) =>
        decl.match(new RegExp(`${property}\\s*:\\s*([^;]+);`))![1].trim();
      expect(value(layer)).toBe(value(surface));
    }
  });

  it("sets code and frontmatter inside a rendered document in the Source code role (EDT-FR-62)", () => {
    // Code reads as code on whichever surface the author is on.
    expect(rule(tokens, ".doc code, .t-code")).toMatch(
      /font-family\s*:\s*var\(--font-source-family\)/,
    );
    const pre = rule(tokens, ".doc pre");
    expect(pre).toMatch(/font-family\s*:\s*var\(--font-source-family\)/);
    expect(pre).toMatch(/font-size\s*:\s*var\(--fs-src-lg\)/);
    expect(pre).toMatch(/line-height\s*:\s*var\(--font-source-line-height\)/);

    // EDT-FR-21: both frontmatter layers, because their box metrics must stay
    // identical for the caret to sit under the styled text.
    const fm = rule(kit, ".editor__frontmatter-input,\n.editor__frontmatter-hl");
    expect(fm).toMatch(/font-family\s*:\s*var\(--font-source-family\)/);
    expect(fm).toMatch(/font-size\s*:\s*var\(--fs-src-sm\)/);
    // All three, not two: a literal line height here would leave the region
    // silently not tracking the role while every other source surface does.
    expect(fm).toMatch(/line-height\s*:\s*var\(--font-source-line-height\)/);
    expect(rule(kit, ".editor__frontmatter-preview")).toMatch(
      /font-family\s*:\s*var\(--font-source-family\)/,
    );
  });

  it("keeps a comment or note body in the UI role, code inside it excepted", () => {
    // A comment body is a panel row, so it is the UI role — not the Rich
    // Markdown one `.doc` brings. The `.doc--ui` modifier is what takes the
    // family back, and `.doc code` above is what leaves code alone.
    expect(rule(tokens, ".doc--ui")).toMatch(
      /font-family\s*:\s*var\(--font-ui-family\)/,
    );
  });
});

describe("DFV-FR-37, DFV-FR-17, DFV-FR-21: a Diff tab's two renderings", () => {
  it("DFV-FR-37: Source and its gutter take the Source code role", () => {
    const diff = rule(components, ".diff");
    expect(diff).toMatch(/font-family\s*:\s*var\(--font-source-family\)/);
    expect(diff).toMatch(/font-size\s*:\s*var\(--fs-src-sm\)/);
    expect(diff).toMatch(/line-height\s*:\s*var\(--lh-source-compact\)/);
    // The compact measure is still a ratio of the role's line height, so the
    // role's knob moves the diff rather than leaving it fixed.
    expect(tokens).toMatch(
      /--lh-source-compact\s*:\s*calc\(\s*var\(--font-source-line-height\)/,
    );

    // The gutter inherits rather than naming a size of its own, which is what
    // keeps its fixed width and the two panes' alignment holding at any size
    // the role carries.
    const gutter = rule(components, ".diff-line__gutter");
    expect(gutter).not.toMatch(/font-size\s*:/);
    expect(gutter).not.toMatch(/font-family\s*:/);
    // Identical on every row — `min-width` or `auto` would let row 999 and row
    // 1000 size differently and step their text columns apart — but derived
    // from the role rather than pinned, or a four-digit number's glyphs run
    // past the column at any size above the default and the old and new
    // gutters read as one number.
    expect(gutter).toMatch(
      /width\s*:\s*calc\(\s*var\(--font-source-size\)/,
    );
    expect(gutter).not.toMatch(/min-width\s*:/);
    expect(gutter).not.toMatch(/width\s*:\s*auto/);
  });

  it("DFV-FR-37 / DFV-FR-17: Rich takes the same role as the Editor's WYSIWYG surface", () => {
    // This is what makes the typographic identity of DFV-FR-17 hold under any
    // font the user picks: the two name one role rather than two values that
    // happen to match today.
    const rich = rule(components, ".diff-rich");
    expect(rich).toMatch(/font-family\s*:\s*var\(--font-rich-family\)/);
    expect(rich).toMatch(/font-size\s*:\s*var\(--fs-doc-md\)/);
    expect(rich).toMatch(/line-height\s*:\s*var\(--font-rich-line-height\)/);

    // And the Editor's page reads the same three, via `.doc`.
    expect(rule(tokens, ".doc")).toMatch(
      /font-family\s*:\s*var\(--font-rich-family\)/,
    );
    expect(rule(tokens, ".doc p,  .t-p")).toMatch(
      /font-size\s*:\s*var\(--fs-doc-md\)/,
    );
  });
});
