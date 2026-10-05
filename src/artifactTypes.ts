/**
 * The fixed built-in artifact-type set (ASC-FR-02) and the artifact-type filter
 * lens shared by every surface that offers one.
 *
 * The Project and the Changes panel must offer the *same* option list and the
 * same matching regimes (CHG-FR-14), and both default to the "All artifacts"
 * lens (LIB-FR-12 / CHG-FR-15). Keeping the table and the matcher here is what
 * makes that structural rather than a duplication two files apart.
 */
import { Icon } from "./components/icons";
import type { SelectorPosition } from "./components/SelectorRow";
import type { ArtifactType, ArtifactTypeFilter } from "./types";

export const ARTIFACT_TYPES: {
  value: ArtifactType;
  label: string;
  chip: string;
  icon: typeof Icon.Folder;
}[] = [
  { value: "skill", label: "Skill", chip: "SKL", icon: Icon.Boxes },
  { value: "agent", label: "Agent", chip: "AGT", icon: Icon.Terminal },
  { value: "prompt", label: "Prompt", chip: "PRO", icon: Icon.Notes },
  { value: "spec", label: "Spec", chip: "SPC", icon: Icon.Book },
  { value: "flow", label: "Flow", chip: "FLW", icon: Icon.Branch },
  { value: "instructions", label: "Instructions", chip: "INS", icon: Icon.Doc },
  { value: "scenario", label: "Scenario", chip: "SCN", icon: Icon.Play },
  { value: "scratchpad", label: "Scratchpad", chip: "SCR", icon: Icon.Code },
];

/** The short tag rendered on a row for a resolved artifact type. */
export function typeChip(type: ArtifactType): string {
  return ARTIFACT_TYPES.find((t) => t.value === type)?.chip ?? type.toUpperCase();
}

/**
 * The artifact-type filter is a single-select lens with three regimes
 * (LIB-FR-04 / LIB-FR-05): `"artifacts"` (the default) shows only classified
 * files, a specific `ArtifactType` narrows to that type, and `"files"` shows
 * every file including unclassified ones. The two sentinels can't collide with
 * an `ArtifactType` (none of the eight built-ins is named "artifacts"/"files").
 */
export type TypeLens = "artifacts" | ArtifactType | "files";

/** The lens every surface starts on (LIB-FR-12 / CHG-FR-15). */
export const DEFAULT_TYPE_LENS: TypeLens = "artifacts";

/**
 * How the two sentinel lenses read. Here rather than spelled out at each
 * position for the same reason the table above is: the Project and the Changes
 * panel must offer the *same* position list (CHG-FR-14), and a label typed twice
 * is a label that drifts — which for these two would be a visible inconsistency
 * in the capitalisation every control in the app follows.
 */
export const LENS_ALL_ARTIFACTS_LABEL = "All Artifacts";
export const LENS_ALL_FILES_LABEL = "All Files";

/**
 * LIB-FR-19 (and CHG-FR-14, which adopts it wholesale): the lens is the toggle
 * row of SNV-FR-62, and it offers the types the content in front of the author
 * actually has rather than the eight it could have — a project of Skills and
 * Specs shows two type buttons, not eight.
 *
 * `present` is the set of types the loaded tree or change set carries. The two
 * sentinels name no type and always render, and so does whichever position is
 * active: a lens the author selected never vanishes from under them when the
 * last file carrying that type leaves, and what they get instead is the
 * narrowed-to-nothing state (SNV-FR-61) rather than the panel silently changing
 * lens on their behalf.
 */
export function typeLensPositions(
  present: ReadonlySet<ArtifactType>,
  active: TypeLens,
): SelectorPosition<TypeLens>[] {
  return [
    { value: "artifacts", tag: "All", title: LENS_ALL_ARTIFACTS_LABEL },
    ...ARTIFACT_TYPES.filter(
      (t) => present.has(t.value) || active === t.value,
    ).map((t) => ({
      value: t.value as TypeLens,
      tag: t.chip,
      title: t.label,
      dataType: t.value,
    })),
    { value: "files", tag: "Files", title: LENS_ALL_FILES_LABEL },
  ];
}

/** The set of resolved types a collection of nodes or entries carries. */
export function presentTypes(
  types: Iterable<ArtifactType | undefined | null>,
): Set<ArtifactType> {
  const set = new Set<ArtifactType>();
  for (const t of types) if (t != null) set.add(t);
  return set;
}

/** Whether a file with this resolved type is visible under `lens`. */
export function matchesLens(
  lens: TypeLens,
  artifactType: ArtifactType | undefined,
): boolean {
  if (lens === "files") return true;
  if (lens === "artifacts") return artifactType != null;
  return artifactType === lens;
}

/** The text filter's matching regime: a case-insensitive basename substring. */
export function matchesText(text: string, name: string): boolean {
  const needle = text.trim().toLowerCase();
  return needle === "" || name.toLowerCase().includes(needle);
}

/**
 * The lens's persisted spelling (PSS-FR-18). The two sentinels differ from the
 * in-memory `TypeLens` — `all_artifacts` / `all_files` on the wire, the shorter
 * `artifacts` / `files` in the component — so the conversion lives here beside
 * the lens itself rather than being open-coded at the call site.
 */
export function lensToFilter(lens: TypeLens): ArtifactTypeFilter {
  if (lens === "artifacts") return "all_artifacts";
  if (lens === "files") return "all_files";
  return lens;
}

/**
 * The inverse. A value this build does not recognise — a record written by a
 * newer one — falls back to the default lens rather than being rendered as a
 * type that matches nothing, which would look like an empty project.
 */
export function filterToLens(filter: ArtifactTypeFilter | undefined): TypeLens {
  if (filter === "all_artifacts" || filter == null) return DEFAULT_TYPE_LENS;
  if (filter === "all_files") return "files";
  return ARTIFACT_TYPES.some((t) => t.value === filter)
    ? filter
    : DEFAULT_TYPE_LENS;
}
