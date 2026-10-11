/**
 * Which agentic vendors Synthesis implements (AII-FR-OTQY, SET-FR-GVED).
 *
 * Only Claude Code is tested. The settings surfaces keep a tab for every other
 * vendor, but show it as "To Be Implemented..." and let the author change
 * nothing in it. The backend is not gated: this set decides only what the
 * settings UI offers.
 */
import type { AgenticVendorId } from "../types";

/** The vendors whose settings the author can change. */
export const IMPLEMENTED_AGENTIC_VENDORS: ReadonlySet<AgenticVendorId> = new Set<
  AgenticVendorId
>(["claude_code"]);

/** The statement at the top of a tab whose vendor is not implemented yet. */
export const TO_BE_IMPLEMENTED_MESSAGE = "To Be Implemented...";

/**
 * A set a test puts in place of {@link IMPLEMENTED_AGENTIC_VENDORS}, so that it
 * can reach the controls of a tab that is read-only today. `null` outside tests.
 */
let implementedForTest: ReadonlySet<AgenticVendorId> | null = null;

/** True when the author can configure this vendor in the settings UI. */
export function isAgenticVendorImplemented(vendor: AgenticVendorId): boolean {
  return (implementedForTest ?? IMPLEMENTED_AGENTIC_VENDORS).has(vendor);
}

/** Tests only: treat every vendor as implemented until the next reset. */
export function implementAllAgenticVendorsForTest(): void {
  implementedForTest = new Set<AgenticVendorId>([
    "claude_code",
    "codex",
    "opencode",
    "claude_agent_api",
    "custom_agent_api",
  ]);
}

/** Tests only: return to {@link IMPLEMENTED_AGENTIC_VENDORS}. */
export function resetImplementedAgenticVendorsForTest(): void {
  implementedForTest = null;
}
