import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: vi.fn(async () => {}) }));

import catalogJson from "../../../../resources/a2ui/discussion-catalog.json";
import { readFileSync } from "node:fs";
import pkg from "../../../../package.json";
import { CATALOG_ID, REPLACED_COMPONENTS, discussionCatalog } from "./catalog";
import { APP_PART_NAMES } from "./parts";

const fullCss = readFileSync(`${process.cwd()}/src/styles/kit/agent-surface.css`, "utf8");
/** The part of the panel stylesheet that maps the renderer's variables. */
const surfaceCss = fullCss.slice(fullCss.indexOf("/* ASF-FR-HFPF"));

/** The pinned official catalog, with its references made local as the generator makes them. */
function officialCatalog(): Record<string, Record<string, unknown>> {
  const root = `${process.cwd()}/node_modules/@a2ui/web_core/src/v1_0/schemas/catalogs/basic/catalog.json`;
  const text = readFileSync(root, "utf8")
    .replace(/"common_types\.json#/g, '"#')
    .replace(/"catalog\.json#/g, '"#');
  return JSON.parse(text);
}

describe("the renderer catalog", () => {
  it("AUI-FR-CSOS / AUI-FR-PMXJ / AUI-FR-MAMC: the vendored catalog holds the pinned official components and functions as they are, but the obscured TextField", () => {
    const official = officialCatalog();
    const vendored = catalogJson as unknown as Record<string, Record<string, unknown>>;
    for (const [name, schema] of Object.entries(official.components)) {
      if (name === "TextField") {
        expect(JSON.stringify(vendored.components[name])).toBe(
          JSON.stringify(schema).replace(',"obscured"', ""),
        );
      } else {
        expect(vendored.components[name]).toEqual(schema);
      }
    }
    expect(vendored.functions).toEqual(official.functions);
  });

  it("ASF-FR-HRQL: the renderer holds exactly the components and functions the backend validates against", () => {
    const catalog = discussionCatalog();
    expect(catalog.id).toBe(CATALOG_ID);
    expect([...catalog.components.keys()].sort()).toEqual(Object.keys(catalogJson.components).sort());
    // `@index` is the system function of a template; the catalog document
    // holds it with the common types rather than with the functions.
    const functions = [...catalog.functions.keys()].filter((n) => n !== "@index");
    expect(functions.sort()).toEqual(Object.keys(catalogJson.functions).sort());
  });

  it("ASF-FR-NOBK / ASF-FR-NEPE / ASF-FR-VZCC: the application draws media and the dialog itself", () => {
    for (const name of REPLACED_COMPONENTS) expect(APP_PART_NAMES).toContain(name);
  });

  it("ASF-FR-HRQL: the official packages are pinned to exact versions", () => {
    const deps = pkg.dependencies as Record<string, string>;
    for (const name of ["@a2ui/react", "@a2ui/web_core"]) {
      expect(deps[name]).toMatch(/^\d+\.\d+\.\d+$/);
    }
  });

  it("ASF-FR-HFPF / ASF-FR-JVOK: the panel maps the renderer's variables to the application's tokens, and names the bundled icon font", () => {
    for (const variable of [
      "--a2ui-color-primary: var(--accent)",
      "--a2ui-color-on-surface: var(--fg-1)",
      "--a2ui-color-border: var(--border-1)",
      "--a2ui-font-family: var(--font-ui-family)",
      "--a2ui-border-radius: var(--r-sm)",
    ]) {
      expect(surfaceCss).toContain(variable);
    }
    expect(surfaceCss).toContain('--a2ui-icon-font-family: "Material Symbols Outlined"');
    expect((pkg.dependencies as Record<string, string>)["material-symbols"]).toBeDefined();
    // Every rule of the mapping stays inside the panel.
    const selectors = surfaceCss.replace(/\/\*[\s\S]*?\*\//g, "").match(/^[^\s{}][^{]*\{/gm) ?? [];
    for (const selector of selectors) expect(selector.trim()).toMatch(/^\.agent-surface/);
  });
});
