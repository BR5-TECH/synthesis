/**
 * The main branch write flow (MBW-FR-RMCO, MBW-FR-YSMC, MBW-FR-ILFN,
 * MBW-FR-PJTG): read the guard before a write, ask for the confirmation where
 * it is needed, invoke the write, and answer a refusal of the backend with the
 * confirmation and one more invocation.
 */
import { logInfo, logWarn } from "../../logging";
import type { BranchWriteGuard, MainBranchWriteKind } from "../../types";
import { guardReads } from "./reads";
import { requestMainBranchWrite } from "./requests";

export const ERR_UNCONFIRMED = "main_branch_write_unconfirmed";
export const ERR_PROTECTED = "main_branch_protected";

/** What a write answers when the author did not let it run. */
export const WRITE_CANCELLED: unique symbol = Symbol("main branch write cancelled");
export type WriteCancelled = typeof WRITE_CANCELLED;

/**
 * Whether the guard asks for a confirmation or a hard block before a write.
 * MBW-FR-ILFN: the backend holds the confirmation for the project and the
 * branch, so a guard read on any surface after a confirm answers `confirmed`.
 */
export function needsConfirmation(guard: BranchWriteGuard): boolean {
  if (guard.protection === "protected") return true;
  return guard.isMain && !guard.confirmed;
}

/** MBW-FR-YSMC: the refusal of a write, with the guard it carries. */
export interface MainBranchRefusal {
  code: typeof ERR_UNCONFIRMED | typeof ERR_PROTECTED;
  guard: BranchWriteGuard | null;
}

export function parseMainBranchRefusal(error: unknown): MainBranchRefusal | null {
  const raw = String(error instanceof Error ? error.message : error ?? "")
    .replace(/^Error:\s*/, "")
    .trim();
  const code = raw.startsWith(ERR_UNCONFIRMED)
    ? ERR_UNCONFIRMED
    : raw.startsWith(ERR_PROTECTED)
      ? ERR_PROTECTED
      : null;
  if (!code) return null;
  const at = raw.indexOf(": ");
  let guard: BranchWriteGuard | null = null;
  if (at !== -1) {
    try {
      guard = JSON.parse(raw.slice(at + 2)) as BranchWriteGuard;
    } catch {
      guard = null;
    }
  }
  return { code, guard };
}

/** MBW-FR-PJTG: one record of the surface, with no file content. */
export function logConfirmation(
  event: "shown" | "confirmed" | "cancelled" | "blocked",
  write: MainBranchWriteKind,
  branch: string | null,
  surface: "dialog" | "inline",
): void {
  const fields = { branch: branch ?? "", write, surface };
  if (event === "blocked") logWarn(["frontend"], "main branch write blocked", fields);
  else logInfo(["frontend"], `main branch write confirmation ${event}`, fields);
}

/**
 * MBW-FR-RMCO: read the guard of `branch` and ask the author where it is
 * needed. Resolves `true` when the write may be invoked.
 */
export async function ensureMainBranchWrite(
  write: MainBranchWriteKind,
  branch: string | null = null,
): Promise<boolean> {
  let guard: BranchWriteGuard;
  try {
    guard = await guardReads().read(branch, false);
  } catch (e) {
    // MBW-FR-UNAN: a failed read shows the confirmation with the error.
    logWarn(["frontend"], "branch write guard read failed", { write });
    return requestMainBranchWrite({ write, branch, guard: null, readError: String(e) });
  }
  if (!needsConfirmation(guard)) return true;
  return requestMainBranchWrite({ write, branch, guard, readError: null });
}

/**
 * MBW-FR-RMCO / MBW-FR-YSMC: run `invoke` as a write to `branch`. The guard is
 * read first unless `ensured` says the surface already holds a confirmation.
 * A refusal of the backend shows the confirmation with the guard it carries,
 * and after a confirm the write is invoked once more.
 */
export async function runMainBranchWrite<T>(
  write: MainBranchWriteKind,
  invoke: () => Promise<T>,
  options: { branch?: string | null; ensured?: boolean } = {},
): Promise<T | WriteCancelled> {
  const branch = options.branch ?? null;
  if (!options.ensured && !(await ensureMainBranchWrite(write, branch))) {
    return WRITE_CANCELLED;
  }
  try {
    return await invoke();
  } catch (e) {
    const refusal = parseMainBranchRefusal(e);
    if (!refusal) throw e;
    logWarn(["frontend"], "main branch write refused by the backend", {
      write,
      code: refusal.code,
      branch: refusal.guard?.branch ?? "",
    });
    const answer = await requestMainBranchWrite({
      write,
      branch: refusal.guard?.branch ?? branch,
      guard: refusal.guard,
      readError: null,
    });
    if (!answer) return WRITE_CANCELLED;
    return await invoke();
  }
}
