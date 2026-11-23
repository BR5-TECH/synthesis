/**
 * JSON Pointers (RFC 6901) over the local data model of a surface
 * (`ASF-agent-surfaces.md` ASF-FR-ENRG, ASF-FR-TURS).
 */

/** The tokens of an absolute pointer. `""` is the whole document. */
function tokens(pointer: string): string[] {
  if (pointer === "") return [];
  return pointer
    .slice(1)
    .split("/")
    .map((t) => t.replace(/~1/g, "/").replace(/~0/g, "~"));
}

function escape(token: string): string {
  return token.replace(/~/g, "~0").replace(/\//g, "~1");
}

/**
 * ASF-FR-TURS: the absolute form of `pointer`. A relative pointer, with no
 * leading `/`, reads from the template item at `base`.
 */
export function absolute(pointer: string, base: string): string {
  if (pointer === "" || pointer.startsWith("/")) return pointer;
  return `${base}/${pointer}`;
}

/** The pointer of one item of a list at `list`. */
export function itemPointer(list: string, index: number): string {
  return `${list}/${escape(String(index))}`;
}

/** ASF-FR-ENRG: the value at an absolute pointer, or `undefined`. */
export function getAt(model: unknown, pointer: string): unknown {
  let current: unknown = model;
  for (const token of tokens(pointer)) {
    if (Array.isArray(current)) {
      const index = Number(token);
      if (!Number.isInteger(index)) return undefined;
      current = current[index];
    } else if (current !== null && typeof current === "object") {
      current = (current as Record<string, unknown>)[token];
    } else {
      return undefined;
    }
  }
  return current;
}

/**
 * ASF-FR-ENRG: a copy of `model` with `value` at an absolute pointer. The
 * objects on the way are created where they are missing; `model` itself is
 * not changed.
 */
export function setAt(
  model: Record<string, unknown>,
  pointer: string,
  value: unknown,
): Record<string, unknown> {
  const path = tokens(pointer);
  if (path.length === 0) {
    return value !== null && typeof value === "object" && !Array.isArray(value)
      ? (value as Record<string, unknown>)
      : model;
  }
  const write = (node: unknown, depth: number): unknown => {
    const token = path[depth];
    const last = depth === path.length - 1;
    if (Array.isArray(node)) {
      const copy = [...node];
      const index = Number(token);
      if (!Number.isInteger(index) || index < 0) return node;
      copy[index] = last ? value : write(copy[index], depth + 1);
      return copy;
    }
    const base =
      node !== null && typeof node === "object" ? { ...(node as Record<string, unknown>) } : {};
    base[token] = last ? value : write(base[token], depth + 1);
    return base;
  };
  return write(model, 0) as Record<string, unknown>;
}
