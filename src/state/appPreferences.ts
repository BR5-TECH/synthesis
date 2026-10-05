/**
 * The user-global app-preferences record (GSS-global-settings-storage.md
 * GSS-FR-04 / GSS-FR-19 / GSS-FR-20).
 *
 * One record holds several facts edited from unrelated places: the **theme**,
 * chosen in the Global settings Appearance section or the top-chrome selector
 * (SNV-FR-15); the main window's **OS full-screen state**, written by the shell
 * whenever the window enters or leaves full-screen (SNV-FR-38); the search
 * bar's **active query mode**, written by the search input whenever the user
 * changes the active toggle (SCH-FR-13 / GSS-FR-21); and the Diff tab's
 * **visualization and rendering modes**, written whenever the user activates a
 * different toggle in any Diff tab (DFV-FR-23 / GSS-FR-24).
 *
 * `save_app_preferences` is a whole-record write (GSS-FR-20): whatever the
 * caller sends *replaces* the stored record. So a caller that sent only
 * `{ theme }` would silently clear the full-screen flag and the query mode — the
 * exact clobber GLS-FR-14 requires not to happen. This module is where "the
 * caller supplies the other fields unchanged" is implemented once, instead of
 * every call site remembering to.
 *
 * The cached record is seeded by the first `load()` and updated on every
 * successful write, so a `patch()` always has the other fields to carry
 * through. A `patch()` that arrives before any load still reads the record
 * first, rather than writing defaults over what is on disk.
 */
import * as api from "../api";
import { emitAppPreferencesChanged } from "../events";
import type { AppPreferences } from "../types";

/**
 * The record as it was last read from, or successfully written to, the backend.
 * Non-null means "we know what is stored" — a failed read deliberately leaves it
 * null so a later patch cannot write defaults over what is really on disk.
 */
let cached: AppPreferences | null = null;

/** In-flight load, so concurrent callers share one round-trip rather than racing. */
let inFlight: Promise<AppPreferences | null> | null = null;

/**
 * Serialises patches. Two overlapping patches — a theme change and a
 * full-screen transition landing together — would each read the same base and
 * write independently, and the later completion would drop the earlier field,
 * which is exactly the clobber GSS-FR-20 exists to prevent. Chaining makes the
 * second read the first's result.
 */
let queue: Promise<unknown> = Promise.resolve();

function enqueue<T>(op: () => Promise<T>): Promise<T> {
  const run = queue.then(op, op);
  // Swallow rejections on the chain itself, so one failed patch does not reject
  // every patch queued behind it.
  queue = run.then(
    () => undefined,
    () => undefined,
  );
  return run;
}

export const DEFAULT_APP_PREFERENCES: AppPreferences = {
  theme: "system",
  mainWindowFullscreen: false,
  // GSS-FR-21 / SCH-FR-13: a user who has never chosen a mode starts in
  // case-insensitive literal.
  searchQueryMode: "literal_insensitive",
  // GSS-FR-24 / DFV-FR-23: a user who has never chosen starts in Unified and
  // Source.
  diffVisualizationMode: "unified",
  diffRenderingMode: "source",
  // GSS-FR-25 / CHG-FR-34: a user who has never chosen starts on Commit.
  changesCommitAction: "commit",
  // GSS-FR-32 / GLS-FR-25: notifications are on until the author turns them
  // off. `true` rather than `false` deliberately — the field is absent from
  // every record written before it existed, and a `false` default would turn
  // notifications off for every existing install on upgrade.
  notificationsEnabled: true,
  // GSS-FR-33 / GLS-FR-28: the panel follows the active tab until the author
  // turns it off. `true` rather than `false` for the reason above — the field is
  // absent from every record written before it existed, and the requirement is
  // explicit that those records read back as the behaviour ON.
  selectionFollowsTab: true,
  // GSS-FR-34 / GRH-FR-MCHQ: a rail the author has never sized takes a fifth of
  // the Runs panel's usable content width.
  graduationRailWidthFraction: 0.2,
  // GSS-FR-QDNV / GRU-FR-KWRB: a paths column the author has never sized takes
  // 30 % of the run region's two columns.
  graduationPathsWidthFraction: 0.3,
  // GSS-FR-MSPQ / GIT-FR-FATV: a files column the author has never sized takes
  // 30 % of the Git panel's large view.
  gitFilesWidthFraction: 0.3,
};

