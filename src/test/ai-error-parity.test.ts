import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

import { AI_ERRORS } from "../types";

/**
 * The AI typed errors are a cross-process contract kept as hand-written lists:
 * `ERR_*` constants in **two** Rust modules — `src-tauri/src/agentic.rs` and
 * `src-tauri/src/ai_api.rs` — and `AI_ERRORS` in `src/types.ts`.
 *
 * Every failure mode of a divergence is silent. `aiErrorMessage` falls through
 * to `default: return raw`, so a drifted slug does not throw or break a build —
 * it renders on the verification status line as `not_the_expected_cli`, exactly
 * defeating the "named in terms the author can act on" clause of AII-FR-09 and
 * AII-FR-20. Two modules rather than one makes the drift likelier, not less:
 * the same wire value (`timed_out`, `unreachable`) is declared in both, and a
 * change to one is easy to forget in the other.
 *
 * Reading the Rust source from here is what turns that drift into a red test —
 * the same technique `github-error-parity.test.ts` and `ci-workflow.test.ts`
 * use.
 */

// Vitest runs with the project root as cwd.
const MODULES = ["agentic", "ai_api"] as const;

const SOURCES = new Map(
  MODULES.map((m) => [
    m,
    readFileSync(resolve(process.cwd(), `src-tauri/src/${m}.rs`), "utf8"),
  ]),
);

/** Every `pub const ERR_NAME: &str = "value";` declared by a Rust module. */
function rustErrorConstants(): Map<string, { wire: string; module: string }> {
  const out = new Map<string, { wire: string; module: string }>();
  const re = /pub const (ERR_[A-Z_]+): &str = "([^"]+)";/g;
  for (const [module, source] of SOURCES) {
    for (const [, name, wire] of source.matchAll(re)) {
      out.set(`${module}::${name}`, { wire, module });
    }
  }
  return out;
}

/**
 * The five the UI never matches on, because they can only arise from paths it
 * cannot reach: `ERR_NO_PROJECT` needs an override attempted with no project
 * open; the two `none_*` codes are returned only by `resolve_agentic_
 * invocation` / `resolve_ai_api_call`, which are deliberately not registered as
 * Tauri commands (AIC-FR-19 / AAP-FR-19); and the last two are returned only by
 * `resolve_agent_launch_credential`, the executor-only credential handoff,
 * which reaches no Tauri command and no caller but the agent-CLI executor
 * (AIC-FR-30 / AIC-FR-31).
 *
 * Listing one here is a claim that the frontend *cannot* receive it, not that
 * it would be inconvenient to translate. Adding an entry for a code a command
 * can actually return would defeat the whole test.
 */
const NOT_MATCHED_ON_BY_THE_UI = new Set([
  "ERR_NO_PROJECT",
  "ERR_NONE_CONFIGURED",
  "ERR_NONE_SELECTED",
  "ERR_NOT_AN_EXECUTABLE_CLI",
  "ERR_CODEX_CONFIG_MISSING",
]);

describe("AI typed-error parity across the IPC boundary", () => {
  it("finds the Rust constants at all (the test is worthless if the regex drifts)", () => {
    const constants = rustErrorConstants();
    expect(constants.size).toBeGreaterThanOrEqual(25);
    const wires = [...constants.values()].map((c) => c.wire);
    expect(wires).toContain("not_the_expected_cli");
    expect(wires).toContain("not_an_ai_endpoint");
    // Both modules must actually have been read.
    for (const m of MODULES) {
      expect(
        [...constants.values()].some((c) => c.module === m),
        `no ERR_* constants were found in ${m}.rs — the path or regex has drifted`,
      ).toBe(true);
    }
  });

  it("every error the frontend matches on is declared by a backend module", () => {
    const declared = new Set([...rustErrorConstants().values()].map((c) => c.wire));
    for (const [key, wire] of Object.entries(AI_ERRORS)) {
      expect(
        declared.has(wire),
        `AI_ERRORS.${key} is "${wire}", which no ERR_* constant in ` +
          `agentic.rs or ai_api.rs declares — the frontend would silently never match it`,
      ).toBe(true);
    }
  });

  it("every error a backend module can return is one the frontend knows", () => {
    const known = new Set<string>(Object.values(AI_ERRORS));
    for (const [qualified, { wire }] of rustErrorConstants()) {
      const bare = qualified.split("::")[1];
      if (NOT_MATCHED_ON_BY_THE_UI.has(bare)) continue;
      expect(
        known.has(wire),
        `${qualified} = "${wire}" is returned by the backend but absent from ` +
          `AI_ERRORS — the UI would render it to the author as a raw slug`,
      ).toBe(true);
    }
  });

  it("a wire value declared in both modules means the same thing in both", () => {
    // `timed_out`, `unreachable`, `rejected`, and `keychain_unavailable` are
    // each declared twice. A change to one module's spelling that missed the
    // other would leave the UI matching only half the cases — which reads as
    // "this failure is unrecognised" exactly half the time.
    const byName = new Map<string, Set<string>>();
    for (const [qualified, { wire }] of rustErrorConstants()) {
      const bare = qualified.split("::")[1];
      const wires = byName.get(bare) ?? new Set<string>();
      wires.add(wire);
      byName.set(bare, wires);
    }
    for (const [name, wires] of byName) {
      expect(
        wires.size,
        `${name} is declared in both modules with different wire values ` +
          `(${[...wires].join(", ")}) — the UI can only match one of them`,
      ).toBe(1);
    }
  });
});
