/**
 * The main branch write surface (`specifications/ui/MBW-main-branch-write.md`).
 */
export * from "./reads";
export * from "./requests";
export * from "./flow";
export * from "./wording";
export * from "./editGate";

import { notMainReads, setBranchWriteGuardReadsForTest } from "./reads";
import { resetMainBranchWriteRequestsForTest } from "./requests";
import { resetNewWorktreeRequestsForTest } from "../newWorktreeRequests";
import { resetEditGateForTest } from "./editGate";

/**
 * Reset every store of the surface, and answer every guard read as a branch
 * that is not a main branch. `src/test/setup.ts` calls it before each test.
 */
export function resetMainBranchWriteForTest(): void {
  resetMainBranchWriteRequestsForTest();
  resetNewWorktreeRequestsForTest();
  resetEditGateForTest();
  setBranchWriteGuardReadsForTest(notMainReads);
}
