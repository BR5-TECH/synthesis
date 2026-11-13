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
  MapIssue,
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

export const OPENSPEC_LEVELS: LevelName[] = [
  { plural: "groups", singular: "group" },
  { plural: "capabilities", singular: "capability" },
  { plural: "requirements", singular: "requirement" },
];

/** OSI-FR-SEYB: requirement `n` of a capability, `gap` where it holds an issue. */
export function requirement(
  capability: string,
  n: number,
  scenarios = 1,
  issues?: MapIssue[],
): SpecNode {
  return {
    code: `${capability}#${n}`,
    badge: `R${n}`,
    path: `openspec/specs/${capability}/spec.md`,
    label: `${capability} requirement ${n}`,
    summary: `The text of ${capability} requirement ${n}.`,
    requirements: 1,
    scenarios,
    state: issues && issues.some((i) => i.level !== "INFO") ? "gap" : "built",
    ...(issues ? { issues } : {}),
  };
}

const STREAM = { kind: "stream", streamId: "s-1", streamName: "editor-work" } as const;

/**
 * An OpenSpec index in the session's shape (OSI contract), built by hand:
 * - `group:/` specs → `capability:auth` (R1 built, R2 gap with a WARNING;
 *   chips add-logout in the active worktree, with an ERROR, and on editor-work),
 *   and the planned `capability:ui` (chip add-logout on editor-work)
 * - `group:billing` billing → `capability:billing/pay` (an INFO issue on the
 *   capability, R1 gap with an ERROR; chip add-refunds on editor-work)
 */
export function openspecIndex(over: Partial<SpecificationIndex> = {}): SpecificationIndex {
  return {
    levels: OPENSPEC_LEVELS,
    roots: [
      node("group:/", [
        node(
          "capability:auth",
          [
            requirement("auth", 1, 2),
            requirement("auth", 2, 0, [{ level: "WARNING", message: "needs a scenario" }]),
          ],
          {
            label: "Authentication",
            changes: [
              {
                change: "add-logout",
                location: { kind: "active_worktree" },
                issues: [{ level: "ERROR", message: "delta has no scenario" }],
              },
              { change: "add-logout", location: STREAM },
            ],
          },
        ),
        node("capability:ui", [], {
          label: "ui",
          summary: "",
          plannedCapability: true,
          changes: [{ change: "add-logout", location: STREAM }],
        }),
      ], { label: "specs", summary: "" }),
      node(
        "group:billing",
        [
          node(
            "capability:billing/pay",
            [
              requirement("billing/pay", 1, 0, [{ level: "ERROR", message: "no scenario" }]),
              requirement("billing/pay", 2, 3),
            ],
            {
              label: "Payments",
              issues: [{ level: "INFO", message: "purpose is short" }],
              changes: [{ change: "add-refunds", location: STREAM }],
            },
          ),
        ],
        { label: "billing", summary: "" },
      ),
    ],
    dependencies: [],
    source: "openspec",
    validation: "invalid",
    unreadLocations: 0,
    ...over,
  };
}
