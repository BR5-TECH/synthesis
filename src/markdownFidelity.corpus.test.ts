import { describe, expect, it } from "vitest";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { Editor } from "@tiptap/react";
import { markdownExtensions } from "./components/markdownFidelity";

function roundTrip(src: string): string {
  const editor = new Editor({ extensions: markdownExtensions(), content: src });
  const storage = editor.storage as unknown as {
    markdown?: { getMarkdown?: () => string };
  };
  const out = storage.markdown?.getMarkdown?.() ?? "";
  editor.destroy();
  return out;
}

function walk(dir: string, acc: string[] = []): string[] {
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p, acc);
    else if (name.endsWith(".md")) acc.push(p);
  }
  return acc;
}

// No try/catch: a directory that has moved must fail loudly rather than
// silently shrink the corpus this suite is the measure of.
const FILES = ["specifications", "resources", ".claude"].flatMap((d) => walk(d));

describe("the project's own Markdown corpus survives the round trip", () => {
  it("found the corpus", () => {
    // A floor close to the real count, so a corpus that silently halves is a
    // failure rather than a smaller green run.
    expect(FILES.length).toBeGreaterThan(120);
  });

  // The round trip is not linear in the size of its input: above near 200 kB the
  // cost increases near n^1.5. A file that becomes too large thus makes the case
  // below stop on its time limit, which names a line number and no file. This
  // budget stops first, in milliseconds, and names the file. The value is near
  // 1.5 times the largest file today, and near one third of CASE_TIMEOUT_MS on
  // CI hardware.
  const MAX_FILE_BYTES = 600_000;

  it("keeps every corpus file inside the round-trip budget", () => {
    const tooLarge = FILES.map((path) => ({
      path,
      bytes: statSync(path).size,
    })).filter((f) => f.bytes > MAX_FILE_BYTES);

    expect(
      tooLarge,
      `these files are above the ${MAX_FILE_BYTES} byte round-trip budget: ` +
        `${tooLarge.map((f) => `${f.path} (${f.bytes})`).join(", ")}. ` +
        "Divide the file, or measure the round trip again and lift the budget.",
    ).toEqual([]);
  });

  // The cost of a case increases with the size of its file. Each case does the
  // round trip two times, for the fixed-point check. The largest specification
  // file is near 400 kB and takes near 1.4 s on a development machine. CI
  // hardware is slower, thus the 5 s default limit is not sufficient. This limit
  // gives space for the corpus to grow.
  const CASE_TIMEOUT_MS = 30_000;

  it.each(FILES)(
    "%s keeps its content",
    (path) => {
      const src = readFileSync(path, "utf8");
      const out = roundTrip(src);

      // Nothing is entity-escaped that the author wrote as a character. Asserted
      // unconditionally: a guard skipping the files that already contain `&lt;`
      // would skip exactly the files where a regression would show.
      expect(out).not.toContain("&lt;");
      expect(out).not.toContain("&gt;");

      // Every tag-shaped placeholder survives.
      for (const tag of new Set(src.match(/<[a-z][a-z0-9_ -]*>/g) ?? [])) {
        expect(out, `lost ${tag}`).toContain(tag);
      }

      // Every HTML comment survives.
      expect((out.match(/<!--/g) ?? []).length, "lost an HTML comment").toBe(
        (src.match(/<!--/g) ?? []).length,
      );

      // Every table keeps its rows.
      expect(
        (out.match(/^\s*\|/gm) ?? []).length,
        "lost table rows",
      ).toBeGreaterThanOrEqual((src.match(/^\s*\|/gm) ?? []).length);

      // And the result is a fixed point (EDT-FR-69).
      expect(roundTrip(out)).toBe(out);
    },
    CASE_TIMEOUT_MS,
  );
});
