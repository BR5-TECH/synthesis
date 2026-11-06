/**
 * How the main branch write surface reads the guard (MBW-FR-RMCO,
 * MBW-FR-XBJS).
 *
 * The reads go through the backend by default. A test replaces them, so the
 * many tests that are not about this surface get a guard of a branch that is
 * not a main branch and see no confirmation (per `src/test/setup.ts`).
 */
import * as api from "../../api";
import type { BranchWriteGuard } from "../../types";

export interface BranchWriteGuardReads {
  /** `"get branch write guard (branch, refresh)"`. `null` is the active branch. */
  read(branch: string | null, refresh: boolean): Promise<BranchWriteGuard>;
  /** `"confirm main branch write (branch)"`. */
  confirm(branch: string): Promise<BranchWriteGuard>;
  /**
   * The answer of a read that the reads know with no call, or `null`. The
   * backend reads know none. The default reads of the tests know every answer,
   * so a test that is not about this surface never waits for a read.
   */
  known?(branch: string | null): BranchWriteGuard | null;
}

/** The reads of the backend. */
export const backendReads: BranchWriteGuardReads = {
  read: (branch, refresh) => api.getBranchWriteGuard(branch, refresh),
  confirm: (branch) => api.confirmMainBranchWrite(branch),
};

/** The guard of a branch that is not a main branch. */
export const NOT_MAIN_GUARD: BranchWriteGuard = {
  branch: "feature",
  isDefault: false,
  protection: "not_applicable",
  isMain: false,
  confirmed: false,
};

/** Reads that answer every branch as one that is not a main branch. */
export const notMainReads: BranchWriteGuardReads = {
  read: (branch) => Promise.resolve({ ...NOT_MAIN_GUARD, branch: branch ?? "feature" }),
  known: (branch) => ({ ...NOT_MAIN_GUARD, branch: branch ?? "feature" }),
  confirm: (branch) =>
    Promise.resolve({ ...NOT_MAIN_GUARD, branch, confirmed: true }),
};

let reads: BranchWriteGuardReads = backendReads;

export function guardReads(): BranchWriteGuardReads {
  return reads;
}

/** Replace the reads. For tests. */
export function setBranchWriteGuardReadsForTest(next: BranchWriteGuardReads): void {
  reads = next;
}
