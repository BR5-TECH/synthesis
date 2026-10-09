import { readdirSync, readFileSync } from "node:fs";
import { join, resolve } from "node:path";
import { describe, expect, it } from "vitest";

/**
 * Conventions that hold across the app's own words rather than inside any one
 * surface.
 *
 * A single component's copy is nobody's bug — the defect is *drift*, and drift
 * is invisible from inside the file that drifted. The Light-mode audit found
 * three placeholders in three unrelated components disagreeing with the other
 * sixteen, and no test could have caught any of them, because each was
 * self-consistent and none is named by a spec.
 *
 * These guards are derived from the source rather than written as a list of
 * expected strings: a change-detector asserting today's wording would fail on
 * the next copy edit while catching nothing. What is asserted is the shape the
 * next placeholder has to take too.
 */

const SRC = resolve(process.cwd(), "src");

function tsxFiles(dir: string): string[] {
  const out: string[] = [];
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name);
    if (entry.isDirectory()) out.push(...tsxFiles(path));
    else if (entry.name.endsWith(".tsx") && !entry.name.includes(".test.")) {
      out.push(path);
    }
  }
  return out;
}

/**
 * Placeholders that are a specimen of the value, not a phrase about it. Sentence
 * case would misstate these — a project really is named `my-project` in
 * lowercase, and a token really does begin `ghp_`. Kept as an explicit list so
 * that adding one is a decision rather than a side effect of a regex loosening.
 */
const LITERAL_EXAMPLES = new Set([
  "my-project",
  "git@github.com:org/repo.git",
  "ghp_…",
  // A Docker image reference, a tag, and a Dockerfile path are all values
  // Docker itself spells in lowercase (SET-FR-21): `latest` is the tag Docker
  // applies when none is given, and a capitalised specimen of any of the three
  // would show the author something that would not work if they typed it.
  "registry.example/agent",
  "latest",
  "docker/agent.Dockerfile",
  // A relay address is a URL, and a URL's scheme and host are lowercase
  // (GLS-FR-KVNP). A capitalised specimen would show the author something they
  // could not type.
  "https://relay.example.com",
  // A gateway address is a URL on the same terms (AII-FR-PHFX).
  "https://gateway.example.com",
]);

describe("a placeholder reads as a phrase in sentence case", () => {
  const found: { file: string; text: string }[] = [];
  for (const file of tsxFiles(SRC)) {
    const src = readFileSync(file, "utf8");
    for (const m of src.matchAll(/placeholder=\{?"([^"]*)"/g)) {
      found.push({ file: file.slice(SRC.length + 1), text: m[1] });
    }
  }

  // A guard that stops matching passes silently and defends nothing, so the
  // sweep asserts it still found the corpus it is meant to be checking.
  it("finds the placeholders to check", () => {
    expect(found.length).toBeGreaterThan(10);
  });

  it("starts every phrase with a capital", () => {
    const offenders = found
      .filter((p) => !LITERAL_EXAMPLES.has(p.text))
      .filter((p) => /^[a-z]/.test(p.text))
      .map((p) => `${p.file}: "${p.text}"`);
    expect(offenders).toEqual([]);
  });

  // Nine surfaces narrow a list with a text field. They are the same control
  // doing the same job, and an author who has learnt one should recognise the
  // next — which they do not if one of them asks them to "Type to filter".
  it("opens every list filter with the same verb", () => {
    const filters = found.filter((p) => /filter/i.test(p.text));
    expect(filters.length).toBeGreaterThan(4);
    const offenders = filters
      .filter((p) => !p.text.startsWith("Filter"))
      .map((p) => `${p.file}: "${p.text}"`);
    expect(offenders).toEqual([]);
  });
});
