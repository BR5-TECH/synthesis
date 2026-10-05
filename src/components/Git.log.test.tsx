import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Git } from "./Git";
import { formatCommitDate, formatCommitInstant } from "./Git/format";
import type { CommitFile, CommitHistory, DiffPayload } from "../types";
import {
  COMMIT_DIFF,
  COMMIT_FILES,
  commit,
  deferred,
  history,
} from "../test/gitPanelFixtures";

// The Log section (GIT-FR-KPTE through GIT-FR-TFAU): the commit rail, the files
// of one commit grouped by folder, and the diff of one file.

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
}));

let historyRead: () => Promise<CommitHistory>;
let filesRead: (commitId: string) => Promise<CommitFile[]>;
let diffRead: (commitId: string, path: string) => Promise<DiffPayload>;

beforeEach(() => {
  historyRead = async () => history(3);
  filesRead = async () => COMMIT_FILES;
  diffRead = async () => COMMIT_DIFF;
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, string>) => {
    if (cmd === "list_commit_history") return historyRead();
    if (cmd === "list_commit_files") return filesRead(args.commitId);
    if (cmd === "get_commit_file_diff") return diffRead(args.commitId, args.path);
    if (cmd === "get_upstream_sync_state")
      return { hasRemote: true, hasUpstream: true, ahead: 0, behind: 0 };
    return undefined;
  });
});

afterEach(cleanup);

const panel = () =>
  render(
    <Git
      onSwitchWorktree={vi.fn(async () => ({ ok: true as const }))}
      canCheckOutBranches
    />,
  );

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);
const rows = () => screen.getAllByTestId("git-commit-row");

describe("the Commits rail (GIT-FR-KPTE, GIT-FR-MVBZ)", () => {
  it("GIT-FR-03, GIT-FR-KPTE: lists 100 commits in the order the backend returned them, under the current branch", async () => {
    historyRead = async () => history(100);
    panel();
    await screen.findAllByTestId("git-commit-row");

    expect(rows()).toHaveLength(100);
    expect(rows()[0]).toHaveTextContent("Subject of commit 0");
    expect(rows()[1]).toHaveTextContent("Subject of commit 1");
    expect(rows()[99]).toHaveTextContent("Subject of commit 99");
    expect(screen.getByTestId("git-log-context")).toHaveTextContent("main");
    expect(calls("list_commit_history")).toHaveLength(1);
  });

  it("GIT-FR-KPTE: names Detached HEAD with the short commit id when HEAD is detached", async () => {
    historyRead = async () =>
      history(2, { branch: undefined, isDetached: true, headId: commit(0).id });
    panel();
    expect(await screen.findByTestId("git-log-context")).toHaveTextContent(
      `Detached HEAD ${commit(0).shortId}`,
    );
    expect(screen.getByTestId("git-log-context")).not.toHaveTextContent("main");
  });

  it("GIT-FR-MVBZ: a row shows the author name and email, the local date, the refs and the subject, with the whole message as its description", async () => {
    historyRead = async () =>
      history(2, {
        commits: [
          commit(0, { refs: ["main", "origin/main"] }),
          commit(1),
        ],
      });
    panel();
    await screen.findAllByTestId("git-commit-row");
    const row = rows()[0];

    expect(within(row).getByText(/Author 0 <author0@example.com>/)).toBeInTheDocument();
    expect(within(row).getByText(formatCommitDate(commit(0).authoredAt))).toBeInTheDocument();
    // The absolute date and time stand in the row's title.
    expect(row.querySelector("time")).toHaveAttribute(
      "title",
      formatCommitInstant(commit(0).authoredAt),
    );
    expect(within(row).getByText("main")).toBeInTheDocument();
    expect(within(row).getByText("origin/main")).toBeInTheDocument();
    expect(within(row).getByText("Subject of commit 0")).toBeInTheDocument();
    expect(row).toHaveAccessibleDescription(/Body of commit 0\./);
  });

  it("GIT-FR-MVBZ: a commit that is not a branch tip still names its branch, and a detached history says so", async () => {
    historyRead = async () =>
      history(3, { branch: "feature", commits: [commit(0, { refs: ["feature"] }), commit(1), commit(2)] });
    panel();
    await screen.findAllByTestId("git-commit-row");
    for (const row of rows()) {
      expect(within(row).getByTestId("git-commit-branch")).toHaveTextContent("on feature");
    }
    expect(within(rows()[1]).queryByText("feature")).not.toBeInTheDocument();
  });

  it("GIT-FR-MVBZ: every row of a detached history names Detached HEAD", async () => {
    historyRead = async () =>
      history(2, { branch: undefined, isDetached: true, headId: commit(0).id });
    panel();
    await screen.findAllByTestId("git-commit-row");
    for (const row of rows()) {
      expect(within(row).getByTestId("git-commit-branch")).toHaveTextContent("Detached HEAD");
    }
  });

  it("GIT-FR-DXNC: a commit row is operable by keyboard and the arrow keys move between rows", async () => {
    panel();
    await screen.findAllByTestId("git-commit-row");
    rows()[0].focus();
    await userEvent.keyboard("{ArrowDown}");
    expect(rows()[1]).toHaveFocus();
    await userEvent.keyboard("{Enter}");
    await waitFor(() => expect(rows()[1]).toHaveAttribute("aria-selected", "true"));
    expect(calls("list_commit_files")[0][1]).toEqual({ commitId: commit(1).id });
  });
});

