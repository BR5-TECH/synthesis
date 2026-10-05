/**
 * The Diff tab (`specifications/ui/DFV-diff-viewer.md`).
 *
 * One file, one comparison, read three ways. A toolbar under the tab header
 * holds two toggle groups (DFV-FR-07): **visualization** — Unified, Side-by-side,
 * Final (DFV-FR-08) — and **rendering** — Source or Rich (DFV-FR-16). Both are
 * one user-global choice rather than a property of this tab, so activating a
 * toggle here re-renders every open Diff tab (DFV-FR-24); the store behind
 * `useDiffModes` is what makes that true.
 *
 * The **target** revision is editable in place (DFV-FR-41), and it is not this
 * tab's own: it is the artifact's editing session (DFV-FR-42), the same buffer,
 * dirty state, undo history and write schedule an Editor tab on that file edits.
 * So the tab presents no Save control and runs no save mechanism of its own, and
 * the only bytes it writes are the target's (DFV-FR-06) — it stages nothing,
 * commits nothing, and reaches the repository no further.
 *
 * The **original** is the fixed thing the comparison is against. It is fetched
 * once and re-fetched on `"changes updated"` so the tab never keeps comparing
 * against a base that has moved on (DFV-FR-26); no fetch ever supplies the
 * target, because an edited target is not on disk for anything to read.
 *
 * The comparison itself is derived **here**, from those two, and re-derived as
 * the target changes (DFV-FR-43) — for the same reason `DCR-draft-change-review`
 * derives its own: no operation anywhere can diff a revision that only exists in
 * a buffer.
 */
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import * as api from "../../api";
import { onChangesUpdated } from "../../events";
import { ERR_NOT_A_REPOSITORY } from "../../comparison";
import { hydrateDiffModes, useDiffModes } from "../../state/diffModes";
import { ActionControl } from "../ActionControl";
import { useDiscussionControl } from "../../hooks/useDiscussionControl";
import { useArtifactDocument } from "../../hooks/useArtifactDocument";
import { useExternalChanges } from "../../hooks/useExternalChanges";
import { useHistoryAccelerators } from "../../hooks/useHistoryAccelerators";
import { recordEdit, redoStep, undoStep } from "../../state/editHistory";
import type { EditSessionStore } from "../../state/editSessions";
import { hunksBetween } from "../../diff/localHunks";
import { replaceLineRange } from "../../diff/targetEdit";
import { logError, logInfo } from "../../logging";
import type { TargetEditing } from "../DiffTarget";
import type {
  DiscussionTarget,
  DiffRenderingMode,
  DiffTarget,
  DiffVisualizationMode,
  FileRevisions,
} from "../../types";
import { useSettledText } from "./settle";
import { DiffComparisonBody, DiffModeToolbar } from "./comparison";

export { DERIVE_SETTLE_MS } from "./settle";
export { DiffComparisonBody, DiffModeToolbar } from "./comparison";

interface DiffViewProps {
  target: DiffTarget;
  /**
   * EDT-FR-28 / DFV-FR-42: the shell-owned store the **target** lives in. The
   * tab renders and edits the artifact's editing session rather than a copy of
   * it, so it needs the same store an Editor tab on the file binds to — that is
   * what makes the two surfaces one buffer, one dirty state, one undo history
   * and one write.
   */
  sessions: EditSessionStore;
}

/** DFV-FR-17: Markdown is one of the two shapes rich rendering has a reading of. */
export function isMarkdownPath(path: string): boolean {
  return /\.(md|markdown|mdown|mkd)$/i.test(path);
}

/**
 * DFV-FR-33: the other is a Flow, whose rich reading is its graph rather than
 * its text.
 *
 * Decided by the file's resolved artifact type (ASC-FR-06) rather than by its
 * name: `.flow` is only the path convention, and a user's explicit assignment
 * overrides it — a Flow the author named something else is still a Flow, and a
 * `.flow` file they assigned another type is not.
 */
export function isFlow(target: DiffTarget): boolean {
  return target.artifactType === "flow";
}

