/**
 * Paths chosen in a native picker, read against the project folder
 * (SET-FR-22, SET-FR-FPCX).
 */

/** One path with `/` separators and no trailing separator. */
function slashed(path: string): string {
  const flat = path.replace(/\\/g, "/");
  return flat.length > 1 ? flat.replace(/\/+$/, "") : flat;
}

/** A Windows drive path compares without regard to case. */
function foldedFor(root: string, path: string): [string, string] {
  return /^[A-Za-z]:\//.test(root)
    ? [root.toLowerCase(), path.toLowerCase()]
    : [root, path];
}

/**
 * The path of `picked` relative to the project folder `root`, with `/` as the
 * separator, or `null` when `picked` lies outside it. The folder itself is `.`.
 */
export function projectRelativePath(root: string, picked: string): string | null {
  const base = slashed(root);
  const target = slashed(picked);
  if (base === "") return null;
  const [foldedBase, foldedTarget] = foldedFor(base, target);
  if (foldedTarget === foldedBase) return ".";
  const prefix = foldedBase.endsWith("/") ? foldedBase : `${foldedBase}/`;
  if (!foldedTarget.startsWith(prefix)) return null;
  return target.slice(prefix.length);
}

/** Is this path absolute on a POSIX or a Windows machine? */
export function isAbsolutePath(path: string): boolean {
  return path.startsWith("/") || path.startsWith("\\") || /^[A-Za-z]:[\\/]/.test(path);
}

/**
 * Where a picker opens for a configured value: the value itself when it is
 * absolute, the value resolved against the project folder when it is relative,
 * and the project folder when there is no value.
 */
export function pickerStart(root: string, value: string): string | undefined {
  const trimmed = value.trim();
  if (trimmed === "") return root || undefined;
  if (isAbsolutePath(trimmed)) return trimmed;
  if (!root) return undefined;
  return `${slashed(root)}/${trimmed.replace(/^\.\//, "")}`;
}
