/**
 * Small specification indexes for the Map tab's tests
 * (`SMP-specification-map.md`). Built by hand so every count a test asserts can
 * be read off the fixture.
 */
import type {
  CompletenessState,
  Dependency,
  IndexNode,
  LevelName,
  SpecNode,
  SpecificationIndex,
} from "../state/specMap/types";

export const LEVELS_4: LevelName[] = [
  { plural: "domains", singular: "domain" },
  { plural: "features", singular: "feature" },
  { plural: "groups", singular: "group" },
  { plural: "specs", singular: "spec" },
];

export function spec(
  code: string,
  state: CompletenessState,
  requirements = 10,
  folder = "ui",
): SpecNode {
  return {
    code,
    path: `${folder}/${code}-${code.toLowerCase()}.md`,
    label: `${code.toLowerCase()} label`,
    summary: `${code} summary.`,
    requirements,
    scenarios: Math.round(requirements / 2),
    state,
  };
}

export function node(
  id: string,
  children: IndexNode[] | SpecNode[],
  extra: Partial<IndexNode> = {},
): IndexNode {
  return { id, label: `${id} label`, summary: `${id} summary.`, children, ...extra };
}

/**
 * Two domains:
 * - d1 → f1 (g1: A1 verified, A2 gap; g2: A3 built), f2 (g3: B1 drafted)
 * - d2 → f3 (g4: C1 built, C2 verified)
 */
export function smallIndex(dependencies?: Dependency[]): SpecificationIndex {
  return {
    levels: LEVELS_4,
    roots: [
      node(
        "d1",
        [
          node("f1", [
            node("g1", [spec("A1", "verified", 10), spec("A2", "gap", 4, "core")]),
            node("g2", [spec("A3", "built", 6)]),
          ]),
          node("f2", [node("g3", [spec("B1", "drafted", 8, "ai")])]),
        ],
        { hue: "#6BD7AA" },
      ),
      node("d2", [node("f3", [node("g4", [spec("C1", "built", 12), spec("C2", "verified", 2)])])], {
        hue: "#C19EFF",
      }),
    ],
    dependencies: dependencies ?? [
      { from: "A1", to: "C1", citations: 5 },
      { from: "A3", to: "B1", citations: 2 },
      { from: "C2", to: "A1", citations: 1, unresolved: true },
    ],
  };
}
