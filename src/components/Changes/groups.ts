/**
 * The repository groups of the Changes tree (CHG-FR-BYWJ, CHG-FR-IGSD,
 * CHG-FR-UMCU, CHG-FR-EQYC), the pointer rows (CHG-FR-GBTE) and their implied
 * check (CHG-FR-AYTR), and the keys a reveal opens (CHG-FR-54). Pure functions,
 * no rendering.
 */
import type {
  ChangeEntry,
  PushSkipReason,
  PushSummary,
  RepositoryInfo,
} from "../../types";
import type { CommitFile, CommitRepository } from "../CommitMessageModal";
import {
  buildChangeTree,
  relativeToRoot,
  visibleFilesUnder,
  type ChangeNode,
  type CheckState,
} from "./tree";

/** GSM-FR-SFEI: the path of the main repository. */
export const MAIN_REPOSITORY = "";

/** The owner of an entry. An entry with no owner belongs to the main repository. */
export function entryRepository(entry: ChangeEntry): string {
  return entry.repository ?? MAIN_REPOSITORY;
}

/** CHC-FR-EBLI: whether an entry is the pointer of a submodule. */
export function isPointer(entry: ChangeEntry): boolean {
  return entry.entryKind === "submodule";
}

/** CHG-FR-UMCU / CHG-FR-GBTE: the short form of a commit id. */
export function shortId(id: string | undefined): string {
  return (id ?? "").slice(0, 7);
}

/**
 * CHG-FR-UMCU: the name of the main repository, and the project-relative path
 * of a submodule.
 */
export function repositoryLabel(info: RepositoryInfo): string {
  return info.path === MAIN_REPOSITORY ? info.name : info.path;
}

/** CHG-FR-UMCU: the current branch, or `detached` and the short commit id. */
export function repositoryBranchLabel(info: RepositoryInfo): string {
  if (info.branch) return info.branch;
  const id = shortId(info.headId);
  return id ? `detached ${id}` : "detached";
}

/**
 * CHG-FR-GBTE: what a pointer row shows in place of the diffstat — the short
 * ids of the previous and the current commit, or `+` and the current one for
 * an added submodule.
 */
export function pointerCommitsText(entry: ChangeEntry): string {
  const current = shortId(entry.currentCommit);
  if (!entry.previousCommit) return `+ ${current}`;
  return `${shortId(entry.previousCommit)} → ${current}`;
}

/**
 * CHC-FR-TMNY: the repository set as the change set lists it. A repository that
 * an entry names and the list does not hold is added after the others, so no
 * entry is lost from the tree. An empty list is the main repository alone.
 */
export function repositoriesOf(
  repositories: RepositoryInfo[] | undefined,
  entries: ChangeEntry[],
): RepositoryInfo[] {
  const listed = [...(repositories ?? [])];
  const known = new Set(listed.map((r) => r.path));
  const missing = [...new Set(entries.map(entryRepository))]
    .filter((path) => !known.has(path))
    .sort((a, b) => a.localeCompare(b));
  for (const path of missing) {
    listed.push({ path, name: path.split("/").pop() ?? path });
  }
  if (listed.length === 0) listed.push({ path: MAIN_REPOSITORY, name: "" });
  return listed;
}

/** CHG-FR-BYWJ / CHG-FR-IGSD: repository groups render only for more than one repository. */
export function isMultiRepository(repositories: RepositoryInfo[]): boolean {
  return repositories.length > 1;
}

/**
 * The namespace of the node keys of one repository. With one repository it is
 * empty, so the keys are the ones the panel used before repository groups.
 */
export function repositoryKeyPrefix(repository: string, multi: boolean): string {
  return multi ? `m:${repository}|` : "";
}

/** CHG-FR-BYWJ: the key of a repository group node. */
export function repositoryGroupKey(repository: string): string {
  return `repo:${repository}`;
}

/** One of the two groups of CHG-FR-09 inside one repository. */
export interface SectionModel {
  kind: "revisioned" | "unrevisioned";
  node: ChangeNode;
}

/** CHG-FR-BYWJ: one repository and the groups of CHG-FR-09 for its entries. */
export interface RepositoryGroupModel {
  info: RepositoryInfo;
  /** The repository group node. Its children are the two section nodes. */
  node: ChangeNode;
  sections: SectionModel[];
}

