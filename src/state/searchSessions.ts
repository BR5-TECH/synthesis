/**
 * The completed result set of every open Search results tab
 * (`SCH-search.md` SCH-FR-11).
 *
 * A Search results tab that is not the active tab is unmounted by the viewport,
 * which renders only the focused tab. Without somewhere outside that component
 * to keep it, returning to the tab would re-dispatch the whole sweep — and
 * SCH-FR-11 requires the opposite: navigating from a result preserves the tab,
 * so the user can come back to the same query via Back or tab focus and find the
 * same results.
 *
 * This is the same shape the Editor's `EditSessionStore` has, for the same
 * reason: state that belongs to the *content* rather than to the tab showing it
 * must be owned by the shell, which outlives any one mount.
 *
 * Only **terminal** result sets are recorded. A tab abandoned mid-sweep has its
 * search cancelled (SCH-FR-20), so what it had was a partial answer to a
 * question that was never finished — resuming from it would leave the page
 * claiming to be searching forever. That tab re-dispatches on return, which is
 * the honest outcome.
 */
import type { SearchEndReason, SearchHit } from "../types";

/** A finished search, exactly as its tab last rendered it. */
export interface SearchSession {
  hits: SearchHit[];
  reason: SearchEndReason | null;
  /** SCH-FR-21: an uncompilable query is a terminal outcome worth resuming. */
  error: string | null;
}

export class SearchSessionStore {
  private sessions = new Map<string, SearchSession>();

  /** The recorded result set for `tabId`, or undefined if it has none. */
  get(tabId: string): SearchSession | undefined {
    return this.sessions.get(tabId);
  }

  /** Record a finished result set. Called only once a search has ended. */
  save(tabId: string, session: SearchSession): void {
    this.sessions.set(tabId, session);
  }

  /** Forget a tab's results when the tab closes — its search ends with it. */
  drop(tabId: string): void {
    this.sessions.delete(tabId);
  }

  /**
   * Drop everything. A project or worktree switch closes every tab (TAB-FR-14),
   * and a result set names paths in the outgoing content root, so none of it
   * survives into the new one.
   */
  clear(): void {
    this.sessions.clear();
  }
}
