import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Git } from "./Git";
import { resetAppPreferencesCache } from "../state/appPreferences";
import { resetLogBufferForTest } from "../logging";
import type { BranchComparison, DiffPayload, GitBranch } from "../types";
import { COMMIT_DIFF, COMMIT_FILES, history } from "../test/gitPanelFixtures";

// The Branches section's large view: the selected branch against its base
// (GIT-FR-LNEI, GIT-FR-GDMG, GIT-FR-SION), and the two-column view the
// Commits and Branches sections share (GIT-FR-WMVK, GIT-FR-FATV).

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

type EventHandler = (ev: { payload: unknown }) => void;
let handlers: [string, EventHandler][] = [];
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: EventHandler) => {
    handlers.push([name, handler]);
    return Promise.resolve(() => {
      handlers = handlers.filter(([, h]) => h !== handler);
    });
  },
}));
const fireBranchesChanged = () =>
  handlers
    .filter(([n]) => n === "branches-changed")
    .forEach(([, h]) => h({ payload: { repositoryRoot: "/dev/acme" } }));

const MERGE_BASE = "abcdef0123456789abcdef0123456789abcdef01";

let branches: GitBranch[];
let compare: (name: string, kind: string) => Promise<BranchComparison>;
let fileDiff: (name: string, kind: string, path: string) => Promise<DiffPayload>;
/** The stored files width; a save writes it, as the real store does. */
let storedWidth: number | undefined;

const comparison = (over: Partial<BranchComparison> = {}): BranchComparison => ({
  branch: "develop",
  kind: "local",
  base: "main",
  mergeBase: MERGE_BASE,
  sameAsBase: false,
  files: COMMIT_FILES,
  ...over,
});

beforeEach(() => {
  handlers = [];
  storedWidth = undefined;
  branches = [
    { name: "develop", kind: "local", isCurrent: false },
    { name: "main", kind: "local", isCurrent: true },
    { name: "origin/develop", kind: "remote", isCurrent: false },
  ];
  compare = async (name, kind) =>
    comparison({ branch: name, kind: kind as "local" | "remote" });
  fileDiff = async () => COMMIT_DIFF;
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown>) => {
    switch (cmd) {
      case "list_branches":
        return branches;
      case "list_branch_compare_files":
        return compare(args.name as string, args.kind as string);
      case "get_branch_compare_file_diff":
        return fileDiff(args.name as string, args.kind as string, args.path as string);
      case "list_commit_history":
        return history(2);
      case "list_commit_files":
        return COMMIT_FILES;
      case "get_commit_file_diff":
        return COMMIT_DIFF;
      case "get_upstream_sync_state":
        return { hasRemote: true, hasUpstream: true, ahead: 0, behind: 0 };
      case "load_app_preferences":
        return { gitFilesWidthFraction: storedWidth };
      case "save_app_preferences":
        storedWidth = (args as { preferences: { gitFilesWidthFraction?: number } })
          .preferences.gitFilesWidthFraction;
        return undefined;
      default:
        return undefined;
    }
  });
  resetAppPreferencesCache();
  resetLogBufferForTest();
});

afterEach(() => {
  cleanup();
  resetLogBufferForTest();
});

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);
const rail = () => screen.getByTestId("git-branches");
const row = (name: string) =>
  within(rail()).getByText(name).closest<HTMLElement>('[role="button"]')!;

const openBranches = async () => {
  render(<Git onSwitchWorktree={vi.fn(async () => ({ ok: true as const }))} canCheckOutBranches />);
  await userEvent.click(screen.getByRole("tab", { name: "Branches" }));
  await within(await screen.findByTestId("git-branches")).findByText("develop");
};

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((r) => (resolve = r));
  return { promise, resolve };
}

