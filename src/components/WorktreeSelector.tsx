import { useEffect, useMemo, useRef, useState } from "react";
import * as api from "../api";
import { onBranchesChanged, onWorktreeContextChanged } from "../events";

import { graduationErrorMessage, splitTyped } from "../state/graduation";
import { Icon } from "./icons";
import {
  GITHUB_TOKEN_ERRORS,
  GIT_REMOTE_ERRORS,
  type BranchEntry,
  type RefreshOutcome,
  type WorktreeContext,
  type WorktreeEntry,
} from "../types";

/**
 * The top-chrome worktree selector (`WTS-worktree-selector.md`).
 *
 * Sits immediately after the project switcher (SNV-FR-32) and answers "which
 * checkout am I working in?". It lists the repository's worktrees first, then —
 * only while the repository's own checkout is active — the branches that have
 * none, filters what it shows from a search field at the top, and hands the
 * chosen switch to `onSwitch` — which owns the transition itself (flush, close
 * every tab, invoke) per OVW-FR-12.
 *
 * A branch is checked out only in the primary worktree (WTS-FR-12 / WTS-FR-28,
 * per WTC-FR-21). Inside a linked worktree the Branches group is absent
 * entirely rather than rendered inert, so the control offers moving between
 * worktrees and creating new ones — both of which change which checkout the
 * project reads from — and nothing that moves a checkout onto another branch.
 *
 * Beside the selector sits the refresh control (WTS-FR-29 .. WTS-FR-36), which
 * lives in this file because what it does is inseparable from the dropdown: a
 * returning refresh re-renders the open rows in place (WTS-FR-32), its outcome
 * note moves inside the dropdown while that is open (WTS-FR-34), and the token
 * picker it can open has to close it (WTS-FR-36). It is nonetheless a *sibling*
 * of the selector, not part of it — its own button, outside the selector's
 * activation area, so pressing it never opens the dropdown (WTS-FR-30).
 *
 * A refresh is not a switch (WTS-FR-33): it flushes nothing, closes no tab, and
 * changes no active worktree, so it deliberately does NOT go through `onSwitch`.
 *
 * The component performs no Git operation and computes nothing about the
 * repository: it renders what `"list worktrees and branches"` reports.
 */

/** WTS-FR-26: rows visible in a group before its region scrolls. */
export const WORKTREE_GROUP_MAX_ROWS = 6;
/** Height of one worktree row (two lines) in px; keeps the cap in sync with CSS. */
export const WORKTREE_ROW_PX = 40;

/**
 * The outcome of a requested switch, as the caller reports it back.
 *
 * `cancelled` is a flush that could not proceed (WTS-FR-23): nothing was
 * invoked and nothing must be shown as an error — the blocking tab has already
 * been focused. `error` is a typed backend refusal (WTS-FR-24), which renders
 * inline in the surface that asked for it.
 */
export type SwitchOutcome =
  | { ok: true }
  | { ok: false; cancelled: true }
  | { ok: false; cancelled?: false; error: string };

export interface WorktreeSelectorProps {
  /**
   * The active worktree, or `null` while the project's content root is not
   * inside a Git repository — in which case nothing renders at all (WTS-FR-02).
   */
  active: WorktreeEntry | null;
  /**
   * WTS-FR-22 / OVW-FR-12: run one of the three switching operations through
   * the worktree-switch transition. The selector never invokes them directly,
   * so the flush-then-close-then-invoke ordering has exactly one implementation.
   */
  onSwitch: (
    operation: () => Promise<WorktreeContext>,
  ) => Promise<SwitchOutcome>;
  /**
   * NAW-FR-01 (single-overlay invariant): called when this dropdown opens, so
   * the other floating overlays of the main window close rather than coexist.
   */
  onOpen?: () => void;
  /**
   * WTS-FR-35 / GHA-FR-16: open the token picker because a refresh's remote leg
   * was blocked on selection, and resolve with whether a token was chosen. The
   * shell owns the picker — two surfaces ask for it and only one may exist.
   */
  onRequestGithubToken?: () => Promise<boolean>;
  /**
   * WTS-FR-35 / GHA-FR-19: the route the note offers when no token is stored at
   * all. There is nothing to choose between, so the author is sent to where one
   * is added rather than shown a picker with no rows.
   */
  onOpenGlobalSettings?: () => void;
}

