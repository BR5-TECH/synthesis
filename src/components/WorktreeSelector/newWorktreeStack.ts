/**
 * The stack step of the New worktree dialog
 * (`../../../specifications/ui/WTS-worktree-selector.md` WTS-FR-HUQW,
 * WTS-FR-PZEO, WTS-FR-GDKX, WTS-FR-XQOB, WTS-FR-BMQI, WTS-FR-KVPA,
 * WTS-FR-DXCE; `NTF-notifications.md` NTF-FR-HCWM;
 * `../../../specifications/core/WTC-worktree-context.md` WTC-FR-SBNV).
 *
 * A new branch that the author creates from a layer of a GitHub stack is the
 * next layer of that stack. The dialog reads the stack of the active branch
 * when it opens, and refuses a new branch on a layer that has no open pull
 * request. On a layer with one, the new branch starts at the stack top
 * (WTC-FR-NQZE), and after the worktree is created the dialog adds the new
 * branch on top of the stack. `create_worktree` itself reads and writes no
 * stack (WTC-FR-SBNV). A read that fails or still runs blocks nothing.
 */
import { useEffect, useRef, useState } from "react";

import * as api from "../../api";
import { logInfo, logWarn } from "../../logging";
import { showToast } from "../../state/toasts";
import type { BranchStack, WorktreeContext } from "../../types";
import { stackCauseWords } from "../CreatePullRequest/stackMessages";
import { parseRejection } from "../Git/errors";

/** NTF-FR-HCWM: the key of the toast of a branch not added to its stack. */
export const WORKTREE_STACK_TOAST_KEY = "worktree-stack";

/** What the dialog read when it opened. Null is a read that runs or failed. */
export interface NewWorktreeStackState {
  /** The active branch of the opening, or null for a detached `HEAD`. */
  activeBranch: string | null;
  stack: BranchStack | null;
  localBranches: string[] | null;
  defaultBranch: string | null;
}

/** The reads of one opening, so the step after the creation can wait for them. */
export interface PendingStackReads {
  activeBranch: string;
  stack: Promise<BranchStack | null>;
  localBranches: Promise<string[] | null>;
  defaultBranch: Promise<string | null>;
}

const NOTHING_READ: NewWorktreeStackState = {
  activeBranch: null,
  stack: null,
  localBranches: null,
  defaultBranch: null,
};

/**
 * WTC-FR-XLXY: the name `create_worktree` makes of `raw` when no local branch
 * has `raw` itself. Unlike the field's own rule, no `-` or `/` stays at an end.
 */
export function finalBranchName(raw: string): string {
  return raw
    .toLowerCase()
    .replace(/[^a-z0-9_/-]/g, "-")
    .split("/")
    .map((segment) => segment.replace(/-+/g, "-").replace(/^-+|-+$/g, ""))
    .filter((segment) => segment !== "")
    .join("/");
}

/**
 * WTS-FR-PZEO: the Branch field names a new branch, as WTC-FR-14 resolves a
 * name. An unknown list of local branches names no new branch, so it blocks
 * nothing and stacks nothing.
 */
export function namesNewBranch(typed: string, localBranches: string[] | null): boolean {
  if (!localBranches) return false;
  const trimmed = typed.trim();
  if (trimmed === "" || localBranches.includes(trimmed)) return false;
  const normalized = finalBranchName(trimmed);
  if (normalized === "") return false;
  return !localBranches.some((name) => name.toLowerCase() === normalized);
}

/**
 * WTS-FR-XQOB / WTS-FR-KVPA: how a new branch goes on the stack. `start` is
 * the branch the new branch starts at, and `parent` the branch it stands on.
 * Both are the stack top.
 */
export interface StackPlan {
  start: string;
  parent: string;
}

/**
 * WTS-FR-XQOB: the plan for a new branch on any layer of a stack that has an
 * open pull request, or null. An answer with no top has no plan.
 */
export function stackPlanFor(
  stack: BranchStack | null,
  activeBranch: string,
  typed: string,
  localBranches: string[] | null,
): StackPlan | null {
  if (!stack || stack.branch !== activeBranch) return null;
  if (!stack.available || !stack.inStack || !stack.pullRequest) return null;
  if (!namesNewBranch(typed, localBranches)) return null;
  const top = stack.top ?? (stack.isTop ? activeBranch : null);
  return top ? { start: top, parent: top } : null;
}

/**
 * WTS-FR-GDKX / WTS-FR-BMQI: why Create is unavailable for `typed`, or null.
 * Only an available answer for a new branch blocks. The active branch without
 * an open pull request comes first, then a stack top without one.
 */