describe("selecting a branch (GIT-FR-LNEI)", () => {
  it("GIT-FR-SION: with no branch selected the large view says so", async () => {
    await openBranches();
    expect(screen.getByTestId("git-compare-no-branch")).toHaveTextContent(
      "Select a branch to see what it changed against its base.",
    );
    expect(calls("list_branch_compare_files")).toHaveLength(0);
  });

  it("GIT-FR-LNEI: Enter and Space select a row, marked in words and in its accessible state", async () => {
    await openBranches();
    row("develop").focus();
    await userEvent.keyboard("{Enter}");
    const selected = await within(rail()).findByTestId("git-branch-selected");
    expect(selected).toHaveTextContent("develop");
    expect(selected).toHaveAttribute("aria-current", "true");
    expect(within(selected).getByText("selected")).toBeInTheDocument();

    row("main").focus();
    await userEvent.keyboard(" ");
    await waitFor(() =>
      expect(within(rail()).getByTestId("git-branch-selected")).toHaveTextContent("main"),
    );
  });

  it("GIT-FR-LNEI: the selection stays while the Local / Remote switch changes", async () => {
    await openBranches();
    await userEvent.click(row("develop"));
    await screen.findByTestId("git-compare-head");
    await userEvent.click(within(rail()).getByRole("radio", { name: "Remote" }));
    expect(screen.getByTestId("git-compare-head")).toHaveTextContent("develop");
    await userEvent.click(within(rail()).getByRole("radio", { name: "Local" }));
    expect(within(rail()).getByTestId("git-branch-selected")).toHaveTextContent("develop");
  });

  it("GIT-FR-LNEI: a reload that still holds the selected branch keeps the selection, the file and the diff", async () => {
    await openBranches();
    await userEvent.click(row("develop"));
    await userEvent.click(within(await screen.findByTestId("git-compare-files")).getByText("main.ts"));
    await screen.findByText("new line");

    await act(async () => {
      fireBranchesChanged();
    });

    await waitFor(() => expect(calls("list_branches").length).toBeGreaterThan(1));
    expect(within(rail()).getByTestId("git-branch-selected")).toHaveTextContent("develop");
    expect(screen.getByText("new line")).toBeInTheDocument();
  });

  it("GIT-FR-LNEI: a failed reload keeps the selection", async () => {
    await openBranches();
    await userEvent.click(row("develop"));
    await screen.findByTestId("git-compare-head");

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "list_branches") throw "not a git repository";
      return undefined;
    });
    await act(async () => {
      fireBranchesChanged();
    });

    await screen.findAllByTestId("git-branch-error");
    expect(screen.getByTestId("git-compare-head")).toHaveTextContent("develop");
  });

  it("GIT-FR-LNEI: a reload without the selected branch clears the selection and its view", async () => {
    await openBranches();
    await userEvent.click(row("develop"));
    await screen.findByTestId("git-compare-head");

    branches = branches.filter((b) => b.name !== "develop");
    await act(async () => {
      fireBranchesChanged();
    });

    await screen.findByTestId("git-compare-no-branch");
    expect(within(rail()).queryByTestId("git-branch-selected")).toBeNull();
  });
});

describe("a request against a stale listing (GIT-FR-FZMS, GIT-FR-LNEI)", () => {
  it("GIT-FR-FZMS: a request for a branch the stale listing lacks is applied once the listing reloads", async () => {
    const onSwitchWorktree = vi.fn(async () => ({ ok: true as const }));
    const ui = (request: { branch: string; nonce: number } | null) => (
      <Git
        onSwitchWorktree={onSwitchWorktree}
        canCheckOutBranches
        selectBranch={request}
        onBranchSelected={() => {}}
      />
    );
    const view = render(ui(null));
    // The listing is read once, without the new branch.
    await userEvent.click(screen.getByRole("tab", { name: "Branches" }));
    await within(await screen.findByTestId("git-branches")).findByText("develop");
    await userEvent.click(screen.getByRole("tab", { name: "Commits" }));

    // A stream is created meanwhile, and a push of its branch is activated.
    branches = [...branches, { name: "synthesis/stream/new", kind: "local", isCurrent: false }];
    await act(async () => {
      fireBranchesChanged();
    });
    view.rerender(ui({ branch: "synthesis/stream/new", nonce: 1 }));
    await screen.findByTestId("git-transfer-output");

    await userEvent.click(screen.getByRole("tab", { name: "Branches" }));
    expect(
      await within(await screen.findByTestId("git-branches")).findByTestId("git-branch-selected"),
    ).toHaveTextContent("synthesis/stream/new");
  });
});

