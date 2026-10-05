/**
 * The Changes vertical panel (`specifications/ui/CHG-changes.md`).
 *
 * Renders the project's changed files as the same folder-mirroring tree the
 * Project panel uses, tagged by artifact type and filtered by the same lenses, with a
 * `+added −removed` summary on every row and a checkbox on every row deciding
 * what the next commit carries. The tree's top level is two groups —
 * **Revisioned** then **Unrevisioned** (CHG-FR-09) — so one tick takes the whole
 * of either. Two mutually-exclusive modes serve two
 * questions: **Uncommitted** (CHG-FR-03), where committing happens, and
 * **Branch** against a configurable target (CHG-FR-04 / CHG-FR-05).
 *
 * The panel's reach over the repository is exactly two operations (CHG-FR-21):
 * committing the checked set and pushing the current branch. It exposes no
 * staging — the author ticks paths rather than manipulating an index — creates
 * and checks out no branch, pulls nothing, and opens no pull request; those
 * belong to the bottom-panel Git surface.
 */
import {
  Fragment,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import * as api from "../api";
import { onChangesUpdated, onGitOperationFinished } from "../events";
import {
  loadAppPreferences,
  patchAppPreferences,
} from "../state/appPreferences";
import { tokenErrorMessage } from "./GithubTokens";
// One implementation of "is there anything to push", shared with the Git panel
// so the two surfaces can never disagree about it (CHG-FR-37).
import {
  canPush,
  canStartPush,
  PUSH_RUNNING_REASON,
  pushUnavailableReason,
} from "../gitSync";
import { useGitTransfer } from "../hooks/useGitTransfer";
import type { CommitFile } from "./CommitMessageModal";
import type { RollbackFile } from "./RollbackConfirm";
import type { RollbackResult } from "../types";
import { logWarn } from "../logging";
import {
  DEFAULT_TYPE_LENS,
  matchesLens,
  matchesText,
  presentTypes,
  typeChip,
  typeLensPositions,
  type TypeLens,
} from "../artifactTypes";
import { Icon } from "./icons";
import { SplitAction } from "./SplitAction";
import { SelectorRow, type SelectorPosition } from "./SelectorRow";
import {
  markRevealConsumed,
  revealAlreadyConsumed,
} from "../state/panelReveal";
import {
  EMPTY_BODY_CLASS,
  FILTERED_BODY_CLASS,
  PanelEmptyState,
  PanelFilteredState,
} from "./PanelEmptyState";
import {
  ERR_NOT_A_REPOSITORY,
  ERR_UNKNOWN_BRANCH,
  comparisonLabel,
  diffScopeFor,
} from "../comparison";
import type {
  BranchOption,
  ChangeEntry,
  ChangeSet,
  ChangesCommitAction,
  ChangesMode,
  CommitOutcome,
  DiffTarget,
  PanelRevealRequest,
} from "../types";

// ---------------------------------------------------------------------------
// Tree assembly (CHG-FR-08 / CHG-FR-09) — pure, and exported for its tests
// ---------------------------------------------------------------------------

export interface ChangeNode {
  /** Stable key for expand/collapse and selection state (CHG-FR-17). */
  key: string;
  name: string;
  /** Project-relative path of the folder or file. */
  path: string;
  kind: "folder" | "file";
  /** Files only. */
  entry?: ChangeEntry;
  children: ChangeNode[];
}

/**
 * Build the nested folder tree for a set of already-filtered entries, with the
 * changed files as the only leaves (CHG-FR-08). Because the entries are filtered
 * first, a folder with no visible changed descendant simply never gets created —
 * which is exactly the rule CHG-FR-08 states.
 *
 * `keyPrefix` namespaces the node keys so the same path under **Revisioned** and
 * under **Unrevisioned** keeps independent expand state.
 */
export function buildChangeTree(
  entries: ChangeEntry[],
  keyPrefix: string,
): ChangeNode[] {
  const roots: ChangeNode[] = [];
  // Folder nodes by path, so repeated visits to the same folder reuse one node.
  const folders = new Map<string, ChangeNode>();

  const folderAt = (path: string): ChangeNode[] => {
    if (path === "") return roots;
    const existing = folders.get(path);
    if (existing) return existing.children;
    const slash = path.lastIndexOf("/");
    const parent = slash === -1 ? "" : path.slice(0, slash);
    const name = slash === -1 ? path : path.slice(slash + 1);
    const node: ChangeNode = {
      // Keys carry the node kind: a branch comparison can hold both a deleted
      // file `a` and a new file `a/b.md`, and two sibling nodes named `a` with
      // one key would collide in React and share expand state.
      key: `${keyPrefix}d:${path}`,
      name,
      path,
      kind: "folder",
      children: [],
    };
    folders.set(path, node);
    folderAt(parent).push(node);
    return node.children;
  };

  for (const entry of entries) {
    const slash = entry.path.lastIndexOf("/");
    const parent = slash === -1 ? "" : entry.path.slice(0, slash);
    folderAt(parent).push({
      key: `${keyPrefix}f:${entry.path}`,
      name: entry.name,
      path: entry.path,
      kind: "file",
      entry,
      children: [],
    });
  }

  // Folders first, then files, each alphabetical — matching the Library's order.
  const sort = (nodes: ChangeNode[]) => {
    nodes.sort((a, b) => {
      if (a.kind !== b.kind) return a.kind === "folder" ? -1 : 1;
      return a.name.localeCompare(b.name);
    });
    for (const n of nodes) if (n.kind === "folder") sort(n.children);
  };
  sort(roots);
  return roots;
}

/** Whether an entry survives the two AND-combined filters (CHG-FR-14). */
export function entryVisible(
  entry: ChangeEntry,
  lens: TypeLens,
  text: string,
): boolean {
  return matchesLens(lens, entry.artifactType) && matchesText(text, entry.name);
}

/**
 * Split a change set into the two top-level groups (CHG-FR-09): **Revisioned**,
 * holding every entry Git already tracks, and **Unrevisioned**, holding the
 * entries whose change status is untracked. The filters apply inside both groups
 * identically (CHG-FR-16).
 */
export function partitionEntries(
  entries: ChangeEntry[],
  lens: TypeLens,
  text: string,
): { revisioned: ChangeEntry[]; unrevisioned: ChangeEntry[] } {
  const visible = entries.filter((e) => entryVisible(e, lens, text));
  return {
    revisioned: visible.filter((e) => e.changeStatus !== "untracked"),
    unrevisioned: visible.filter((e) => e.changeStatus === "untracked"),
  };
}

/**
 * Every changed file at or below `node`, in tree order. Because the tree is
 * built from already-filtered entries, this is exactly the node's *currently
 * visible* descendants — which is what a folder's check cascades over and what
 * its tri-state is computed from (CHG-FR-28).
 */
export function visibleFilesUnder(node: ChangeNode): ChangeEntry[] {
  if (node.kind === "file") return node.entry ? [node.entry] : [];
  return node.children.flatMap(visibleFilesUnder);
}

/**
 * CHG-FR-50 / CHG-FR-52: how many changed files are currently visible beneath a
 * node, counted recursively over its whole subtree rather than over its direct
 * children. Because the tree is built from already-filtered entries, this is by
 * construction the count the active filters admit — it is the same number the
 * node reveals when it is expanded, and it re-computes with the filters rather
 * than on a reload.
 */
export function visibleFileCount(node: ChangeNode): number {
  // Counted rather than collected: every folder and group row asks for this on
  // every render, and `visibleFilesUnder` allocates an array per level.
  if (node.kind === "file") return node.entry ? 1 : 0;
  return node.children.reduce((sum, child) => sum + visibleFileCount(child), 0);
}

/** How a folder or group node's checkbox renders (CHG-FR-28). */
export type CheckState = "checked" | "unchecked" | "indeterminate";

/**
 * CHG-FR-28: a folder or group is checked when every visible changed file
 * beneath it is checked, unchecked when none is, and indeterminate when some
 * are. A folder with nothing visible beneath it cannot be rendered at all
 * (CHG-FR-08), so the empty case only arises defensively.
 */
export function folderCheckState(
  node: ChangeNode,
  checked: ReadonlySet<string>,
): CheckState {
  const files = visibleFilesUnder(node);
  if (files.length === 0) return "unchecked";
  const ticked = files.filter((e) => checked.has(e.path)).length;
  if (ticked === 0) return "unchecked";
  return ticked === files.length ? "checked" : "indeterminate";
}

/**
 * CHG-FR-27: the **commit set** is exactly the file rows that are both checked
 * and currently visible under the active filters. A row the filters hide
 * contributes nothing whether or not it was checked while visible, so the panel
 * never commits a path the author cannot see.
 */
export function commitSetOf(
  visible: ChangeEntry[],
  checked: ReadonlySet<string>,
): CommitFile[] {
  return visible
    .filter((e) => checked.has(e.path))
    .map((e) => ({ path: e.path, untracked: e.changeStatus === "untracked" }));
}

/** The label of the primary button for each action (CHG-FR-33). */
const ACTION_LABELS: Record<ChangesCommitAction, string> = {
  commit: "Commit",
  commit_and_push: "Commit & Push",
  push: "Push",
};

const ACTIONS: ChangesCommitAction[] = ["commit", "commit_and_push", "push"];

/**
 * CHG-FR-02 / SNV-FR-62: the mode selector's two positions. Module-level so its
 * identity is stable across renders — `SelectorRow` re-measures its fit when the
 * position list changes, and a fresh array every render would make that a
 * per-render cost for a list that never varies.
 */
const MODE_POSITIONS: SelectorPosition<ChangesMode>[] = [
  { value: "uncommitted", tag: "Uncommitted", title: "Uncommitted" },
  { value: "branch", tag: "Branch", title: "Branch" },
];

export { canPush };

// ---------------------------------------------------------------------------
// Rows
// ---------------------------------------------------------------------------

/**
 * The diffstat cell (CHG-FR-10 / CHG-FR-11). Rendered at a fixed width so rows
 * do not shift horizontally as counts change during a reload.
 */
function DiffStat({ entry }: { entry: ChangeEntry }) {
  if (entry.isBinary) {
    return (
      <span className="change-row__stat" title="Binary file">
        <span className="change-row__binary">binary</span>
      </span>
    );
  }
  return (
    <span className="change-row__stat">
      <span className="change-row__added">+{entry.addedLines ?? 0}</span>
      <span className="change-row__removed">−{entry.removedLines ?? 0}</span>
    </span>
  );
}

/**
 * A checkbox whose indeterminate state is applied imperatively — the DOM
 * property has no JSX attribute, and a folder with only some of its visible
 * descendants ticked has to render as neither on nor off (CHG-FR-28).
 */
function CheckBox({
  state,
  label,
  onChange,
}: {
  state: CheckState;
  label: string;
  onChange: (next: boolean) => void;
}) {
  const ref = useRef<HTMLInputElement>(null);
  useEffect(() => {
    if (ref.current) ref.current.indeterminate = state === "indeterminate";
  }, [state]);
  return (
    <input
      ref={ref}
      type="checkbox"
      className="change-check"
      aria-label={label}
      checked={state === "checked"}
      // CHG-FR-18: toggling a checkbox is not a click on the row — it changes
      // the check and opens nothing.
      onClick={(e) => e.stopPropagation()}
      onChange={(e) => onChange(e.target.checked)}
    />
  );
}

/**
 * CHG-FR-50 / CHG-FR-53: the count of changed files visible beneath a folder or
 * group node, stated immediately after its label. It answers "how much is in
 * here?" without the node being opened and keeps answering it once it is, so it
 * is not conditioned on the node's expand state.
 *
 * CHG-FR-51: stated with the noun it counts and agreeing with it — `1 file`,
 * `13 files` — so the row reads as a sentence about its contents rather than
 * leaving the reader to work out what the digit beside a folder measures.
 *
 * It is text on the row and nothing more (CHG-FR-53): no activation target of
 * its own, so a pointer on it toggles the node exactly as the label does. A node
 * with nothing visible beneath it is never rendered (CHG-FR-08 / CHG-FR-16), so
 * no zero reaches the screen.
 */
function NodeCount({ node }: { node: ChangeNode }) {
  const count = visibleFileCount(node);
  // No tooltip: the row carries none anywhere else, and one here would be an
  // affordance the count is not supposed to have.
  return (
    <span className="tree-row__count">
      {count} {count === 1 ? "file" : "files"}
    </span>
  );
}

interface ChangeRowProps {
  node: ChangeNode;
  depth: number;
  open: boolean;
  selected: boolean;
  /** CHG-FR-26: null in Branch mode, which renders no checkbox at all. */
  checkState: CheckState | null;
  onCheck: (node: ChangeNode, next: boolean) => void;
  onToggle: (node: ChangeNode) => void;
  onOpen: (node: ChangeNode) => void;
}

function ChangeRow({
  node,
  depth,
  open,
  selected,
  checkState,
  onCheck,
  onToggle,
  onOpen,
}: ChangeRowProps) {
  const isFolder = node.kind === "folder";
  const entry = node.entry;
  return (
    <div
      className="tree-row"
      style={{ paddingLeft: 4 + depth * 14 }}
      data-selected={selected}
      // CHG-FR-54: what a reveal scrolls to. The key rather than the path,
      // because the same path under **Revisioned** and **Unrevisioned** is two
      // distinct rows (CHG-FR-09).
      data-node-key={node.key}
      data-change-status={entry?.changeStatus}
      onClick={() => (isFolder ? onToggle(node) : onOpen(node))}
    >
      {checkState && (
        <CheckBox
          state={checkState}
          label={`Include ${node.path}`}
          onChange={(next) => onCheck(node, next)}
        />
      )}
      <span className="tree-row__caret">
        {isFolder ? (
          open ? (
            <Icon.Caret size={12} />
          ) : (
            <Icon.CaretRight size={12} />
          )
        ) : null}
      </span>
      <span className="tree-row__icon">
        {isFolder ? <Icon.Folder size={13} /> : <Icon.File size={13} />}
      </span>
      <span
        className={
          isFolder ? "tree-row__name tree-row__name--counted" : "tree-row__name"
        }
      >
        {node.name}
        {/* CHG-FR-12: a renamed entry sits at its current path and additionally
            shows where it came from. */}
        {entry?.previousPath && (
          <span className="change-row__renamed"> (was {entry.previousPath})</span>
        )}
      </span>
      {/* CHG-FR-50: how many changed files are visible beneath this folder,
          against the label rather than in the trailing column the diffstats
          occupy — a folder's file count read as a line count would be worse
          than no count at all. */}
      {isFolder && <NodeCount node={node} />}
      {entry?.artifactType && (
        <span
          className="chip-type"
          data-type={entry.artifactType}
          title={
            entry.typeSource
              ? `${entry.artifactType} (${entry.typeSource})`
              : entry.artifactType
          }
        >
          {typeChip(entry.artifactType)}
        </span>
      )}
      {entry && <DiffStat entry={entry} />}
    </div>
  );
}

// ---------------------------------------------------------------------------
// Panel
// ---------------------------------------------------------------------------

/**
 * CHG-FR-65: render a typed rollback failure as a sentence.
 *
 * The kinds are the ones `../specifications/core/GTC-git.md` GTC-FR-25 returns.
 * An unrecognised kind is shown as it arrived rather than swallowed — a cause
 * the panel cannot name is still a cause the author needs to see.
 */
/**
 * CHG-FR-61: the pseudo-kind a save failure carried into the rollback report
 * takes, so it renders beside the backend's own typed causes without pretending
 * to be one of them.
 */
export const SAVE_FAILED_KIND = "unsaved_edit_lost";

export function rollbackFailureText(kind: string): string {
  switch (kind) {
    case SAVE_FAILED_KIND:
      return "an unsaved edit could not be written before it was discarded";
    case "permission_denied":
      return "permission denied";
    case "not_found":
      return "no longer exists";
    case "is_directory":
      return "is a folder, not a file";
    case "path_outside_content_root":
      return "lies outside the project";
    case "write_failed":
      return "could not be written";
    default:
      return kind;
  }
}

interface ChangesProps {
  /** CHG-FR-18: open a Diff tab for a changed file under the active comparison. */
  onOpenDiff: (target: DiffTarget) => void;
  /**
   * CHG-FR-39 / CHG-FR-41: open the commit message window carrying `files` and
   * the changed files the filters are hiding (CHG-FR-48), and resolve with the
   * **outcome** of the commit made from it — the commit and the paths it
   * recorded — or `null` if it was dismissed or refused.
   *
   * The outcome rather than a bare `true` because the paths a commit recorded
   * are what close the Diff tabs it has finished with (`TAB-tabs.md`
   * TAB-FR-22), and re-deriving them from the set submitted here would miss a
   * rename's previous location.
   *
   * The window is a modal action surface of the main window and is mutually
   * exclusive with the other overlays (CMW-FR-01), so it is opened by the shell
   * rather than mounted here.
   */
  onRequestCommitMessage?: (
    files: CommitFile[],
    hidden: CommitFile[],
  ) => Promise<CommitOutcome | null>;
  /**
   * CHG-FR-59 – CHG-FR-63: confirm and perform a rollback of `files`, resolving
   * with the backend's per-path outcome, or `null` if the confirmation was
   * dismissed and nothing was done.
   *
   * Owned by the shell rather than mounted here for two reasons. The
   * confirmation is a floating overlay and is mutually exclusive with the
   * window's others (SNV-FR-56), like the commit message window; and the
   * preparation and the reset either side of the call reach the artifact editing
   * sessions and the tab strip (CHG-FR-60, CHG-FR-63), neither of which this
   * panel owns. What comes back is the typed outcome, so this panel can clear
   * exactly the checks the backend confirmed and report the rest (CHG-FR-65).
   */
  onRequestRollback?: (
    files: RollbackFile[],
    onConfirmed: () => void,
  ) => Promise<RollbackResult | null>;
  /**
   * CHG-FR-45 / GHA-FR-16: a push blocked on token selection opens the picker,
   * and resolves with whether one was chosen. The same shell-owned picker the
   * Git panel opens.
   */
  onRequestGithubToken?: () => Promise<boolean>;
  /** CHG-FR-45 / GHA-FR-19: no token is stored at all — route to where one is added. */
  onOpenGlobalSettings?: () => void;
  /**
   * SNV-FR-67: hand each loaded change set's paths up to the shell, the way the
   * Project panel hands up its tree.
   *
   * The shell has to know whether a Diff tab's file is in this comparison
   * *before* it activates this panel to reveal it — a panel that is not mounted
   * cannot answer, and switching to it first is the thing SNV-FR-67 forbids.
   */
  onChangeSetLoaded?: (paths: string[]) => void;
  /**
   * CHG-FR-54: a pending reveal-and-select, naming a changed file by its
   * project-relative path, or null. Reached from the active tab being followed
   * (SNV-FR-64 / SNV-FR-66) — a Diff tab naming the file half of the pair that
   * identifies it (DFV-FR-04).
   *
   * A nonce rather than a bare path, so two consecutive requests for the same
   * file both run.
   */
  reveal?: PanelRevealRequest | null;
}

export function Changes({
  reveal,
  onChangeSetLoaded,
  onOpenDiff,
  onRequestCommitMessage,
  onRequestRollback,
  onRequestGithubToken,
  onOpenGlobalSettings,
}: ChangesProps) {
  const [mode, setMode] = useState<ChangesMode>("uncommitted");
  const [targetBranch, setTargetBranch] = useState<string | null>(null);
  const [branches, setBranches] = useState<BranchOption[]>([]);
  const [changeSet, setChangeSet] = useState<ChangeSet | null>(null);
  const [error, setError] = useState<string | null>(null);
  // The persisted state has to land before the first list call, or the panel
  // would fetch for the wrong mode and then immediately refetch.
  const [restored, setRestored] = useState(false);

  // Expand state is the set of explicitly-collapsed folder keys, so folders
  // default to open and the state survives reloads (CHG-FR-17).
  const [collapsed, setCollapsed] = useState<Set<string>>(new Set());
  const [selectedKey, setSelectedKey] = useState<string | null>(null);
  const [text, setText] = useState("");
  // CHG-FR-15: the panel opens on the "All artifacts" lens, matching LIB-FR-12.
  const [typeFilter, setTypeFilter] = useState<TypeLens>(DEFAULT_TYPE_LENS);

  /**
   * CHG-FR-30 / CHG-FR-31 / CHG-FR-32: which paths are ticked. Keyed by path so
   * a reload that returns the same entry keeps its check, held in memory and
   * never persisted, and started empty — nothing is ever committed that the
   * author did not tick in this session. The panel is remounted by a project or
   * worktree switch, which is what discards it there.
   */
  const [checked, setChecked] = useState<Set<string>>(new Set());
  /** CHG-FR-34: the selected action, read on mount from user-global preferences. */
  const [action, setAction] = useState<ChangesCommitAction>("commit");
  /**
   * CHG-FR-37 / CHG-FR-38 / GIT-FR-QMYB: the branch's standing against its
   * upstream and whether any push is running, read from the window's one push
   * state so this panel and the other Push controls always agree.
   */
  const transfer = useGitTransfer();
  const { sync, running: pushRunning } = transfer;
  /** CHG-FR-44: a commit or push started here is in flight. */
  const [busy, setBusy] = useState(false);
  /**
   * CHG-FR-43 / CHG-FR-46: what the panel renders about a push — that one is
   * running, how it ended, or why it could not start. The transcript itself is
   * wide and line-shaped and belongs in the Git panel's output area, so nothing
   * here accumulates output.
   */
  const [note, setNote] = useState<{ tone: "info" | "error"; text: string } | null>(
    null,
  );
  /** CHG-FR-45: a missing token has no picker to open, so the note routes instead. */
  const [noteRoutesToSettings, setNoteRoutesToSettings] = useState(false);
  /**
   * CHG-FR-60: a rollback is being confirmed, prepared, or executed.
   *
   * Distinct from `busy`, which also covers a commit and a push: this is what
   * makes the *file rows* inert and what the button renders its in-progress
   * state from, because the preparation waits on somebody else's in-flight save
   * and is otherwise indistinguishable from nothing happening.
   */
  const [rollingBack, setRollingBack] = useState(false);
  /**
   * CHG-FR-60: the author has confirmed and the operation is preparing or
   * executing — distinct from `rollingBack`, which starts when the confirmation
   * opens so the set cannot move under it.
   *
   * The visible in-progress state is this one: the preparation waits on
   * somebody else's in-flight save and is otherwise indistinguishable from
   * nothing happening, while a confirmation the author is still reading is not
   * an operation in progress at all.
   */
  const [preparing, setPreparing] = useState(false);
  /**
   * CHG-FR-65: the paths a rollback could not reach, with their typed causes.
   *
   * Held apart from `note` because it is a list rather than a sentence, and
   * because a partial rollback must show no success state at all — the two are
   * never rendered together.
   */
  const [rollbackFailures, setRollbackFailures] = useState<
    { path: string; kind: string }[]
  >([]);

  // The mode + target the panel last wrote, so a value that merely arrived from
  // the backend is never written straight back. Declared here, beside the state
  // it guards, because the restore below seeds it.
  const persisted = useRef<string | null>(null);

  // CHG-FR-07: restore the persisted mode and target branch; on a project with
  // none, CHG-FR-06 seeds the target from the repository's default branch.
  useEffect(() => {
    let cancelled = false;
    void (async () => {
      let restoredMode: ChangesMode = "uncommitted";
      let restoredTarget: string | undefined;
      try {
        const state = await api.loadChangesPanelState();
        if (cancelled) return;
        restoredMode = state.mode;
        restoredTarget = state.targetBranch;
        setMode(state.mode);
      } catch {
        // A state that cannot be read leaves the panel on its defaults rather
        // than blocking it; nothing here is worth an error banner.
      }
      if (!restoredTarget) {
        try {
          restoredTarget = await api.getDefaultBranch();
        } catch {
          // Not a repository, or no branches yet. The list call below surfaces
          // the real reason inline.
        }
      }
      if (cancelled) return;
      // Seed the persistence guard with what the restore produced, so the
      // restore itself writes nothing. A derived default is re-derivable from
      // `get_default_branch` on the next open (CHG-FR-06), so only a choice the
      // user actually makes is worth a write.
      persisted.current = `${restoredMode} ${restoredTarget ?? ""}`;
      if (restoredTarget) setTargetBranch(restoredTarget);
      setRestored(true);
    })();
    return () => {
      cancelled = true;
    };
  }, []);

  // Every list request takes a ticket; only the newest one may commit its
  // result. Without this a slow Branch-mode call landing after a fast
  // Uncommitted one would paint branch entries under an "Uncommitted" toggle —
  // and since a row's Diff scope is derived from the rendered change set, the
  // tab it opened would carry the wrong comparison (CHG-FR-19). The manual
  // refresh and the event-driven reload share the same counter, so a burst can
  // never commit an older snapshot last.
  const requestSeq = useRef(0);
  /** CHG-FR-54: the scrolling body a revealed row is brought into view within. */
  const treeRef = useRef<HTMLDivElement>(null);

  // Read through a ref so `load` keeps a stable identity: it is the dependency
  // of the mount effect and of the event subscription, and a new identity each
  // render would re-subscribe on every shell render.
  const publishChangeSet = useRef(onChangeSetLoaded);
  publishChangeSet.current = onChangeSetLoaded;

  const load = useCallback(async () => {
    if (!restored) return;
    const ticket = ++requestSeq.current;
    try {
      const next =
        mode === "branch"
          ? await api.listBranchChanges(targetBranch ?? "")
          : await api.listUncommittedChanges();
      if (ticket !== requestSeq.current) return;
      setChangeSet(next);
      publishChangeSet.current?.(next.entries.map((e) => e.path));
      setError(null);
    } catch (e) {
      if (ticket !== requestSeq.current) return;
      setChangeSet(null);
      setError(String(e));
    }
  }, [mode, targetBranch, restored]);

  useEffect(() => {
    void load();
  }, [load]);

  // CHG-FR-21: reload on the debounced backend event. Expand/collapse and
  // selection live in component state, so they survive the reload untouched
  // (CHG-FR-17).
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void onChangesUpdated(() => {
      // CHG-FR-38: the push state re-reads the branch's standing on the same
      // event, so Push becomes available as soon as a commit lands.
      void load();
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [load]);

  // CHG-FR-05: the picker's options. Fetched when Branch mode is entered, since
  // Uncommitted mode shows no picker at all.
  useEffect(() => {
    if (mode !== "branch" || !restored) return;
    let cancelled = false;
    void api
      .listComparisonBranches()
      .then((list) => {
        if (!cancelled) setBranches(list);
      })
      .catch(() => {
        // The tree's own inline error already explains a repository problem.
      });
    return () => {
      cancelled = true;
    };
  }, [mode, restored]);

  // CHG-FR-07: persist mode + target per machine. Skipped until the restore has
  // landed so the initial defaults never overwrite what was stored. Best-effort:
  // a failed write must not break the panel.
  useEffect(() => {
    if (!restored) return;
    const signature = `${mode} ${targetBranch ?? ""}`;
    if (persisted.current === signature) return;
    persisted.current = signature;
    void api
      .saveChangesPanelState({
        mode,
        targetBranch: targetBranch ?? undefined,
      })
      .catch(() => {});
  }, [mode, targetBranch, restored]);

  /**
   * CHG-FR-34: the selected action is user-global, so it is one choice across
   * every project and every worktree. Read on mount; a record that cannot be
   * read leaves the panel on the **Commit** default rather than blocking it.
   */
  useEffect(() => {
    let cancelled = false;
    void loadAppPreferences().then((prefs) => {
      if (!cancelled) setAction(prefs.changesCommitAction ?? "commit");
    });
    return () => {
      cancelled = true;
    };
  }, []);

  const selectAction = (next: ChangesCommitAction) => {
    // CHG-FR-35: an unavailable action is still selectable — the selection
    // describes the author's habit rather than what the repository can do at
    // this moment.
    setAction(next);
    // GSS-FR-20: a whole-record write that carries every other field through.
    void patchAppPreferences({ changesCommitAction: next }).catch(() => {});
  };

  const toggleFolder = (node: ChangeNode) => {
    setSelectedKey(node.key);
    setCollapsed((prev) => {
      const next = new Set(prev);
      if (next.has(node.key)) next.delete(node.key);
      else next.add(node.key);
      return next;
    });
  };

  // CHG-FR-18 / CHG-FR-19: open the file's diff for the panel's currently
  // active comparison. The shell keys the tab on (file, comparison), so a
  // second click on the same row jumps focus instead of opening a duplicate.
  const openDiff = (node: ChangeNode) => {
    // CHG-FR-60: inert while a rollback runs — a Diff tab opened onto a file
    // the rollback is about to restore would render a comparison from under it.
    if (rollingBack) return;
    if (!node.entry || !changeSet) return;
    setSelectedKey(node.key);
    onOpenDiff({
      path: node.entry.path,
      name: node.entry.name,
      scope: diffScopeFor(
        changeSet.comparison,
        node.entry.path,
        // A renamed entry carries both halves so the backend can pair them and
        // the tab shows the edit rather than the whole file as new (CHG-FR-12).
        node.entry.previousPath,
      ),
      comparisonLabel: comparisonLabel(changeSet.comparison),
      // CHC-FR-10 resolves a changed file's type through the same classifier
      // the Project panel uses, so the Diff tab reads a Flow as a Flow whether the
      // extension or an assignment made it one (DFV-FR-17).
      artifactType: node.entry.artifactType,
    });
  };

  const notARepository = error?.includes(ERR_NOT_A_REPOSITORY) ?? false;
  const unknownBranch = error?.includes(ERR_UNKNOWN_BRANCH) ?? false;

  const { revisioned, unrevisioned } = partitionEntries(
    changeSet?.entries ?? [],
    typeFilter,
    text,
  );

  /**
   * CHG-FR-14: the lens renders a per-type button only for a type at least one
   * entry in the current change set resolves to, on the terms LIB-FR-19 states
   * — so the row offers the types this comparison actually touched. Computed
   * from the whole change set rather than from what the filters currently admit,
   * or narrowing to one type would leave that type as the only button left.
   */
  const lensPositions = useMemo(
    () =>
      typeLensPositions(
        presentTypes((changeSet?.entries ?? []).map((e) => e.artifactType)),
        typeFilter,
      ),
    [changeSet, typeFilter],
  );
  const revisionedTree = buildChangeTree(revisioned, "r:");
  const unrevisionedTree = buildChangeTree(unrevisioned, "u:");

  const revealPath = reveal?.id ?? null;
  const revealNonce = reveal?.nonce;
  /**
   * CHG-FR-54: the revealed row's key, scrolled into view once it has rendered.
   * The row is two renders away at the point the reveal runs — the ancestors it
   * has to expand are state that effect only just set.
   */
  const scrollTarget = useRef<string | null>(null);

  /**
   * CHG-FR-54: reveal and select the changed file another surface named —
   * expanding every ancestor folder node and the group node above it, relaxing
   * whichever local filter would hide the row, and making it the selection.
   *
   * **No check is touched anywhere in here.** Selecting a row and ticking one
   * are separate acts in this panel (CHG-FR-18), and the commit set is derived
   * from `checked` alone (CHG-FR-27) — so the one thing a reveal can do to it is
   * un-hide rows the author had already ticked, which re-enter on the ordinary
   * terms of CHG-FR-29.
   *
   * The row is reached where the change set already places it: the group is read
   * off the entry's own `changeStatus` rather than re-derived, because CHG-FR-54
   * is explicit that a reveal renders what the panel has rather than
   * reclassifying anything.
   *
   * Mode and target branch are deliberately absent from all of this. They are
   * not filters — they choose which comparison the panel is answering about
   * (CHG-FR-02 / CHG-FR-05) — so a file the active mode's change set does not
   * hold simply cannot be revealed, and everything is left as it was.
   */
  useEffect(() => {
    if (!revealPath || revealNonce === undefined) return;
    // Shared rather than a local ref: `VPanel` unmounts this panel on every
    // surface switch, and a local guard would let a finished reveal re-apply
    // over the row the author has since selected (SNV-FR-68).
    if (revealAlreadyConsumed(revealNonce)) return;
    // The change set is still in flight; the request stands until it lands.
    if (!changeSet) return;

    const entry = changeSet.entries.find((e) => e.path === revealPath);
    markRevealConsumed(revealNonce);
    // CHG-FR-54 / SNV-FR-67: a file this comparison does not hold changes
    // nothing at all — not the mode, not the target branch, not either filter,
    // not a check, not the selection — and reports nothing.
    if (!entry) return;

    // CHG-FR-54: relax only what would actually hide the row.
    if (!matchesLens(typeFilter, entry.artifactType))
      setTypeFilter("files");
    if (!matchesText(text, entry.name)) setText("");

    const groupPrefix = entry.changeStatus === "untracked" ? "u:" : "r:";
    const groupKey = entry.changeStatus === "untracked"
      ? "unrevisioned"
      : "revisioned";
    // Expansion here is the *removal* of collapse: this panel records which
    // nodes are collapsed rather than which are expanded, so opening an ancestor
    // means dropping its key rather than adding one.
    const opening = new Set<string>([groupKey]);
    const segments = entry.path.split("/");
    for (let i = 1; i < segments.length; i += 1)
      opening.add(`${groupPrefix}d:${segments.slice(0, i).join("/")}`);
    setCollapsed((prev) => {
      const next = new Set([...prev].filter((k) => !opening.has(k)));
      return next.size === prev.size ? prev : next;
    });

    // CHG-FR-54: what is selected is the file row, never a folder or a group.
    const key = `${groupPrefix}f:${entry.path}`;
    setSelectedKey(key);
    scrollTarget.current = key;
  }, [revealNonce, revealPath, changeSet, typeFilter, text]);

  /**
   * CHG-FR-54: "scrolls the file row into view". A tree long enough to overflow
   * the panel is the ordinary case here, and a row selected below the fold tells
   * the author nothing about where they are — the selection is the whole signal.
   *
   * Deliberately un-keyed: a ref check that returns immediately unless a reveal
   * is outstanding, which is cheaper than enumerating every piece of state that
   * could bring the row into the tree (the expansion above, a filter relaxation,
   * or the reload that produced the entry in the first place).
   */
  useEffect(() => {
    const key = scrollTarget.current;
    if (!key) return;
    const row = treeRef.current?.querySelector<HTMLElement>(
      `[data-node-key="${CSS.escape(key)}"]`,
    );
    if (!row) return;
    scrollTarget.current = null;
    row.scrollIntoView?.({ block: "nearest" });
  });

  // CHG-FR-31: an entry that leaves the change set loses its check and does not
  // regain it by reappearing. Pruned against the reloaded set rather than
  // against what is visible, so a check the filters hide survives (CHG-FR-29).
  useEffect(() => {
    const present = new Set((changeSet?.entries ?? []).map((e) => e.path));
    setChecked((prev) => {
      const next = new Set([...prev].filter((p) => present.has(p)));
      return next.size === prev.size ? prev : next;
    });
  }, [changeSet]);

  // CHG-FR-26: checkboxes exist in Uncommitted mode only. Branch mode's change
  // set mixes committed and uncommitted work, so a check over it would name no
  // commit the panel could make. CHG-FR-47: and the not-a-repository state
  // renders none, because there is nothing to commit to.
  const showChecks = mode === "uncommitted" && !notARepository;
  const commitSet = commitSetOf([...revisioned, ...unrevisioned], checked);
  // CHG-FR-48: the changed files the active filters are hiding right now. The
  // default lens hides exactly the code and tests an artifact produces, so a
  // commit of the Spec alone would be a hand-off missing what it describes.
  // Only the hidden ones: a row the author can see and left unticked is a
  // decision, not an oversight.
  const hiddenFromCommit: CommitFile[] = (changeSet?.entries ?? [])
    .filter((e) => !entryVisible(e, typeFilter, text))
    .map((e) => ({ path: e.path, untracked: e.changeStatus === "untracked" }));

  /**
   * CHG-FR-57: the rollback set is derived exactly as the commit set is — the
   * file rows that are both checked and currently visible, folder and group
   * cascades having contributed their visible descendants on the ordinary terms
   * of CHG-FR-28.
   *
   * It carries each entry's rename and deletion facts as well as its path,
   * because the confirmation names both identities of a rename and says which
   * files are deleted rather than restored (CHG-FR-59). Unlike a commit, the
   * paths the filters hide are never offered back (CHG-FR-48 is a commit's
   * concern): a rollback that leaves work behind has simply discarded less.
   */
  const rollbackSet: RollbackFile[] = (() => {
    const byPath = new Map(
      (changeSet?.entries ?? []).map((e) => [e.path, e] as const),
    );
    return commitSet.map((f) => {
      const entry = byPath.get(f.path);
      return {
        path: f.path,
        previousPath: entry?.previousPath ?? null,
        untracked: f.untracked,
        deleted: entry?.changeStatus === "deleted",
      };
    });
  })();

  // CHG-FR-36: Commit and Commit & Push are enabled only in Uncommitted mode
  // with a non-empty commit set — a clean working tree, a change set with
  // nothing ticked, and Branch mode each leave them disabled.
  const canCommit = mode === "uncommitted" && commitSet.length > 0;
  // CHG-FR-44: while either is in flight the primary button is disabled, so
  // neither can be submitted twice.
  // GIT-FR-QMYB: a push another control started makes Push and Commit & Push
  // unavailable here too.
  const primaryEnabled =
    !busy &&
    (action === "push"
      ? canStartPush(sync, pushRunning)
      : action === "commit_and_push"
        ? canCommit && !pushRunning
        : canCommit);
  const primaryReason =
    action === "push"
      ? pushUnavailableReason(sync, pushRunning)
      : action === "commit_and_push" && pushRunning
        ? PUSH_RUNNING_REASON
        : null;

  // CHG-FR-28: checking a folder cascades over its currently-visible changed
  // descendants. A group node is one such node, so a tick on **Revisioned**
  // takes every tracked change the filters are showing and a tick on
  // **Unrevisioned** takes every new file, neither disturbing the other.
  const onCheck = (node: ChangeNode, next: boolean) => {
    // CHG-FR-60: the file rows are inert while a rollback is being confirmed,
    // prepared, or executed — the set it is acting on must not move under it.
    if (rollingBack) return;
    const paths = visibleFilesUnder(node).map((e) => e.path);
    setChecked((prev) => {
      const s = new Set(prev);
      for (const p of paths) {
        if (next) s.add(p);
        else s.delete(p);
      }
      return s;
    });
  };

  /**
   * CHG-FR-43: the terminal status of a push arrives on `"git operation
   * finished"`. The panel renders only how it ended — the transcript is in the
   * Git panel's push/pull output area, which every push streams into whichever
   * surface started it (GIT-FR-PZIE).
   *
   * The two token causes are deliberately left to the caller below: one opens
   * the picker and one routes to Global settings (CHG-FR-45), and neither reads
   * as a plain failure.
   */
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void onGitOperationFinished((payload) => {
      if (payload.operation !== "push") return;
      const cause = payload.error ?? "";
      if (
        cause.includes("github_token_selection_required") ||
        cause.includes("github_token_missing")
      ) {
        return;
      }
      setNote(
        payload.ok
          ? { tone: "info", text: "Push complete." }
          : { tone: "error", text: `Push failed: ${tokenErrorMessage(cause)}` },
      );
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  /**
   * CHG-FR-43 / CHG-FR-45 / CHG-FR-46: the push both **Push** and the second
   * half of **Commit & Push** perform, identical in every respect including its
   * failures.
   *
   * `retried` bounds the token-selection loop to one retry: a binding that is
   * still unresolved after the picker confirmed is a failure to report, not a
   * reason to prompt again.
   */
  const runPush = useCallback(
    async (retried = false): Promise<boolean> => {
      setNote({ tone: "info", text: "Pushing…" });
      setNoteRoutesToSettings(false);
      const outcome = await transfer.pushBranch();
      if (outcome.ok) return true;
      // GIT-FR-QMYB: a push is running, so none starts here and the panel says why.
      if (outcome.cause === "busy") {
        setNote({ tone: "error", text: PUSH_RUNNING_REASON });
        return false;
      }
      // CHG-FR-45: tokens are stored but this project has not been pointed at
      // one, so the picker opens and the push runs on confirmation.
      if (outcome.cause === "selection_required" && !retried) {
        const chosen = (await onRequestGithubToken?.()) ?? false;
        if (!chosen) {
          setNote({
            tone: "error",
            text: "Push cancelled — no GitHub token was selected.",
          });
          return false;
        }
        return runPush(true);
      }
      // CHG-FR-45: none is stored at all, so there is nothing to choose
      // between and the note routes to where one is added.
      if (outcome.cause === "token_missing") {
        setNote({
          tone: "error",
          text: "Push needs a GitHub token.",
        });
        setNoteRoutesToSettings(true);
        return false;
      }
      // CHG-FR-46: any other failure renders inline naming what failed.
      setNote({
        tone: "error",
        text: `Push failed: ${tokenErrorMessage(outcome.error)}`,
      });
      return false;
    },
    [onRequestGithubToken, transfer.pushBranch],
  );

  /**
   * CHG-FR-39 / CHG-FR-41: open the commit message window carrying the commit
   * set as it stands, and clear every check once a commit has been made from
   * it. A dismissal, and a commit the backend rejects, both leave the checks
   * exactly as they were.
   */
  const runCommit = async (): Promise<boolean> => {
    if (commitSet.length === 0) return false;
    const outcome =
      (await onRequestCommitMessage?.(commitSet, hiddenFromCommit)) ?? null;
    const committed = outcome != null;
    if (committed) {
      setChecked(new Set());
      setNote(null);
      setNoteRoutesToSettings(false);
      // The panel itself reloads on the `"changes updated"` the commit produced
      // (CHG-FR-19); this is what re-reads whether the branch is now pushable.
      transfer.refreshSync();
    }
    return committed;
  };

  /**
   * CHG-FR-58: why the rollback button cannot be activated right now, or null
   * when it can.
   *
   * Both reasons are named when both apply, because fixing one and finding the
   * button still greyed is exactly the confusion the tooltip exists to prevent.
   */
  const rollbackUnavailableReason = (): string | null => {
    const reasons: string[] = [];
    if (rollbackSet.length === 0) reasons.push("no files are selected");
    if (typeFilter !== "files")
      reasons.push("the artifact lens must be All files");
    if (reasons.length === 0) return null;
    const why = reasons.join(", and ");
    return `Cannot discard changes: ${why}.`;
  };

  /**
   * CHG-FR-59 – CHG-FR-65: confirm, prepare, perform, and apply a rollback.
   *
   * Only a path the backend **confirmed** in `restoredPaths` or `removedPaths`
   * has its check cleared (CHG-FR-63); a path that failed keeps its check, so
   * the author can see what is still there and act on it. The session resets and
   * the tab closes behind those same paths are the shell's, performed inside
   * `onRequestRollback` before it resolves.
   */
  const runRollback = async () => {
    // CHG-FR-60: inert while confirming, preparing, or executing, so it cannot
    // be submitted twice.
    if (busy || rollbackUnavailableReason() !== null) return;
    setBusy(true);
    // CHG-FR-60: inert from the moment the confirmation opens — the set must
    // not move while the author reads it.
    setRollingBack(true);
    try {
      // The in-progress *label* starts only once the author has confirmed —
      // the shell says when, because the confirmation and the work it gates
      // both live behind this one call. CHG-FR-60 asks for the preparation wait
      // to be visible; showing "Discarding…" while the confirmation is still up
      // would announce an operation Escape will cancel without touching
      // anything.
      const result =
        (await onRequestRollback?.(rollbackSet, () => setPreparing(true))) ??
        null;
      // A dismissed confirmation performs nothing and leaves every check and
      // buffer exactly as it was (CHG-FR-59).
      if (result == null) return;
      const { outcome, saveFailures } = result;

      // CHG-FR-63: applied per path rather than per operation.
      const settled = new Set<string>();
      const failures: { path: string; kind: string }[] = [];
      for (const entry of outcome.entries) {
        if (entry.outcome === "failed") {
          failures.push(...entry.failures);
          continue;
        }
        settled.add(entry.path);
      }
      // CHG-FR-65: the report carries the save failures from preparation
      // (CHG-FR-61) beside the backend's own. A rollback whose only failure was
      // a save still shows no overall success state, because an edit the author
      // made did not reach disk before it was discarded.
      for (const f of saveFailures) {
        failures.push({ path: f.artifactId, kind: SAVE_FAILED_KIND });
      }
      if (settled.size > 0) {
        setChecked((prev) => {
          const next = new Set(prev);
          settled.forEach((p) => next.delete(p));
          return next;
        });
      }

      // CHG-FR-65: a rollback in which any path failed shows no success state.
      if (failures.length > 0) {
        setRollbackFailures(failures);
        setNote(null);
        setNoteRoutesToSettings(false);
        logWarn(["frontend"], "rollback completed with failures", {
          selected: outcome.entries.length,
          failed: failures.length,
        });
      } else {
        setRollbackFailures([]);
        setNote({
          tone: "info",
          text: `Discarded ${settled.size} ${settled.size === 1 ? "file" : "files"}.`,
        });
        setNoteRoutesToSettings(false);
      }
      // CHG-FR-64: reload the active change set, preserving expand/collapse,
      // the selection, and every check the rollback did not clear. The
      // `"changes updated"` the rollback's own writes produced drives the same
      // reload through the same ticketed `load`, so the two coalesce rather
      // than thrashing the tree.
      await load();
      transfer.refreshSync();
    } finally {
      setPreparing(false);
      setRollingBack(false);
      setBusy(false);
    }
  };

  const runPrimary = async () => {
    // CHG-FR-44: neither a commit nor a push can be submitted twice.
    if (busy) return;
    setBusy(true);
    try {
      if (action === "push") {
        await runPush();
        return;
      }
      const committed = await runCommit();
      // CHG-FR-42: the push half runs only after the commit has succeeded; a
      // rejected or abandoned commit pushes nothing.
      if (committed && action === "commit_and_push") await runPush();
    } finally {
      setBusy(false);
    }
  };

  const renderNodes = (nodes: ChangeNode[], depth: number): React.ReactNode[] =>
    nodes.flatMap((node) => {
      const isFolder = node.kind === "folder";
      const open = isFolder && !collapsed.has(node.key);
      const rows: React.ReactNode[] = [
        <ChangeRow
          key={node.key}
          node={node}
          depth={depth}
          open={open}
          selected={selectedKey === node.key}
          checkState={
            showChecks
              ? isFolder
                ? folderCheckState(node, checked)
                : checked.has(node.path)
                  ? "checked"
                  : "unchecked"
              : null
          }
          onCheck={onCheck}
          onToggle={toggleFolder}
          onOpen={openDiff}
        />,
      ];
      if (isFolder && open) rows.push(...renderNodes(node.children, depth + 1));
      return rows;
    });

  const hasVisibleRows = revisionedTree.length > 0 || unrevisionedTree.length > 0;

  /**
   * CHG-FR-23 / SNV-FR-60: the active mode's comparison yields no change at
   * all. The first-class empty state — the centred block, carrying no action,
   * because what the author does next is edit something rather than anything
   * this panel offers.
   */
  const emptyChangeSet =
    !error && changeSet != null && changeSet.entries.length === 0;
  /**
   * SNV-FR-61: the comparison yields changes and the filters admitted none of
   * them. A different state, staying in the tree's own region with every
   * control still present — one says nothing has changed, the other says the
   * lens and the text in the author's hands are hiding what has.
   */
  const filteredToNothing =
    !error && changeSet != null && changeSet.entries.length > 0 && !hasVisibleRows;

  /**
   * CHG-FR-09: the tree's top level is exactly these two groups, **Revisioned**
   * first. Each is a node like any other for the cascade's purposes, so it
   * carries the same tri-state its folders do and one tick takes everything
   * visible beneath it (CHG-FR-28).
   */
  const groups: { node: ChangeNode; icon: React.ReactNode }[] = [
    {
      node: {
        key: "revisioned",
        name: "Revisioned",
        path: "Revisioned",
        kind: "folder",
        children: revisionedTree,
      },
      icon: <Icon.Commit size={13} />,
    },
    {
      node: {
        key: "unrevisioned",
        name: "Unrevisioned",
        path: "Unrevisioned",
        kind: "folder",
        children: unrevisionedTree,
      },
      icon: <Icon.Diamond size={13} />,
    },
  ];

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      <div className="panel-header">
        <span className="panel-header__title">Changes</span>
        <div className="panel-header__actions">
          {/* CHG-FR-23: manual refresh. */}
          <button
            className="btn btn--ghost btn--icon btn--sm"
            title="Refresh changes"
            aria-label="Refresh changes"
            // CHG-FR-65: the report clears on the next rollback and on a manual
            // refresh. Without this a stale failure would outlive the state it
            // described — the author fixes the file by hand, refreshes, and is
            // still told it could not be rolled back.
            onClick={() => {
              setRollbackFailures([]);
              void load();
            }}
          >
            <Icon.History size={12} />
          </button>
        </div>
      </div>

      {/* SNV-FR-58: the text filter first, directly beneath the panel header,
          then the selectors beneath it broadest first — the mode toggle, then
          the artifact-type lens directly above the tree it narrows.

          SNV-FR-60 takes the stack away outside a repository, where there is no
          comparison to select and nothing to narrow (CHG-FR-22). Inside one,
          the mode toggle and its branch picker survive an empty change set:
          they choose *which* comparison is computed rather than narrowing the
          one on screen, and a clean working tree is exactly when the author
          reaches for Branch mode. The two controls that do narrow — the text
          filter and the lens — go, per SNV-FR-60. */}
      {!notARepository && (
      <div className="panel-controls">
        {!emptyChangeSet && (
          <div className="search-input" style={{ height: 24 }}>
            <Icon.Search size={12} />
            <input
              placeholder="Filter changes…"
              aria-label="Filter changes"
              value={text}
              onChange={(e) => setText(e.target.value)}
            />
          </div>
        )}

        {/* CHG-FR-02: the two mutually-exclusive modes, in the one selector
            form SNV-FR-62 gives every panel. The branch picker rides beside the
            row rather than in it: it names *what* Branch mode compares against
            rather than being a position of the mode selector itself. */}
        {/* Centred as one group rather than leaving the toggle to centre
            itself: inside this row the selector is content-sized, so it is the
            wrapper that has width to distribute — and in Branch mode the
            picker beside it is part of what the author reads as one control. */}
        <div
          style={{
            display: "flex",
            gap: 4,
            alignItems: "center",
            justifyContent: "center",
            minWidth: 0,
          }}
        >
          <SelectorRow
            label="Comparison mode"
            positions={MODE_POSITIONS}
            value={mode}
            onChange={setMode}
          />
          {/* CHG-FR-05: shown in Branch mode only. */}
          {mode === "branch" && (
            <>
              <span style={{ fontSize: "var(--fs-ui-xs)", color: "var(--fg-3)" }}>
                against
              </span>
              <select
                className="select select--sm"
                aria-label="Target branch"
                value={targetBranch ?? ""}
                onChange={(e) => setTargetBranch(e.target.value)}
              >
                {/* A persisted branch that no longer exists is still shown as
                    the current value, so the picker never silently retargets
                    the comparison (CHG-FR-27). */}
                {targetBranch != null &&
                  !branches.some((b) => b.name === targetBranch) && (
                    <option value={targetBranch}>{targetBranch}</option>
                  )}
                {branches.map((b) => (
                  <option key={b.name} value={b.name}>
                    {b.name}
                    {b.isDefault ? " (default)" : ""}
                  </option>
                ))}
              </select>
            </>
          )}
        </div>

        {/* CHG-FR-14: the lens, on the terms LIB-FR-19 states — it offers the
            types this comparison actually touched. Absent while the change set
            is empty: SNV-FR-60 leaves nothing beside the block for it to
            narrow. */}
        {!emptyChangeSet && (
          <SelectorRow
            label="Filter by type"
            positions={lensPositions}
            value={typeFilter}
            onChange={setTypeFilter}
          />
        )}
      </div>
      )}

      <div
        ref={treeRef}
        className={
          notARepository || emptyChangeSet
            ? EMPTY_BODY_CLASS
            : filteredToNothing
              ? FILTERED_BODY_CLASS
              : "vpanel__body"
        }
      >
        {/* CHG-FR-22 / SNV-FR-60: not a Git repository — explanatory, not a
            failure, and carrying no action, because there is nothing here to
            commit to (CHG-FR-47). */}
        {notARepository && (
          <PanelEmptyState line="Not a Git repository.">
            This project isn’t tracked by Git, so there are no changes to show
            and nothing to commit.
          </PanelEmptyState>
        )}
        {/* CHG-FR-24: the configured target branch no longer resolves. An
            inline error rather than an empty state — the picker above stays
            usable and nothing has been retargeted. */}
        {!notARepository && unknownBranch && (
          <div className="changes-state" data-state="unknown-branch">
            Branch “{targetBranch}” no longer exists. Pick another branch to
            compare against.
          </div>
        )}
        {!notARepository && !unknownBranch && error && (
          <div className="changes-state" data-state="error">
            {error}
          </div>
        )}
        {/* CHG-FR-23 / SNV-FR-60: the comparison yields no change. Composed and
            placed exactly as the not-a-repository block above, and saying a
            different thing — the two are told apart by what they state, because
            an author who cannot commit needs to know which of the two
            situations they are in. */}
        {emptyChangeSet && (
          <PanelEmptyState line="No changes.">
            {mode === "branch"
              ? `Nothing differs between this worktree and ${targetBranch ?? "the target branch"}.`
              : "This worktree matches its last commit — nothing has been edited since."}
          </PanelEmptyState>
        )}
        {/* CHG-FR-23 / SNV-FR-61: the change set holds entries and the filters
            admitted none of them. Not the block above: it stays in the tree's
            own region with every control still present, because what the author
            changes next is the filter in front of them. */}
        {filteredToNothing && (
          <PanelFilteredState>
            No changes match the current filters.
          </PanelFilteredState>
        )}

        {/* CHG-FR-09: the two top-level groups, each carrying its own folder
            nesting, **Revisioned** first. A group holding a visible file always
            renders its own label — the tree has no flattened form in which
            changed files sit at the top level — and CHG-FR-16 hides a group
            entirely when nothing inside it is currently visible. */}
        {groups.map(({ node, icon }) => {
          if (node.children.length === 0) return null;
          const open = !collapsed.has(node.key);
          return (
            <Fragment key={node.key}>
              <div
                className="tree-row tree-row--group"
                style={{ paddingLeft: 4 }}
                data-selected={selectedKey === node.key}
                onClick={() => toggleFolder(node)}
              >
                {showChecks && (
                  <CheckBox
                    state={folderCheckState(node, checked)}
                    label={`Include ${node.name}`}
                    onChange={(next) => onCheck(node, next)}
                  />
                )}
                <span className="tree-row__caret">
                  {open ? <Icon.Caret size={12} /> : <Icon.CaretRight size={12} />}
                </span>
                <span className="tree-row__icon">{icon}</span>
                <span className="tree-row__name tree-row__name--counted">
                  {node.name}
                </span>
                {/* CHG-FR-50: a group states the total of everything it holds,
                    so **Revisioned** answers how much of this session's work
                    Git already tracks before either spine is opened. */}
                <NodeCount node={node} />
              </div>
              {open && renderNodes(node.children, 1)}
            </Fragment>
          );
        })}
      </div>

      {/* CHG-FR-43 / CHG-FR-46: what the panel says about a push. The output
          itself is wide and line-shaped and renders in the Git panel's push/pull
          output area, not here. */}
      {note && (
        <div className="changes-note" data-tone={note.tone} role="status">
          {note.text}
          {/* CHG-FR-45: no token is stored at all, so there is nothing to choose
              between and the note routes to where one is added (GHA-FR-19). */}
          {noteRoutesToSettings && onOpenGlobalSettings && (
            <>
              {" "}
              <button
                className="btn btn--ghost btn--sm"
                onClick={() => onOpenGlobalSettings()}
              >
                Open Global settings → GitHub
              </button>
            </>
          )}
        </div>
      )}

      {/* CHG-FR-65: what a rollback could not reach, above the count row. No
          success state is shown alongside it — a rollback in which any path
          failed is not a rollback that succeeded. */}
      {!notARepository && rollbackFailures.length > 0 && (
        <div className="changes-rollback-failures" role="status">
          <div className="changes-rollback-failures__summary">
            <Icon.Warning size={12} />
            {rollbackFailures.length}{" "}
            {rollbackFailures.length === 1 ? "file" : "files"} could not be
            rolled back:
          </div>
          <ul className="changes-rollback-failures__list">
            {rollbackFailures.map((f) => (
              <li key={`${f.path}:${f.kind}`}>
                <span className="changes-rollback-failures__path">{f.path}</span>
                <span className="changes-rollback-failures__cause">
                  {rollbackFailureText(f.kind)}
                </span>
              </li>
            ))}
          </ul>
        </div>
      )}

      {/* CHG-FR-55: how many files are selected, on a row of its own between
          the tree and the footer. Uncommitted mode alone — Branch mode has no
          checkbox to count — and rendered whether the number is zero or not, so
          the row does not appear and disappear beneath the tree as ticks
          change. */}
      {!notARepository && showChecks && (
        <div className="changes-selected-count">
          {commitSet.length} {commitSet.length === 1 ? "file" : "files"} selected
        </div>
      )}

      {/* CHG-FR-33 / CHG-FR-47: the footer's rollback button and split control.
          Absent entirely outside a repository, because there is nothing to
          commit to. */}
      {!notARepository && (
        <div className="changes-actions">
          {/* CHG-FR-56: the rollback button holds the leading edge, in
              Uncommitted mode of a Git repository and nowhere else. Because the
              split control holds the trailing edge regardless, a mode that
              renders no rollback button moves nothing in the footer. */}
          {showChecks && (
            <button
              className="btn btn--sm btn--icon changes-actions__rollback"
              // CHG-FR-58: the icon is never the only indication of the action —
              // the accessible name and the tooltip both say it in words.
              aria-label="Discard selected changes"
              title={
                rollbackUnavailableReason() ??
                "Discard selected changes, returning these files to HEAD"
              }
              disabled={rollingBack || rollbackUnavailableReason() !== null}
              data-busy={preparing || undefined}
              onClick={() => void runRollback()}
            >
              <Icon.Rollback size={14} />
              {/* CHG-FR-60: the preparation waits on somebody else's in-flight
                  save, which is otherwise indistinguishable from nothing
                  happening. */}
              {preparing && (
                <span className="changes-actions__rollback-progress">
                  Discarding…
                </span>
              )}
            </button>
          )}
          {/* CHG-FR-33 / CHG-FR-35 / CHG-FR-49: the split control, which is
              the shared one the graduation publication choice also uses — one
              object whose two halves dim together when the selected action
              cannot be performed, whose dropdown stays operable while they do,
              and whose inactive entry carries its reason on hover.

              Commit and Commit & Push stay selectable while nothing is ticked,
              because ticking is how the author makes them available. Push does
              not: nothing done in this panel makes a branch that is level with
              its remote pushable, so it renders inactive with the reason rather
              than disappearing. */}
          <SplitAction<ChangesCommitAction>
            className="changes-actions__split"
            value={action}
            options={ACTIONS.map((value) => ({
              value,
              label: ACTION_LABELS[value],
              unavailable:
                value === "push" ? pushUnavailableReason(sync, pushRunning) : null,
            }))}
            onChange={selectAction}
            onActivate={() => void runPrimary()}
            available={primaryEnabled}
            // CHG-FR-60: inert while a rollback is being confirmed, prepared, or
            // executed. The dropdown is otherwise deliberately operable even
            // when the selected action cannot be performed (CHG-FR-49), so this
            // is the one condition that closes it — changing the commit action
            // mid-rollback would act on a set that is moving.
            primaryDisabled={!primaryEnabled || rollingBack}
            menuDisabled={rollingBack}
            primaryLabel={busy ? "Working…" : undefined}
            primaryTitle={primaryReason}
            menuLabel="Commit action"
          />
        </div>
      )}
    </div>
  );
}
