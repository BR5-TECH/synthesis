/**
 * The **automatic** tab closures — the ones the shell performs because
 * something happened to the content behind a tab, rather than because the
 * author closed it.
 *
 * The three of them are held together because what separates them is exactly
 * one decision: whether the tab's pending write is brought forward first. A
 * removal (TAB-FR-19) and a rollback (TAB-FR-41) discard, because a flush would
 * recreate the file the author just got rid of; a commit closure (TAB-FR-22)
 * writes and can be refused. Reading them side by side is what keeps that
 * distinction visible.
 *
 * Plain closures rather than a hook: every piece of state they act on stays
 * owned by `useShellSession`.
 */
import type { Dispatch, SetStateAction } from "react";
import * as api from "../../api";
import { logDebug, logError, logInfo } from "../../logging";
import type { EditSessionStore } from "../../state/editSessions";
import type { FlowSessionStore } from "../../state/flowSessions";
import type { RollbackOutcome, RollbackResult, Tab } from "../../types";
import { neverEmpty, tabPath } from "./tabRecords";

export interface TabClosureDeps {
  /** The strip as this render sees it. */
  tabs: Tab[];
  setTabs: Dispatch<SetStateAction<Tab[]>>;
  setActiveTab: Dispatch<SetStateAction<string>>;
  sessions: EditSessionStore;
  flows: FlowSessionStore;
}

export interface TabClosures {
  closeTabsForRemovedPaths: (removed: string[]) => void;
  closeDiffTabsForCommittedPaths: (committed: string[]) => Promise<void>;
  closeTabsForRolledBackPaths: (removed: string[]) => void;
  performRollback: (paths: string[]) => Promise<RollbackResult | null>;
}