/**
 * CHG-FR-BYWJ / CHG-FR-IGSD / CHG-FR-09 / CHG-FR-EQYC: split the visible
 * entries into one model for each repository with a visible entry, in the
 * order of the repository set. With one repository the result is one model
 * with every visible entry, and the caller renders its sections as the top
 * level.
 */
export function buildRepositoryGroups(
  visible: ChangeEntry[],
  repositories: RepositoryInfo[],
): RepositoryGroupModel[] {
  const multi = isMultiRepository(repositories);
  const models: RepositoryGroupModel[] = [];
  for (const info of multi ? repositories : repositories.slice(0, 1)) {
    const own = multi
      ? visible.filter((e) => entryRepository(e) === info.path)
      : visible;
    if (multi && own.length === 0) continue;
    const prefix = repositoryKeyPrefix(info.path, multi);
    const root = multi ? info.path : MAIN_REPOSITORY;
    const section = (
      kind: SectionModel["kind"],
      entries: ChangeEntry[],
    ): SectionModel => ({
      kind,
      node: {
        key: `${prefix}${kind}`,
        name: kind === "revisioned" ? "Revisioned" : "Unrevisioned",
        path: kind === "revisioned" ? "Revisioned" : "Unrevisioned",
        kind: "folder",
        children: buildChangeTree(
          entries,
          `${prefix}${kind === "revisioned" ? "r:" : "u:"}`,
          root,
        ),
      },
    });
    const sections = [
      section(
        "revisioned",
        own.filter((e) => e.changeStatus !== "untracked"),
      ),
      section(
        "unrevisioned",
        own.filter((e) => e.changeStatus === "untracked"),
      ),
    ];
    const label = repositoryLabel(info);
    models.push({
      info,
      node: {
        key: repositoryGroupKey(info.path),
        name: label,
        path: label,
        kind: "folder",
        children: sections
          .map((s) => s.node)
          .filter((n) => n.children.length > 0),
      },
      sections,
    });
  }
  return models;
}

/**
 * CHG-FR-54: the nodes a reveal of `entry` must expand, and the key of its
 * file row. The group is read off the entry's own status, so the row is reached
 * where the change set places it.
 */
export function revealKeys(
  entry: ChangeEntry,
  multi: boolean,
): { opening: Set<string>; fileKey: string } {
  const repository = entryRepository(entry);
  const prefix = repositoryKeyPrefix(repository, multi);
  const untracked = entry.changeStatus === "untracked";
  const groupPrefix = `${prefix}${untracked ? "u:" : "r:"}`;
  const opening = new Set<string>([
    `${prefix}${untracked ? "unrevisioned" : "revisioned"}`,
  ]);
  if (multi) opening.add(repositoryGroupKey(repository));
  const root = multi ? repository : MAIN_REPOSITORY;
  const segments = relativeToRoot(root, entry.path).split("/");
  for (let i = 1; i < segments.length; i += 1) {
    const folder = segments.slice(0, i).join("/");
    opening.add(`${groupPrefix}d:${root === "" ? folder : `${root}/${folder}`}`);
  }
  return { opening, fileKey: `${groupPrefix}f:${entry.path}` };
}

/**
 * CHG-FR-AYTR / GSM-FR-MPWK: the pointer paths that a commit of `entries`
 * records without a check. A commit in a submodule makes its parent record the
 * new pointer, and that commit of the parent makes the parent of the parent
 * record its pointer, up to the main repository.
 */
export function impliedPointerPaths(
  entries: ChangeEntry[],
  repositories: RepositoryInfo[],
): Set<string> {
  const byPath = new Map(repositories.map((r) => [r.path, r] as const));
  const implied = new Set<string>();
  for (const entry of entries) {
    let repository = entryRepository(entry);
    // The visited set stops a parent cycle in a malformed repository list.
    while (repository !== MAIN_REPOSITORY && !implied.has(repository)) {
      implied.add(repository);
      repository = byPath.get(repository)?.parent ?? MAIN_REPOSITORY;
    }
  }
  return implied;
}

/** CHG-FR-AYTR: whether an entry is a pointer row whose check is implied. */
export function isImpliedPointer(
  entry: ChangeEntry,
  implied: ReadonlySet<string>,
): boolean {
  return isPointer(entry) && implied.has(entry.path);
}

