import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, vi } from "vitest";

import type { Mock } from "vitest";

import { Changes } from "../components/Changes";
import type { CommitFile } from "../components/CommitMessageModal";
import type { RollbackFile } from "../components/RollbackConfirm";
import type { RollbackResult } from "../types";
import type {
  BranchOption,
  ChangeEntry,
  ChangeSet,
  ChangesPanelState,
  CommitOutcome,
  DiffTarget,
  PanelRevealRequest,
  UpstreamSyncState,
} from "../types";
import { pickSelector } from "./selectors";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

export function change(
  path: string,
  over: Partial<ChangeEntry> = {},
): ChangeEntry {
  return {
    id: path,
    path,
    name: path.split("/").pop()!,
    changeStatus: "modified",
    addedLines: 1,
    removedLines: 0,
    isBinary: false,
    ...over,
  };
}

export const SKILL = change(".claude/skills/onboarding.md", {
  artifactType: "skill",
  typeSource: "inferred",
  addedLines: 12,
  removedLines: 3,
});
export const SPEC = change("specifications/ui/CHG-changes.md", {
  artifactType: "spec",
  typeSource: "inferred",
  addedLines: 91,
  removedLines: 0,
});
export const LIB = change("src/components/Library.tsx", {
  addedLines: 18,
  removedLines: 4,
});
export const HOOK = change("src/hooks/useEditHistory.ts", {
  addedLines: 64,
  removedLines: 12,
});

export function changeSet(entries: ChangeEntry[], targetBranch?: string): ChangeSet {
  return {
    comparison: targetBranch
      ? { kind: "branch", targetBranch, mergeBase: "abc123" }
      : { kind: "uncommitted" },
    entries,
  };
}

export interface Backend {
  panelState?: ChangesPanelState;
  defaultBranch?: string;
  branches?: BranchOption[];
  uncommitted?: ChangeSet | (() => ChangeSet);
  branchChanges?: ChangeSet | (() => ChangeSet);
  /** GTC-FR-21 / CHG-FR-37: the branch's standing against its upstream. */
  sync?: UpstreamSyncState | (() => UpstreamSyncState);
  /** GSS-FR-25 / CHG-FR-34: the persisted selected action. */
  prefs?: Record<string, unknown>;
  /** Command name -> error message, for the inline-state tests. */
  fail?: Record<string, string>;
}

export function makeBackend(invokeMock: Mock) {
  return function backend(config: Backend = {}) {
    invokeMock.mockImplementation(async (cmd: string, args?: Record<string, unknown>) => {
      const failure = config.fail?.[cmd];
      if (failure) throw failure;
      switch (cmd) {
        case "load_changes_panel_state":
          return config.panelState ?? { mode: "uncommitted" };
        case "save_changes_panel_state":
          return undefined;
        case "get_default_branch":
          return config.defaultBranch ?? "main";
        case "list_comparison_branches":
          return (
            config.branches ?? [
              { name: "feature/x", isCurrent: true, isDefault: false },
              { name: "main", isCurrent: false, isDefault: true },
            ]
          );
        case "list_uncommitted_changes":
          return typeof config.uncommitted === "function"
            ? config.uncommitted()
            : (config.uncommitted ?? changeSet([SKILL, LIB]));
        case "get_upstream_sync_state":
          return typeof config.sync === "function"
            ? config.sync()
            : (config.sync ?? {
                hasRemote: true,
                hasUpstream: true,
                ahead: 0,
                behind: 0,
              });
        case "load_app_preferences":
          return { theme: "system", ...(config.prefs ?? {}) };
        case "save_app_preferences":
          return undefined;
        case "push_current_branch":
          return undefined;
        case "list_branch_changes":
          return typeof config.branchChanges === "function"
            ? config.branchChanges()
            : (config.branchChanges ??
                changeSet([SPEC], String(args?.targetBranch ?? "main")));
        default:
          return undefined;
      }
    });
  };
}

export const onOpenDiff = vi.fn();
/**
 * CHG-FR-39 / CHG-FR-41: resolves with the outcome of the commit made from the
 * window — the commit and the paths it recorded (GTC-FR-19) — or null when it
 * was dismissed or refused.
 */
export const COMMITTED: CommitOutcome = { commitId: "c0ffee", committedPaths: [] };
export let onRequestCommitMessage: Mock<
  (files: CommitFile[], hidden: CommitFile[]) => Promise<CommitOutcome | null>
