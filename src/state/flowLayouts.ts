import * as api from "../api";
import { errorLogCode } from "../errorLogCode";
import { logWarn } from "../logging";
import type { StoredLayout } from "./flowDocument";

/** The empty layout: every item takes its automatic place (FLO-FR-PLUZ). */
export const EMPTY_LAYOUT: StoredLayout = { items: {} };

/** Start a backend call now; a call that throws at once is a rejected promise. */
function call<T>(start: () => Promise<T>): Promise<T> {
  try {
    return start();
  } catch (e) {
    return Promise.reject(e);
  }
}

interface ScopeRecord {
  /** The stored layout, or null while the first read is in flight. */
  layout: StoredLayout | null;
  /** A save happened, so a read that lands later is older than what we hold. */
  saved: boolean;
}

/**
 * FLO-FR-21 / FLO-FR-PLUZ: the layout of each scope a canvas shows, as the
 * machine stores it (`DMS-delivery-machine-state.md`).
 *
 * The layout is held here and not by the canvas, because only the active
 * tab's canvas is mounted (FLO-FR-30): a canvas that mounts again reads what
 * this store holds, also while a save is in flight.
 *
 * A layout is never part of a plan, so nothing here marks a plan dirty or
 * schedules its write (FLO-FR-TDHR). Each change is one save of the whole
 * scope through `"save delivery layout (scope, layout)"` (DMS-FR-XAZV).
 */
export class FlowLayoutStore {
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

  /** The stored layout of `scope`, or null until its first read lands. */
  get(scope: string): StoredLayout | null {
    return this.scopes.get(scope)?.layout ?? null;
  }

  /**
   * FLO-FR-PLUZ: read the layout of `scope` through `"load delivery layout
   * (scope)"`, once. A read that fails gives the empty layout, so the plan
   * still renders with its automatic layout.
   */
  ensure(scope: string): void {
    if (this.scopes.has(scope)) return;
    const record: ScopeRecord = { layout: null, saved: false };
    this.scopes.set(scope, record);
    void call(() => api.loadDeliveryLayout(scope)).then(
      (layout) => {
        if (record.saved || this.scopes.get(scope) !== record) return;
        record.layout = { items: { ...(layout?.items ?? {}) } };
        this.notify();
      },
      (e) => {
        logWarn(["frontend"], "delivery layout not read", {
          scope_kind: scope.split(":")[0] ?? "",
          code: errorLogCode(e),
        });
        if (record.saved || this.scopes.get(scope) !== record) return;
        record.layout = EMPTY_LAYOUT;
        this.notify();
      },
    );
  }

  /**
   * FLO-FR-21 / FLO-FR-TDHR: keep `layout` for `scope` and store it through
   * `"save delivery layout (scope, layout)"`. The canvas shows it at once. A
   * save that fails keeps the layout in memory for this session and says why
   * in the log.
   */
  save(scope: string, layout: StoredLayout): Promise<void> {
    const record = this.scopes.get(scope) ?? { layout: null, saved: false };
    record.layout = layout;
    record.saved = true;
    this.scopes.set(scope, record);
    this.notify();
    return call(() => api.saveDeliveryLayout(scope, layout)).catch((e) => {
      logWarn(["frontend"], "delivery layout not saved", {
        scope_kind: scope.split(":")[0] ?? "",
        items: Object.keys(layout.items).length,
        code: errorLogCode(e),
      });
    });
  }

  /** MDL-FR-QAJN: store the empty layout, so the plan is laid out again. */
  reset(scope: string): Promise<void> {
    return this.save(scope, EMPTY_LAYOUT);
  }

  /** Forget every scope (project close or switch). */
  clear(): void {
    this.scopes.clear();
    this.notify();
  }
}