/**
 * WTS-FR-34: the remote outcome, phrased by the cause the author would act on
 * differently — no remote configured, a remote that could not be reached, and a
 * credential GitHub refused. `null` when there is nothing worth reporting.
 *
 * A skipped leg is reported too: "there is no remote" is exactly the thing an
 * author who expected remote branches needs told, and staying silent would look
 * like a refresh that found nothing.
 */
export function remoteNote(outcome: RefreshOutcome): string | null {
  if (outcome.remoteState === "refreshed") return null;
  if (outcome.remoteState === "skipped") {
    return "No remote is configured — local branches are up to date.";
  }
  switch (outcome.remoteError) {
    case GITHUB_TOKEN_ERRORS.tokenMissing:
      return "Fetching from the remote needs a GitHub token.";
    case GITHUB_TOKEN_ERRORS.selectionRequired:
      return "No GitHub token was selected — local branches are up to date.";
    case GITHUB_TOKEN_ERRORS.invalidToken:
      return "The remote refused the credential — local branches are up to date.";
    case GITHUB_TOKEN_ERRORS.keychainUnavailable:
      return "That GitHub token could not be read from the keychain.";
    case GITHUB_TOKEN_ERRORS.hostMismatch:
      return "The project token belongs to another GitHub host than this remote — local branches are up to date.";
    case GITHUB_TOKEN_ERRORS.githubUnreachable:
      return "Couldn't reach the remote — local branches are up to date.";
    case GIT_REMOTE_ERRORS.noRemoteConfigured:
      return "No remote is configured — local branches are up to date.";
    default:
      // A cause outside the vocabulary above must not be dressed up as one of
      // WTS-FR-34's three: claiming the remote was unreachable when the real
      // failure was something else sends the author to check their network over
      // a problem that has nothing to do with it.
      return "The remote could not be refreshed — local branches are up to date.";
  }
}

/**
 * WTS-FR-BQCI: a switch refused because a direct graduation run is working is
 * said in words. Any other refusal is rendered as the backend gave it.
 */
export function switchRefusalText(error: string): string {
  const [code] = splitTyped(error);
  return code === "direct graduation active"
    ? graduationErrorMessage(error)
    : error;
}

export function errorMessage(e: unknown): string {
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return "the operation failed";
}

/** WTS-FR-03: the resting label — the branch, or the detached commit id. */
export function restingLabel(entry: WorktreeEntry): string {
  if (entry.isDetached) return `detached at ${entry.headShortHash}`;
  return entry.branch ?? entry.name;
}

/**
 * WTS-FR-07: match a worktree on its branch, its displayed name, and its
 * absolute path; match a branch on its name. Client-side over the loaded
 * payload, so typing never triggers a round-trip and never re-orders a group.
 */
export function matchesFilter(
  haystacks: (string | undefined)[],
  filter: string,
): boolean {
  const needle = filter.trim().toLowerCase();
  if (!needle) return true;
  return haystacks.some((h) => !!h && h.toLowerCase().includes(needle));
}

/**
 * WTS-FR-34 / WTS-FR-35: the dismissible remote-outcome note, rendered in
 * whichever of its two positions applies — one component so the two cannot
 * diverge in what they say or in what they offer.
 */
function RemoteNote({
  note,
  onDismiss,
  onOpenGlobalSettings,
}: {
  note: { message: string; missingToken: boolean };
  onDismiss: () => void;
  onOpenGlobalSettings?: () => void;
}) {
  return (
    <>
      <span className="wt-select__note-text">⚠ {note.message}</span>
      {/* WTS-FR-35 / GHA-FR-19: a real route, not a sentence naming one. */}
      {note.missingToken && onOpenGlobalSettings && (
        <button
          type="button"
          className="btn btn--ghost btn--sm"
          data-testid="worktree-refresh-note-settings"
          onClick={() => {
            onDismiss();
            onOpenGlobalSettings();
          }}
        >
          Global settings
        </button>
      )}
      <button
        type="button"
        className="btn btn--ghost btn--icon btn--sm"
        aria-label="Dismiss"
        data-testid="worktree-refresh-note-dismiss"
        onClick={onDismiss}
      >
        <Icon.X size={11} />
      </button>
    </>
  );
}

