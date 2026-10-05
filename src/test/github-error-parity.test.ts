import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

import { GITHUB_TOKEN_ERRORS } from "../types";

/**
 * The GitHub typed errors are a cross-process contract kept as two hand-written
 * lists: `ERR_*` constants in `src-tauri/src/github_tokens.rs` and
 * `GITHUB_TOKEN_ERRORS` in `src/types.ts`.
 *
 * Every failure mode of a divergence is silent. The frontend matches on these
 * strings to decide what to tell the user and what to do next — whether to open
 * the token picker or route to Global settings (GHA-FR-16), whether to blame
 * the token or the network (GHA-FR-10). A typo on either side does not break a
 * build or throw: it falls through to a default branch and the user gets a raw
 * slug, or worse, the wrong recovery path. Reading the Rust source from here is
 * what turns that drift into a red test — the same technique
 * `ci-workflow.test.ts` uses for the workflow file.
 */

// Vitest runs with the project root as cwd.
const RUST_SOURCE = readFileSync(
  resolve(process.cwd(), "src-tauri/src/github_tokens.rs"),
  "utf8",
);

/** Every `pub const ERR_NAME: &str = "value";` declared by the Rust module. */
function rustErrorConstants(): Map<string, string> {
  const out = new Map<string, string>();
  const re = /pub const (ERR_[A-Z_]+): &str = "([^"]+)";/g;
  for (const [, name, value] of RUST_SOURCE.matchAll(re)) out.set(name, value);
  return out;
}

describe("GitHub typed-error parity across the IPC boundary", () => {
  it("finds the Rust constants at all (the test is worthless if the regex drifts)", () => {
    const constants = rustErrorConstants();
    expect(constants.size).toBeGreaterThanOrEqual(7);
    expect([...constants.values()]).toContain("invalid_token");
  });

  it("every error the frontend matches on is declared by the backend", () => {
    const declared = new Set(rustErrorConstants().values());
    for (const [key, wire] of Object.entries(GITHUB_TOKEN_ERRORS)) {
      expect(
        declared.has(wire),
        `GITHUB_TOKEN_ERRORS.${key} is "${wire}", which no ERR_* constant in ` +
          `github_tokens.rs declares — the frontend would silently never match it`,
      ).toBe(true);
    }
  });

  it("every error the backend can return is one the frontend knows", () => {
    const known = new Set<string>(Object.values(GITHUB_TOKEN_ERRORS));
    // `ERR_NO_PROJECT` is a plain sentence rather than a matched code: it can
    // only arise from binding with no project open, which the UI never does.
    const notMatchedOnByTheUi = new Set(["ERR_NO_PROJECT"]);
    for (const [name, wire] of rustErrorConstants()) {
      if (notMatchedOnByTheUi.has(name)) continue;
      expect(
        known.has(wire),
        `${name} = "${wire}" is returned by the backend but absent from ` +
          `GITHUB_TOKEN_ERRORS — the UI would render it as a raw slug`,
      ).toBe(true);
    }
  });
});
