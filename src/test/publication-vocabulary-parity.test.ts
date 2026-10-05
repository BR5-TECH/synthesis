import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * The publication vocabulary is a cross-process contract kept as two
 * hand-written lists: `ERR_*` constants and the `RemoteEligibility` variants in
 * `src-tauri/src/github_publication/records.rs`, and the `PublicationErrorCode`
 * and `PublicationRemoteEligibility` unions in `src/types/publication.ts`.
 *
 * GHP-FR-PVOA says the vocabulary "is exactly" its list, and nothing enforces
 * that across the boundary. Divergence is silent both ways: a value the backend
 * can return and the union does not name reaches the UI as an unhandled string,
 * and a union member the backend never sends is dead code nobody can find. No
 * build catches either, because the payload crosses the boundary as JSON and
 * every component here renders the accompanying reason verbatim.
 *
 * Reading both sources from here is what turns that drift into a red test — the
 * technique `github-error-parity.test.ts` already uses for the token module.
 */

// Vitest runs with the project root as cwd.
const read = (path: string) => readFileSync(resolve(process.cwd(), path), "utf8");
const RUST_SOURCE = read("src-tauri/src/github_publication/records.rs");
const TS_SOURCE = read("src/types/publication.ts");

/** Every `pub const ERR_NAME: &str = "value";` the Rust module declares. */
function rustErrorCodes(): Map<string, string> {
  const out = new Map<string, string>();
  const re = /pub const (ERR_[A-Z_]+): &str = "([^"]+)";/g;
  for (const [, name, value] of RUST_SOURCE.matchAll(re)) out.set(name, value);
  return out;
}

/**
 * The `RemoteEligibility` variants, in the spelling `#[serde(rename_all =
 * "snake_case")]` gives them on the wire.
 */
function rustEligibilityValues(): string[] {
  const body = /pub enum RemoteEligibility \{([^}]+)\}/.exec(RUST_SOURCE);
  expect(body, "RemoteEligibility is no longer declared as this regex expects").not.toBeNull();
  return [...body![1].matchAll(/^\s*([A-Z][A-Za-z]*),/gm)].map(([, variant]) =>
    variant.replace(/(?<!^)([A-Z])/g, "_$1").toLowerCase(),
  );
}

/** The string members of a `export type Name = | "a" | "b";` union. */
function tsUnionMembers(name: string): string[] {
  const re = new RegExp(`export type ${name} =([^;]+);`);
  const body = re.exec(TS_SOURCE);
  expect(body, `${name} is no longer declared as this regex expects`).not.toBeNull();
  return [...body![1].matchAll(/"([a-z_]+)"/g)].map(([, member]) => member);
}

describe("publication vocabulary parity across the IPC boundary", () => {
  it("finds both sides at all (the test is worthless if a regex drifts)", () => {
    expect(rustErrorCodes().size).toBeGreaterThanOrEqual(16);
    expect(rustEligibilityValues()).toContain("eligible");
    expect(tsUnionMembers("PublicationErrorCode")).toContain("no_project_open");
    expect(tsUnionMembers("PublicationRemoteEligibility")).toContain("not_github");
  });

  it("GHP-FR-PVOA: the error vocabulary is the same set on both sides", () => {
    const rust = [...rustErrorCodes().values()].sort();
    const typescript = [...tsUnionMembers("PublicationErrorCode")].sort();
    expect(typescript).toEqual(rust);
  });

  it("GHP-FR-MZPR: the eligibility values are the same set on both sides", () => {
    const rust = [...rustEligibilityValues()].sort();
    const typescript = [...tsUnionMembers("PublicationRemoteEligibility")].sort();
    expect(typescript).toEqual(rust);
  });

  it("GHP-FR-MZPR: every ineligible value is also a typed error code", () => {
    // A remote's eligibility becomes the refusal a publish answers with
    // (GHP-FR-ZRFP), so every value except `eligible` must be spelled the same
    // in both vocabularies.
    const codes = new Set(rustErrorCodes().values());
    for (const value of rustEligibilityValues()) {
      if (value === "eligible") continue;
      // `not_github` is the one that is named differently as a refusal: it
      // describes one remote, and the refusal it produces describes the project
      // (`no_github_remote`).
      if (value === "not_github") {
        expect(codes.has("no_github_remote")).toBe(true);
        continue;
      }
      expect(
        codes.has(value),
        `RemoteEligibility::${value} is not declared as an ERR_* code, so a ` +
          `refusal standing on it would reach the UI as an unknown string`,
      ).toBe(true);
    }
  });

  it("GHP-FR-AZPF: the milestone policies are the same set on both sides", () => {
    const source = read("src-tauri/src/project_settings/publication_settings.rs");
    const body = /pub enum MilestonePolicy \{([^}]+)\}/.exec(source);
    expect(body, "MilestonePolicy is no longer declared as this regex expects").not.toBeNull();
    const rust = [...body![1].matchAll(/^\s*([A-Z][A-Za-z]*),/gm)].map(([, variant]) =>
      variant.replace(/(?<!^)([A-Z])/g, "_$1").toLowerCase(),
    );
    expect(rust.length).toBe(3);
    expect(tsUnionMembers("MilestonePolicy").sort()).toEqual(rust.sort());
  });

  it("GHP-FR-HRUN: the mismatch names a recovery choice carries are the same set on both sides", () => {
    const flow = read("src-tauri/src/github_publication/flow.rs");
    const rust = [...flow.matchAll(/differs\.push\("([a-z]+)"\.to_string\(\)\)/g)].map(
      ([, name]) => name,
    );
    expect(rust.length).toBeGreaterThanOrEqual(5);
    expect(tsUnionMembers("PublicationMismatch").sort()).toEqual([...new Set(rust)].sort());
  });
});