describe("the branch against its base (GIT-FR-GDMG)", () => {
  it("GIT-FR-GDMG, GIT-FR-WMVK: lists the changed files, names the base and the merge base, and shows a file's diff beside them", async () => {
    await openBranches();
    await userEvent.click(row("develop"));

    expect(calls("list_branch_compare_files")[0][1]).toEqual({
      name: "develop",
      kind: "local",
    });
    const head = await screen.findByTestId("git-compare-head");
    expect(head).toHaveTextContent("develop");
    expect(await screen.findByTestId("git-compare-base")).toHaveTextContent(
      "vs main at abcdef0",
    );
    const files = await screen.findByTestId("git-compare-files");
    expect(within(files).getAllByTestId("git-compare-folder").length).toBeGreaterThan(1);

    await userEvent.click(within(files).getByText("main.ts"));
    expect(calls("get_branch_compare_file_diff")[0][1]).toEqual({
      name: "develop",
      kind: "local",
      path: "src/app/main.ts",
    });
    const diff = screen.getByTestId("git-compare-diff");
    expect(await within(diff).findByText("new line")).toBeInTheDocument();
    expect(within(diff).getByText("src/app/main.ts")).toBeInTheDocument();
    // GIT-FR-WMVK: the files and the diff stand side by side in one view.
    expect(files.parentElement).toBe(diff.parentElement);
    expect(files.parentElement).toHaveClass("git-log__view");
  });

  it("GIT-FR-GDMG: a remote branch is compared by its kind", async () => {
    await openBranches();
    await userEvent.click(within(rail()).getByRole("radio", { name: "Remote" }));
    await userEvent.click(row("origin/develop"));
    await screen.findByTestId("git-compare-files");
    expect(calls("list_branch_compare_files")[0][1]).toEqual({
      name: "origin/develop",
      kind: "remote",
    });
  });

  it("GIT-FR-GDMG: another branch clears the file and the diff", async () => {
    await openBranches();
    await userEvent.click(row("develop"));
    await userEvent.click(within(await screen.findByTestId("git-compare-files")).getByText("main.ts"));
    await screen.findByText("new line");

    await userEvent.click(row("main"));
    await screen.findByTestId("git-compare-no-file");
    expect(screen.queryByText("new line")).toBeNull();
    expect(calls("list_branch_compare_files")[1][1]).toEqual({ name: "main", kind: "local" });
  });

  it("GIT-FR-LNEI: selecting checks nothing out and starts nothing but the comparison read", async () => {
    const onSwitchWorktree = vi.fn(async () => ({ ok: true as const }));
    render(<Git onSwitchWorktree={onSwitchWorktree} canCheckOutBranches />);
    await userEvent.click(screen.getByRole("tab", { name: "Branches" }));
    await within(await screen.findByTestId("git-branches")).findByText("develop");
    const before = invokeMock.mock.calls.length;

    await userEvent.click(row("develop"));
    await screen.findByTestId("git-compare-files");

    expect(onSwitchWorktree).not.toHaveBeenCalled();
    // The comparison read, and the files column's stored width (GIT-FR-FATV).
    const after = new Set(invokeMock.mock.calls.slice(before).map((c) => c[0]));
    expect(after.has("list_branch_compare_files")).toBe(true);
    after.delete("list_branch_compare_files");
    after.delete("load_app_preferences");
    // The log buffer flushes in the background; it is not a Git read.
    after.delete("append_log_records");
    expect([...after]).toEqual([]);
  });
});

