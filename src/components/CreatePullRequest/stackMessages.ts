/**
 * The causes of a stack step in words
 * (`../../../specifications/ui/NTF-notifications.md` NTF-FR-SXLR, NTF-FR-HCWM;
 * `../../../specifications/core/GTC-git.md` GTC-FR-KQVA, GTC-FR-LCYE).
 *
 * A code is not a sentence. This module is the one place that turns a typed
 * cause of a stack operation into words, so the toast of a created pull
 * request and the toast of a new worktree say the same thing about one code.
 */
import type { StackStep } from "../../types";
import { parseRejection } from "../Git/errors";

const CAUSE_WORDS: Record<string, string> = {
  stack_record_missing: "this machine holds no stack record of the branch on that base",
  stack_parent_has_no_pull_request: "the parent branch has no open pull request",
  stack_parent_not_in_stack: "the parent branch is not in a stack",
  stack_parent_not_top: "the parent branch is not the top of its stack",
  stack_unavailable: "the stacks of the repository cannot be read",
  stacks_not_enabled: "stacked pull requests are not enabled for the repository",
  stack_busy: "another change to the stack was in progress",
  store_unavailable: "the stack record of this machine cannot be written",
  github_token_rejected: "GitHub refused the token",
  github_unreachable: "GitHub could not be reached",
  github_token_missing: "no GitHub token is stored",
  github_token_selection_required: "no GitHub token is selected for the project",
  "unknown branch": "the branch does not exist",
};

/** GTC-FR-KQVA: one typed cause, in words. */
export function stackCauseWords(error: unknown): string {
  const { code, detail } = parseRejection(error);
  if (code === "stack_rejected") {
    return detail ? `GitHub refused the change of the stack: ${detail}` : "GitHub refused the change of the stack";
  }
  return CAUSE_WORDS[code] ?? `the stack operation failed (${code || "no cause"})`;
}

/** NTF-FR-SXLR: the sentence of the toast about the stack step. */
export function stackStepSentence(step: StackStep | null | undefined): string | null {
  if (!step) return null;
  if (step.state === "added" || step.state === "created") {
    return step.stackNumber !== undefined
      ? `It is added to stack #${step.stackNumber}.`
      : "It is added to its stack.";
  }
  return `It is not added to the stack, because ${stackCauseWords(step.error ?? "")}.`;
}