export function createTabClosures(deps: TabClosureDeps): TabClosures {
  const { tabs, setTabs, setActiveTab, sessions, flows } = deps;

  /**
   * TAB-FR-19 / TAB-FR-20 / TAB-FR-21: a file that no longer exists holds no tab.
   *
   * Closed directly rather than through `closeTab`, and that is the substance of
   * TAB-FR-20 rather than an implementation shortcut: `closeTab` writes pending
   * changes first (TAB-FR-10), and the only file it could write to here is the
   * one that was just removed — so an ordinary close would recreate what the
   * user deleted. The removal is their most recent expression of intent, so the
   * edits are discarded and the retained editing session goes with them.
   *
   * The close refusals of TAB-FR-11 and TAB-FR-13 do not apply either: each of
   * them guards a write that is no longer going to happen.
   *
   * A removed folder takes the tabs beneath it, which is why this matches on the
   * path boundary as well as on equality — `docs` removes `docs/a.md` without
   * the backend having to enumerate the subtree (ASC-FR-22).
   */
  const closeTabsForRemovedPaths = (removed: string[]) => {
    if (removed.length === 0) return;
    const isUnder = (path: string) =>
      removed.some((r) => path === r || path.startsWith(`${r}/`));
    const shouldClose = (t: Tab) => {
      const p = tabPath(t);
      return p != null && isUnder(p);
    };

    // Computed from the live `tabs` rather than from inside a `setTabs`
    // updater. React may invoke an updater more than once for a single update,
    // so an updater that assigns to variables out here runs twice and the second
    // pass — against the already-filtered array — sees nothing left to close.
    // This handler is re-created every render and reached through a ref, so
    // `tabs` here is current.
    const closed = tabs.filter(shouldClose);
    if (closed.length === 0) return;
    const remaining = neverEmpty(tabs.filter((t) => !shouldClose(t)));
    setTabs(remaining);

    for (const t of closed) {
      const path = tabPath(t)!;
      // Read the dirty flag from the session stores, which own it, and read it
      // BEFORE the forget below discards the record. The `tabs` state array does
      // not carry `dirty` — it is computed onto the derived `tabsView` that
      // callers see — so asking `t.dirty` here would silently always be false
      // and TAB-FR-21's "say so when work was discarded" would never fire.
      const discarded = t.artifactId
        ? t.kind === "flow"
          ? flows.get(t.artifactId)?.dirty === true
          : sessions.get(t.artifactId)?.dirty === true
        : false;
      // TAB-FR-20: drop the retained edit state too, so reopening a file later
      // recreated at this path starts from disk with an empty undo history.
      //
      // `forget`, not `closeTab`: `closeTab` deliberately *retains* a record
      // that carries edits so a reopen resumes it (EDT-FR-29), which is right
      // for a file that still exists and wrong for one that does not — and a
      // retained dirty buffer would be swept into the next teardown's flush and
      // written back to the path the user just removed.
      if (t.artifactId) {
        if (t.kind === "flow") flows.closeTab(t.artifactId);
        else sessions.forget(t.artifactId);
      }
      // TAB-FR-21: one DEBUG record per closed tab. Work discarded without a
      // prompt is named explicitly — it is the one thing here a user could
      // otherwise not account for. A path is safe to log; no content is.
      logDebug(["frontend"], "tab closed: backing file removed", {
        path,
        tabKind: t.kind ?? "editor",
        discardedUnsavedChanges: discarded,
      });
    }
    // SNV-FR-65: a removal is not an activation — the panel does not follow.
    setActiveTab((current) =>
      remaining.some((t) => t.id === current)
        ? current
        : remaining[remaining.length - 1].id,
    );
  };

  /**
   * TAB-FR-22 / DFV-FR-55: a Diff tab whose file a commit has successfully
   * included closes itself.
   *
   * The comparison it was open on is the change that has just been committed,
   * and a review the author has acted on is not a thing the strip should keep
   * asking them to finish. Three things bound it, and each is load-bearing:
   *
   * - It binds **Diff tabs alone**. No Editor tab, Flow tab, or New Artifact tab
   *   is closed by a commit, and the artifact's editing session survives
   *   untouched (EDT-FR-28) — committing a file says nothing about whether the
   *   author is finished writing it.
   * - It acts on the paths the commit **recorded** (`commit_paths` returns them,
   *   GTC-FR-19), so a rename closes the tab at both its locations. **No
   *   fallback ever closes every Diff tab**: called with nothing named, this
   *   closes nothing at all, because a strip that cannot say what was committed
   *   would otherwise throw away reviews the commit never touched.
   * - It closes on **confirmed success** alone. The one caller is the commit
   *   window's success path (CMW-FR-07 → CHG-FR-41); a commit merely started,
   *   abandoned, or refused reports nothing and reaches this not at all.
   *
   * Unlike TAB-FR-19's closures this is not a discarding close: the target's
   * pending write is brought forward first and a write that cannot proceed
   * safely refuses the close, on the ordinary terms of TAB-FR-10 and TAB-FR-11.
   */
  const closeDiffTabsForCommittedPaths = async (committed: string[]) => {
    if (committed.length === 0) return;
    const named = new Set(committed);
    const closing = tabs.filter(
      (t) => t.kind === "diff" && t.diff != null && named.has(t.diff.path),
    );
    if (closing.length === 0) return;

    const closed: Tab[] = [];
    let blocked: string | null = null;
    for (const t of closing) {
      const artifactId = t.artifactId;
      if (artifactId) {
        // TAB-FR-10 / TAB-FR-11: bring the write forward, and leave the tab
        // standing if it cannot proceed. A commit is not a reason to drop an
        // edit made after it, and a tab left open over an unresolved blocker is
        // how the author gets to see it — which is why the first refusal takes
        // the focus. A blocker nobody is looking at is a tab that never closes
        // for a reason the author was never shown.
        const res = await sessions.flush(artifactId);
        if (!res.ok) {
          blocked ??= t.id;
          continue;
        }
      }
      closed.push(t);
    }
    // SNV-FR-65: part of the commit-driven closure sweep rather than an
    // activation, and the moment the requirement singles out as the wrong one to
    // move the panel in — the author is committing.
    if (blocked !== null) setActiveTab(blocked);
    if (closed.length === 0) return;

    const going = new Set(closed.map((t) => t.id));
    const remaining = neverEmpty(tabs.filter((t) => !going.has(t.id)));
    setTabs(remaining);

    for (const t of closed) {
      const artifactId = t.artifactId;
      // EDT-FR-28: the session outlives the tab, and an Editor tab on the same
      // file keeps it standing — this only says that *this* view of it is gone.
      if (artifactId && !remaining.some((r) => r.artifactId === artifactId)) {
        sessions.closeTab(artifactId);
      }
      // TAB-FR-21: one DEBUG record per closed tab, naming which of the two
      // rules closed it. A path is safe to log; no content is.
      logDebug(["frontend"], "tab closed: file committed", {
        path: t.diff?.path ?? artifactId ?? "",
        tabKind: "diff",
        rule: "TAB-FR-22",
      });
    }
    setActiveTab((current) =>
      // A tab this close refused stays focused: it is the one the author has to
      // deal with, and moving off it would hide the blocker. Not an activation
      // either way (SNV-FR-65).
      blocked ??
      (remaining.some((t) => t.id === current)
        ? current
        : remaining[remaining.length - 1].id),
    );
  };

  /**
   * TAB-FR-41: close every tab bound to a file a rollback removed.
   *
   * Deliberately not `closeDiffTabsForCommittedPaths`'s path: that one brings
   * each tab's pending write forward and lets a blocker refuse the close
   * (TAB-FR-10, TAB-FR-11). Here the file is gone because the author asked for
   * it to be gone, so a flush would recreate it and a refusal would hold a tab
   * open over content the filesystem no longer has.
   */
  const closeTabsForRolledBackPaths = (removed: string[]) => {
    if (removed.length === 0) return;
    const gone = new Set(removed);
    const bound = (t: Tab) =>
      (t.artifactId != null && gone.has(t.artifactId)) ||
      (t.kind === "diff" && t.diff != null && gone.has(t.diff.path));

    // Computed inside the updater rather than from the `tabs` this closure
    // captured. `performRollback` awaits three times before reaching here — the
    // quiesce, the backend call, and the session resets — and a tab opened by
    // any other route during those awaits would be silently discarded by a
    // wholesale `setTabs` built on the pre-rollback array.
    let remaining: Tab[] = [];
    setTabs((prev) => {
      const closing = prev.filter(bound);
      if (closing.length === 0) {
        remaining = prev;
        return prev;
      }
      // TAB-FR-41 / TAB-FR-15: the strip never empties, however it was emptied.
      remaining = neverEmpty(prev.filter((t) => !bound(t)));
      for (const t of closing) {
        logDebug(["frontend"], "tab closed: file rolled back", {
          path: t.diff?.path ?? t.artifactId ?? "",
          tabKind: t.kind,
          rule: "TAB-FR-41",
        });
      }
      return remaining;
    });
    setActiveTab((current) =>
      remaining.some((t) => t.id === current)
        ? current
        : (remaining[remaining.length - 1]?.id ?? current),
    );
  };

  /**
   * CHG-FR-60 – CHG-FR-63: perform a confirmed rollback of `paths` and apply
   * its outcome to the artifact editing sessions and the tab strip.
   *
   * The order is the whole point of the requirement. Every affected session is
   * quiesced and its in-flight save awaited *before* the backend is reached
   * (EDT-FR-81), so no write can land on a file the rollback is about to touch;
   * and nothing in memory is discarded until the backend has **confirmed** the
   * corresponding path on disk, so a path that failed keeps its buffer, its
   * dirty indicator, and its undo history (CHG-FR-63).
   *
   * Returns the outcome for the panel to apply to its own checks, together with
   * the saves that failed while preparing (CHG-FR-61).
   */
  const performRollback = async (
    paths: string[],
  ): Promise<RollbackResult | null> => {
    if (paths.length === 0) return null;

    // EDT-FR-81 / CHG-FR-60: an artifact showing in several Editor and Diff
    // tabs is one session and one write, so it is quiesced and awaited once.
    const affected = [...new Set(paths)];
    const saveFailures = await sessions.quiesce(affected);
    // CHG-FR-61: one ERROR record per failed save, under the `frontend` domain,
    // with the artifact and the typed reason in flat fields. Logged whether or
    // not the rollback then succeeds — a write that failed while the author was
    // discarding is invisible everywhere else.
    for (const f of saveFailures) {
      logError(["frontend"], "save failed while preparing a rollback", {
        artifact: f.artifactId,
        reason: f.reason,
      });
    }

    let outcome: RollbackOutcome;
    try {
      outcome = await api.rollbackPaths(paths);
    } catch (e) {
      // The call itself was refused, so nothing was written and no session may
      // be reset. Every quiesce is lifted or the artifacts would never write
      // again (EXC-FR-UOJF).
      affected.forEach((id) => sessions.unquiesce(id));
      logError(["frontend"], "rollback refused", {
        pathCount: paths.length,
        reason: String(e),
      });
      throw e;
    }

    // CHG-FR-63: applied per path. A renamed entry is two identities and each
    // is applied on its own (GTC-FR-26), so half a rename failing leaves the
    // other half's reset standing.
    const restored = new Set<string>();
    const removed = new Set<string>();
    for (const entry of outcome.entries) {
      entry.restoredPaths.forEach((p) => restored.add(p));
      entry.removedPaths.forEach((p) => removed.add(p));
    }

    // EXC-FR-UVJY: a restored path reloads from disk with its dirty indicator
    // cleared and its undo history discarded, and raises no external-change
    // modal — the author already chose to discard this buffer.
    await Promise.all(
      [...restored].map((id) =>
        sessions.has(id) ? sessions.resetRestored(id) : Promise.resolve(),
      ),
    );
    // EXC-FR-ZXWI / TAB-FR-41: a removed path's session is discarded without
    // being written and its tabs close, which must not take the
    // write-before-close path — a flush here would recreate the file.
    for (const id of removed) sessions.discardRemoved(id);
    closeTabsForRolledBackPaths([...removed]);

    // EXC-FR-UOJF: a path the backend confirmed as neither keeps everything it
    // had; only the quiesce is lifted, so its next edit schedules a write again.
    for (const id of affected) {
      if (!restored.has(id) && !removed.has(id)) sessions.unquiesce(id);
    }

    logInfo(["frontend"], "rollback applied", {
      selected: paths.length,
      restored: restored.size,
      removed: removed.size,
      failedEntries: outcome.entries.filter((e) => e.outcome === "failed")
        .length,
      saveFailures: saveFailures.length,
    });
    // CHG-FR-61 / CHG-FR-65: the save failures travel back with the outcome
    // rather than stopping at the log. The requirement is explicit that the
    // author sees them "as well as" the log — a write that failed while they
    // were discarding is otherwise invisible to everyone but a developer with
    // the Logs panel open.
    return { outcome, saveFailures };
  };

  return {
    closeTabsForRemovedPaths,
    closeDiffTabsForCommittedPaths,
    closeTabsForRolledBackPaths,
    performRollback,
  };
}