describe("the states of the branch view (GIT-FR-SION)", () => {
  it("GIT-FR-SION: a branch that is its own base has nothing to compare", async () => {
    compare = async () =>
      comparison({ branch: "main", sameAsBase: true, mergeBase: undefined, files: [] });
    await openBranches();
    await userEvent.click(row("main"));
    expect(await screen.findByTestId("git-compare-same-as-base")).toHaveTextContent(
      "main is its own base, so there is nothing to compare.",
    );
    expect(screen.queryByTestId("git-compare-files")).toBeNull();
  });

  it("GIT-FR-SION: a branch that changed no file says so against its base", async () => {
    compare = async () => comparison({ files: [] });
    await openBranches();
    await userEvent.click(row("develop"));
    expect(await screen.findByTestId("git-compare-files-empty")).toHaveTextContent(
      "develop changed no files against main.",
    );
  });

  it("GIT-FR-SION: the file list shows loading, then its typed error with a retry", async () => {
    const pending = deferred<BranchComparison>();
    compare = () => pending.promise;
    await openBranches();
    await userEvent.click(row("develop"));
    expect(await screen.findByTestId("git-compare-files-loading")).toBeInTheDocument();

    await act(async () => {
      pending.resolve(Promise.reject("no_merge_base") as unknown as BranchComparison);
    });
    compare = async () => comparison();
    const error = await screen.findByTestId("git-compare-files-error");
    expect(error).toHaveTextContent("shares no commit with its base branch");
    await userEvent.click(within(error).getByRole("button", { name: /Retry/ }));
    expect(await screen.findByTestId("git-compare-files")).toHaveTextContent("main.ts");
  });

  it("GIT-FR-SION: a failed diff shows in the diff region and leaves the file list usable", async () => {
    fileDiff = async () => {
      throw "path_not_in_comparison";
    };
    await openBranches();
    await userEvent.click(row("develop"));
    const files = await screen.findByTestId("git-compare-files");
    await userEvent.click(within(files).getByText("main.ts"));
    expect(await screen.findByTestId("git-compare-diff-error")).toBeInTheDocument();

    fileDiff = async () => COMMIT_DIFF;
    await userEvent.click(within(files).getByText("README.md"));
    expect(await screen.findByText("new line")).toBeInTheDocument();
  });

  it("GIT-FR-SION, GIT-FR-GDMG: the diff shows loading, then the diff head names the branch and the file", async () => {
    const pending = deferred<DiffPayload>();
    fileDiff = () => pending.promise;
    await openBranches();
    await userEvent.click(row("develop"));
    await userEvent.click(within(await screen.findByTestId("git-compare-files")).getByText("main.ts"));
    expect(await screen.findByTestId("git-compare-diff-loading")).toBeInTheDocument();

    await act(async () => pending.resolve(COMMIT_DIFF));
    const diff = screen.getByTestId("git-compare-diff");
    expect(await within(diff).findByText("new line")).toBeInTheDocument();
    expect(diff.querySelector(".git-log__diff-head")).toHaveTextContent("develop");
  });

  it("GIT-FR-SION: a file the comparison did not change is refused in words", async () => {
    fileDiff = async () => {
      throw "path_not_in_comparison";
    };
    await openBranches();
    await userEvent.click(row("develop"));
    await userEvent.click(within(await screen.findByTestId("git-compare-files")).getByText("main.ts"));
    expect(await screen.findByTestId("git-compare-diff-error")).toHaveTextContent(
      "Branch develop did not change this file against its base.",
    );
  });

  it("GIT-FR-GDMG: a binary file shows the binary marker in place of hunks", async () => {
    fileDiff = async () => ({ isBinary: true, hunks: [] });
    await openBranches();
    await userEvent.click(row("develop"));
    await userEvent.click(within(await screen.findByTestId("git-compare-files")).getByText("logo.png"));
    expect(await screen.findByTestId("git-compare-diff-binary")).toBeInTheDocument();
  });

  it("GIT-FR-SION: a newer file supersedes the diff response of an older one", async () => {
    const slow = deferred<DiffPayload>();
    fileDiff = async (_name, _kind, path) =>
      path === "src/app/main.ts"
        ? slow.promise
        : {
            isBinary: false,
            hunks: [{ header: "@@ -1 +1 @@", lines: [{ kind: "add", newLineno: 1, content: "readme line" }] }],
          };
    await openBranches();
    await userEvent.click(row("develop"));
    const files = await screen.findByTestId("git-compare-files");
    await userEvent.click(within(files).getByText("main.ts"));
    await userEvent.click(within(files).getByText("README.md"));
    await screen.findByText("readme line");

    await act(async () => slow.resolve(COMMIT_DIFF));
    expect(screen.queryByText("new line")).toBeNull();
    expect(screen.getByText("readme line")).toBeInTheDocument();
  });

  it("GIT-FR-SION: a newer selection supersedes the response of an older one", async () => {
    const slow = deferred<BranchComparison>();
    compare = async (name) =>
      name === "develop" ? slow.promise : comparison({ branch: name, files: [COMMIT_FILES[0]] });
    await openBranches();
    await userEvent.click(row("develop"));
    await userEvent.click(row("main"));
    const files = await screen.findByTestId("git-compare-files");
    await within(files).findByText("README.md");

    await act(async () => slow.resolve(comparison()));
    expect(within(files).queryByText("main.ts")).toBeNull();
  });
});

