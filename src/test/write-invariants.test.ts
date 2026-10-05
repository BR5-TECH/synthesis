import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";

/**
 * Structural invariants for two deletions.
 *
 * A deletion is the one kind of change behavioural tests cover badly: every
 * surface has to be asserted *not* to offer the thing, one negative per surface,
 * and a new surface that reintroduces it is a surface nobody wrote a negative
 * for. These hold the claim once, over the source itself — the same shape
 * `style-invariants.test.ts` and `notification-invariants.test.ts` already use.
 *
 * - **EDT-FR-16 / FLO-FR-27**: a document writes itself, so no tab renders a
 *   Save control. `SNV-shell-navigation.md`'s File menu keeps its Save and Save
 *   All items — those are native menu items, not controls in a tab.
 * - **Injection out of the UI**: `INJ-injection.md` is gone, `perform_injection`
 *   has no UI consumer (`../core/ADP-adapters.md`), and no surface renders a way
 *   to reach it.
 */

const SRC = "src";

function sourceFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) {
      out.push(...sourceFiles(path));
    } else if (/\.tsx?$/.test(entry) && !/\.test\.tsx?$/.test(entry)) {
      out.push(path);
    }
  }
  return out;
}

/** Every non-test source file, paired with its text. */
function sources(): Array<[string, string]> {
  return sourceFiles(SRC).map((f) => [f, readFileSync(f, "utf8")]);
}

/**
 * The surfaces a content tab is made of.
 *
 * Scoped rather than repo-wide, because the claim is about tabs: a settings
 * window keeps its per-section Save buttons, and the Notes panel keeps the
 * inline Save on a note it is editing. Neither is a document that writes
 * itself, and neither is a tab (SWN-FR-01).
 */
const TAB_SURFACES = [
  ...sourceFiles(join("src", "components", "Editor")),
  ...sourceFiles(join("src", "components", "Flow")),
  join("src", "components", "FindPanel.tsx"),
  join("src", "components", "CommentRail", "index.tsx"),
  join("src", "components", "CommentRail", "ThreadCard.tsx"),
  join("src", "components", "discussion", "DiscussionSurface.tsx"),
  join("src", "components", "discussion", "DiscussionComposer.tsx"),
  join("src", "components", "ActionControl.tsx"),
  join("src", "components", "NewArtifactWorkspace", "index.tsx"),
  join("src", "components", "NewArtifactWorkspace", "DraftHistoryRail.tsx"),
  join("src", "components", "Viewport.tsx"),
  join("src", "components", "TabStrip.tsx"),
];

describe("no content tab renders a Save control (EDT-FR-16, FLO-FR-27)", () => {
  it("names no Save affordance in the surfaces a tab is built from", () => {
    // A control is reachable by its accessible name, its tooltip, or its own
    // label, so all three are what this looks for. The File menu's Save and Save
    // All are declared on the Rust side (`src-tauri/src/menu.rs`) and are
    // deliberately out of scope: a native menu item is not a control in a tab.
    const offenders: string[] = [];
    for (const file of TAB_SURFACES) {
      const text = readFileSync(file, "utf8");
      for (const [i, line] of text.split("\n").entries()) {
        if (
          /(?:aria-label|title)=\{?["']Save/.test(line) ||
          />\s*Save\b/.test(line)
        ) {
          offenders.push(`${file}:${i + 1}`);
        }
      }
    }
    expect(offenders).toEqual([]);
  });

  it("covers surfaces that exist", () => {
    // The list above is only an invariant for as long as its entries are real
    // files — a renamed component would silently drop out of the check.
    for (const file of TAB_SURFACES) {
      expect(statSync(file).isFile()).toBe(true);
    }
  });
});

describe("injection is out of the UI", () => {
  it("is named by no surface, and reached by no call", () => {
    // The backend commands survive with no UI consumer (`ADP-adapters.md`), so
    // what must stay absent is anything in the window that reaches them or
    // offers them: the command names, the modal, and the affordance's label.
    const offenders: string[] = [];
    for (const [file, text] of sources()) {
      for (const [i, line] of text.split("\n").entries()) {
        if (/perform_injection|performInjection|InjectionModal/.test(line)) {
          offenders.push(`${file}:${i + 1} — reaches the injection primitive`);
        }
        if (/(?:aria-label|title)=\{?["']Inject/.test(line)) {
          offenders.push(`${file}:${i + 1} — offers an Inject control`);
        }
      }
    }
    expect(offenders).toEqual([]);
  });
});
