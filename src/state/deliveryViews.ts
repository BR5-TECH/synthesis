import * as api from "../api";
import { errorLogCode } from "../errorLogCode";
import { logWarn } from "../logging";
import type { DeliveryBoardStatus, DeliveryView } from "../types";

/** DMS-FR-TXMH: the empty view: nothing hidden, and Blocked only off. */
export const EMPTY_VIEW: DeliveryView = { hidden: [], blockedOnly: false };

const STATUSES: readonly DeliveryBoardStatus[] = ["backlog", "ready", "in progress", "in review", "done"];

/** Start a backend call now; a call that throws at once is a rejected promise. */
function call<T>(start: () => Promise<T>): Promise<T> {
  try {
    return start();
  } catch (e) {
    return Promise.reject(e);
  }
}

/** The view as the lens uses it: each known status once, in board order. */
function normalized(view: Partial<DeliveryView> | null | undefined): DeliveryView {
  const hidden = new Set(Array.isArray(view?.hidden) ? view.hidden : []);
  return {
    hidden: STATUSES.filter((s) => hidden.has(s)),
    blockedOnly: view?.blockedOnly === true,
  };
}

/** DMS-FR-ROMF: a log names the kind of a scope, never the scope. */
const scopeKind = (scope: string) => scope.split(":")[0] ?? "";

interface ScopeRecord {
  /** The view, or null while the first read is in flight. */
  view: DeliveryView | null;
  /** A save happened, so a read that lands later is older than what we hold. */
  saved: boolean;
}

/**
 * MDL-FR-ZAGM: the status filter of each layout scope of the Delivery lens,
 * as the machine stores it (`DMS-delivery-machine-state.md` DMS-FR-TXMH).
 *
 * The view is held here and not by the lens, so it outlives each mount of the
 * canvas, also while a save is in flight. Each change is one save of the
 * whole view through `"save delivery view (scope, view)"`.
 */
export class DeliveryViewStore {
  private scopes = new Map<string, ScopeRecord>();
  private listeners = new Set<() => void>();
  private version = 0;

  subscribe = (listener: () => void): (() => void) => {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  };

  /** Snapshot for `useSyncExternalStore`. */
  getVersion = (): number => this.version;

  private notify(): void {
    this.version += 1;
    this.listeners.forEach((l) => l());
  }

  /** The view of `scope`, or null until its first read lands. */
  get(scope: string): DeliveryView | null {
    return this.scopes.get(scope)?.view ?? null;
  }

  /**
   * MDL-FR-ZAGM: read the view of `scope` through `"load delivery view
   * (scope)"`, once. A read that fails gives the empty view, so every item
   * shows, and the log says why.
   */
  ensure(scope: string): void {
    if (this.scopes.has(scope)) return;
    const record: ScopeRecord = { view: null, saved: false };
    this.scopes.set(scope, record);
    void call(() => api.loadDeliveryView(scope)).then(
      (view) => {
        if (record.saved || this.scopes.get(scope) !== record) return;
        record.view = normalized(view);
        this.notify();
      },
      (e) => {
        logWarn(["frontend"], "delivery view not read", {
          scope_kind: scopeKind(scope),
          code: errorLogCode(e),
        });
        if (record.saved || this.scopes.get(scope) !== record) return;
        record.view = EMPTY_VIEW;
        this.notify();
      },
    );
  }

  /**
   * MDL-FR-ZAGM: keep `view` for `scope` and store it through `"save delivery
   * view (scope, view)"`. The board shows it at once. A save that fails keeps
   * the view on screen for this session, and the log says why.
   */
  save(scope: string, view: DeliveryView): Promise<void> {
    const next = normalized(view);
    const record = this.scopes.get(scope) ?? { view: null, saved: false };
    record.view = next;
    record.saved = true;
    this.scopes.set(scope, record);
    this.notify();
    return call(() => api.saveDeliveryView(scope, next)).catch((e) => {
      logWarn(["frontend"], "delivery view not saved", {
        scope_kind: scopeKind(scope),
        hidden: next.hidden.length,
        blocked_only: next.blockedOnly,
        code: errorLogCode(e),
      });
    });
  }

  /** Forget every scope (project close or switch). */
  clear(): void {
    this.scopes.clear();
    this.notify();
  }
}
