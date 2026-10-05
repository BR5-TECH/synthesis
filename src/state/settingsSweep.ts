/**
 * The save-before-close sweep of a settings window
 * (`specifications/ui/SWN-settings-windows.md` SWN-FR-08 through SWN-FR-12).
 *
 * A settings window writes **every pending change in every one of its
 * sections** before it goes — not only the section on screen — so leaving one
 * never asks the author to choose between their edits and their exit
 * (SWN-FR-10). Only the sections know what they are holding, so each publishes
 * itself here while it is mounted and the window's close path spends the
 * registry.
 *
 * A **pending change** is either a section holding a dirty state or a settings
 * write already in flight (SWN-FR-09). A write already in flight is *awaited*
 * rather than started again, which is the section's own responsibility: `save`
 * is asked for exactly once per sweep, and a section with a write outstanding
 * returns that write's promise rather than issuing a second one.
 *
 * Module state rather than React state, deliberately: a section that is not the
 * one on screen is not mounted, and the sweep still has to reach what it holds.
 * The registry therefore holds a *handle* per section rather than a component,
 * and a section registers on mount and deregisters on unmount — leaving behind
 * whatever a still-mounted section can answer for.
 */

/** What one section publishes about the changes it is holding. */
export interface SettingsSection {
  /**
   * The section's own key within its window, so a failure can be presented
   * rather than left behind whichever section the author happens to be on
   * (SWN-FR-11).
   */
  section: string;
  /**
   * Whether anything is pending right now — a dirty state, or a write already
   * in flight. A section with nothing pending is not asked to save.
   */
  pending: () => boolean;
  /**
   * Write what is pending, resolving `true` once it has landed and `false` when
   * it failed. A section that already has a write in flight awaits that one
   * rather than issuing a second (SWN-FR-09).
   */
  save: () => Promise<boolean>;
}

/** The outcome of one sweep. */
export type SweepResult =
  | { ok: true }
  /** SWN-FR-11: the section whose save failed, so the window can present it. */
  | { ok: false; section: string };

const sections = new Map<string, SettingsSection>();

/**
 * The sweep in flight, or null (SWN-FR-12).
 *
 * A promise rather than a flag, so a request that arrives mid-sweep **joins**
 * the one already running rather than starting a second or being told, falsely,
 * that there was nothing to write.
 */
let running: Promise<SweepResult> | null = null;

/**
 * Publish a section's pending changes for the life of its mount.
 *
 * Keyed on the section, so a remount replaces its own entry rather than
 * accumulating a second one that would be asked to save an unmounted surface's
 * state. Returns the deregistration the section calls on unmount.
 */
export function registerSettingsSection(entry: SettingsSection): () => void {
  sections.set(entry.section, entry);
  return () => {
    if (sections.get(entry.section) === entry) sections.delete(entry.section);
  };
}

/**
 * The sections holding a pending change right now, as the sweep would find them
 * (SWN-FR-09).
 *
 * The direct answer to "does this section contribute to the save-before-close
 * sweep?" — which several sections' specs make a claim about (GLS-FR-15,
 * GLS-FR-20, GLS-FR-25, GLS-FR-28, AGT-FR-21, AII-FR-28, SET-FR-17) and which
 * is otherwise only observable by closing the window and watching what gets
 * written.
 *
 * A section whose `pending()` throws is reported as pending: the sweep will ask
 * it to save and take the throw as a failure (SWN-FR-11), so saying it holds
 * nothing here would describe the opposite of what happens.
 */
export function settingsSectionsPending(): string[] {
  return [...sections.values()]
    .filter((entry) => {
      try {
        return entry.pending();
      } catch {
        return true;
      }
    })
    .map((entry) => entry.section);
}

/** Whether a save sweep is currently running (SWN-FR-12). */
export function settingsSweepRunning(): boolean {
  return running !== null;
}

/**
 * SWN-FR-08 / SWN-FR-11: write every pending change in every section.
 *
 * Every pending section is asked, not only up to the first failure: the author
 * asked to leave, and a section that *can* be written should be, so a retry of
 * the one that could not is the only thing left outstanding. A section that
 * saved successfully in the same sweep is clean afterwards, so a second close
 * request does not write it again.
 *
 * The first failure is what the window presents. A request that arrives while a
 * sweep is running **joins** it (SWN-FR-12): a second close request, a request
 * for the same window, and a request for the other one each start no further
 * save, duplicate no write already in flight, and get the running sweep's own
 * answer rather than a fabricated one.
 */
export function runSettingsSaveSweep(): Promise<SweepResult> {
  if (running) return running;
  const pass = sweep();
  running = pass;
  void pass.finally(() => {
    if (running === pass) running = null;
  });
  return pass;
}

async function sweep(): Promise<SweepResult> {
  let failed: string | null = null;
  // Snapshotted before the loop: a save that unmounts a section (or mounts one)
  // must not shorten the sweep out from under it.
  for (const entry of [...sections.values()]) {
    let ok = false;
    try {
      // Inside the `try` with the save, deliberately: a section that throws
      // while merely being ASKED what it holds would otherwise abort the whole
      // pass, and the window would never answer at all — leaving the author
      // with a settings window that cannot be closed and a parent that cannot
      // be reached (SWN-FR-02, SWN-FR-12).
      if (!entry.pending()) continue;
      ok = await entry.save();
    } catch {
      // A section that threw rather than reporting is a failed save; the window
      // stays open either way (SWN-FR-11).
      ok = false;
    }
    if (!ok && failed === null) failed = entry.section;
  }
  return failed === null ? { ok: true } : { ok: false, section: failed };
}

/** Drop every registration. Tests only — a window holds its sections for life. */
export function resetSettingsSections(): void {
  sections.clear();
  running = null;
}

/**
 * A request to present a named section (SWN-FR-06, SWN-FR-11, SWN-FR-13).
 *
 * Carries a nonce rather than being a bare key, for the same reason the agent
 * editor request does: the same section asked for twice must present twice, and
 * a request left standing would fight the author every time they navigated away
 * from it.
 */
export interface SectionRequest {
  section: string;
  nonce: number;
}