describe("the files column width (GIT-FR-FATV)", () => {
  const divider = () => screen.getByTestId("git-files-divider");
  const saved = () =>
    calls("save_app_preferences").map(
      ([, args]) =>
        (args as { preferences: { gitFilesWidthFraction: number } }).preferences
          .gitFilesWidthFraction,
    );

  const openCommit = async () => {
    render(<Git onSwitchWorktree={vi.fn(async () => ({ ok: true as const }))} canCheckOutBranches />);
    await userEvent.click(await screen.findByText("Subject of commit 0"));
    await screen.findByTestId("git-log-files");
  };

  it("GIT-FR-FATV, GIT-FR-WMVK: the Commits view stands in two columns with a focusable separator at 30 percent", async () => {
    await openCommit();
    const view = document.querySelector<HTMLElement>(".git-log__view")!;
    expect(view).toContainElement(screen.getByTestId("git-log-files"));
    expect(view).toContainElement(screen.getByTestId("git-log-diff"));
    expect(divider()).toHaveAttribute("role", "separator");
    expect(divider()).toHaveAttribute("aria-orientation", "vertical");
    expect(divider()).toHaveAttribute("tabindex", "0");
    expect(divider()).toHaveAttribute("aria-valuenow", "30");
    expect(divider()).toHaveAttribute("aria-valuemin", "15");
    expect(divider()).toHaveAttribute("aria-valuemax", "70");
    expect(view.style.getPropertyValue("--git-files")).toBe("30%");
  });

  it("GIT-FR-FATV: the arrow keys, Home and End set the width within its bounds and store it", async () => {
    await openCommit();
    divider().focus();
    await userEvent.keyboard("{ArrowRight}");
    expect(divider()).toHaveAttribute("aria-valuenow", "31");
    await userEvent.keyboard("{End}");
    expect(divider()).toHaveAttribute("aria-valuenow", "70");
    await userEvent.keyboard("{End}");
    expect(divider()).toHaveAttribute("aria-valuenow", "70");
    await userEvent.keyboard("{Home}");
    expect(divider()).toHaveAttribute("aria-valuenow", "15");
    await userEvent.keyboard("{ArrowLeft}");
    expect(divider()).toHaveAttribute("aria-valuenow", "15");
    await waitFor(() => expect(saved()).toEqual([0.31, 0.7, 0.7, 0.15, 0.15]));
  });

  it("GIT-FR-FATV: the Commits and Branches sections share one stored width", async () => {
    storedWidth = 0.42;
    await openCommit();
    await waitFor(() => expect(divider()).toHaveAttribute("aria-valuenow", "42"));
    divider().focus();
    await userEvent.keyboard("{ArrowRight}");
    await waitFor(() => expect(saved()).toEqual([0.43]));

    await userEvent.click(screen.getByRole("tab", { name: "Branches" }));
    await userEvent.click(await within(await screen.findByTestId("git-branches")).findByText("develop"));
    await screen.findByTestId("git-compare-files");
    expect(divider()).toHaveAttribute("aria-valuenow", "43");
  });

  it("GIT-FR-FATV: a pointer drag sets the width, held within its bounds, and stores it once when it ends", async () => {
    await openCommit();
    const view = document.querySelector<HTMLElement>(".git-log__view")!;
    view.getBoundingClientRect = () =>
      ({ left: 0, top: 0, width: 1000, height: 400, right: 1000, bottom: 400, x: 0, y: 0 }) as DOMRect;

    fireEvent.pointerDown(divider(), { clientX: 300, button: 0, pointerId: 1 });
    fireEvent.pointerMove(divider(), { clientX: 500, pointerId: 1 });
    expect(divider()).toHaveAttribute("aria-valuenow", "50");
    expect(view.style.getPropertyValue("--git-files")).toBe("50%");
    expect(saved()).toEqual([]);
    fireEvent.pointerUp(divider(), { clientX: 500, pointerId: 1 });
    await waitFor(() => expect(saved()).toEqual([0.5]));

    fireEvent.pointerDown(divider(), { clientX: 500, button: 0, pointerId: 1 });
    fireEvent.pointerMove(divider(), { clientX: 990, pointerId: 1 });
    expect(divider()).toHaveAttribute("aria-valuenow", "70");
    fireEvent.pointerMove(divider(), { clientX: 5, pointerId: 1 });
    expect(divider()).toHaveAttribute("aria-valuenow", "15");
    fireEvent.pointerUp(divider(), { clientX: 5, pointerId: 1 });
    await waitFor(() => expect(saved()).toEqual([0.5, 0.15]));
  });

  it("GIT-FR-FATV: a cancelled drag stores nothing and shows the width before it", async () => {
    await openCommit();
    const view = document.querySelector<HTMLElement>(".git-log__view")!;
    view.getBoundingClientRect = () =>
      ({ left: 0, top: 0, width: 1000, height: 400, right: 1000, bottom: 400, x: 0, y: 0 }) as DOMRect;

    fireEvent.pointerDown(divider(), { clientX: 300, button: 0, pointerId: 1 });
    fireEvent.pointerMove(divider(), { clientX: 600, pointerId: 1 });
    expect(divider()).toHaveAttribute("aria-valuenow", "60");
    fireEvent.pointerCancel(divider(), { pointerId: 1 });
    expect(divider()).toHaveAttribute("aria-valuenow", "30");
    expect(saved()).toEqual([]);
  });

  it("GIT-FR-FATV: a stored width outside the bounds is held inside them", async () => {
    storedWidth = 0.95;
    await openCommit();
    await waitFor(() => expect(divider()).toHaveAttribute("aria-valuenow", "70"));
  });
});
