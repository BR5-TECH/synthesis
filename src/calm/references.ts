/**
 * CLM-FR-DFFG, CLM-FR-DHXO: the references step of a read. It runs after the
 * schema step, so the shapes it reads are the shapes that the schema allows.
 */
import { CALM_ERROR, type CalmError, type JsonObject } from "./types";

/** The value at `key` of `value` when it is an object, else undefined. */
export const field = (value: unknown, key: string): unknown =>
  value !== null && typeof value === "object" && !Array.isArray(value)
    ? (value as JsonObject)[key]
    : undefined;

/** The array at `key`, or an empty array. */
export const listOf = (value: unknown, key: string): unknown[] => {
  const found = field(value, key);
  return Array.isArray(found) ? found : [];
};

/** The string at `key`, or undefined. */
export const textOf = (value: unknown, key: string): string | undefined => {
  const found = field(value, key);
  return typeof found === "string" ? found : undefined;
};

/** The strings of the array at `key`. */
export const textsOf = (value: unknown, key: string): string[] =>
  listOf(value, key).filter((item): item is string => typeof item === "string");

/** JSON pointer escaping of one reference token. */
const token = (part: string | number) => String(part).replace(/~/g, "~0").replace(/\//g, "~1");
export const pointer = (...parts: Array<string | number>) =>
  parts.length === 0 ? "" : `/${parts.map(token).join("/")}`;

/** The one relationship type key of a relationship, or undefined. */
export function relationshipTypeOf(relationship: unknown): string | undefined {
  const type = field(relationship, "relationship-type");
  if (type === null || typeof type !== "object" || Array.isArray(type)) return undefined;
  return Object.keys(type)[0];
}

interface Reference {
  id: string;
  at: string;
}

/** The ids of the interfaces of each node, by node id. */
function interfaceIds(document: JsonObject): Map<string, Set<string>> {
  const out = new Map<string, Set<string>>();
  for (const node of listOf(document, "nodes")) {
    const id = textOf(node, "unique-id");
    if (id === undefined) continue;
    const ids = new Set<string>();
    for (const item of listOf(node, "interfaces")) {
      const iface = textOf(item, "unique-id");
      if (iface !== undefined) ids.add(iface);
    }
    out.set(id, ids);
  }
  return out;
}

/** CLM-FR-DHXO: the ids that occur more than once in one list. */
function repeated(items: unknown[]): string[] {
  const seen = new Set<string>();
  const twice = new Set<string>();
  for (const item of items) {
    const id = textOf(item, "unique-id");
    if (id === undefined) continue;
    if (seen.has(id)) twice.add(id);
    seen.add(id);
  }
  return [...twice];
}

/** CLM-FR-DFFG, CLM-FR-DHXO: null when every reference holds, else the error. */
export function checkReferences(document: JsonObject): CalmError | null {
  const nodes = listOf(document, "nodes");
  const relationships = listOf(document, "relationships");
  const flows = listOf(document, "flows");

  const twice = [...repeated(nodes), ...repeated(relationships), ...repeated(flows)];
  if (twice.length > 0) {
    return {
      state: "error",
      step: "references",
      summary: CALM_ERROR.duplicates,
      detail: [...new Set(twice)].map((id) => `"${id}" is used more than once.`).join("\n"),
    };
  }

  const nodeIds = new Set(nodes.map((node) => textOf(node, "unique-id")).filter(Boolean));
  const relationshipIds = new Set(
    relationships.map((rel) => textOf(rel, "unique-id")).filter(Boolean),
  );
  const interfaces = interfaceIds(document);
  const missing: Reference[] = [];
  const needNode = (id: string | undefined, at: string) => {
    if (id !== undefined && !nodeIds.has(id)) missing.push({ id, at });
  };
  const needRelationship = (id: string | undefined, at: string) => {
    if (id !== undefined && !relationshipIds.has(id)) missing.push({ id, at });
  };

  relationships.forEach((rel, r) => {
    const kind = relationshipTypeOf(rel);
    if (kind === undefined) return;
    const body = field(field(rel, "relationship-type"), kind);
    const base = ["relationships", r, "relationship-type", kind] as const;
    if (kind === "interacts") {
      needNode(textOf(body, "actor"), pointer(...base, "actor"));
      textsOf(body, "nodes").forEach((id, n) => needNode(id, pointer(...base, "nodes", n)));
    } else if (kind === "connects") {
      for (const end of ["source", "destination"] as const) {
        const side = field(body, end);
        const node = textOf(side, "node");
        needNode(node, pointer(...base, end, "node"));
        const known = node === undefined ? undefined : interfaces.get(node);
        textsOf(side, "interfaces").forEach((iface, i) => {
          if (known !== undefined && !known.has(iface)) {
            missing.push({ id: iface, at: pointer(...base, end, "interfaces", i) });
          }
        });
      }
    } else if (kind === "deployed-in" || kind === "composed-of") {
      needNode(textOf(body, "container"), pointer(...base, "container"));
      textsOf(body, "nodes").forEach((id, n) => needNode(id, pointer(...base, "nodes", n)));
    } else if (kind === "options" && Array.isArray(body)) {
      body.forEach((decision, d) => {
        textsOf(decision, "nodes").forEach((id, n) =>
          needNode(id, pointer(...base, d, "nodes", n)),
        );
        textsOf(decision, "relationships").forEach((id, n) =>
          needRelationship(id, pointer(...base, d, "relationships", n)),
        );
      });
    }
  });

  flows.forEach((flow, f) => {
    listOf(flow, "transitions").forEach((transition, t) =>
      needRelationship(
        textOf(transition, "relationship-unique-id"),
        pointer("flows", f, "transitions", t, "relationship-unique-id"),
      ),
    );
  });

  if (missing.length === 0) return null;
  return {
    state: "error",
    step: "references",
    summary: CALM_ERROR.references,
    detail: missing.map(({ id, at }) => `"${id}" does not exist (named at ${at}).`).join("\n"),
  };
}
