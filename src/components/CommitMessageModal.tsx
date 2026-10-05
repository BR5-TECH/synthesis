/**
 * The commit message window (`../specifications/ui/CMW-commit-message.md`).
 *
 * The single place a commit is authored in the application (CMW-FR-03): it
 * opens from the Changes panel's **Commit** and **Commit & Push** actions
 * carrying that panel's commit set, shows the set so the author can tell the
 * message and the change agree, and refuses to commit until a message has been
 * written (CMW-FR-05).
 *
 * It does not decide which files it commits — the set is fixed by the surface
 * that opened it (CMW-FR-11) — and it does not push; that belongs to the panel.
 */
import { useEffect, useRef, useState } from "react";
import * as api from "../api";
import { Icon } from "./icons";
import { worktreeIdentityChanged } from "../state/graduation";
import type { CommitOutcome } from "../types";

/** One entry of the commit set, as the Changes panel hands it over. */
export interface CommitFile {
  /** Project-relative path — what `commit_paths` is called with. */
  path: string;
  /** CMW-FR-06: marked in place, because it is added to VCS by the commit. */
  untracked: boolean;
}

/**
 * Render a typed commit failure as a sentence (CMW-FR-08). The causes are the
 * ones `../specifications/core/GTC-git.md` GTC-FR-20 returns; anything else is
 * shown as it arrived rather than swallowed.
 */
export function commitErrorMessage(error: unknown): string {
  const raw = String(
    error instanceof Error ? error.message : (error ?? "commit failed"),
  );
  if (raw.includes("empty_commit_message")) {
    return "A commit message is required.";
  }
  if (raw.includes("no_paths_selected")) {
    return "No files were selected for this commit.";
  }
  if (raw.includes("nothing_to_commit")) {
    return "Nothing to commit in the selected files — none of them differs from the last commit.";
  }
  // CMW-FR-08 / GTC-FR-31: the commit was aimed at one checkout and the project
  // is now in another, so nothing was written. It renders here like any other
  // refusal; what the author does about a checkout that moved under them is the
  // opening surface's to say.
  if (raw.includes("worktree_identity_changed")) {
    const moved = worktreeIdentityChanged(error);
    return moved
      ? `The checkout this commit was for (${moved.expected}) is no longer the one you are in (${moved.active}). Nothing was committed.`
      : "The checkout this commit was for is no longer the one you are in. Nothing was committed.";
  }
  if (raw.includes("not a git repository")) {
    return "This project isn’t a Git repository, so there is nothing to commit to.";
  }
  return raw;
}

/**
 * CMW-FR-03: the work stream whose merge this message is being written for.
 *
 * Present means the window carries no file set at all (CMW-FR-ZDMT) and holds
 * no in-flight state (CMW-FR-KRVP): the confirm hands the message to the
 * surface that opened the window and closes, and that surface starts the merge,
 * shows it and reports what it settles. It carries no inclusion step either —
 * there is no set for a filter to have hidden anything from (CMW-FR-WKDP).
 */
export interface StreamMergeCommit {
  /** The stream whose merge this commit message is for. */
  streamId: string;
  /** Its name, for the window's own heading. */
  streamName: string;
}

/** The three answers the inclusion step offers (CMW-FR-13). */
export type InclusionAnswer = "no" | "revisioned" | "all";

/**
 * CMW-FR-13: the file set a given answer commits — the set as it arrived, plus
 * whichever hidden files the answer takes in.
 *
 * Pure and exported so the rule is testable without driving the window: **No**
 * takes none, **All revisioned** takes the hidden files Git already tracks, and
 * **All files** takes every hidden one. Order follows the arrival order of each
 * list, so the message step's rendering is stable.
 */
export function resolveInclusion(
  files: CommitFile[],
  hidden: CommitFile[],
  answer: InclusionAnswer,
): CommitFile[] {
  if (answer === "no") return files;
  const taken =
    answer === "all" ? hidden : hidden.filter((f) => !f.untracked);
  return [...files, ...taken];
}