/**
 * Read the record, caching it. A backend that rejects (or a non-Tauri context)
 * yields the defaults rather than throwing — every consumer here would
 * otherwise have to invent the same fallback, and none of them can do anything
 * more useful with the failure.
 */
export async function loadAppPreferences(): Promise<AppPreferences> {
  if (cached) return { ...cached };
  if (!inFlight) {
    inFlight = api
      .loadAppPreferences()
      .then((prefs) => {
        cached = { ...DEFAULT_APP_PREFERENCES, ...(prefs ?? {}) };
        return cached;
      })
      // Leave `cached` null: the read failed, so we do not know the stored
      // record. A later patch re-reads rather than writing these defaults over
      // whatever is really on disk.
      .catch(() => null)
      .finally(() => {
        inFlight = null;
      });
  }
  const loaded = await inFlight;
  // A copy, always: handing out the cached object by reference would let any
  // consumer mutate the process-wide record.
  return { ...(loaded ?? DEFAULT_APP_PREFERENCES) };
}

/**
 * GSS-FR-20: write `patch` on top of the current record, sending every field.
 *
 * Rejects if the record cannot be read or the write fails, so callers that show
 * the value optimistically (the theme selector) can roll back. Rejecting on an
 * unreadable record is deliberate: writing `{ ...defaults, ...patch }` would
 * silently reset the *other* field — a transient read failure at startup would
 * turn a full-screen toggle into "and your theme is back to system".
 *
 * The cache is advanced only once the write lands, so a failed write leaves the
 * next patch building on the last value that really reached disk.
 */
export async function patchAppPreferences(
  patch: Partial<AppPreferences>,
): Promise<AppPreferences> {
  return enqueue(async () => {
    if (!cached) await loadAppPreferences();
    if (!cached) {
      throw new Error(
        "app preferences are unreadable; refusing to overwrite them with defaults",
      );
    }
    const next: AppPreferences = { ...cached, ...patch };
    await api.saveAppPreferences(next);
    cached = next;
    // GLS-FR-05 / GLS-FR-20: the theme and the three typographic roles apply to
    // the WHOLE application immediately, and since SWN-FR-01 the window that
    // edits them is a child window rather than a tab of the one showing most of
    // it. Each window holds its own copy of this module, so the other one hears
    // about the write here or not at all.
    void emitAppPreferencesChanged();
    return { ...next };
  });
}

/**
 * Re-read the record from the backend, replacing the cache.
 *
 * What a window does when it hears that the *other* window wrote the record
 * (`APP_PREFERENCES_CHANGED`). Since SWN-FR-01 the Appearance section lives in a
 * child window with its own copy of this module, so the main window's cache is
 * the one thing that would otherwise go on describing the theme and the fonts
 * as they were before the author changed them.
 *
 * Queued behind any patch in flight, so a refresh cannot land between a patch's
 * read and its write and leave the cache describing the older record.
 */
export async function refreshAppPreferences(): Promise<AppPreferences> {
  return enqueue(async () => {
    try {
      const prefs = await api.loadAppPreferences();
      cached = { ...DEFAULT_APP_PREFERENCES, ...(prefs ?? {}) };
      return { ...cached };
    } catch {
      // A failed re-read leaves the cache exactly as it was: it is the last
      // record we know really reached disk, and defaults would be a fiction.
      return { ...(cached ?? DEFAULT_APP_PREFERENCES) };
    }
  });
}

/** The cached record without a round-trip, or `null` before the first load. */
export function peekAppPreferences(): AppPreferences | null {
  return cached ? { ...cached } : null;
}

/** Drop the cache. Tests only — production holds one record for the app's life. */
export function resetAppPreferencesCache(): void {
  cached = null;
  inFlight = null;
  queue = Promise.resolve();
}