export function newWorktreeStackBlock(
  state: NewWorktreeStackState,
  typed: string,
): string | null {
  const { stack, activeBranch } = state;
  if (!stack || !activeBranch || stack.branch !== activeBranch) return null;
  if (!stack.available || !stack.inStack) return null;
  if (!namesNewBranch(typed, state.localBranches)) return null;
  const example = state.defaultBranch ?? "the default branch";
  const advice = `Create the worktree from a branch that is not in a stack, for example ${example}.`;
  if (!stack.pullRequest) {
    return `${activeBranch} is in a stack and has no open pull request. ${advice}`;
  }
  if (stack.top && !stack.topHasPullRequest) {
    return `The top of the stack, ${stack.top}, has no open pull request. ${advice}`;
  }
  return null;
}

/** What Create does, after the reads of the opening have answered. */
export interface CreatePlan {
  /** WTS-FR-GDKX: the reason that Create starts nothing, or null. */
  block: string | null;
  stack: StackPlan | null;
}

/**
 * WTS-FR-XQOB: wait for the reads of the opening that still run, then decide
 * what Create does for `typed`. A read that failed gives no block and no plan.
 */
export async function planCreate(reads: PendingStackReads, typed: string): Promise<CreatePlan> {
  const [stack, localBranches, defaultBranch] = await Promise.all([
    reads.stack,
    reads.localBranches,
    reads.defaultBranch,
  ]);
  const block = newWorktreeStackBlock(
    { activeBranch: reads.activeBranch, stack, localBranches, defaultBranch },
    typed,
  );
  if (block) return { block, stack: null };
  return { block: null, stack: stackPlanFor(stack, reads.activeBranch, typed, localBranches) };
}

/**
 * WTS-FR-HUQW: the reads of the dialog while it is open on the named branch
 * `activeBranch`. A detached `HEAD` (null) reads nothing.
 */
export function useNewWorktreeStack(activeBranch: string | null, dialogOpen: boolean) {
  const [state, setState] = useState<NewWorktreeStackState>(NOTHING_READ);
  const pending = useRef<PendingStackReads | null>(null);

  useEffect(() => {
    setState({ ...NOTHING_READ, activeBranch });
    pending.current = null;
    if (!dialogOpen || !activeBranch) return;
    let cancelled = false;
    const failed = (read: string) => (error: unknown) => {
      // WTS-FR-BMQI: a read that fails adds no block.
      logWarn(["frontend", "remote"], "new worktree stack read failed", {
        read,
        branch: activeBranch,
        error: parseRejection(error).code,
      });
      return null;
    };
    const stack = api.getBranchStack(activeBranch).catch(failed("stack"));
    const localBranches = api
      .listBranches()
      .then((rows) => rows.filter((row) => row.kind === "local").map((row) => row.name))
      .catch(failed("branches"));
    const defaultBranch = api
      .getDefaultBranch()
      .then((name) => name || null)
      .catch(failed("default branch"));
    pending.current = { activeBranch, stack, localBranches, defaultBranch };
    void stack.then((answer) => {
      if (!cancelled) setState((s) => ({ ...s, stack: answer ?? null }));
    });
    void localBranches.then((names) => {
      if (!cancelled) setState((s) => ({ ...s, localBranches: names }));
    });
    void defaultBranch.then((name) => {
      if (!cancelled) setState((s) => ({ ...s, defaultBranch: name }));
    });
    return () => {
      cancelled = true;
    };
  }, [activeBranch, dialogOpen]);

  return { state, pending };
}

/** The branch of the active worktree that `create_worktree` answered. */
function createdBranch(context: WorktreeContext, typed: string): string {
  const created = context.worktrees.find((w) => w.isActive)?.branch;
  return created ?? finalBranchName(typed);
}

/**
 * WTS-FR-KVPA / WTS-FR-DXCE: after `create_worktree` succeeded, add its new
 * branch on top of the stack, with the stack top of the plan as its parent. A
 * failure leaves the worktree as it is and shows the toast of NTF-FR-HCWM.
 */
export async function stackCreatedBranch(
  plan: StackPlan,
  typed: string,
  context: WorktreeContext,
): Promise<void> {
  const { parent } = plan;
  const branch = createdBranch(context, typed);
  try {
    await api.addBranchToStack(branch, parent);
    logInfo(["frontend", "remote"], "new worktree branch added to stack", {
      branch,
      parent,
    });
  } catch (error: unknown) {
    const code = parseRejection(error).code;
    logWarn(["frontend", "remote"], "new worktree branch not added to stack", {
      branch,
      parent,
      error: code,
    });
    showToast({
      key: WORKTREE_STACK_TOAST_KEY,
      level: "Error",
      title: "Branch not added to the stack",
      body: `${branch} is not added to the stack on ${parent}, because ${stackCauseWords(error)}. The worktree stays created.`,
      address: null,
    });
  }
}
