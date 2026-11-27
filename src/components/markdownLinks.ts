/**
 * What the destination of a Markdown link names, and the anchor of a heading
 * (`specifications/ui/EDT-editor.md` EDT-FR-UXYY to EDT-FR-QBIE).
 *
 * Pure functions only. The surface that follows a link and the host that opens
 * a file use them, so that each surface reads a destination in the same way.
 */
import { isAbsolutePath, projectRelativePath } from "../projectPaths";

/** Why a link opens nothing (EDT-FR-AWFH, EDT-FR-QBIE). */
export type LinkRefusal = "outside" | "scheme" | "no-file";

export type LinkTarget =
  /** EDT-FR-UXYY: an address for the system browser or mail client. */
  | { kind: "external"; url: string }
  /** EDT-FR-HMJI: a heading of the same document. */
  | { kind: "fragment"; fragment: string }
  /** EDT-FR-RFCU: a project-relative file, with an optional heading. */
  | { kind: "file"; path: string; fragment: string | null }
  /** An empty destination, or `#` alone: the link goes nowhere. */
  | { kind: "none" }
  | { kind: "refused"; reason: LinkRefusal };

/** EDT-FR-QBIE: what a host resolves a file link against. */
export interface LinkBase {
  /**
   * The project-relative folder that a relative path starts from (`""` is the
   * project root), or `null` when the host resolves no file.
   */
  fileDir: string | null;
  /** The absolute paths of the project root (EDT-FR-AWFH). */
  roots: readonly string[];
}

const EXTERNAL_RE = /^(https?:\/\/|mailto:)./i;
const SCHEME_RE = /^[a-zA-Z][a-zA-Z0-9+.-]*:/;
const DRIVE_RE = /^[a-zA-Z]:[\\/]/;

/** EDT-FR-UXYY: does the destination open in the system browser or mail client? */
export function isExternalLink(href: string): boolean {
  return EXTERNAL_RE.test(href.trim());
}

/** A scheme name that is safe to log, never the address (EDT-FR-AWFH). */
export function schemeOfLink(href: string): string {
  return /^([a-z][a-z0-9+.-]{0,15}):/i.exec(href.trim())?.[1]?.toLowerCase() ?? "none";
}

function decoded(text: string): string {
  try {
    return decodeURIComponent(text);
  } catch {
    // A malformed escape is kept as the author wrote it.
    return text;
  }
}

/**
 * EDT-FR-RFCU: `relative` resolved onto the project-relative folder `baseDir`,
 * with `.` and `..` segments resolved. `null` when the path goes above the
 * project root.
 */
export function joinProjectPath(baseDir: string, relative: string): string | null {
  const parts = baseDir === "" ? [] : baseDir.split("/");
  for (const part of relative.split("/")) {
    if (part === "" || part === ".") continue;
    if (part === "..") {
      if (parts.length === 0) return null;
      parts.pop();
    } else {
      parts.push(part);
    }
  }
  return parts.join("/");
}

/** The project-relative folder of a project-relative file path. */
export function folderOf(path: string): string {
  const at = path.lastIndexOf("/");
  return at < 0 ? "" : path.slice(0, at);
}

/**
 * EDT-FR-AWFH: the project-relative path of an absolute filesystem path, or
 * `null` when it is in no root. `"outside"` when it starts in a root and then
 * goes above it.
 */
function inRoots(path: string, roots: readonly string[]): string | null | "outside" {
  for (const root of roots) {
    const relative = projectRelativePath(root, path);
    if (relative === null) continue;
    if (relative === ".") return "";
    return joinProjectPath("", relative) ?? "outside";
  }
  return null;
}

/**
 * EDT-FR-AWFH: does an absolute path in no root start with the first folder of
 * a root, as `/Users/someone/other/x.md` does for the root `/Users/me/project`?
 * Such a path is a filesystem path outside the project, not a path from the
 * project root.
 */