export function DiffView({ target, sessions }: DiffViewProps) {
  const modes = useDiffModes();

  /**
   * DFV-FR-39 / ACT-FR-02: the item is the file at its current project path, not
   * the revision the tab renders.
   */
  const discussionTarget = useMemo<DiscussionTarget>(
    () => ({ kind: "artifact", artifactId: target.path }),
    [target.path],
  );
  const control = useDiscussionControl(discussionTarget);

  /**
   * DFV-FR-25 / DFV-FR-43: the **original** revision, and the facts the new side
   * carries about whether the comparison adds or deletes the file and whether it
   * is binary.
   *
   * The target does not come from here. It is the artifact's own editing session
   * (DFV-FR-42), and the tab derives the whole comparison itself from the two
   * (DFV-FR-43) — which it has to, because an edited target is not on disk for
   * anything else to diff.
   */
  const [revisions, setRevisions] = useState<FileRevisions | null>(null);
  const [error, setError] = useState<string | null>(null);
  /** DFV-FR-54: a failure to create the file the restore affordance names. */
  const [restoreError, setRestoreError] = useState<string | null>(null);

  // The loader keys on the scope's *value*, not its identity: an owner that
  // rebuilds the target object each render would otherwise re-fetch on every
  // render. The ref carries the live scope through so the value key is the only
  // dependency.
  const scopeKey = JSON.stringify(target.scope);
  const scopeRef = useRef(target.scope);
  scopeRef.current = target.scope;

  // Two `changes updated` events in quick succession put two fetches in flight;
  // only the newest may commit, or the tab settles on the older original — the
  // staleness DFV-FR-26 exists to prevent.
  const revisionSeq = useRef(0);

  const loadRevisions = useCallback(async () => {
    const ticket = ++revisionSeq.current;
    try {
      const next = await api.getFileRevisions(scopeRef.current);
      if (ticket !== revisionSeq.current) return;
      setRevisions(next);
      setError(null);
    } catch (e) {
      if (ticket !== revisionSeq.current) return;
      setRevisions(null);
      setError(String(e));
    }
  }, [scopeKey]);

  // DFV-FR-23: the stored modes, read once for the whole app however many tabs
  // mount at once.
  useEffect(() => {
    void hydrateDiffModes();
  }, []);

  useEffect(() => {
    setRevisions(null);
    setError(null);
    void loadRevisions();
  }, [loadRevisions]);

  /**
   * DFV-FR-26: re-fetch the **original** when the change set moves, discarding
   * the one held so the tab never keeps comparing against a base that has moved
   * on.
   *
   * It takes no target from that fetch. The target is the editing session's, and
   * a working-tree change that reached it while the author's edits stood is
   * answered by the external-change resolution of DFV-FR-53 rather than by a
   * re-fetch replacing the buffer. The active modes, the buffer, its undo
   * history, and the scroll position are all unaffected.
   */
  useEffect(() => {
    let unlisten: (() => void) | undefined;
    let cancelled = false;
    void onChangesUpdated(() => {
      void loadRevisions();
    }).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [loadRevisions]);

  const isBinary = revisions?.isBinary ?? false;
  const flow = isFlow(target);
  const markdown = isMarkdownPath(target.path);
  // DFV-FR-18: the toggles stay in the toolbar and render disabled; the stored
  // rendering mode is not written by the disablement, so a file rich rendering
  // does have a reading of, opened afterwards, still renders rich if that is
  // the stored choice.
  const richApplies = (markdown || flow) && !isBinary;
  const rendering: DiffRenderingMode =
    richApplies && modes.rendering === "rich" ? "rich" : "source";

  // DFV-FR-15: the comparison deletes the file outright, so there is no target
  // to hold an editing session on until the author restores it (DFV-FR-54).
  const deleted = revisions != null && revisions.new == null;
  // DFV-FR-27 / DFV-FR-48: a binary file has no textual target, and a Flow's
  // rich reading is a reading of two graphs rather than a document.
  const hasTarget = revisions != null && !deleted && !isBinary;

  /**
   * DFV-FR-54: restore the file the comparison deleted, through the one
   * operation that creates one (`PST-project-storage.md` PST-FR-25). The tab
   * then holds an editing session on the recreated file whose target opens empty
   * and editable, the original unchanged and the comparison re-derived against
   * it.
   */
  const restore = useCallback(async () => {
    const slash = target.path.lastIndexOf("/");
    const location = slash < 0 ? "" : target.path.slice(0, slash);
    const name = slash < 0 ? target.path : target.path.slice(slash + 1);
    setRestoreError(null);
    logInfo(["frontend"], "restoring a file a comparison deleted", {
      path: target.path,
    });
    try {
      await api.createFile({ location: location === "" ? null : location, name });
      // The comparison is re-derived against the same original: only the target
      // side of it has changed.
      await loadRevisions();
    } catch (e) {
      setRestoreError(String(e));
      logError(["frontend"], "could not restore a deleted file", {
        path: target.path,
        reason: String(e),
      });
    }
  }, [target.path, loadRevisions]);

  return (
    <div className="diff-view">
      <div className="panel-header">
        {/* DFV-FR-03 / SNV-FR-57: a file name is content, so it reads in the
            case it has on disk. `.panel-header__title` is the eyebrow treatment
            panels use for their own NAME, which is a label — the modifier turns
            that treatment off for the one header whose title is a filename. */}
        <span className="panel-header__title panel-header__title--file">
          {target.name}
        </span>
        <span className="diff-view__path">{target.path}</span>
        {/* DFV-FR-03: the comparison is named in the tab's own header. */}
        <span className="diff-view__comparison">{target.comparisonLabel}</span>
      </div>

      {/* DFV-FR-42: the target is the artifact's editing session, so everything
          that reads or writes it is mounted only where there is one to bind to.
          A deleted, binary, or unread comparison renders the states below and
          establishes no session — opening a Diff tab must not create an editing
          session for a file that is not there. */}
      {hasTarget ? (
        <DiffTargetView
          target={target}
          sessions={sessions}
          revisions={revisions}
          visualization={modes.visualization}
          rendering={rendering}
          richApplies={richApplies}
          flow={flow}
        />
      ) : (
        <>
          <DiffModeToolbar
            visualization={modes.visualization}
            rendering={rendering}
            richApplies={richApplies}
          />
          <div className="diff-view__body" data-page="off">
            <DiffComparisonBody
              // DFV-FR-15: a comparison with no target still has an original,
              // and Unified and Side-by-side render the deletion as they render
              // any change — every line removed. Only Final has nothing to show
              // in place of an outcome, and it shows the restore affordance.
              payload={
                revisions == null || isBinary
                  ? revisions == null
                    ? null
                    : { hunks: [], isBinary: true }
                  : hunksBetween(revisions.old, revisions.new)
              }
              revisions={revisions}
              visualization={modes.visualization}
              rendering={rendering}
              flow={flow}
              isBinary={isBinary}
              /* DFV-FR-57: the file whose language the Source rendering reads
                 in — the path, so the extension map sees the same name the
                 Editor's own surface does. */
              fileName={target.path}
              error={
                error == null
                  ? null
                  : error.includes(ERR_NOT_A_REPOSITORY)
                    ? "This project isn’t a Git repository, so there is no diff to show."
                    : error
              }
              awaiting={revisions == null && error == null}
              deletedState={
                <DeletedTargetState
                  path={target.path}
                  onRestore={restore}
                  error={restoreError}
                />
              }
            />
          </div>
        </>
      )}

      {/* DFV-FR-39 / DFV-FR-40: the one control every tab carries, bound to the
          file being compared at its **current** identity in the project rather
          than to either revision on screen — so a conversation begun while
          reading a change is the same conversation the Editor tab on that file
          shows. It offers Discuss alone whatever the file's type, this tab being
          one reading of one comparison of a file (ACT-FR-04), and what it writes
          is a comment log and never a byte of the material under comparison. */}
      <ActionControl
        discussions={control.discussions}
        readOnly
        itemNoun="file"
        /* DFV-FR-40: a file the comparison shows as deleted has nothing left to
           talk about. */
        missing={deleted}
      />
    </div>
  );
}

/**
 * DFV-FR-15 / DFV-FR-54: the deleted state, and the one affordance that turns
 * an outcome which is nothing at all into something to type into.
 *
 * It states what it will do before it does it, because creating a file is a
 * change to the project and the author has so far only asked to read a diff.
 */
function DeletedTargetState({
  path,
  onRestore,
  error,
}: {
  path: string;
  onRestore: () => void;
  error: string | null;
}) {
  return (
    <div className="changes-state" data-state="deleted">
      <p>This file does not exist in the new revision.</p>
      <button type="button" className="btn btn--sm" onClick={onRestore}>
        Restore {path} as an empty file
      </button>
      {error != null && (
        <p className="changes-state__error" role="alert">
          {error}
        </p>
      )}
    </div>
  );
}

/**
 * The half of the tab that exists only where there is a target to edit.
 *
 * Split from `DiffView` because binding the artifact's editing session is what
 * *establishes* it (DFV-FR-25, EDT-FR-28): a comparison that deletes the file,
 * or one that has not been read yet, must not create a session for a document
 * that is not there — and a hook cannot be called conditionally, so the
 * condition is a component boundary instead.
 */
function DiffTargetView({
  target,
  sessions,
  revisions,
  visualization,
  rendering,
  richApplies,
  flow,
}: {
  target: DiffTarget;
  sessions: EditSessionStore;
  revisions: FileRevisions;
  visualization: DiffVisualizationMode;
  rendering: DiffRenderingMode;
  richApplies: boolean;
  flow: boolean;
}) {
  // EXC-FR-QCNO / DFV-FR-53: the store's divergence watch stays alive while this
  // tab is up, so a target rewritten outside the application raises the same
  // blocking modal here that it raises on an Editor tab.
  useExternalChanges(sessions);
  const doc = useArtifactDocument(target.path, sessions);
  const session = doc.session;

  /**
   * DFV-FR-43: the comparison, re-derived from the original the tab holds and
   * the target as it currently stands.
   *
   * Settled rather than run per keystroke (DFV non-functional requirements): a
   * burst of typing re-marks once at the end of it rather than once per
   * character, and the caret never moves as a consequence.
   */
  /**
   * Bumped by every edit this tab makes.
   *
   * The session store deliberately does **not** notify on a buffer change: it
   * happens on every keystroke and an Editor tab reads the buffer through
   * uncontrolled DOM, so notifying would cost a re-render per character for
   * nothing. Only the clean→dirty transition notifies — which means that after
   * the first edit, nothing at all would tell this tab its target had moved,
   * and the comparison would stop re-deriving (DFV-FR-43) with every row left
   * addressing the line it occupied before.
   *
   * The re-render this forces is cheap; what is expensive is the derivation,
   * and that is still settled (`useSettledText`).
   */
  const [editEpoch, setEditEpoch] = useState(0);
  /**
   * Bumped whenever the target is replaced wholesale rather than edited — which
   * is the one thing a rich run cannot detect for itself (see
   * `TargetEditing.seed`).
   */
  const [seedEpoch, setSeedEpoch] = useState(0);
  /**
   * DFV-FR-43 / DFV-FR-47: the derivation is **held** while the rich target has
   * the caret.
   *
   * Rich editing is one surface over a run of blocks, and re-deriving under it
   * re-groups those runs — a paragraph the author has just merged into the one
   * above leaves the original's block with no counterpart, which puts a removed
   * block through the middle of the run and splits the document they are typing
   * into in two. The marking does not go stale in the meantime: it is carried as
   * decorations that map through their own edits. Releasing on blur re-derives
   * at once, so the comparison is never more than a caret's departure behind.
   */
  const [held, setHeld] = useState(false);
  const { settled, adopt } = useSettledText(
    session.buffer,
    session.seedToken,
    editEpoch,
    held,
  );

  const derived = useMemo(() => {
    const original = revisions.old;
    return {
      payload: hunksBetween(original, settled),
      // The two revisions the modes render from. The **original** is the
      // revision the comparison captured and stays exactly that: an edit to the
      // target re-derives what is shown *against* it and never re-bases onto it.
      pair: { old: original, new: settled, isBinary: false } as FileRevisions,
    };
  }, [revisions.old, settled]);

  /**
   * DFV-FR-41 / DFV-FR-42 / DFV-FR-50: the target's editing surface.
   *
   * `replaceLines` writes straight into the artifact's editing session — the
   * same buffer, dirty state, undo history and write schedule an Editor tab on
   * this file edits (EDT-FR-28, EDT-FR-72) — so the tab presents no Save control
   * and runs no save mechanism of its own.
   *
   * DFV-FR-48: a Flow's rich reading is read-only, and DFV-FR-53 makes the
   * surface inert while an external change is unresolved.
   */
  const richFlow = flow && rendering === "rich";
  const editing = useMemo<TargetEditing | undefined>(() => {
    if (richFlow) return undefined;
    return {
      label: `${target.name} — target revision (editable)`,
      seed: seedEpoch + session.seedToken,
      // Read at the moment it is needed. `session` is a stable object whose
      // `buffer` field is mutated in place, so a snapshot captured here would
      // never be recomputed by any amount of typing.
      currentText: () => session.buffer,
      disabled: doc.conflict,
      replaceLines: (from, to, replacement) => {
        const next = replaceLineRange(session.buffer, from, to, replacement);
        if (next === session.buffer) return;
        session.buffer = next;
        setEditEpoch((n) => n + 1);
        // EDT-FR-22: one history for the artifact, whichever surface produced
        // the step, so an undo here reverses an edit made in the Editor tab and
        // the other way round.
        recordEdit(session.history, next, session.mode, "source");
        // EDT-FR-70: marks dirty and restarts the rest after which the artifact
        // writes itself. There is no second schedule here to disagree with it.
        sessions.noteEdit(target.path);
      },
    };
  }, [
    richFlow,
    target.name,
    target.path,
    session,
    sessions,
    doc.conflict,
    seedEpoch,
  ]);

  const richPage = rendering === "rich" && !flow;

  /**
   * DFV-FR-50 / EDT-FR-22: undo and redo act on the **artifact's single
   * history**, so one undo reverses the last edit whichever surface produced it.
   *
   * Claimed on the tab's own root and in the capture phase, exactly as the
   * Editor claims it (EDT-FR-15): the contenteditable rows would otherwise each
   * run their own surface-local stack first, and a row-local undo can only ever
   * disagree with the artifact's. Both routes are intercepted — the accelerators
   * as keydown, and the native Edit menu's Undo/Redo, which reach the webview as
   * a `beforeinput` carrying `historyUndo` / `historyRedo`.
   *
   * DFV-FR-53: inert while the external-change modal is unresolved, like every
   * other edit operation on the tab.
   */
  const rootRef = useRef<HTMLDivElement>(null);
  const traverse = useCallback(
    (direction: "undo" | "redo") => {
      if (doc.conflict) return;
      const move =
        direction === "undo"
          ? undoStep(session.history)
          : redoStep(session.history);
      if (!move) return;
      session.buffer = move.doc;
      setEditEpoch((n) => n + 1);
      // A traversal replaces the whole target rather than editing it, so the
      // rich surfaces have to be re-seeded from it: the derivation is adopted
      // now even if the caret is still in one of them, and from the document
      // that came back rather than from the one the last render saw.
      adopt(move.doc);
      setSeedEpoch((n) => n + 1);
      // The document that came back is dirty until it is written: even a
      // traversal all the way back to the loaded bytes leaves the indicator
      // standing, exactly as it does in an Editor tab.
      sessions.noteEdit(target.path);
    },
    [adopt, doc.conflict, session, sessions, target.path],
  );
  useHistoryAccelerators(rootRef, traverse);

  // A mode switch unmounts whatever held the caret, and a hold that outlived its
  // surface would freeze the comparison for good (DFV-FR-52). Only a *change*
  // does this: adopting on mount would queue a state update carrying the buffer
  // as it stood before the load, which React applies after the load's own — and
  // the tab would settle back onto an empty target it had already left behind.
  const richTarget = rendering === "rich" && !flow && editing != null;
  const modeRef = useRef(`${visualization}/${rendering}`);
  useEffect(() => {
    const now = `${visualization}/${rendering}`;
    if (modeRef.current === now) return;
    modeRef.current = now;
    setHeld(false);
    adopt(session.buffer);
  }, [adopt, session, visualization, rendering]);

  return (
    <div className="diff-view__target" ref={rootRef}>
      {/* DFV-FR-07: one toolbar row holding both groups, centred so the two sit
          together as one cluster of controls rather than at opposite ends of the
          header — and, at its trailing end and nowhere else, the target's write
          state (DFV-FR-51). */}
      <DiffModeToolbar
        visualization={visualization}
        rendering={rendering}
        richApplies={richApplies}
        writeState={
          <TargetWriteState
            dirty={doc.dirty}
            error={doc.error}
            conflict={doc.conflict}
          />
        }
      />

      {/* DFV-FR-38: Rich renders the document on the page an Editor tab sets its
          WYSIWYG surface on, which means the body behind it is that page's
          **field** (per `EDT-editor.md` EDT-FR-63). Source takes no page — a
          gutter aligned to a bounded measure would be reading the file as a
          document it is deliberately not rendering here — so the field is
          declared only for the rendering that has a page to set on it. */}
      <div
        className="diff-view__body"
        data-page={richPage ? "on" : "off"}
        onFocusCapture={(event) => {
          if (!richTarget) return;
          if ((event.target as HTMLElement).closest?.(".diff-run")) setHeld(true);
        }}
        onBlurCapture={() => {
          if (!held) return;
          // From the buffer itself rather than from the last render's reading of
          // it: the edit that prompted this blur may not have been rendered yet.
          adopt(session.buffer);
          setHeld(false);
        }}
      >
        <DiffComparisonBody
          payload={derived.payload}
          revisions={derived.pair}
          fileName={target.path}
          visualization={visualization}
          rendering={rendering}
          flow={flow}
          isBinary={false}
          error={null}
          awaiting={!session.loaded}
          editing={editing}
          emptyTargetLabel="The new revision of this file holds nothing. Type to give it content."
        />
      </div>

      {/* DFV-FR-53: a target that changed outside the application is answered by
          the artifact's own external-change resolution and by nothing else — the
          same two explicit resolutions an Editor tab offers, raised on this tab.
          Until the author resolves it the buffer is neither reloaded nor
          overwritten and the editing surface is inert. */}
      {doc.conflict && (
        <DiffExternalChangeModal
          name={target.name}
          onLoad={() => void doc.loadFromFilesystem()}
          onKeep={doc.keepMine}
        />
      )}
    </div>
  );
}

/**
 * DFV-FR-51: what the toolbar row's trailing end says about the target's write.
 *
 * Read off the artifact's own dirty state and write outcome (EDT-FR-04,
 * EDT-FR-71) rather than tracked here, so this tab and an Editor tab on the same
 * file can never disagree about whether the file is written.
 */
function TargetWriteState({
  dirty,
  error,
  conflict,
}: {
  dirty: boolean;
  error: string | null;
  conflict: boolean;
}) {
  const state = error ? "error" : conflict ? "conflict" : dirty ? "dirty" : "saved";
  const says =
    error != null
      ? `Not saved: ${error}`
      : conflict
        ? "This file changed on disk — resolve before saving."
        : dirty
          ? "Unsaved changes"
          : "Saved";
  return (
    <span
      className="diff-toolbar__write-state"
      data-state={state}
      role="status"
      aria-live="polite"
    >
      {says}
    </span>
  );
}

/**
 * DFV-FR-53 / `EXC-editor-external-change.md` EXC-FR-VTUH – EXC-FR-WDEJ: the blocking external-change modal, raised
 * on this tab exactly as it is on an Editor tab.
 *
 * Two explicit resolutions and no third: nothing here dismisses without
 * deciding, because a dismissal would leave the divergence unresolved with the
 * surface live again and the next write would silently pick a side.
 */
function DiffExternalChangeModal({
  name,
  onLoad,
  onKeep,
}: {
  name: string;
  onLoad: () => void;
  onKeep: () => void;
}) {
  return (
    <div className="scrim" role="dialog" aria-modal="true" aria-label="File changed on disk">
      <div className="modal">
        <p>
          <strong>{name}</strong> was changed outside the application while you
          were editing it here.
        </p>
        <div className="modal__actions">
          <button type="button" className="btn" onClick={onLoad}>
            Load from filesystem
          </button>
          <button type="button" className="btn btn--primary" onClick={onKeep}>
            Keep my version
          </button>
        </div>
      </div>
    </div>
  );
}