/**
 * CHG-FR-AYTR / CHG-FR-28: the visible files beneath a node that the author
 * can toggle — every one but an implied pointer row, which is inert. This is
 * what a folder, group, or repository cascade acts on.
 */
export function toggleableFilesUnder(
  node: ChangeNode,
  implied: ReadonlySet<string>,
): ChangeEntry[] {
  return visibleFilesUnder(node).filter((e) => !isImpliedPointer(e, implied));
}

/**
 * CHG-FR-28 / CHG-FR-AYTR: how the checkbox of a row renders. A file row is
 * checked by its own check; an implied pointer row is checked and inert. A
 * folder, group, or repository node is computed over the rows the author can
 * toggle only, so an inert row never makes a cascade look done or undone. A
 * node whose visible rows are all implied renders checked and inert, like
 * those rows.
 */
export function nodeCheckOf(
  node: ChangeNode,
  checked: ReadonlySet<string>,
  implied: ReadonlySet<string>,
): { state: CheckState; inert: boolean } {
  if (node.kind === "file") {
    if (node.entry && isImpliedPointer(node.entry, implied))
      return { state: "checked", inert: true };
    return { state: checked.has(node.path) ? "checked" : "unchecked", inert: false };
  }
  const files = visibleFilesUnder(node);
  const toggleable = files.filter((e) => !isImpliedPointer(e, implied));
  if (toggleable.length === 0)
    return files.length > 0
      ? { state: "checked", inert: true }
      : { state: "unchecked", inert: false };
  const ticked = toggleable.filter((e) => checked.has(e.path)).length;
  if (ticked === 0) return { state: "unchecked", inert: false };
  return {
    state: ticked === toggleable.length ? "checked" : "indeterminate",
    inert: false,
  };
}

/**
 * CMW-FR-WZRQ: the repository that owns an entry, then each parent up to the
 * main repository — every repository that a commit of the entry is made in.
 */
export function commitRepositoryChain(
  repository: string,
  repositories: RepositoryInfo[],
): CommitRepository[] {
  const byPath = new Map(repositories.map((r) => [r.path, r] as const));
  const chain: CommitRepository[] = [];
  const seen = new Set<string>();
  let current: string | undefined = repository;
  while (current !== undefined && !seen.has(current)) {
    seen.add(current);
    const info = byPath.get(current);
    chain.push({
      path: current,
      label: info ? repositoryLabel(info) : current,
    });
    if (current === MAIN_REPOSITORY) break;
    current = info?.parent ?? MAIN_REPOSITORY;
  }
  return chain;
}

/**
 * CHG-FR-27 / CMW-FR-WZRQ: the commit window's form of an entry. With more than
 * one repository it carries the repositories its commit is made in.
 */
export function commitFileOf(
  entry: ChangeEntry,
  repositories: RepositoryInfo[],
): CommitFile {
  const file: CommitFile = {
    path: entry.path,
    untracked: entry.changeStatus === "untracked",
  };
  if (isMultiRepository(repositories)) {
    file.repositoryChain = commitRepositoryChain(
      entryRepository(entry),
      repositories,
    );
  }
  return file;
}

/** CHG-FR-NUZB / GSM-FR-MJHC: the reason a push skipped a repository, in words. */
export function pushSkipReasonText(reason: PushSkipReason | string): string {
  switch (reason) {
    case "detached_head":
      return "detached HEAD";
    case "no_remote":
      return "no remote configured";
    default:
      return reason;
  }
}

/** CHG-FR-NUZB: one skipped repository as the panel states it. */
export interface PushSkipLine {
  repository: string;
  label: string;
  reason: string;
}

/** CHG-FR-NUZB: the repositories a push skipped, with their labels. */
export function pushSkipLines(
  summary: PushSummary | undefined,
  repositories: RepositoryInfo[],
): PushSkipLine[] {
  const byPath = new Map(repositories.map((r) => [r.path, r] as const));
  return (summary?.skipped ?? []).map((s) => {
    const info = byPath.get(s.repository);
    return {
      repository: s.repository,
      label: info
        ? repositoryLabel(info) || "main repository"
        : s.repository || "main repository",
      reason: pushSkipReasonText(s.reason),
    };
  });
}