interface CommitMessageModalProps {
  /** CMW-FR-06: the fixed set, rendered read-only. */
  files: CommitFile[];
  /**
   * CHG-FR-48: the changed files the Changes panel's filters were hiding when
   * the window opened. Non-empty means the window opens on its inclusion step
   * (CMW-FR-13) so a commit of a Spec does not silently leave behind the code
   * and tests written from it. Empty — the ordinary case — means it opens
   * straight on the message.
   */
  hidden?: CommitFile[];
  /** CMW-FR-09: dismissal — creates no commit and invokes nothing. */
  onClose: () => void;
  /**
   * CMW-FR-07: the commit succeeded, reported with **the outcome the operation
   * returned** — the commit and the paths it recorded. The parent unmounts this
   * window and takes whatever follows (a **Commit & Push** pushes from here,
   * CHG-FR-42), and the paths are what let the strip close exactly the Diff tabs
   * the commit has finished with (`TAB-tabs.md` TAB-FR-22).
   *
   * The paths reported are the ones the commit **recorded** rather than the ones
   * this window submitted (GTC-FR-19), so a rename committed at both its
   * locations is named at both. A commit still running, abandoned, or refused
   * reports nothing at all: there is nothing yet a surface downstream should act
   * on.
   */
  onCommitted: (outcome: CommitOutcome) => void;
  /**
   * CMW-FR-03: the third surface this window opens from — a work stream's
   * merge, which wants a message and nothing else.
   */
  streamMerge?: StreamMergeCommit;
  /**
   * CMW-FR-03 / CMW-FR-07: the **worktree identity** a graduation's start
   * preflight handed this window, which it submits unread so the commit lands in
   * the checkout the author was shown or in none at all (GTC-FR-31).
   *
   * Absent from the Changes panel's route, which commits the checkout it is
   * rendering at that moment and is served exactly as before.
   */
  expectedWorktree?: string;
  /**
   * CMW-FR-KRVP: the merge route's confirm. The message is handed over and this
   * window closes at once; the merge itself is the opening surface's to start,
   * to show while it runs, and to report (per WSS-FR-HGWL).
   *
   * A merge may spend three agent turns, so a window that awaited one would hold
   * the whole application for as long as that took.
   */
  onMergeMessage?: (message: string) => void;
  /**
   * CMW-FR-09: whether a commit is in flight right now.
   *
   * Reported upward because the window cannot enforce its own undismissability
   * on its own: the overlays of the main window close their siblings when they
   * open, and a native menu accelerator reaches those openers straight past this
   * modal's scrim. Without this the parent would tear the window down mid-commit
   * and answer the surface that opened it with "nothing was committed" while the
   * commit was in fact landing.
   */
  onCommittingChange?: (committing: boolean) => void;
}

