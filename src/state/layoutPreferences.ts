/**
 * The per-project layout record (SNV-shell-navigation.md SNV-FR-08, backed by
 * `GSS-global-settings-storage.md` GSS-FR-17 / `PSS-project-settings-storage.md`
 * PSS-FR-06).
 *
 * `save_layout_preferences` replaces the stored record wholesale — the backend
 * does no per-field merge. Two unrelated writers share it:
 *
 *   - `useMainWindowState` writes the window's geometry and maximized state on
 *     an OS resize.
 *   - `useVerticalPanel` writes the vertical panel's width fraction when a drag
 *     ends.
 *
 * If each kept its own snapshot of the record and wrote it whole, the second
 * writer would revert the first: drag the panel, then resize the window, and
 * the panel width the user just set is gone on relaunch. This module is the one
 * place the record is read and merged, so every write composes with every write
 * before it.
 *
 * The cache is keyed by project. A project switch (OVW-FR-11) opens a different
 * slot in the backend store, so serving the previous project's record would
 * both render B with A's layout and write A's layout into B's slot.
 */
import * as api from "../api";
import type { LayoutPreferences } from "../types";

let cached: LayoutPreferences | null = null;
/** Project the cached record belongs to, so a switch cannot reuse it. */
let cachedKey: string | null = null;
let inFlight: Promise<LayoutPreferences | null> | null = null;
let inFlightKey: string | null = null;
/**
 * Serialises patches. Two overlapping patches that each read the same base and
 * write independently would lose one of the two edits; chaining makes the
 * second read the first's result.
 */
let queue: Promise<unknown> = Promise.resolve();
/**
 * The project the shell currently has open, as far as the backend is concerned.
 *
 * `save_layout_preferences` takes no project argument — it writes to whichever
 * slot the backend's `ProjectState` resolves to *at the moment it runs*. So a
 * patch issued for project A and still in flight when the shell switches to B
 * would land A's record in B's slot. Declaring the live project here lets a
 * stale patch be dropped just before it writes.
 *
 * `null` means undeclared, and every write proceeds — the guard engages only
 * once the shell has said which project is open.
 */
let activeKey: string | null = null;

/**
 * Name the project the shell has open. Called whenever that changes, including
 * with `""` when the project closes and the backend falls back to its global
 * default slot.
 */
export function setActiveLayoutProject(projectKey: string): void {
  activeKey = projectKey;
}

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

/**
 * Read the project's layout record, caching it for `projectKey`.
 *
 * Returns `{}` when the backend has no slot for this project yet (a first
 * launch, or the first time this project is opened) or when the read fails —
 * consumers substitute their own defaults for the fields they own.
 */
export async function loadLayoutPreferences(
  projectKey: string,
): Promise<LayoutPreferences> {
  if (cached && cachedKey === projectKey) return cached;
  if (!inFlight || inFlightKey !== projectKey) {
    inFlightKey = projectKey;
    inFlight = api
      .loadLayoutPreferences()
      .then((prefs) => {
        cached = prefs ?? {};
        cachedKey = projectKey;
        return cached;
      })
      .catch(() => null)
      .finally(() => {
        inFlight = null;
        inFlightKey = null;
      });
  }
  return (await inFlight) ?? {};
}

/**
 * Merge `patch` into the project's record and persist the result.
 *
 * The read is part of the same serialised step as the write, so a patch always
 * builds on the record as it stands after every earlier patch — never on a
 * snapshot taken at some hook's mount.
 */
export async function patchLayoutPreferences(
  projectKey: string,
  /**
   * The fields to write, or a function of the record as it now stands.
   *
   * A field that is a **map** must be patched through the function form. The
   * plain form replaces a map wholesale, so two writers each folding their own
   * entry into a copy they took earlier would each drop the other's — one
   * draft's split ratio wiping another draft's. The function runs inside the
   * serialised step, after the read, so it always folds into the record every
   * earlier patch has already written.
   */
  patch:
    | Partial<LayoutPreferences>
    | ((base: LayoutPreferences) => Partial<LayoutPreferences>),
): Promise<LayoutPreferences> {
  return enqueue(async () => {
    const base = await loadLayoutPreferences(projectKey);
    const fields = typeof patch === "function" ? patch(base) : patch;
    const next: LayoutPreferences = { ...base, ...fields };
    // The project may have changed while this patch waited its turn in the
    // queue, or while its read resolved. Writing now would put this project's
    // record into whichever slot the backend has since moved to, so the patch
    // is dropped instead — the layout it described belongs to a project the
    // shell is no longer showing.
    if (activeKey !== null && activeKey !== projectKey) return next;
    await api.saveLayoutPreferences(next);
    cached = next;
    cachedKey = projectKey;
    return next;
  });
}

/** Drop the cache. Tests, and any code that invalidates the open project. */
export function resetLayoutPreferencesCache(): void {
  cached = null;
  cachedKey = null;
  inFlight = null;
  inFlightKey = null;
  queue = Promise.resolve();
  activeKey = null;
}
