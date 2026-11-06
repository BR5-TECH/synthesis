/**
 * Fixtures of the main branch write tests (`MBW-main-branch-write.md`).
 *
 * `src/test/setup.ts` answers every guard read as a branch that is not a main
 * branch. A test of the confirmation installs `scriptedReads` instead, which
 * answer from a script and record each call.
 */
import {
  setBranchWriteGuardReadsForTest,
  type BranchWriteGuardReads,
} from "../state/mainBranchWrite";
import type { BranchWriteGuard } from "../types";

/** The guard of an unprotected default branch with no confirmation. */
export function mainGuard(overrides: Partial<BranchWriteGuard> = {}): BranchWriteGuard {
  return {
    branch: "main",
    isDefault: true,
    protection: "unprotected",
    isMain: true,
    confirmed: false,
    ...overrides,
  };
}

export interface ScriptedReads extends BranchWriteGuardReads {
  reads: { branch: string | null; refresh: boolean }[];
  /** Each confirm command, as the fake backend received it. */
  confirms: string[];
  /** The guard the next reads answer with, or an error to reject with. */
  answer: BranchWriteGuard | Error;
  /** An error the next confirms reject with. */
  confirmError: Error | null;
}

/**
 * Reads that answer from a script, and install them.
 *
 * They answer as the backend does (`GTC-git.md` GTC-FR-YWRZ): the confirm
 * command records the branch, and a later read of that branch answers
 * `confirmed` because of that record. Nothing else sets `confirmed`, so a
 * surface that never sends the confirm never sees it.
 */
export function scriptedReads(answer: BranchWriteGuard | Error = mainGuard()): ScriptedReads {
  /** The branches the fake backend holds a confirmation for. */
  const recorded = new Set<string>();
  const answered = (guard: BranchWriteGuard): BranchWriteGuard => ({
    ...guard,
    confirmed: guard.confirmed || recorded.has(guard.branch),
  });
  const reads: ScriptedReads = {
    reads: [],
    confirms: [],
    answer,
    confirmError: null,
    read(branch, refresh) {
      reads.reads.push({ branch, refresh });
      const next = reads.answer;
      return next instanceof Error ? Promise.reject(next) : Promise.resolve(answered(next));
    },
    confirm(branch) {
      reads.confirms.push(branch);
      if (reads.confirmError) return Promise.reject(reads.confirmError);
      const base = reads.answer instanceof Error ? mainGuard({ branch }) : reads.answer;
      const guard = { ...base, branch };
      // The backend refuses a protected branch and records nothing.
      if (guard.protection === "protected") return Promise.reject(protectedRefusal(guard));
      recorded.add(branch);
      return Promise.resolve(answered(guard));
    },
  };
  setBranchWriteGuardReadsForTest(reads);
  return reads;
}

/** The refusal text the backend answers an unconfirmed write with. */
export function unconfirmedRefusal(guard: BranchWriteGuard = mainGuard()): string {
  return `main_branch_write_unconfirmed: ${JSON.stringify(guard)}`;
}

/** The refusal text the backend answers a write to a protected branch with. */
export function protectedRefusal(
  guard: BranchWriteGuard = mainGuard({ protection: "protected" }),
): string {
  return `main_branch_protected: ${JSON.stringify(guard)}`;
}
