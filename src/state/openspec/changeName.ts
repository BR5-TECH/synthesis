/**
 * The OpenSpec change name rule (`../../../specifications/core/OSM-openspec-mode.md`
 * OSM-FR-IHUO) and the name the start dialog derives from a draft
 * (`../../../specifications/ui/GSD-graduation-start-dialog.md` GSD-FR-CKSV).
 *
 * Pure functions, so the dialog and the Library use one rule.
 */

/** OSM-FR-IHUO: the longest valid change name. */
export const CHANGE_NAME_MAX = 64;

/** GSD-FR-CKSV: the longest name part before the id part is added. */
const NAME_PART_MAX = 59;

/** GSD-FR-CKSV: how many letters or digits of the draft id the name takes. */
const ID_PART_LENGTH = 4;

const CHANGE_NAME_PATTERN = /^[a-z0-9]+(-[a-z0-9]+)*$/;

/** GSD-FR-DCBH: the rule, in words, that the dialog states under the field. */
export const CHANGE_NAME_RULE =
  "Use 1 to 64 lowercase letters and digits, in groups joined by single hyphens. The name cannot be “archive”.";

/** OSM-FR-IHUO: whether `name` is a valid change name. */
export function isValidChangeName(name: string): boolean {
  return (
    name.length >= 1 &&
    name.length <= CHANGE_NAME_MAX &&
    name !== "archive" &&
    CHANGE_NAME_PATTERN.test(name)
  );
}

/** Trim `-` at both ends. */
function trimHyphens(text: string): string {
  return text.replace(/^-+/, "").replace(/-+$/, "");
}

/**
 * GSD-FR-CKSV: the change name the dialog prefills for a draft. The draft name
 * is lowercased, each run of other characters than `a-z` and `0-9` becomes one
 * `-`, `-` is trimmed at both ends, and the result is cut to 59 characters.
 * Then `-` and the first four letters or digits of the draft id follow. An
 * empty name part gives the id part alone.
 */
export function deriveChangeName(draftName: string, draftId: string): string {
  const namePart = trimHyphens(
    trimHyphens(draftName.toLowerCase().replace(/[^a-z0-9]+/g, "-")).slice(0, NAME_PART_MAX),
  );
  const idPart = draftId
    .toLowerCase()
    .replace(/[^a-z0-9]/g, "")
    .slice(0, ID_PART_LENGTH);
  if (!namePart) return idPart;
  if (!idPart) return namePart;
  return `${namePart}-${idPart}`;
}

/** Where a project file of a change is, in the terms of LIB-FR-OKIZ. */
export interface ChangeFileTarget {
  /** The change name. */
  name: string;
  /** The change-relative path of the file. */
  file: string;
}

const CHANGES_ROOT = "openspec/changes/";

/**
 * LIB-FR-OKIZ: the change a project file belongs to, or null. The file must be
 * below `openspec/changes/<name>/` and not below `openspec/changes/archive/`,
 * and `<name>` must be a valid change name.
 */
export function changeFileTarget(projectPath: string): ChangeFileTarget | null {
  const path = projectPath.replace(/\\/g, "/");
  if (!path.startsWith(CHANGES_ROOT)) return null;
  const rest = path.slice(CHANGES_ROOT.length);
  const slash = rest.indexOf("/");
  if (slash <= 0) return null;
  const name = rest.slice(0, slash);
  const file = rest.slice(slash + 1);
  if (!file || !isValidChangeName(name)) return null;
  return { name, file };
}