function startsLikeARoot(path: string, roots: readonly string[]): boolean {
  const first = path.split("/")[1];
  if (!first) return false;
  return roots.some((root) => root.replace(/\\/g, "/").split("/")[1] === first);
}

/**
 * EDT-FR-UXYY, EDT-FR-HMJI, EDT-FR-RFCU, EDT-FR-AWFH, EDT-FR-QBIE: what the
 * destination `href` of a link names, read against `base`.
 */
export function classifyLink(href: string, base: LinkBase): LinkTarget {
  const raw = href.trim();
  if (raw === "") return { kind: "none" };
  if (isExternalLink(raw)) return { kind: "external", url: raw };

  const hashAt = raw.indexOf("#");
  const beforeHash = hashAt < 0 ? raw : raw.slice(0, hashAt);
  const fragmentText = hashAt < 0 ? "" : raw.slice(hashAt + 1);
  const fragment = fragmentText === "" ? null : decoded(fragmentText);
  // EDT-FR-RFCU: a query is ignored.
  const path = decoded(beforeHash.split("?")[0]);

  if (path === "") return fragment === null ? { kind: "none" } : { kind: "fragment", fragment };
  // A drive letter looks like a scheme, so it is checked first.
  const drive = DRIVE_RE.test(path);
  if (!drive && SCHEME_RE.test(path)) return { kind: "refused", reason: "scheme" };
  if (base.fileDir === null) return { kind: "refused", reason: "no-file" };

  if (isAbsolutePath(path)) {
    const slashed = path.replace(/\\/g, "/");
    const rooted = inRoots(slashed, base.roots);
    if (rooted === "outside") return { kind: "refused", reason: "outside" };
    if (rooted !== null) return { kind: "file", path: rooted, fragment };
    // A drive path in no root is outside the project. A path that starts with
    // `/` and is in no root is relative to the project root.
    if (drive || slashed.startsWith("//") || startsLikeARoot(slashed, base.roots)) {
      return { kind: "refused", reason: "outside" };
    }
    const fromRoot = joinProjectPath("", slashed.slice(1));
    return fromRoot === null
      ? { kind: "refused", reason: "outside" }
      : { kind: "file", path: fromRoot, fragment };
  }

  // A `\` is a separator on Windows, so `..\x.md` cannot step past the
  // project root as one segment.
  const resolved = joinProjectPath(base.fileDir, path.replace(/\\/g, "/"));
  return resolved === null
    ? { kind: "refused", reason: "outside" }
    : { kind: "file", path: resolved, fragment };
}

/**
 * EDT-FR-BJFP: the slug of a heading's plain text — lower case, only letters,
 * digits, spaces, `-`, and `_` kept, and each space changed to `-`. The marks
 * that belong to a letter, such as the vowel signs of Devanagari, stay with it.
 */
export function headingSlug(text: string): string {
  return text
    .normalize("NFC")
    .toLowerCase()
    .replace(/[^\p{L}\p{M}\p{N} _-]/gu, "")
    .replace(/ /g, "-");
}

/**
 * EDT-FR-BJFP: the anchor of each heading, in document order. A slug that an
 * earlier heading has already used gets `-1`, then `-2`, and so on.
 */
export function headingAnchors(texts: readonly string[]): string[] {
  const occurrences = new Map<string, number>();
  return texts.map((text) => {
    const slug = headingSlug(text);
    let anchor = slug;
    while (occurrences.has(anchor)) {
      const count = (occurrences.get(slug) ?? 0) + 1;
      occurrences.set(slug, count);
      anchor = `${slug}-${count}`;
    }
    occurrences.set(anchor, 0);
    return anchor;
  });
}

/**
 * EDT-FR-BJFP: the index of the heading whose anchor `fragment` names, with no
 * regard to case, or `-1` when no heading matches.
 */
export function anchorIndex(texts: readonly string[], fragment: string): number {
  const wanted = fragment.normalize("NFC").toLowerCase();
  return headingAnchors(texts).indexOf(wanted);
}
