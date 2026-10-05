/**
 * SMN-FR-OJAY: the counts of an index node, computed from the spec nodes
 * beneath it in the current tree. The index supplies no aggregate.
 */
import { specChildren, indexChildren, type TreeIndex } from "./tree";
import {
  COMPLETENESS_STATES,
  type CompletenessState,
  type SpecNode,
} from "./types";

export interface Rollup {
  specs: number;
  requirements: number;
  scenarios: number;
  verified: number;
  built: number;
  drafted: number;
  gap: number;
}

export const EMPTY_ROLLUP: Rollup = Object.freeze({
  specs: 0,
  requirements: 0,
  scenarios: 0,
  verified: 0,
  built: 0,
  drafted: 0,
  gap: 0,
});

export function rollup(specs: readonly SpecNode[]): Rollup {
  const out = { ...EMPTY_ROLLUP };
  for (const spec of specs) {
    out.specs += 1;
    out.requirements += spec.requirements;
    out.scenarios += spec.scenarios;
    out[spec.state] += 1;
  }
  return out;
}

function add(a: Rollup, b: Rollup): Rollup {
  return {
    specs: a.specs + b.specs,
    requirements: a.requirements + b.requirements,
    scenarios: a.scenarios + b.scenarios,
    verified: a.verified + b.verified,
    built: a.built + b.built,
    drafted: a.drafted + b.drafted,
    gap: a.gap + b.gap,
  };
}

/** The rollup of every index node, keyed by id. */
export function rollupAll(tree: TreeIndex): Map<string, Rollup> {
  const out = new Map<string, Rollup>();
  // Pre-order reversed visits every child before its parent.
  for (let i = tree.order.length - 1; i >= 0; i--) {
    const node = tree.nodes.get(tree.order[i])!;
    let total = rollup(specChildren(node));
    for (const child of indexChildren(node)) {
      total = add(total, out.get(child.id) ?? EMPTY_ROLLUP);
    }
    out.set(node.id, total);
  }
  return out;
}

/** The rollup of every spec node in the tree. */
export function rollupTotal(tree: TreeIndex): Rollup {
  return rollup([...tree.specs.values()]);
}

/** A rollup of one spec node, for the bars that show a single spec. */
export function rollupOfSpec(spec: SpecNode): Rollup {
  return rollup([spec]);
}

/**
 * SMN-FR-OJAY: each state's share of the spec nodes, in percent. An empty
 * rollup has no share at all rather than a divide by zero.
 */
export function segments(r: Rollup): Record<CompletenessState, number> {
  const share = (n: number) => (r.specs === 0 ? 0 : (n / r.specs) * 100);
  return {
    verified: share(r.verified),
    built: share(r.built),
    drafted: share(r.drafted),
    gap: share(r.gap),
  };
}

export const STATE_ORDER = COMPLETENESS_STATES;