describe("the files and the diff of a commit (GIT-FR-DXNC, GIT-FR-JRYS)", () => {
  it("GIT-FR-DXNC: selecting a commit marks it in words and loads its files, grouped by folder with the root first", async () => {
    panel();
    await screen.findAllByTestId("git-commit-row");
    await userEvent.click(rows()[0]);

    expect(rows()[0]).toHaveAttribute("aria-selected", "true");
    expect(within(rows()[0]).getByText("selected")).toBeInTheDocument();
    expect(rows()[1]).toHaveAttribute("aria-selected", "false");
    expect(calls("list_commit_files")[0][1]).toEqual({ commitId: commit(0).id });

    const folders = await screen.findAllByTestId("git-log-folder");
    expect(folders.map((f) => f.getAttribute("aria-label"))).toEqual([
      "Repository root",
      "docs",
      "src/app",
    ]);
    expect(within(folders[0]).getByText("README.md")).toBeInTheDocument();
    const docs = within(folders[1]).getAllByTestId("git-log-file");
    expect(docs.map((f) => f.querySelector(".git__file-name")?.textContent)).toEqual([
      "guide.md",
      "logo.png",
    ]);
    // The change status of each file stands beside its name, in a letter and in words.
    expect(within(folders[0]).getByText("M")).toBeInTheDocument();
    expect(within(folders[2]).getByText("A")).toBeInTheDocument();
    expect(within(folders[2]).getByText("deleted")).toBeInTheDocument();
    expect(within(docs[0]).getByText(/from docs\/old-guide\.md/)).toBeInTheDocument();
  });

  it("GIT-FR-JRYS: selecting a file marks it, names the commit and the file above the diff, and renders the hunks", async () => {
    panel();
    await screen.findAllByTestId("git-commit-row");
    await userEvent.click(rows()[0]);
    await userEvent.click(await screen.findByText("main.ts"));

    const file = screen.getByText("main.ts").closest("button")!;
    expect(file).toHaveAttribute("aria-current", "true");
    expect(within(file).getByText("selected")).toBeInTheDocument();
    // The selected commit stays marked while a file is selected.
    expect(rows()[0]).toHaveAttribute("aria-selected", "true");

    expect(calls("get_commit_file_diff")[0][1]).toEqual({
      commitId: commit(0).id,
      path: "src/app/main.ts",
    });
    const diff = await screen.findByTestId("git-log-diff");
    expect(within(diff).getByText("src/app/main.ts")).toBeInTheDocument();
    expect(within(diff).getByText(commit(0).shortId)).toBeInTheDocument();
    expect(within(diff).getByText("@@ -1,2 +1,2 @@")).toBeInTheDocument();
    const added = within(diff).getByText("new line").closest(".diff-line");
    expect(added).toHaveAttribute("data-kind", "add");
    expect(within(diff).getByText("old line").closest(".diff-line")).toHaveAttribute(
      "data-kind",
      "del",
    );
    expect(within(diff).getByText("# Title").closest(".diff-line")).toHaveAttribute(
      "data-kind",
      "context",
    );
  });

  it("GIT-FR-JRYS: selecting another commit clears the file selection and the diff", async () => {
    panel();
    await screen.findAllByTestId("git-commit-row");
    await userEvent.click(rows()[0]);
    await userEvent.click(await screen.findByText("main.ts"));
    await screen.findByText("new line");

    await userEvent.click(rows()[1]);
    expect(rows()[1]).toHaveAttribute("aria-selected", "true");
    expect(rows()[0]).toHaveAttribute("aria-selected", "false");
    await screen.findAllByTestId("git-log-folder");
    expect(screen.queryByText("new line")).toBeNull();
    expect(screen.queryByRole("button", { current: true })).toBeNull();
    expect(screen.getByTestId("git-log-no-file")).toHaveTextContent(
      "Select a file to see its diff.",
    );
  });

  it("GIT-FR-JRYS: a binary file shows a binary marker in place of hunks", async () => {
    diffRead = async (_id, path) =>
      path === "docs/logo.png" ? { isBinary: true, hunks: [] } : COMMIT_DIFF;
    panel();
    await screen.findAllByTestId("git-commit-row");
    await userEvent.click(rows()[0]);
    const logo = (await screen.findByText("logo.png")).closest("button")!;
    expect(within(logo).getByText("binary")).toBeInTheDocument();
    await userEvent.click(logo);

    expect(await screen.findByTestId("git-log-diff-binary")).toHaveTextContent(
      "Binary file",
    );
    expect(document.querySelector(".diff-line")).toBeNull();
  });
});

