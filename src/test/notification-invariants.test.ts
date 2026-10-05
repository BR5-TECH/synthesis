import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, sep } from "node:path";

/**
 * Structural invariants for the notification facility.
 *
 * These are properties the prose asserts and no behavioural test can reach:
 * NTF-FR-01's "there is no second route to the notification centre" and
 * NTF-FR-24's "ordinary work raises nothing" are both claims about the shape of
 * the codebase, true only for as long as nobody adds a call. Written in the
 * style of `style-invariants.test.ts`, which guards comparable claims.
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

describe("one route to the notification centre (NTF-FR-01)", () => {
  it("is reached only through the facility, never by a surface directly", () => {
    // NTF-FR-01: "no surface reaches `post_notification` directly, so the
    // policy of NTF-FR-08 through NTF-FR-11 governs every notification the
    // application ever posts". A second call site would silently bypass the
    // switch, the permission check, and the focus policy all at once.
    const offenders = sourceFiles(SRC)
      .filter((f) => f !== join("src", "state", "notifications.ts"))
      // The typed wrappers are the one place a command name lives; the rule is
      // that no *surface* reaches this one, not that no wrapper declares it.
      .filter((f) => !f.startsWith(join("src", "api") + sep))
      .filter((f) => /\bpostNotification\b/.test(readFileSync(f, "utf8")));

    expect(offenders).toEqual([]);
  });

  it("keeps the raise API as the only exported way to post", () => {
    // The facility must not re-export the raw command alongside `raise`, which
    // would make the bypass a one-import mistake rather than a deliberate one.
    const facility = readFileSync(join("src", "state", "notifications.ts"), "utf8");
    expect(facility).not.toMatch(/export\s+\{[^}]*postNotification/);
    expect(facility).toMatch(/export async function raiseNotification/);
  });
});

describe("ordinary work raises nothing (NTF-FR-24)", () => {
  it("has exactly one caller of raiseNotification, and names it", () => {
    // NTF-FR-24: opening, editing, saving, committing, scanning and searching
    // post no notification, because a raise is something a surface asks for
    // explicitly. Two things ask, and both ask from the same file:
    //
    //  - the rehearsal of GLS-FR-27, which raises only when the author
    //    activates it. Its control is in the Global settings **window**, which
    //    is a webview of its own (SWN-FR-01) and therefore holds a different
    //    copy of the facility; so the section asks the main window for the
    //    raise rather than making it, which is also what keeps the notification
    //    coming when the author closes that window before the delay elapses
    //    (NTF-FR-25, NTF-FR-17, GLS-FR-27); and
    //  - a change an agent has proposed to a draft file (DCR-FR-18), which
    //    raises when the proposal is recorded, because it is a decision the
    //    author is now owed and the tab holding it may not be one they are
    //    looking at. Nothing raises when such a proposal is *decided*.
    //
    // This test is the deliberate gate on that list: adding a call means
    // updating it, and updating it means saying which surface now interrupts
    // the author.
    //
    // The one caller is `useAppNotifications`, which is the main window's own
    // notification surface — split out of `App.tsx` so the file stays readable,
    // and still the single place a raise is made from.
    const callers = sourceFiles(SRC)
      // The facility's own definition, not a call site.
      .filter((f) => f !== join("src", "state", "notifications.ts"))
      .filter((f) => /\braiseNotification\s*\(/.test(readFileSync(f, "utf8")));
    expect(callers.sort()).toEqual([
      join("src", "hooks", "useAppNotifications.ts"),
    ]);
  });
});

describe("nothing outside the application can address it (NTD-FR-20)", () => {
  it("registers no URL scheme, protocol handler, or file association", () => {
    // NTD-FR-20. The security-shaped half of the design: a `synthesis://`
    // address is minted inside the running application and parsed on its way
    // back into it, and no process outside can hand one in or launch the app
    // by URL. A deep-link plugin or a `CFBundleURLTypes` entry would quietly
    // undo that, and nothing else in the suite would notice.
    const config = readFileSync(join("src-tauri", "tauri.conf.json"), "utf8");
    for (const forbidden of [
      "deep-link",
      "deepLink",
      "CFBundleURLTypes",
      "CFBundleURLSchemes",
      "synthesis://",
      "protocol-handler",
    ]) {
      expect(config, `tauri.conf.json declares ${forbidden}`).not.toContain(
        forbidden,
      );
    }

    const cargo = readFileSync(join("src-tauri", "Cargo.toml"), "utf8");
    expect(cargo).not.toMatch(/^\s*tauri-plugin-deep-link/m);
  });
});
