import { describe, expect, it, vi } from "vitest";

const openUrlMock = vi.hoisted(() => vi.fn(async () => {}));
vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: openUrlMock }));

import catalogJson from "../../../../resources/a2ui/discussion-catalog.json";
import { readFileSync } from "node:fs";
import pkg from "../../../../package.json";
import { basicCatalog } from "@a2ui/web_core/v1_0";
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

  it("ASF-FR-VZCC: openUrl is the application's function, and it opens the address through the opener", () => {
    const own = discussionCatalog().functions.get("openUrl");
    expect(own).toBeDefined();
    expect(own).not.toBe(basicCatalog.functions.get("openUrl"));
    own?.execute({ url: "https://a2ui.org" } as never, {} as never);
    expect(openUrlMock).toHaveBeenCalledWith("https://a2ui.org");
    own?.execute({ url: "file:///etc/hosts" } as never, {} as never);
    expect(openUrlMock).toHaveBeenCalledTimes(1);
  });

  it("ASF-FR-JVOK: the icon font loads from the bundle and never from the network", () => {
    const renderer = readFileSync(`${process.cwd()}/src/components/AgentSurface/a2ui/SurfaceRenderer.tsx`, "utf8");
    expect(renderer).toContain('import "material-symbols/outlined.css";');
    const fontCss = readFileSync(`${process.cwd()}/node_modules/material-symbols/outlined.css`, "utf8");
    const urls = [...fontCss.matchAll(/url\(\s*["']?([^"')]+)/g)].map((m) => m[1]);
    expect(urls.length).toBeGreaterThan(0);
    for (const url of urls) expect(url).toMatch(/^\.\/[\w.-]+\.woff2$/);
    expect(fontCss).not.toMatch(/@import/);
  });

  it("ASF-FR-HFPF: the official sheet on the document sets only its own variables and the color scheme, which the app theme sets again", () => {
    const sheet = readFileSync(
      `${process.cwd()}/node_modules/@a2ui/web_core/src/universal/basic_catalog/styles/default.js`,
      "utf8",
    );
    const css = /const DEFAULT_CSS = `([\s\S]*?)`;/.exec(sheet)?.[1] ?? "";
    expect(css).not.toBe("");
    const properties = [...css.matchAll(/(?:^|[;{])\s*([a-z-]+)\s*:/gm)].map((m) => m[1]);
    expect(properties.length).toBeGreaterThan(0);
    for (const property of properties) expect(property === "color-scheme" || property.startsWith("--a2ui-")).toBe(true);
    const theme = readFileSync(`${process.cwd()}/src/styles/colors_and_type.css`, "utf8");
    for (const mode of ["light", "dark"]) {
      expect(theme).toMatch(new RegExp(`\\[data-theme="${mode}"\\]\\s*\\{[^}]*color-scheme: ${mode};`));
    }
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
