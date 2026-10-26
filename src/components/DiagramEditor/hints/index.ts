/**
 * EDT-FR-NWZX, EDT-FR-ERTS: which hints the hints area of the diagram editor
 * overlay shows for a source.
 *
 * The hints are static text bundled with the application (EDT-FR-NVGA). The
 * chart type comes from the first line of the source that is not empty and is
 * not a comment, as the bundled Mermaid version reads it: a frontmatter block
 * at the start and each `%%` line are skipped.
 */
import { GENERAL_GROUPS } from "./general";
import { OTHER_SETS } from "./otherSets";
import { RICH_SETS } from "./richSets";
import type { HintGroup, HintSet } from "./types";

export type { Hint, HintGroup, HintSet } from "./types";
export { RICH_SETS } from "./richSets";

/** EDT-FR-QJJP: every hint set, in the order that detection tries them. */
export const HINT_SETS: readonly HintSet[] = [...RICH_SETS, ...OTHER_SETS];

/** EDT-FR-ERTS: the set that the hints area shows after the general hints. */
export const FALLBACK_SET_KEY = "flowchart";

/**
 * EDT-FR-NWZX: the first line of `source` that is not empty and is not a
 * comment, trimmed, or null when there is none. A frontmatter block at the
 * start is skipped; a block that does not close holds the rest of the source.
 */
export function declarationLine(source: string): string | null {
  const lines = source.split(/\r\n|\r|\n/);
  let at = 0;
  while (at < lines.length && lines[at].trim() === "") at++;
  if (at < lines.length && lines[at].trim() === "---") {
    let end = at + 1;
    while (end < lines.length && lines[end].trim() !== "---") end++;
    if (end >= lines.length) return null;
    at = end + 1;
  }
  for (; at < lines.length; at++) {
    const line = lines[at].trim();
    if (line === "" || line.startsWith("%%")) continue;
    return line;
  }
  return null;
}

/** EDT-FR-NWZX: the hint set of the chart type of `source`, or null. */
export function hintSetFor(source: string): HintSet | null {
  const line = declarationLine(source);
  if (line === null) return null;
  return HINT_SETS.find((set) => set.detect.test(line)) ?? null;
}

/** What the hints area shows for one source. */
export interface ShownHints {
  /** EDT-FR-YNQF: the name above the list. */
  title: string;
  /** The key of the detected set, or null for the general hints. */
  key: string | null;
  /** The groups of the list, in order. */
  groups: readonly HintGroup[];
}

function setGroups(set: HintSet, prefix = ""): HintGroup[] {
  return [
    ...set.groups.map((group) => ({ ...group, title: `${prefix}${group.title}` })),
    { title: `${prefix}Example`, hints: [{ code: set.example, text: "A short complete diagram." }] },
  ];
}

function fallbackSet(): HintSet {
  const set = HINT_SETS.find((candidate) => candidate.key === FALLBACK_SET_KEY);
  if (!set) throw new Error("the flowchart hint set is missing");
  return set;
}

/**
 * EDT-FR-NWZX, EDT-FR-ERTS: the hints for `source`. An empty source and a
 * source of an unknown chart type get the general hints and then the
 * flowchart set.
 */
export function hintsFor(source: string): ShownHints {
  const set = hintSetFor(source);
  if (set) return { title: set.name, key: set.key, groups: setGroups(set) };
  const fallback = fallbackSet();
  return {
    title: "General hints",
    key: null,
    groups: [
      ...GENERAL_GROUPS,
      {
        title: "Chart types",
        hints: HINT_SETS.map((each) => each.declaration),
      },
      ...setGroups(fallback, `${fallback.name}: `),
    ],
  };
}
