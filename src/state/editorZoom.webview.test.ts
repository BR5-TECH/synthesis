/**
 * EZM-FR-CFWK: the webview's own zoom keys and gestures stay off, so nothing
 * zooms the whole window. They are off unless a window turns them on, so this
 * reads the window configuration and the Rust window builders.
 */
import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, resolve } from "node:path";

const TAURI = resolve(process.cwd(), "src-tauri");

function rustFiles(dir: string): string[] {
  return readdirSync(dir).flatMap((name) => {
    const path = join(dir, name);
    if (statSync(path).isDirectory()) return rustFiles(path);
    return path.endsWith(".rs") ? [path] : [];
  });
}

describe("the webview zoom", () => {
  it("EZM-FR-CFWK: no window in the configuration turns on the webview zoom keys", () => {
    const conf = JSON.parse(readFileSync(join(TAURI, "tauri.conf.json"), "utf8")) as {
      app?: { windows?: Array<Record<string, unknown>> };
    };
    for (const window of conf.app?.windows ?? []) {
      expect(window.zoomHotkeysEnabled ?? false).toBe(false);
    }
  });

  it("EZM-FR-CFWK: no window builder turns on the webview zoom keys", () => {
    const offenders = rustFiles(join(TAURI, "src")).filter((path) =>
      /zoom_hotkeys_enabled\s*\(\s*true\s*\)/.test(readFileSync(path, "utf8")),
    );
    expect(offenders).toEqual([]);
  });
});