export function CommitMessageModal({
  files,
  hidden = [],
  onClose,
  onCommitted,
  streamMerge,
  expectedWorktree,
  onMergeMessage,
  onCommittingChange,
}: CommitMessageModalProps) {
  /**
   * CMW-FR-13: the answer to the inclusion step, `null` while it is unanswered.
   * Seeded already-answered when nothing was hidden, which is what makes the
   * step not exist at all in the ordinary case.
   */
  const [answer, setAnswer] = useState<InclusionAnswer | null>(
    // CMW-FR-WKDP: neither graduation route ever has the step. A merge carries
    // no file set to add to, and a start preflight already carries every
    // uncommitted path in the worktree — the one it handed over its identity
    // for. Neither has a filter over it, so nothing is hidden from either.
    hidden.length > 0 && !streamMerge && !expectedWorktree ? null : "no",
  );
  // The set the commit will carry. Settled by the answer and not revisited:
  // a set that changed under a message already written would leave the message
  // describing something else.
  const committing_set = resolveInclusion(files, hidden, answer ?? "no");
  const [message, setMessage] = useState("");
  const [error, setError] = useState<string | null>(null);
  // CMW-FR-07: the window stays mounted with its inputs and actions inert while
  // the commit runs, so nothing can be submitted twice and nothing can dismiss
  // a commit that is already under way (CMW-FR-09).
  const [committing, setCommitting] = useState(false);
  const editorRef = useRef<HTMLTextAreaElement>(null);

  // CMW-FR-09: keep the parent's view of "a commit is running" in step, and
  // clear it if this window is ever unmounted while one was.
  useEffect(() => {
    onCommittingChange?.(committing);
    return () => {
      if (committing) onCommittingChange?.(false);
    };
  }, [committing, onCommittingChange]);

  // CMW-FR-04 / CMW-FR-12: the field the author must fill takes focus as soon
  // as the message step exists, so the window is usable without a pointer from
  // the first keystroke — on open when nothing was hidden, and otherwise the
  // moment the inclusion step is answered.
  useEffect(() => {
    if (answer !== null) editorRef.current?.focus();
  }, [answer]);

  // CMW-FR-09 / CMW-FR-12: Escape dismisses — but not while a commit is in
  // flight, which nothing may abandon.
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape" && !committing) onClose();
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose, committing]);

  // CMW-FR-05: the window's only client-side validation. Whether the commit can
  // actually be created is the backend's decision (GTC-FR-20).
  const canCommit = message.trim() !== "" && !committing;

  const submit = async () => {
    if (message.trim() === "" || committing) return;
    if (streamMerge) {
      // CMW-FR-KRVP: the message is the whole of what this window owes the
      // merge. Nothing is awaited here and no in-flight state is taken, so the
      // window can always be dismissed (CMW-FR-BZHM).
      onMergeMessage?.(message);
      return;
    }
    setCommitting(true);
    setError(null);
    try {
      const outcome = await api.commitPaths(
        message,
        committing_set.map((f) => f.path),
        // CMW-FR-07: submitted unread. Where the preflight handed one over, the
        // commit lands in that checkout or in none.
        expectedWorktree,
      );
      // The parent unmounts the window on success, so nothing here restores it.
      onCommitted(outcome);
    } catch (e) {
      // CMW-FR-08: the window stays open with the message intact and the file
      // set unchanged, so the author fixes the cause and retries. A merge never
      // reaches here — its refusal is the opening surface's (WSS-FR-NPXC).
      setError(commitErrorMessage(e));
      setCommitting(false);
      // The action row was inert while the commit ran, so focus had nowhere to
      // stand and fell to the document body. "Retries without retyping" means
      // retrying from the keyboard too, so the field the author corrects in
      // takes it back.
      editorRef.current?.focus();
    }
  };

  const untrackedCount = committing_set.filter((f) => f.untracked).length;
  const hiddenTracked = hidden.filter((f) => !f.untracked).length;

  return (
    <div
      className="scrim"
      onClick={(e) => {
        // CMW-FR-09: an outside click dismisses, unless a commit is running.
        if (e.target === e.currentTarget && !committing) onClose();
      }}
    >
      <div className="modal" role="dialog" aria-labelledby="commit-message-title">
        <div className="modal__head">
          <Icon.Commit size={14} />
          <div className="modal__title" id="commit-message-title">
            {/* CMW-FR-06 / CMW-FR-ZDMT: the title states how many files the
                commit carries — except on the merge route, which carries none
                and names its stream instead of counting to zero. */}
            {answer === null
              ? "Include hidden changes?"
              : streamMerge
                ? `Merge message · ${streamMerge.streamName}`
                : `Commit ${committing_set.length} ${committing_set.length === 1 ? "file" : "files"}`}
          </div>
          <button
            className="btn btn--ghost btn--icon btn--sm"
            aria-label="Close"
            disabled={committing}
            onClick={onClose}
          >
            <Icon.X size={12} />
          </button>
        </div>

        {/* CMW-FR-13: the inclusion step. Presented before the message so the
            set is settled by the time the author describes it. */}
        {answer === null ? (
          <div className="modal__body" data-testid="commit-inclusion">
            <div className="picker-field">
              <p style={{ margin: 0, fontSize: "var(--fs-ui-sm)", lineHeight: 1.5 }}>
                {hidden.length} changed{" "}
                {hidden.length === 1 ? "file is" : "files are"} hidden by the
                panel&rsquo;s filters and{" "}
                {hidden.length === 1 ? "is" : "are"} not in this commit —
                typically the code and tests written from the artifacts you are
                committing. Include{" "}
                {hidden.length === 1 ? "it" : "them"}?
              </p>
              {/* Named in full: a count alone would ask the author to agree to
                  something they cannot see (CHG-FR-27). */}
              <ul
                className="commit-files"
                aria-label="Hidden changed files"
                data-testid="commit-hidden-files"
                style={{ marginTop: 8 }}
              >
                {hidden.map((f) => (
                  <li key={f.path} className="commit-files__row">
                    <span className="commit-files__path">{f.path}</span>
                    {f.untracked && (
                      <span
                        className="badge badge--accent"
                        title="Not yet tracked by Git"
                      >
                        new
                      </span>
                    )}
                  </li>
                ))}
              </ul>
            </div>
          </div>
        ) : (
        <div className="modal__body">
          <div className="picker-field">
            <label className="picker-field__label" htmlFor="cmw-message">
              Message
            </label>
            <textarea
              id="cmw-message"
              ref={editorRef}
              className="textarea"
              aria-label="Commit message"
              // CMW-FR-04: at least six lines of text visible.
              rows={6}
              disabled={committing}
              value={message}
              onChange={(e) => setMessage(e.target.value)}
            />
          </div>

          {/* CMW-FR-ZDMT: rendered only where the window was given a set. A
              merge lists nothing, because what it writes is settled while it
              runs (GRB-FR-ASWC). */}
          {!streamMerge && (
          <div className="picker-field">
            <div className="picker-field__label" id="cmw-files-label">
              Files
              {untrackedCount > 0 && (
                <span className="t-meta" style={{ marginLeft: 6 }}>
                  {untrackedCount} new
                </span>
              )}
            </div>
            {/* CMW-FR-06 / CMW-FR-11: read-only. Nothing here adds a path,
                removes one, or reorders the set — that was settled before the
                window opened (CHG-FR-27). Scrolls within its own frame so a
                commit of several hundred files never grows the window past the
                screen. */}
            <ul
              className="commit-files"
              aria-labelledby="cmw-files-label"
              data-testid="commit-files"
            >
              {committing_set.map((f) => (
                <li key={f.path} className="commit-files__row">
                  <span className="commit-files__path">{f.path}</span>
                  {f.untracked && (
                    <span className="badge badge--accent" title="Not yet tracked by Git">
                      new
                    </span>
                  )}
                </li>
              ))}
            </ul>
          </div>
          )}

          {/* CMW-FR-08: the failure attaches directly above the action row. */}
          {error && (
            <div
              role="alert"
              data-testid="commit-error"
              style={{ fontSize: "var(--fs-ui-sm)", color: "var(--danger, #e5484d)" }}
            >
              {error}
            </div>
          )}
        </div>

        )}

        {answer === null ? (
          <div className="modal__actions">
            {/* CMW-FR-13: exactly three answers. "All revisioned" is offered
                only when a tracked file is among the hidden ones — otherwise it
                and "All files" would be the same button twice. */}
            <button className="btn btn--ghost" onClick={() => setAnswer("no")}>
              No
            </button>
            {hiddenTracked > 0 && (
              <button
                className="btn btn--default"
                onClick={() => setAnswer("revisioned")}
              >
                All revisioned
              </button>
            )}
            <button
              className="btn btn--primary"
              onClick={() => setAnswer("all")}
            >
              All files
            </button>
          </div>
        ) : (
        <div className="modal__actions">
          <button className="btn btn--ghost" disabled={committing} onClick={onClose}>
            Cancel
          </button>
          <button
            className="btn btn--primary"
            disabled={!canCommit}
            onClick={() => void submit()}
          >
            <Icon.Commit size={12} />{" "}
            {streamMerge ? "Merge" : committing ? "Committing…" : "Commit"}
          </button>
        </div>
        )}
      </div>
    </div>
  );
}
