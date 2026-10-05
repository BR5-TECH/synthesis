import { describe, expect, it } from "vitest";
import { readdirSync, readFileSync, statSync } from "node:fs";
import { extname, join } from "node:path";

/**
 * Every committed file stays mergeable.
 *
 * Merging a work stream into the branch it was created from is a three-way
 * merge of every path both sides changed (`GRB-graduation-rebase.md`
 * GRB-FR-YDTX). The merge engine reads the first 8000 bytes of each side and
 * treats a side holding a NUL byte as binary, which it refuses to merge — so
 * Git cannot settle such a path and it becomes a question for an agent turn
 * that also cannot settle it (GRB-FR-SRVN).
 *
 * The base side is the blob at the revision the two branches last shared, which
 * is committed history. A file that reaches a commit carrying a NUL is
 * therefore unmergeable from that commit onwards, and no later edit or retry
 * can clear it — the correction has to be a merge by hand. This guard holds the
 * repository on the right side of that line, where it is still free.
 *
 * A NUL is only ever wanted as a *value*, never as a byte in a source file, and
 * the two are not the same thing: `"\u0000"` is the identical string and leaves
 * the file text. Write the escape.
 */

/**
 * Everything but the directories nothing commits.
 *
 * Scoped by exclusion rather than by an allowlist of roots, because a run
 * commits whatever it changed and is not restricted to one root: it may write
 * `package.json`, `vite.config.ts`, `src-tauri/Cargo.toml`, or a workflow, and
 * a guard that listed source roots would quietly not cover them.
 */
const SKIP = new Set([
  ".git",
  "node_modules",
  "target",
  "dist",
  "coverage",
  ".vite",
]);

/**
 * Extensions the merge is expected to take. A genuinely binary asset — an icon,
 * a font — is not a defect and is not scanned; the claim is about text that
 * would otherwise merge.
 */
const TEXT = new Set([
  ".ts",
  ".tsx",
  ".mts",
  ".cts",
  ".js",
  ".jsx",
  ".mjs",
  ".cjs",
  ".rs",
  ".md",
  ".css",
  ".html",
  ".svg",
  ".json",
  ".jsonl",
  ".yml",
  ".yaml",
  ".toml",
  ".lock",
  ".sh",
  ".txt",
  ".snap",
]);

function textFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir)) {
    if (SKIP.has(entry)) continue;
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) {
      out.push(...textFiles(path));
    } else if (TEXT.has(extname(entry))) {
      out.push(path);
    }
  }
  return out;
}

describe("mergeability invariants", () => {
  it("GRB-FR-SRVN: no committed text file carries a NUL byte", () => {
    const offenders = textFiles(".")
      .map((path) => [path, readFileSync(path).indexOf(0)] as const)
      .filter(([, at]) => at !== -1)
      .map(([path, at]) => `${path} (byte ${at})`);

    // Deliberately not scoped to the first 8000 bytes, although that is the
    // window the merge engine reads. A NUL past it is unmergeable the moment an
    // edit above it slides it inside, so the offset is a fact about the failure
    // rather than a threshold anything is safe under.
    expect(offenders).toEqual([]);
  });

  it("the sentinel is escaped, not doubled", () => {
    // The realistic mistake once the raw byte is gone. `"\\u0000"` is seven
    // characters, not one, and nothing downstream would notice: the sentinels
    // are compared only against strings built the same way, so a doubled
    // backslash keeps every test green while the value silently changes.
    // TypeScript only: the sentinels live there, and Rust spells the same
    // escape `\u{0}`, so a `.rs` file carrying this sequence is test data
    // describing the TypeScript form rather than a sentinel of its own.
    const doubled = String.raw`\\u0000`;
    const offenders = textFiles(".")
      .filter((path) => /\.tsx?$/.test(path))
      .filter((path) => path !== "src/test/mergeability-invariants.test.ts")
      .filter((path) => readFileSync(path, "utf8").includes(doubled));

    expect(offenders).toEqual([]);
  });

  it("the scan reaches the files it claims to cover", () => {
    // Without this the guard degrades silently: a renamed directory or an
    // extension that stopped matching leaves it reporting nothing, which reads
    // exactly like a repository with nothing to report.
    const scanned = new Set(textFiles("."));
    for (const path of [
      "src/types/index.ts",
      "src/test/mergeability-invariants.test.ts",
      "src-tauri/src/graduation/record.rs",
      "src-tauri/Cargo.toml",
      "specifications/core/GRD-graduation.md",
      "resources/prompts/graduation/review.md",
      "package.json",
    ]) {
      expect(scanned.has(path), `${path} is scanned`).toBe(true);
    }
  });
});