describe("the states of the Commits regions (GIT-FR-TFAU)", () => {
  it("GIT-FR-TFAU: the commit list shows loading, and then the rows", async () => {
    const pending = deferred<CommitHistory>();
    historyRead = () => pending.promise;
    panel();
    expect(await screen.findByTestId("git-log-loading")).toHaveTextContent(
      "Loading commit history…",
    );
    await act(async () => pending.resolve(history(2)));
    expect(screen.queryByTestId("git-log-loading")).toBeNull();
    expect(rows()).toHaveLength(2);
  });

  it("GIT-FR-TFAU: an empty history says so in words", async () => {
    historyRead = async () => history(0);
    panel();
    expect(await screen.findByTestId("git-log-empty")).toHaveTextContent(
      "This branch has no commits yet.",
    );
  });

  it("GIT-FR-TFAU: a failed history read shows a typed error in words and offers a retry", async () => {
    historyRead = async () => {
      throw "not a git repository";
    };
    panel();
    expect(await screen.findByTestId("git-log-error")).toHaveTextContent(
      "This project is not inside a Git repository.",
    );

    historyRead = async () => history(2);
    await userEvent.click(screen.getByRole("button", { name: "Retry" }));
    await screen.findAllByTestId("git-commit-row");
    expect(screen.queryByTestId("git-log-error")).toBeNull();
    expect(calls("list_commit_history")).toHaveLength(2);
  });

  it("GIT-FR-TFAU: a commit with no changed files says so", async () => {
    filesRead = async () => [];
    panel();
    await screen.findAllByTestId("git-commit-row");
    await userEvent.click(rows()[0]);
    expect(await screen.findByTestId("git-log-files-empty")).toHaveTextContent(
      "This commit changed no files.",
    );
  });

  it("GIT-FR-TFAU, GIT-FR-EPSV: a failed file read shows in the files region and leaves the rail usable", async () => {
    filesRead = async (id) => {
      if (id === commit(0).id) throw "unknown_commit";
      return COMMIT_FILES;
    };
    panel();
    await screen.findAllByTestId("git-commit-row");
    await userEvent.click(rows()[0]);
    expect(await screen.findByTestId("git-log-files-error")).toHaveTextContent(
      "That commit does not exist any more.",
    );
    expect(rows()).toHaveLength(3);

    await userEvent.click(rows()[1]);
    expect(await screen.findAllByTestId("git-log-folder")).toHaveLength(3);
    expect(screen.queryByTestId("git-log-files-error")).toBeNull();
  });

  it("GIT-FR-TFAU, GIT-FR-EPSV: a failed diff read shows in the diff region and leaves the file list usable", async () => {
    diffRead = async (_id, path) => {
      if (path === "src/app/main.ts") throw "path_not_in_commit";
      return COMMIT_DIFF;
    };
    panel();
    await screen.findAllByTestId("git-commit-row");
    await userEvent.click(rows()[0]);
    await userEvent.click(await screen.findByText("main.ts"));
    expect(await screen.findByTestId("git-log-diff-error")).toHaveTextContent(
      "That commit did not change this file.",
    );
    expect(screen.getAllByTestId("git-log-file")).toHaveLength(5);

    await userEvent.click(screen.getByText("util.ts"));
    expect(await screen.findByText("new line")).toBeInTheDocument();
    expect(screen.queryByTestId("git-log-diff-error")).toBeNull();
  });

  it("GIT-FR-TFAU, GIT-FR-EPSV: the files of a superseded commit selection are discarded", async () => {
    const first = deferred<CommitFile[]>();
    const second = deferred<CommitFile[]>();
    filesRead = (id) => (id === commit(0).id ? first.promise : second.promise);
    panel();
    await screen.findAllByTestId("git-commit-row");
    await userEvent.click(rows()[0]);
    await userEvent.click(rows()[1]);

    await act(async () =>
      second.resolve([{ path: "second.txt", status: "added", isBinary: false }]),
    );
    expect(await screen.findByText("second.txt")).toBeInTheDocument();
    // The older response arrives last and changes nothing.
    await act(async () =>
      first.resolve([{ path: "first.txt", status: "added", isBinary: false }]),
    );
    expect(screen.queryByText("first.txt")).toBeNull();
    expect(screen.getByText("second.txt")).toBeInTheDocument();
  });

  it("GIT-FR-TFAU, GIT-FR-EPSV: the diff of a superseded file selection is discarded", async () => {
    const first = deferred<DiffPayload>();
    const second = deferred<DiffPayload>();
    diffRead = (_id, path) =>
      path === "src/app/main.ts" ? first.promise : second.promise;
    panel();
    await screen.findAllByTestId("git-commit-row");
    await userEvent.click(rows()[0]);
    await userEvent.click(await screen.findByText("main.ts"));
    await userEvent.click(screen.getByText("util.ts"));

    const line = (content: string): DiffPayload => ({
      isBinary: false,
      hunks: [
        { header: "@@", lines: [{ kind: "add", newLineno: 1, content }] },
      ],
    });
    await act(async () => second.resolve(line("from util")));
    expect(await screen.findByText("from util")).toBeInTheDocument();
    await act(async () => first.resolve(line("from main")));
    expect(screen.queryByText("from main")).toBeNull();
    expect(screen.getByText("from util")).toBeInTheDocument();
  });

  it("GIT-FR-EPSV: a diff that arrives after another commit was selected is discarded", async () => {
    const late = deferred<DiffPayload>();
    diffRead = () => late.promise;
    panel();
    await screen.findAllByTestId("git-commit-row");
    await userEvent.click(rows()[0]);
    await userEvent.click(await screen.findByText("main.ts"));
    await userEvent.click(rows()[1]);
    await screen.findAllByTestId("git-log-folder");

    await act(async () => late.resolve(COMMIT_DIFF));
    expect(screen.queryByText("new line")).toBeNull();
    expect(screen.getByTestId("git-log-no-file")).toBeInTheDocument();
  });

  it("GIT-FR-EPSV: a history response that arrives after the panel was unmounted changes nothing and raises no error", async () => {
    const pending = deferred<CommitHistory>();
    historyRead = () => pending.promise;
    const view = panel();
    await screen.findByTestId("git-log-loading");
    view.unmount();
    await act(async () => pending.resolve(history(2)));
    expect(screen.queryByTestId("git-commit-row")).toBeNull();
  });
});

describe("a change of the worktree (GIT-FR-09)", () => {
  it("GIT-FR-09: a remount reloads the history against the new content root and discards the selected commit, file and diff", async () => {
    const view = render(
      <Git
        key="worktree-a"
        onSwitchWorktree={vi.fn(async () => ({ ok: true as const }))}
        canCheckOutBranches
      />,
    );
    await screen.findAllByTestId("git-commit-row");
    await userEvent.click(rows()[0]);
    await userEvent.click(await screen.findByText("main.ts"));
    await screen.findByText("new line");
    expect(calls("list_commit_history")).toHaveLength(1);

    historyRead = async () => history(2, { branch: "other" });
    view.rerender(
      <Git
        key="worktree-b"
        onSwitchWorktree={vi.fn(async () => ({ ok: true as const }))}
        canCheckOutBranches
      />,
    );
    await waitFor(() => expect(screen.getByTestId("git-log-context")).toHaveTextContent("other"));
    expect(calls("list_commit_history")).toHaveLength(2);
    expect(screen.queryByText("new line")).toBeNull();
    expect(screen.getByTestId("git-log-no-commit")).toBeInTheDocument();
    for (const r of rows()) expect(r).toHaveAttribute("aria-selected", "false");
  });
});
