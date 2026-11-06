/**
 * The hold of the first change in an editing surface (`EDT-editor.md`
 * EDT-FR-QXEK, EDT-FR-UPNU).
 *
 * An editing surface reads the guard of its file's branch when it opens the
 * file. Every file an editor opens is on the branch of the active worktree,
 * so one gate serves every surface. While the gate holds, a change of the
 * author is not applied and the confirmation shows. After a confirm, changes
 * apply as usual. After a cancel, the next change asks again.
 *
 * Content that the editor writes itself, a load or an external change, never
 * reaches the gate.
 */
import { logInfo, logWarn } from "../../logging";
import type { BranchWriteGuard } from "../../types";
import { ERR_PROTECTED, needsConfirmation, parseMainBranchRefusal } from "./flow";
import { guardReads } from "./reads";
import { requestMainBranchWrite } from "./requests";

let guard: BranchWriteGuard | null = null;
/** MBW-FR-UNAN: why the last read failed. A failed read holds the change. */
let readError: string | null = null;
let asking = false;
/** A counter, so a read that a later read replaced is ignored. */
let generation = 0;
/**
 * Whether a read is under way. Until it lands the branch is not known, so a
 * change is held and nothing shows yet.
 */
let reading = false;
/** A change was held while the read was under way. */
let heldWhileReading = false;
/** The worktree and branch the last read was for (`noteEditContext`). */
let context: string | null = null;

/** EDT-FR-QXEK: read the guard when a file opens. Never awaited by a render. */
export function primeEditGate(): void {
  const mine = ++generation;
  const known = guardReads().known?.(null) ?? null;
  if (known) {
    guard = known;
    readError = null;
    reading = false;
    return;
  }
  reading = true;
  guardReads()
    .read(null, false)
    .then((next) => {
      if (mine !== generation) return;
      guard = next;
      readError = null;
    })
    .catch((e) => {
      if (mine !== generation) return;
      guard = null;
      readError = String(e);
      logWarn(["frontend"], "branch write guard read failed", { write: "edit_file" });
    })
    .finally(() => {
      if (mine !== generation) return;
      reading = false;
      // A change held while the branch was not known asks now, where it must.
      // The held change itself is not applied: the next one is.
      const ask = heldWhileReading;
      heldWhileReading = false;
      if (ask) holdEdit();
    });
}

/**
 * EDT-FR-QXEK: the worktree or the branch of the open files changed while a
 * file stays open, so the gate reads the guard of the new branch. The first
 * call only records the context.
 */
export function noteEditContext(next: string): void {
  const previous = context;
  context = next;
  if (previous === null || previous === next) return;
  logInfo(["frontend"], "edit gate reads the guard of a new branch", {});
  primeEditGate();
}

/**
 * MBW-FR-ILFN: a confirmation on any surface opens the gate for that branch.
 * The surfaces call it with the guard that the confirmation answered.
 */
export function rememberConfirmation(confirmed: BranchWriteGuard): void {
  if (guard && guard.branch === confirmed.branch) guard = { ...guard, confirmed: true };
}

/** Whether a read of the guard is under way. */
export function isEditGateReading(): boolean {
  return reading;
}

/** Whether the next change of the author is held. */
export function isEditHeld(): boolean {
  return reading || readError !== null || (guard !== null && needsConfirmation(guard));
}

/**
 * EDT-FR-QXEK: call before a change of the author is applied. Returns `true`
 * when the change must not be applied; the confirmation then shows.
 */
export function holdEdit(): boolean {
  if (!isEditHeld()) return false;
  if (reading) {
    heldWhileReading = true;
    return true;
  }
  if (!asking) {
    asking = true;
    const held = guard;
    void requestMainBranchWrite({
      write: "edit_file",
      branch: held?.branch ?? null,
      guard: readError === null ? held : null,
      readError,
    })
      .then((confirmed) => {
        // A dialog that read the guard again and let the write go also answers
        // a failed read; the next read tells the gate what holds now.
        if (confirmed && readError !== null) primeEditGate();
      })
      .finally(() => {
        asking = false;
      });
  }
  return true;
}

/**
 * EDT-FR-QXEK / MBW-FR-YSMC: the words a surface shows for a save that the
 * backend refused for the main branch write check, or `null` for any other
 * failure. The refusal carries its guard as data, which is never shown. An
 * autosave never asks for the confirmation (MBW-FR-KDCM), so the gate reads the
 * guard again, and the next change of the author asks for it.
 */
export function mainBranchSaveRefusal(error: unknown): string | null {
  const refusal = parseMainBranchRefusal(error);
  if (!refusal) return null;
  const branch = refusal.guard?.branch || "this branch";
  primeEditGate();
  if (refusal.code === ERR_PROTECTED) {
    return `Not saved: GitHub protects ${branch}. Work reaches it through a pull request. Start a new branch for this work.`;
  }
  return `Not saved: ${branch} is a main branch, and the write to it is not confirmed. Change the file again to confirm the write.`;
}

export function resetEditGateForTest(): void {
  guard = null;
  readError = null;
  asking = false;
  reading = false;
  heldWhileReading = false;
  context = null;
  generation++;
}
