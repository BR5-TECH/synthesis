import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

import { beforeEach, describe, expect, it, vi } from "vitest";

vi.mock("../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
}));

import { readCalm, resetCalmForTest } from "./read";

const SKILL = join(process.cwd(), "resources", "skills", "calm");

/** Each ```json block of each file of the bundled `calm` skill. */
function examples(): Array<{ file: string; index: number; source: string }> {
  const files = ["SKILL.md", ...readdirSync(join(SKILL, "references")).map((f) => `references/${f}`)];
  return files.flatMap((file) => {
    const text = readFileSync(join(SKILL, file), "utf8");
    return Array.from(text.matchAll(/```json\n([\s\S]*?)\n```/g), (match, index) => ({
      file,
      index,
      source: match[1],
    }));
  });
}

beforeEach(() => resetCalmForTest());

describe("the bundled calm skill", () => {
  const found = examples();

  it("DSL-FR-NXRB: the skill holds JSON examples", () => {
    expect(found.length).toBeGreaterThan(0);
  });

  it.each(found.map((example) => [`${example.file} #${example.index + 1}`, example] as const))(
    "DSL-FR-NXRB, CLM-FR-KDTA: the JSON example %s is a document whose read gives a diagram graph",
    async (_name, example) => {
      const result = await readCalm(example.source);
      const why = result.state === "error" ? `${result.summary} ${result.detail}` : "";
      expect(result.state, why).toBe("rendered");
    },
    15_000,
  );
});
