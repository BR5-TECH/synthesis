/**
 * How streamed search hits become the rendered group structure
 * (`SCH-search.md` SCH-FR-03 / SCH-FR-04 / SCH-FR-16).
 *
 * Kept out of the components because the overlay and the full results page
 * render the *same* grouping (SCH-FR-03) — differing only in the filters the
 * page adds (SCH-FR-10) — and a grouping that drifted between the two would be
 * two different answers to one query.
 */
import { ARTIFACT_TYPES } from "./artifactTypes";
import type { ArtifactType, SearchGroup, SearchHit } from "./types";

/**
 * SCH-FR-03: the v1 result groups, in render order. Artifacts is split by
 * subtype into exactly the eight built-in artifact types of ASC-FR-02, and the
 * Files group holds matches in files carrying no artifact type.
 *
 * `run` and `history` are here because the contract carries them (SCC-FR-09);
 * neither is ever populated in v1, and SCH-FR-04 hides an empty group — so they
 * simply never render, without either surface needing to know that.
 */
const GROUP_ORDER: ReadonlyArray<{ group: SearchGroup; title: string }> = [
  { group: "artifact", title: "Artifacts" },
  { group: "playbook", title: "Playbooks" },
  { group: "workstream", title: "Workstreams" },
  { group: "role", title: "Roles" },
  { group: "run", title: "Runs" },
  { group: "history", title: "History matches" },
  { group: "file", title: "Files" },
];

/** One rendered subsection of the Artifacts group — a single artifact subtype. */
export interface RenderedSubgroup {
  /** Stable key, also the collapse key. */
  key: string;
  title: string;
  hits: SearchHit[];
}

/** One rendered group: either flat hits, or (Artifacts) a list of subgroups. */
export interface RenderedGroup {
  key: SearchGroup;
  title: string;
  /** Every hit in the group, subgroups included — the group's whole result set. */
  hits: SearchHit[];
  /** SCH-FR-03: present only for Artifacts, which splits by subtype. */
  subgroups?: RenderedSubgroup[];
}

/** Ascending by `ordinal` — the one ordering (SCH-FR-16 / SCC-FR-10). */
function byOrdinal(a: SearchHit, b: SearchHit): number {
  return a.ordinal - b.ordinal;
}

/**
 * Group `hits` for rendering.
 *
 * SCH-FR-04: an empty group is omitted entirely rather than rendered as a
 * header with nothing under it, so a group appears exactly when its first hit
 * arrives and is never rendered ahead of one. The same holds for each Artifacts
 * subtype.
 *
 * SCH-FR-16: within every group and subgroup, hits stay in ascending `ordinal`,
 * so a later batch inserts into the existing order rather than reshuffling what
 * is already on screen.
 */
export function groupHits(hits: SearchHit[]): RenderedGroup[] {
  const buckets = new Map<SearchGroup, SearchHit[]>();
  for (const hit of hits) {
    const bucket = buckets.get(hit.group);
    if (bucket) bucket.push(hit);
    else buckets.set(hit.group, [hit]);
  }

  const rendered: RenderedGroup[] = [];
  for (const { group, title } of GROUP_ORDER) {
    const bucket = buckets.get(group);
    if (!bucket || bucket.length === 0) continue;
    const sorted = [...bucket].sort(byOrdinal);
    if (group !== "artifact") {
      rendered.push({ key: group, title, hits: sorted });
      continue;
    }
    // SCH-FR-03: Artifacts splits by subtype, in the fixed order of ASC-FR-02.
    const subgroups: RenderedSubgroup[] = [];
    for (const type of ARTIFACT_TYPES) {
      const ofType = sorted.filter((h) => h.subtype === type.value);
      if (ofType.length === 0) continue;
      subgroups.push({
        key: `artifact:${type.value}`,
        title: type.label,
        hits: ofType,
      });
    }
    // A hit in the artifact group with no recognised subtype cannot go under a
    // subtype heading. Rather than dropping it — a result the user can see no
    // trace of is worse than an oddly-grouped one — it gets a trailing bucket.
    // Nothing produces one today: the backend only ever sets `subtype` from the
    // same eight types (SCC-FR-08), so this is the belt to that braces.
    const untyped = sorted.filter(
      (h) => !ARTIFACT_TYPES.some((t) => t.value === h.subtype),
    );
    if (untyped.length > 0) {
      subgroups.push({ key: "artifact:other", title: "Other", hits: untyped });
    }
    rendered.push({ key: group, title, hits: sorted, subgroups });
  }
  return rendered;
}

/**
 * SCH-FR-09: where clicking a result goes. The same routing rule the Dashboard
 * uses for widget click-through (DSH-FR-06).
 *
 * - an artifact in flow edit context, and a Flow, open in **Flow**;
 * - every other artifact opens in the **Editor**;
 * - a workstream reveals the Library filtered to it;
 * - a run opens the Runs bottom panel; a history match the History viewer;
 * - a **file** opens in the Editor as a plain text file (ESH-FR-ATDS).
 */
export type SearchRoute =
  | { kind: "editor"; hit: SearchHit }
  | { kind: "flow"; hit: SearchHit }
  | { kind: "workstream"; hit: SearchHit }
  | { kind: "runs"; hit: SearchHit }
  | { kind: "history"; hit: SearchHit };

export function routeForHit(hit: SearchHit): SearchRoute {
  switch (hit.group) {
    case "workstream":
      return { kind: "workstream", hit };
    case "run":
      return { kind: "runs", hit };
    case "history":
      return { kind: "history", hit };
    case "artifact":
      return hit.editContext === "flow" || hit.subtype === "flow"
        ? { kind: "flow", hit }
        : { kind: "editor", hit };
    // A playbook, a role, and a plain file are all opened as content in the
    // Editor — the last as a plain text file, since it carries no type at all.
    default:
      return { kind: "editor", hit };
  }
}

/** The artifact type a routed hit carries into the tab, if any. */
export function subtypeOf(hit: SearchHit): ArtifactType | undefined {
  return hit.group === "artifact" ? hit.subtype : undefined;
}
