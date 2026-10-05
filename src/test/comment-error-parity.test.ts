import { readdirSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

import { COMMENT_ERRORS, COMMENT_IDENTITY_ERRORS, GITHUB_TOKEN_ERRORS } from "../types";

/**
 * The comment typed errors are a cross-process contract kept as two hand-written
 * lists: `ERR_*` constants in `src-tauri/src/comments.rs` and `COMMENT_ERRORS` in
 * `src/types.ts`.
 *
 * Every failure mode of a divergence is silent, exactly as for the GitHub token
 * vocabulary next door (`github-error-parity.test.ts`). The rail matches on these
 * strings to decide what to do: a locked thread re-renders the card as locked
 * (CMT-FR-34), a selection-required identity opens the token picker (CMT-FR-25)
 * while a missing one routes to Global settings (CMT-FR-26). A typo on either
 * side does not break a build or throw — it falls through to a default branch and
 * the user gets a raw slug, or the wrong recovery path. Reading the Rust source
 * from here is what turns that drift into a red test.
 */

/**
 * The whole Rust module, root file and submodules together.
 *
 * `comments` outgrew one file and is now a directory beside `comments.rs`, so
 * the constants sit in `comments/constants.rs`. Reading only the root file
 * would find no `ERR_*` at all — and the assertion below is "every frontend
 * slug is declared in Rust", which an empty set fails loudly rather than
 * passing vacuously. That is the direction this must fail in, but it should
 * not have to fail at all: the module is what the contract lives in.
 */
const RUST_SOURCE = [
  resolve(process.cwd(), "src-tauri/src/comments.rs"),
  ...readdirSync(resolve(process.cwd(), "src-tauri/src/comments"), {
    withFileTypes: true,
  })
    .filter((entry) => entry.isFile() && entry.name.endsWith(".rs"))
    .map((entry) => resolve(process.cwd(), "src-tauri/src/comments", entry.name)),
]
  .map((path) => readFileSync(path, "utf8"))
  .join("\n");

/** Every `pub const ERR_NAME: &str = "value";` declared by the Rust module. */
function rustErrorConstants(): Map<string, string> {
  const out = new Map<string, string>();
  const re = /pub const (ERR_[A-Z_]+): &str = "([^"]+)";/g;
  for (const [, name, value] of RUST_SOURCE.matchAll(re)) out.set(name, value);
  return out;
}

describe("comment typed-error parity across the IPC boundary", () => {
  it("finds the Rust constants at all (the test is worthless if the regex drifts)", () => {
    const constants = rustErrorConstants();
    expect(constants.size).toBeGreaterThanOrEqual(4);
    expect([...constants.values()]).toContain("discussion_locked");
  });

  it("every error the frontend matches on is declared by the backend", () => {
    const declared = new Set(rustErrorConstants().values());
    for (const [key, wire] of Object.entries(COMMENT_ERRORS)) {
      expect(
        declared.has(wire),
        `COMMENT_ERRORS.${key} is "${wire}", which no ERR_* constant in ` +
          `comments.rs declares — the frontend would silently never match it`,
      ).toBe(true);
    }
  });

  it("every error the backend can return is one the frontend knows", () => {
    const known = new Set<string>(Object.values(COMMENT_ERRORS));
    // No exclusions here, unlike `github-error-parity.test.ts`: the
    // "no project open" refusal comes from `ProjectState::require_root`, shared
    // with every other module, so this one declares no constant the UI cannot
    // match on.
    for (const [name, wire] of rustErrorConstants()) {
      expect(
        known.has(wire),
        `${name} = "${wire}" is returned by the backend but absent from ` +
          `COMMENT_ERRORS — the UI would render it as a raw slug`,
      ).toBe(true);
    }
  });

  it("the identity vocabulary is the GitHub one rather than a second spelling of it", () => {
    // CMS-FR-12: identity resolution *is* token resolution, so these must be the
    // same strings the token surfaces already match on. A parallel list would
    // drift silently — the failure this whole file exists to prevent.
    for (const [key, wire] of Object.entries(COMMENT_IDENTITY_ERRORS)) {
      expect(
        Object.values(GITHUB_TOKEN_ERRORS).includes(wire),
        `COMMENT_IDENTITY_ERRORS.${key} is "${wire}", which GITHUB_TOKEN_ERRORS ` +
          `does not declare — the two vocabularies have diverged`,
      ).toBe(true);
    }
  });

  it("keeps the comment and identity vocabularies disjoint", () => {
    // A string in both would make a rail error ambiguous: it could not tell a
    // thread problem (retry the post) from an identity one (fix the token).
    const comment = new Set<string>(Object.values(COMMENT_ERRORS));
    for (const wire of Object.values(COMMENT_IDENTITY_ERRORS)) {
      expect(comment.has(wire)).toBe(false);
    }
  });
});

describe("the store refusal reaches the author as prose", () => {
  it("CMS-FR-YQND, CMT-FR-34: store_unavailable is not rendered as its own slug", async () => {
    const { commentErrorMessage } = await import("../components/CommentRail/messages");
    const rendered = commentErrorMessage(COMMENT_ERRORS.storeUnavailable);
    // A store that could not be reached is a storage fault the author can act
    // on — retry, check the disk — and it must not read as a conversation that
    // holds nothing (RMS-FR-HAJC).
    expect(rendered).not.toBe(COMMENT_ERRORS.storeUnavailable);
    expect(rendered).not.toContain("_");
  });
});
