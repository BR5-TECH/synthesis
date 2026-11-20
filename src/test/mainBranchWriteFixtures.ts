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
  /** The repository of each confirm command; absent for the main one. */
  confirmRepositories: (string | undefined)[];
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
  /**
   * The confirmations the fake backend holds, by repository and branch
   * (GSM-FR-KGOP): a submodule's `main` is not the main repository's `main`.
   */
  const recorded = new Set<string>();
  const keyOf = (repository: string | undefined, branch: string) => `${repository ?? ""}|${branch}`;
  const answered = (guard: BranchWriteGuard): BranchWriteGuard => ({
    ...guard,
    confirmed: guard.confirmed || recorded.has(keyOf(guard.repository, guard.branch)),
  });
  const reads: ScriptedReads = {
    reads: [],
    confirms: [],
    confirmRepositories: [],
    answer,
    confirmError: null,
    read(branch, refresh) {
      reads.reads.push({ branch, refresh });
      const next = reads.answer;
      return next instanceof Error ? Promise.reject(next) : Promise.resolve(answered(next));
    },
    confirm(branch, repository) {
      reads.confirms.push(branch);
      reads.confirmRepositories.push(repository);
      if (reads.confirmError) return Promise.reject(reads.confirmError);
      const base = reads.answer instanceof Error ? mainGuard({ branch }) : reads.answer;
      const guard = { ...base, branch, repository };
      // MBW-FR-JZME: a protected branch is recorded like any main branch.
      recorded.add(keyOf(repository, branch));
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