>;
export let onRequestRollback: Mock<
  (
    files: RollbackFile[],
    onConfirmed: () => void,
  ) => Promise<RollbackResult | null>
>;
export let onRequestGithubToken: Mock<() => Promise<boolean>>;
export let onOpenGlobalSettings: Mock<() => void>;

/** Fresh handler mocks, as each test begins with (CHG-FR-39 / CHG-FR-59). */
export function resetHandlers() {
  onOpenDiff.mockReset();
  onRequestCommitMessage = vi.fn(async () => COMMITTED);
  // Default: the confirmation is dismissed, so a test that does not care about
  // the rollback's outcome never accidentally performs one.
  onRequestRollback = vi.fn(async () => null);
  onRequestGithubToken = vi.fn(async () => true);
  onOpenGlobalSettings = vi.fn();
}

export function renderPanel() {
  let revealSeq = 0;
  let current: PanelRevealRequest | null = null;
  const tree = () => (
    <Changes
      reveal={current}
      onOpenDiff={onOpenDiff}
      onRequestCommitMessage={onRequestCommitMessage}
      onRequestRollback={onRequestRollback}
      onRequestGithubToken={onRequestGithubToken}
      onOpenGlobalSettings={onOpenGlobalSettings}
    />
  );
  const view = render(tree());
  /**
   * CHG-FR-54: hand the mounted panel a reveal request, as the shell does when
   * a Diff tab becomes the active tab (SNV-FR-64). Each call mints a fresh
   * nonce, so asking twice for the same file is two requests rather than one.
   */
  const revealFile = (path: string) => {
    // A follow (SNV-FR-64): the file is in this comparison or it is nowhere.
    current = {
      panel: "changes",
      id: path,
      nonce: ++revealSeq,
      optimistic: false,
      focus: false,
    };
    view.rerender(tree());
  };
  return Object.assign(view, { revealFile });
}

/** The checkbox on the row labelled `name`. */
export function checkbox(name: string): HTMLInputElement {
  const box = row(name).querySelector<HTMLInputElement>("input[type=checkbox]");
  if (!box) throw new Error(`row ${name} carries no checkbox`);
  return box;
}

/** The footer's primary action button (CHG-FR-33). */
export function primary(): HTMLButtonElement {
  const el = document
    .querySelector(".changes-actions__split")
    ?.querySelector<HTMLButtonElement>("button");
  if (!el) throw new Error("no action control is rendered");
  return el;
}

/** Pick `label` from the footer's attached dropdown (CHG-FR-33 / CHG-FR-35). */
export async function chooseAction(label: string) {
  await userEvent.click(screen.getByRole("button", { name: "Choose action" }));
  const menu = screen.getByRole("listbox", { name: "Commit action" });
  await userEvent.click(within(menu).getByRole("option", { name: label }));
}

/** The visible row labels, in render order. */
export function rowNames(): string[] {
  return Array.from(document.querySelectorAll(".tree-row__name")).map((n) =>
    (n.textContent ?? "").trim(),
  );
}

/**
 * Every row labelled `name`, in render order. The two groups each carry their
 * own folder nesting (CHG-FR-09), so a path present in both — `src` under
 * **Revisioned** and `src` under **Unrevisioned** — is two distinct rows that
 * must be addressed apart.
 */
export function rowsNamed(name: string): HTMLElement[] {
  return Array.from(document.querySelectorAll<HTMLElement>(".tree-row")).filter(
    (el) =>
      (el.querySelector(".tree-row__name")?.textContent ?? "").trim() === name,
  );
}

export function row(name: string): HTMLElement {
  const found = rowsNamed(name)[0];
  if (!found) throw new Error(`no row named ${name}; got ${rowNames().join(", ")}`);
  return found;
}


/** The body of one CSS rule, read off the real stylesheet. */
export function cssRule(css: string, selector: string): string {
  const match = css.match(
    new RegExp(
      `(^|\\n)\\s*${selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}\\s*\\{([^}]*)\\}`,
    ),
  );
  expect(match, `expected \`${selector} { … }\` in the stylesheet`).not.toBeNull();
  return match![2];
}

/**
 * Switch to the "All files" lens. The panel opens on "All artifacts"
 * (CHG-FR-15), so a fixture of unclassified files is invisible until this runs —
 * tests about tree shape, diffstats or reloads say so explicitly rather than
 * quietly depending on the default lens.
 */
export async function showAllFiles() {
  await pickSelector("Filter by type", "files");
}
