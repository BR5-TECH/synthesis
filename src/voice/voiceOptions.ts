/**
 * VOM-FR-UALP, AGR-FR-EPYX: the speed and the style of an agent's voice.
 */
import type { Agent, VoiceOptions } from "../types";

/** The options that the agent sets, or null where it sets none. */
export function agentVoiceOptions(
  agent: Pick<Agent, "voiceSpeedPercent" | "voiceStyle"> | undefined,
): VoiceOptions | null {
  if (!agent) return null;
  const speedPercent = agent.voiceSpeedPercent ?? null;
  const style = agent.voiceStyle?.trim() ?? "";
  return speedPercent === null && style === "" ? null : { speedPercent, style };
}