interface NewWorktreeDraft {
  branch: string;
  location: string;
  /**
   * WTS-FR-19: the user has taken the location over — by typing in it or by
   * picking one through Browse — so editing the branch no longer re-derives it.
   */
  locationOverridden: boolean;
}

export function WorktreeSelector({
  active,
  onSwitch,
  onOpen,
  onRequestGithubToken,
  onOpenGlobalSettings,
}: WorktreeSelectorProps) {
  const [open, setOpen] = useState(false);
  const [context, setContext] = useState<WorktreeContext | null>(null);
  const [filter, setFilter] = useState("");
  // WTS-FR-14: the branch row showing its inline two-way choice, if any.
  const [choiceBranch, setChoiceBranch] = useState<string | null>(null);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  // WTS-FR-16 / WTS-FR-17: the New worktree dialog, or null while closed.
  const [draft, setDraft] = useState<NewWorktreeDraft | null>(null);
  const [dialogError, setDialogError] = useState("");
  /**
   * WTS-FR-31: a refresh is in flight, so the control is busy and not
   * re-activatable — a second refresh cannot be stacked on the first.
   *
   * Mirrored in a ref because the guard has to hold across an `await`: the state
   * read inside `runRefresh` is the one captured at render time, so a second
   * activation arriving before React re-renders would see it stale and start a
   * concurrent refresh. The ref is the gate; the state is what renders.
   */
  const [refreshing, setRefreshing] = useState(false);
  const refreshingRef = useRef(false);
  /**
   * WTS-FR-34: the remote outcome worth reporting, or null. Dismissible, and it
   * carries `missingToken` so the note can offer the Global settings route
   * without re-deriving which cause produced it (WTS-FR-35).
   */
  const [note, setNote] = useState<{
    message: string;
    missingToken: boolean;
  } | null>(null);

  const rootRef = useRef<HTMLDivElement>(null);
  const filterRef = useRef<HTMLInputElement>(null);

  // WTS-FR-05: opening closes every other floating overlay. The side effect is
  // kept out of the `setOpen` updater — an updater must stay pure, or
  // StrictMode's double invocation fires it twice.
  const toggle = () => {
    const next = !open;
    setOpen(next);
    if (next) onOpen?.();
  };

  // WTS-FR-25: bumped when the active worktree changes by any route, so an
  // already-open dropdown re-renders its rows with the new `current` flag
  // rather than sitting on the picture it loaded when it opened.
  const [refresh, setRefresh] = useState(0);

  // WTS-FR-04: the dropdown's contents come from one round-trip on each open.
  useEffect(() => {
    if (!open) {
      setContext(null);
      setFilter("");
      setChoiceBranch(null);
      setError("");
      return;
    }
    let cancelled = false;
    api
      .listWorktreesAndBranches()
      .then((ctx) => {
        if (!cancelled) setContext(ctx);
      })
      .catch((e) => {
        if (cancelled) return;
        setContext({
          repositoryRoot: "",
          activeWorktreePath: "",
          worktrees: [],
          branches: [],
        });
        setError(errorMessage(e));
      });
    return () => {
      cancelled = true;
    };
  }, [open, refresh]);

  /**
   * WTS-FR-25 / WTC-FR-16: a checkout made elsewhere — the Git panel's branches
   * section, say — re-renders the open dropdown. The chrome control's own label
   * follows the same event one level up, in the shell.
   */
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let cancelled = false;
    void onWorktreeContextChanged(() => setRefresh((n) => n + 1)).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  /**
   * WTS-FR-32 / WTC-FR-25: the branch set was re-read, whatever raised it. The
   * open dropdown reloads its listing — keeping its filter query and scroll
   * position, because the group elements are re-rendered rather than remounted.
   *
   * This is a second path to the same re-render the returning refresh below
   * already applies, and deliberately so: the backend's emit is best-effort, and
   * a refresh raised anywhere else has no outcome to hand this component.
   */
  useEffect(() => {
    let unlisten: (() => void) | null = null;
    let cancelled = false;
    void onBranchesChanged(() => setRefresh((n) => n + 1)).then((fn) => {
      if (cancelled) fn();
      else unlisten = fn;
    });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, []);

  // WTS-FR-06: the filter input holds focus from the moment the dropdown opens.
  useEffect(() => {
    if (open) filterRef.current?.focus();
  }, [open]);

  // Dismiss on outside click / Escape, like the project switcher beside it. The
  // dialog never coexists with the dropdown (WTS-FR-05), so there is no case
  // where one key press has to choose between them.
  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (rootRef.current && !rootRef.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", onDown);
    document.addEventListener("keydown", onKey);
    return () => {
      document.removeEventListener("mousedown", onDown);
      document.removeEventListener("keydown", onKey);
    };
  }, [open]);

  // WTS-FR-ROMD: a work stream's working copy is reached through the work
  // stream selector, so it is absent from this list, from its filter results
  // and from its count — whether or not it is the active worktree.
  const worktrees = useMemo(
    () =>
      (context?.worktrees ?? [])
        .filter((w) => !w.stream)
        .filter((w) => matchesFilter([w.branch, w.name, w.path], filter)),
    [context, filter],
  );
  const branches = useMemo(
    () =>
      (context?.branches ?? []).filter((b) => matchesFilter([b.name], filter)),
    [context, filter],
  );

  // WTS-FR-12: the Branches group exists only in the repository's own checkout.
  // The backend reports none from a linked worktree either (WTC-FR-06), so this
  // is the two sides agreeing rather than the UI hiding data it was given.
  const canCheckOutBranches = !!active?.isPrimary;
  const filterLabel = canCheckOutBranches
    ? "Filter worktrees and branches"
    : "Filter worktrees";

  // WTS-FR-02: no repository, no selector — anywhere in the top chrome.
  if (!active) return null;

  /** Run a switching operation and keep the dropdown open on a typed refusal. */
  const runSwitch = async (operation: () => Promise<WorktreeContext>) => {
    if (busy) return;
    setBusy(true);
    setError("");
    try {
      const outcome = await onSwitch(operation);
      if (outcome.ok) {
        setOpen(false);
        setDraft(null);
        return true;
      }
      // WTS-FR-23: a cancelled flush is not an error to render — the blocking
      // tab has been focused, and that is where the user resolves it.
      if (outcome.cancelled) {
        setOpen(false);
        setDraft(null);
        return false;
      }
      // WTS-FR-24 / WTS-FR-BQCI: inline, in the surface that produced it, and
      // in words where the refusal is a graduation run holding the worktree.
      const said = switchRefusalText(outcome.error);
      if (draft) setDialogError(said);
      else setError(said);
      return false;
    } finally {
      setBusy(false);
    }
  };

  /**
   * WTS-FR-30 .. WTS-FR-35: run a refresh and answer its remote outcome.
   *
   * Deliberately not routed through `onSwitch` (WTS-FR-33): a refresh reads what
   * the repository holds rather than moving the project onto a different
   * checkout, so nothing is flushed, no tab closes, and the flush-and-cancel
   * rule of WTS-FR-23 does not apply — which is what makes it available while an
   * Editor tab holds unsaved changes.
   *
   * `retrying` guards the one recursive step: a selection-required failure whose
   * picker was confirmed re-invokes the whole refresh once (WTS-FR-35), and a
   * second selection-required answer must not reopen the picker forever.
   */
  const runRefresh = async (retrying = false) => {
    // WTS-FR-31: not re-activatable while one is in flight. The retry below is
    // the continuation of the refresh already running, not a second one, so it is
    // the one caller allowed past the gate.
    if (!retrying && refreshingRef.current) return;
    refreshingRef.current = true;
    setRefreshing(true);
    setNote(null);
    try {
      const outcome = await api.refreshWorktreesAndBranches();

      // WTS-FR-32 / WTS-FR-34: the locally refreshed listing is applied whatever
      // the remote leg did. Applied straight from the outcome rather than waiting
      // for `"branches changed"`, so an undelivered event cannot leave the rows
      // stale — the event handler above converges on the same content.
      if (open) setContext(outcome.context);

      // WTS-FR-35: a remote leg blocked on token *selection* opens the picker,
      // and confirming re-runs the whole refresh. Cancelling abandons the remote
      // leg — and either way the listing the local half refreshed stays in place.
      if (
        !retrying &&
        outcome.remoteState === "failed" &&
        outcome.remoteError === GITHUB_TOKEN_ERRORS.selectionRequired &&
        onRequestGithubToken
      ) {
        // WTS-FR-36: the picker is a floating overlay, so it closes the dropdown
        // rather than being presented over it.
        setOpen(false);
        // The control stays busy while the picker is up: the refresh is still
        // outstanding, and clearing it here would let a second one start
        // alongside — leaving two in flight with their notes racing.
        const chosen = await onRequestGithubToken();
        if (chosen) {
          await runRefresh(true);
          return;
        }
        setNote({ message: remoteNote(outcome) ?? "", missingToken: false });
        return;
      }

      const message = remoteNote(outcome);
      setNote(
        message
          ? {
              message,
              // WTS-FR-35: nothing to choose between, so the note routes to
              // where a token is added instead of opening a picker.
              missingToken:
                outcome.remoteError === GITHUB_TOKEN_ERRORS.tokenMissing,
            }
          : null,
      );
    } catch (e) {
      // The only rejection is "not a git repository" (WTC-FR-02), which cannot
      // happen while this control is rendered at all (WTS-FR-29) — but a project
      // closed mid-flight would produce it, so it is reported rather than lost.
      setNote({ message: errorMessage(e), missingToken: false });
    } finally {
      refreshingRef.current = false;
      setRefreshing(false);
    }
  };

  const onPickWorktree = (w: WorktreeEntry) => {
    // WTS-FR-10 / WTS-FR-11: the current row and a missing row are inert.
    if (w.isActive || w.isMissing) return;
    void runSwitch(() => api.activateWorktree(w.path));
  };

  /**
   * WTS-FR-16 / WTS-FR-17 / WTS-FR-18: seed the dialog and pre-fill its
   * location for `branch`.
   *
   * WTS-FR-05: the dialog is a floating overlay in its own right, so opening it
   * closes the dropdown rather than being presented over the list it was
   * reached from. Dismissing the dialog returns to the chrome, not to the
   * dropdown.
   */
  const openDialog = async (branch: string) => {
    setOpen(false);
    setDialogError("");
    setDraft({ branch, location: "", locationOverridden: false });
    try {
      const location = await api.proposeWorktreePath(branch);
      setDraft((d) =>
        d && !d.locationOverridden && d.branch === branch ? { ...d, location } : d,
      );
    } catch (e) {
      setDialogError(errorMessage(e));
    }
  };

  /**
   * WTS-FR-19: while the user has not taken the location over, editing the
   * branch re-derives it. Once they have, it is left exactly as they left it.
   */
  const onDraftBranchChange = async (branch: string) => {
    setDraft((d) => (d ? { ...d, branch } : d));
    const overridden = draft?.locationOverridden ?? false;
    if (overridden) return;
    try {
      const location = await api.proposeWorktreePath(branch);
      setDraft((d) =>
        d && !d.locationOverridden && d.branch === branch ? { ...d, location } : d,
      );
    } catch {
      // A proposal that cannot be computed simply leaves the field as it is;
      // the user can always type a location themselves.
    }
  };

  // WTS-FR-20: Browse writes the chosen path in and freezes re-derivation; a
  // cancelled browse leaves the field untouched.
  const onBrowse = async () => {
    try {
      const result = await api.browseForFolder();
      if (result === "cancelled") return;
      setDraft((d) =>
        d
          ? { ...d, location: result.selected.path, locationOverridden: true }
          : d,
      );
    } catch (e) {
      setDialogError(errorMessage(e));
    }
  };

  const createEnabled =
    !!draft && draft.branch.trim() !== "" && draft.location.trim() !== "";

  return (
    <div className="wt-select" ref={rootRef}>
      <button
        type="button"
        className="top-chrome__project"
        data-testid="worktree-selector"
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls="worktree-selector-menu"
        title={active.path}
        onClick={toggle}
      >
        <Icon.Branch size={12} />
        <span className="wt-select__label">{restingLabel(active)}</span>
        <Icon.Caret size={10} />
      </button>

      {/* WTS-FR-29: immediately after the selector, under exactly the condition
          the selector itself is rendered — the `if (!active) return null` above
          is that one condition, so the two cannot drift apart.

          A sibling of the toggle rather than a child of it (WTS-FR-30): pressing
          it never opens the dropdown. It stays INSIDE `rootRef` so the
          outside-pointer rule does not read it as a dismissal either — activating
          it neither opens nor closes the dropdown. */}
      <button
        type="button"
        className="btn btn--ghost btn--icon btn--sm wt-select__refresh"
        data-testid="worktree-refresh"
        data-busy={refreshing}
        aria-label="Refresh branches"
        aria-busy={refreshing}
        // WTS-FR-31: busy and not re-activatable, so a second refresh cannot be
        // stacked on the first.
        disabled={refreshing}
        title="Refresh local and remote branches"
        onClick={() => void runRefresh()}
      >
        {/* WTS-FR-37: the control depicts material arriving from elsewhere,
            and the icon never varies with what a previous refresh reported
            (WTS-FR-34). The busy state of WTS-FR-31 is the one thing that
            replaces it, and only while a refresh is in flight — the circular
            arrows read as motion under the spin the stylesheet applies, which
            is what a wait wants and what a resting control does not. */}
        {refreshing ? <Icon.Refresh size={13} /> : <Icon.PullDown size={13} />}
      </button>

      {/* WTS-FR-34: anchored to the control while the dropdown is closed; it
          moves inside the dropdown when that is open, so one note never renders
          twice. */}
      {note && !open && (
        <div className="wt-select__note" data-testid="worktree-refresh-note">
          <RemoteNote
            note={note}
            onDismiss={() => setNote(null)}
            onOpenGlobalSettings={onOpenGlobalSettings}
          />
        </div>
      )}

      {open && (
        <div
          id="worktree-selector-menu"
          className="wt-select__menu menu"
          role="menu"
          aria-label="Worktrees and branches"
          data-testid="worktree-selector-menu"
        >
          {/* WTS-FR-06: the filter is always the topmost row. */}
          <div className="wt-select__filter">
            <Icon.Search size={12} />
            <input
              ref={filterRef}
              className="input"
              data-testid="worktree-filter"
              aria-label={filterLabel}
              // Sentence case with a trailing ellipsis, as every other filter
              // placeholder in the window reads. Lower-casing the label made
              // this the one field that shouted differently.
              placeholder={`${filterLabel}…`}
              value={filter}
              onChange={(e) => setFilter(e.target.value)}
            />
          </div>

          {/* WTS-FR-34: above the group regions while the dropdown is open, so
              the outcome is read where the refreshed rows are. */}
          {note && (
            <div
              className="wt-select__note wt-select__note--inline"
              data-testid="worktree-refresh-note"
            >
              <RemoteNote
                note={note}
                onDismiss={() => setNote(null)}
                onOpenGlobalSettings={onOpenGlobalSettings}
              />
            </div>
          )}

          {context === null && (
            <div className="wt-select__empty t-muted">Loading…</div>
          )}

          {context !== null && (
            <>
              {/* WTS-FR-08: Worktrees always precede Branches. */}
              <div className="wt-select__head" aria-hidden="true">
                Worktrees
              </div>
              <div
                className="wt-select__group"
                data-testid="worktree-group"
                role="group"
                aria-label="Worktrees"
                style={{
                  maxHeight: WORKTREE_GROUP_MAX_ROWS * WORKTREE_ROW_PX,
                  overflowY: "auto",
                  overflowX: "hidden",
                }}
              >
                {worktrees.length === 0 && (
                  <div className="wt-select__empty t-muted">No worktrees.</div>
                )}
                {worktrees.map((w) => {
                  const inert = w.isActive || w.isMissing;
                  return (
                    <button
                      type="button"
                      key={w.path}
                      role="menuitem"
                      className="wt-select__wt"
                      data-current={w.isActive}
                      data-missing={w.isMissing}
                      aria-disabled={inert || undefined}
                      title={w.path}
                      onClick={() => onPickWorktree(w)}
                    >
                      <span className="wt-select__wt-line">
                        <span className="wt-select__name">
                          {restingLabel(w)}
                        </span>
                        {w.isActive && <span className="badge">current</span>}
                        {w.isMissing && (
                          <span className="badge badge--warn">missing</span>
                        )}
                      </span>
                      <span className="wt-select__path">{w.path}</span>
                    </button>
                  );
                })}
              </div>

              {canCheckOutBranches && (
                <>
              <div className="wt-select__head" aria-hidden="true">
                Branches
              </div>
              <div
                className="wt-select__group"
                data-testid="branch-group"
                role="group"
                aria-label="Branches"
                style={{
                  maxHeight: WORKTREE_GROUP_MAX_ROWS * WORKTREE_ROW_PX,
                  overflowY: "auto",
                  overflowX: "hidden",
                }}
              >
                {branches.length === 0 && (
                  <div className="wt-select__empty t-muted">
                    No branches without a worktree.
                  </div>
                )}
                {branches.map((b: BranchEntry) => (
                  <div key={`${b.kind}:${b.name}`}>
                    <button
                      type="button"
                      role="menuitem"
                      className="wt-select__branch"
                      aria-expanded={choiceBranch === b.name}
                      title={b.name}
                      // WTS-FR-14: selecting a branch invokes nothing; it
                      // reveals the two-way choice on that row.
                      onClick={() =>
                        setChoiceBranch((c) => (c === b.name ? null : b.name))
                      }
                    >
                      <Icon.Branch size={12} />
                      <span className="wt-select__name">{b.name}</span>
                      {b.kind === "remote" && (
                        <span className="badge">remote</span>
                      )}
                    </button>
                    {choiceBranch === b.name && (
                      <div className="wt-select__choice" role="group">
                        <button
                          type="button"
                          className="btn btn--default btn--sm"
                          disabled={busy}
                          onClick={() =>
                            void runSwitch(() =>
                              api.checkOutBranchInActiveWorktree(b.name),
                            )
                          }
                        >
                          Check out here
                        </button>
                        <button
                          type="button"
                          className="btn btn--default btn--sm"
                          onClick={() => void openDialog(b.name)}
                        >
                          New worktree…
                        </button>
                      </div>
                    )}
                  </div>
                ))}
              </div>
                </>
              )}
            </>
          )}

          {/* WTS-FR-24: a typed refusal renders inline and leaves the project
              on its current active worktree. */}
          {error && (
            <div className="wt-select__error" data-testid="worktree-error">
              ✗ {error}
            </div>
          )}

          {/* WTS-FR-17: outside both scrolling group regions. */}
          <div className="menu-sep" />
          <button
            type="button"
            role="menuitem"
            className="proj-switch__footer"
            data-testid="worktree-new"
            onClick={() => void openDialog("")}
          >
            <Icon.Plus size={12} /> New worktree…
          </button>
        </div>
      )}

      {draft && (
        <div
          className="scrim"
          onMouseDown={(e) => {
            if (e.target === e.currentTarget) setDraft(null);
          }}
        >
          <div
            className="modal"
            role="dialog"
            aria-labelledby="new-worktree-title"
          >
            <div className="modal__head">
              <div className="modal__title" id="new-worktree-title">
                New worktree
              </div>
              <button
                type="button"
                className="btn btn--ghost btn--icon btn--sm"
                aria-label="Close"
                onClick={() => setDraft(null)}
              >
                <Icon.X size={12} />
              </button>
            </div>
            <div className="modal__body">
              <div className="picker-field">
                <label className="picker-field__label" htmlFor="nw-branch">
                  Branch
                </label>
                <input
                  id="nw-branch"
                  className="input input--mono"
                  aria-label="Branch"
                  autoFocus
                  value={draft.branch}
                  onChange={(e) => void onDraftBranchChange(e.target.value)}
                />
              </div>
              <div className="picker-field">
                <label className="picker-field__label" htmlFor="nw-location">
                  Location
                </label>
                <div className="wt-select__location">
                  <input
                    id="nw-location"
                    className="input input--mono"
                    aria-label="Location"
                    value={draft.location}
                    onChange={(e) =>
                      setDraft((d) =>
                        d
                          ? {
                              ...d,
                              location: e.target.value,
                              locationOverridden: true,
                            }
                          : d,
                      )
                    }
                  />
                  <button
                    type="button"
                    className="btn btn--default btn--sm"
                    onClick={() => void onBrowse()}
                  >
                    Browse…
                  </button>
                </div>
              </div>
              {dialogError && (
                <div
                  className="wt-select__error"
                  data-testid="worktree-dialog-error"
                >
                  ✗ {dialogError}
                </div>
              )}
            </div>
            <div className="modal__actions">
              <button
                type="button"
                className="btn btn--default btn--sm"
                onClick={() => setDraft(null)}
              >
                Cancel
              </button>
              <button
                type="button"
                className="btn btn--primary btn--sm"
                disabled={!createEnabled || busy}
                onClick={() => {
                  setDialogError("");
                  void runSwitch(() =>
                    api.createWorktree(draft.branch, draft.location),
                  );
                }}
              >
                Create
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
