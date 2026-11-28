/**
 * The Changes vertical panel (`specifications/ui/CHG-changes.md`).
 *
 * Renders the project's changed files as the same folder-mirroring tree the
 * Project panel uses, tagged by artifact type and filtered by the same lenses, with a
 * `+added −removed` summary on every row and a checkbox on every row deciding
 * what the next commit carries. The tree's top level is two groups —
 * **Revisioned** then **Unrevisioned** (CHG-FR-09) — so one tick takes the whole
 * of either. With more than one repository — a project with git submodules —
 * one repository group for each repository holds those two groups
 * (CHG-FR-BYWJ), and a submodule's pointer is a row of its parent
 * (CHG-FR-GBTE). Two mutually-exclusive modes serve two
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
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import * as api from "../api";
import { onChangesUpdated, onGitOperationFinished } from "../events";
import { tokenErrorMessage } from "./GithubTokens";
// One implementation of "is there anything to push", shared with the Git panel
// so the two surfaces can never disagree about it (CHG-FR-37).
import {
  canPush,
  canStartPush,
  pushUnavailableReason,
  runningReason,
} from "../gitSync";
import { useGitTransfer } from "../hooks/useGitTransfer";
import { ChangesFooter } from "./Changes/ChangesFooter";
import { ChangesControls } from "./Changes/ChangesControls";
import { ChangesNotes, type ChangesNote } from "./Changes/ChangesNotes";
import { ChangeTreeBody } from "./Changes/RepositoryGroup";
import {
  buildRepositoryGroups,
  commitFileOf,
  impliedPointerPaths,
  isMultiRepository,
  isPointer,
  pushSkipLines,
  repositoriesOf,
  revealKeys,
  toggleableFilesUnder,
} from "./Changes/groups";
import { SAVE_FAILED_KIND } from "./Changes/rollbackText";
import {
  entryVisible,
  partitionEntries,
  type ChangeNode,
} from "./Changes/tree";
import type { CommitFile } from "./CommitMessageModal";
import type { RollbackFile } from "./RollbackConfirm";
import type { ChangesProps } from "./Changes/props";
import { useChangesPanelState } from "./Changes/usePanelState";
import { logWarn } from "../logging";
import {
  DEFAULT_TYPE_LENS,
  matchesLens,
  matchesText,
  presentTypes,
  typeLensPositions,
  type TypeLens,
} from "../artifactTypes";
import { Icon } from "./icons";
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
import { coalescedRun } from "../coalescedRun";
import type { BranchOption, ChangeSet } from "../types";

export { canPush };
export {
  buildChangeTree,
  commitSetOf,
  entryVisible,
  folderCheckState,
  partitionEntries,
  visibleFileCount,
  visibleFilesUnder,
} from "./Changes/tree";
export type { ChangeNode, CheckState } from "./Changes/tree";
export type { ChangesProps } from "./Changes/props";
export { SAVE_FAILED_KIND, rollbackFailureText } from "./Changes/rollbackText";

export function Changes({
  reveal,
  onChangeSetLoaded,
  onOpenDiff,
  onRequestCommitMessage,
  onRequestRollback,
  onRequestGithubToken,
  onCreatePullRequest,
  onOpenGlobalSettings,
}: ChangesProps) {
  // CHG-FR-07 / CHG-FR-34: the persisted mode, target branch, and action.
  const {
    mode,
    setMode,
    targetBranch,
    setTargetBranch,
    restored,
    action,
    selectAction,
  } = useChangesPanelState();
  const [branches, setBranches] = useState<BranchOption[]>([]);
  const [changeSet, setChangeSet] = useState<ChangeSet | null>(null);
  const [error, setError] = useState<string | null>(null);

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
  /**
   * CHG-FR-37 / CHG-FR-38 / GIT-FR-QMYB: the branch's standing against its
   * upstream and whether any push is running, read from the window's one push
   * state so this panel and the other Push controls always agree.
   */
  const transfer = useGitTransfer();
  const { sync, running: pushRunning, operation: transferOperation } = transfer;
  /** CHG-FR-44: a commit or push started here is in flight. */
  const [busy, setBusy] = useState(false);
  /**
   * CHG-FR-43 / CHG-FR-46: what the panel renders about a push — that one is
   * running, how it ended, or why it could not start. The transcript itself is
   * wide and line-shaped and belongs in the Git panel's output area, so nothing
   * here accumulates output.
   */
  const [note, setNote] = useState<ChangesNote | null>(null);
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
    // CHG-FR-RUAG: one event-driven read in flight at a time, and one more
    // after it for any burst. The ticket still drops a read that a mode change
    // or a manual refresh made stale.
    const runner = coalescedRun(load);
    void onChangesUpdated(() => {
      // CHG-FR-38: the push state re-reads the branch's standing on the same
      // event, so Push becomes available as soon as a commit lands.
      runner.run();
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      runner.dispose();
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
    // CHG-FR-GBTE: a pointer row is selected and opens no Diff tab.
    if (isPointer(node.entry)) return;
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

  // A change set of a large clone holds thousands of entries, so the panel
  // partitions and groups it once for each new change set or filter, not on
  // every render.
  const { revisioned, unrevisioned } = useMemo(
    () => partitionEntries(changeSet?.entries ?? [], typeFilter, text),
    [changeSet, typeFilter, text],
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
  /**
   * CHG-FR-BYWJ / CHG-FR-IGSD: the repository set, and whether the tree holds
   * one repository group for each repository with a visible changed file.
   */
  const repositories = useMemo(
    () => repositoriesOf(changeSet?.repositories, changeSet?.entries ?? []),
    [changeSet],
  );
  const multi = isMultiRepository(repositories);
  // Read through a ref by the push listener, which subscribes once.
  const repositoriesRef = useRef(repositories);
  repositoriesRef.current = repositories;
  const visibleEntries = useMemo(
    () => [...revisioned, ...unrevisioned],
    [revisioned, unrevisioned],
  );
  const groupModels = useMemo(
    () => buildRepositoryGroups(visibleEntries, repositories),
    [visibleEntries, repositories],
  );

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

    // Expansion here is the *removal* of collapse: this panel records which
    // nodes are collapsed rather than which are expanded, so opening an ancestor
    // means dropping its key rather than adding one. The repository group is
    // one of them when the tree holds more than one (CHG-FR-BYWJ).
    const { opening, fileKey } = revealKeys(
      entry,
      isMultiRepository(repositoriesOf(changeSet.repositories, changeSet.entries)),
    );
    setCollapsed((prev) => {
      const next = new Set([...prev].filter((k) => !opening.has(k)));
      return next.size === prev.size ? prev : next;
    });

    // CHG-FR-54: what is selected is the file row, never a folder or a group.
    setSelectedKey(fileKey);
    scrollTarget.current = fileKey;
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
  /** CHG-FR-27: the rows both checked and visible. */
  const checkedVisible = visibleEntries.filter((e) => checked.has(e.path));
  /**
   * CHG-FR-AYTR: the pointers a checked submodule file implies. Their rows
   * render checked and inert, and a visible one joins the commit set, because
   * the commit records it (GSM-FR-MPWK).
   */
  const impliedPointers = impliedPointerPaths(checkedVisible, repositories);
  const commitEntries = visibleEntries.filter(
    (e) => checked.has(e.path) || (isPointer(e) && impliedPointers.has(e.path)),
  );
  const commitSet: CommitFile[] = commitEntries.map((e) =>
    commitFileOf(e, repositories),
  );
  // CHG-FR-48: the changed files the active filters are hiding right now. The
  // default lens hides exactly the code and tests an artifact produces, so a
  // commit of the Spec alone would be a hand-off missing what it describes.
  // Only the hidden ones: a row the author can see and left unticked is a
  // decision, not an oversight. A pointer the commit records anyway
  // (CHG-FR-AYTR) is not a question to ask.
  const hiddenFromCommit: CommitFile[] = (changeSet?.entries ?? [])
    .filter((e) => !entryVisible(e, typeFilter, text))
    .filter((e) => !(isPointer(e) && impliedPointers.has(e.path)))
    .map((e) => commitFileOf(e, repositories));

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
  // CHG-FR-DGHI: a pointer row never joins the rollback set.
  const rollbackSet: RollbackFile[] = checkedVisible
    .filter((e) => !isPointer(e))
    .map((e) => ({
      path: e.path,
      previousPath: e.previousPath ?? null,
      untracked: e.changeStatus === "untracked",
      deleted: e.changeStatus === "deleted",
    }));

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
      ? pushUnavailableReason(sync, pushRunning, transferOperation)
      : action === "commit_and_push" && pushRunning
        ? runningReason(transferOperation)
        : null;

  // CHG-FR-28: checking a folder cascades over its currently-visible changed
  // descendants. A group node is one such node, so a tick on **Revisioned**
  // takes every tracked change the filters are showing and a tick on
  // **Unrevisioned** takes every new file, neither disturbing the other.
  const onCheck = (node: ChangeNode, next: boolean) => {
    // CHG-FR-60: the file rows are inert while a rollback is being confirmed,
    // prepared, or executed — the set it is acting on must not move under it.
    if (rollingBack) return;
    // CHG-FR-AYTR: an implied pointer row is inert, so a cascade passes it by.
    const paths = toggleableFilesUnder(node, impliedPointers).map((e) => e.path);
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
      // CHG-FR-NUZB: each repository the push skipped, with the reason. It is
      // part of this note, so a later note never carries it.
      const skipped = pushSkipLines(payload.pushSummary, repositoriesRef.current);
      setNote(
        payload.ok
          ? { tone: "info", text: "Push complete.", skipped }
          : {
              tone: "error",
              text: `Push failed: ${tokenErrorMessage(cause)}`,
              skipped,
            },
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
      if (outcome.cause === "cancelled") return setNote(null), false;
      // GIT-FR-QMYB: a push is running, so none starts here and the panel says why.
      if (outcome.cause === "busy") {
        setNote({ tone: "error", text: runningReason(transferOperation) });
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
    [onRequestGithubToken, transfer.pushBranch, transferOperation],
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
    // CHG-FR-DGHI: every checked visible row is a pointer row.
    if (rollbackSet.length === 0 && checkedVisible.length > 0)
      reasons.push("a submodule pointer cannot be rolled back");
    else if (rollbackSet.length === 0) reasons.push("no files are selected");
    if (typeFilter !== "files")
      reasons.push("the artifact lens must be All files");
    if (reasons.length === 0) return null;
    const why = reasons.join(", and ");
    return `Cannot discard files: ${why}.`;
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

  const hasVisibleRows = visibleEntries.length > 0;

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

  return (
    <div style={{ display: "flex", flexDirection: "column", height: "100%" }}>
      <div className="panel-header">
        <span className="panel-header__title">Commits</span>
        <div className="panel-header__actions">
          {/* CHG-FR-23: manual refresh. */}
          <button
            className="btn btn--ghost btn--icon btn--sm"
            title="Refresh"
            aria-label="Refresh"
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
        <ChangesControls
          emptyChangeSet={emptyChangeSet}
          text={text}
          onText={setText}
          mode={mode}
          onMode={setMode}
          targetBranch={targetBranch}
          onTargetBranch={setTargetBranch}
          branches={branches}
          lensPositions={lensPositions}
          typeFilter={typeFilter}
          onTypeFilter={setTypeFilter}
        />
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
          <PanelEmptyState line="Nothing to commit.">
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
            No files match the current filters.
          </PanelFilteredState>
        )}

        {/* CHG-FR-09 / CHG-FR-BYWJ: the groups of each repository, each
            carrying its own folder nesting, **Revisioned** first. A group
            holding a visible file always renders its own label — the tree has
            no flattened form in which changed files sit at the top level — and
            CHG-FR-16 hides a group entirely when nothing inside it is
            currently visible. */}
        <ChangeTreeBody
          models={groupModels}
          multi={multi}
          ctx={{
            collapsed,
            selectedKey,
            showChecks,
            checked,
            impliedPointers,
            onCheck,
            onToggle: toggleFolder,
            onOpen: openDiff,
          }}
        />
      </div>

      <ChangesNotes
        note={note}
        routesToSettings={noteRoutesToSettings}
        onOpenGlobalSettings={onOpenGlobalSettings}
        rollbackFailures={notARepository ? [] : rollbackFailures}
      />

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
        <ChangesFooter
          showRollback={showChecks}
          rollbackReason={rollbackUnavailableReason()}
          rollingBack={rollingBack}
          preparing={preparing}
          onRollback={() => void runRollback()}
          action={action}
          sync={sync}
          pushRunning={pushRunning}
          transferOperation={transferOperation}
          busy={busy}
          primaryEnabled={primaryEnabled}
          primaryReason={primaryReason}
          onSelectAction={selectAction}
          onRunPrimary={() => void runPrimary()}
          onCreatePullRequest={onCreatePullRequest}
        />
      )}
    </div>
  );
}
