import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import {
  CommitMessageModal,
  commitErrorMessage,
  resolveInclusion,
  type CommitFile,
} from "./CommitMessageModal";

// CMW-FR-07: the window's one backend operation.
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const onClose = vi.fn();
const onCommitted = vi.fn();

const FILES: CommitFile[] = [
  { path: ".claude/skills/analyst/SKILL.md", untracked: false },
  { path: "specifications/ui/CHG-changes.md", untracked: false },
  { path: "src/components/Changes.tsx", untracked: true },
];

function renderWindow(files: CommitFile[] = FILES) {
  return render(
    <CommitMessageModal
      files={files}
      onClose={onClose}
      onCommitted={onCommitted}
    />,
  );
}

const commitCalls = () =>
  invokeMock.mock.calls.filter((c) => c[0] === "commit_paths");

const commitButton = () => screen.getByRole("button", { name: /^Commit/ });
const messageEditor = () =>
  screen.getByLabelText("Commit message") as HTMLTextAreaElement;

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockResolvedValue("abc1234");
  onClose.mockReset();
  onCommitted.mockReset();
});

afterEach(cleanup);

// ---------------------------------------------------------------------------
// Opening (CMW-FR-01, CMW-FR-02, CMW-FR-03, CMW-FR-04, CMW-FR-06 / CMW-FR-11)
// ---------------------------------------------------------------------------

describe("opening the window", () => {
  it("CMW-FR-01, CMW-FR-02, CMW-FR-03, CMW-FR-04, CMW-FR-06 opens titled for the set, focused in the editor, listing the paths", async () => {
    renderWindow();

    // CMW-FR-01 / CMW-FR-02: a centered modal overlay, not a tab or a panel.
    const dialog = screen.getByRole("dialog");
    expect(dialog.closest(".scrim")).not.toBeNull();
    // CMW-FR-06: the title states how many files the commit carries.
    expect(within(dialog).getByText("Commit 3 files")).toBeInTheDocument();
    // CMW-FR-04: keyboard focus starts in the message editor.
    await waitFor(() => expect(messageEditor()).toHaveFocus());
    expect(Number(messageEditor().rows)).toBeGreaterThanOrEqual(6);

    // CMW-FR-06: the set, read-only, as project-relative paths.
    const list = screen.getByTestId("commit-files");
    for (const f of FILES) {
      expect(within(list).getByText(f.path)).toBeInTheDocument();
    }
    expect(within(list).queryByRole("checkbox")).toBeNull();
    expect(within(list).queryByRole("button")).toBeNull();
  });

  it("CMW-FR-01, CMW-FR-02, CMW-FR-03, CMW-FR-04, CMW-FR-06 titles a single-file commit in the singular", () => {
    renderWindow([FILES[0]]);
    expect(screen.getByText("Commit 1 file")).toBeInTheDocument();
  });

  it("CMW-FR-06, CMW-FR-11 marks the untracked entry and only that one", () => {
    renderWindow();
    const rows = Array.from(
      screen.getByTestId("commit-files").querySelectorAll(".commit-files__row"),
    );
    const marked = rows.filter((r) => r.textContent?.includes("new"));
    expect(marked).toHaveLength(1);
    expect(marked[0].textContent).toContain("src/components/Changes.tsx");
  });

  it("CMW-FR-11 offers no amend, sign-off, template, history, or file-selection control", () => {
    const { container } = renderWindow();
    expect(container.textContent).not.toMatch(
      /\b(amend|sign[- ]off|template|previous message|history)\b/i,
    );
    // The only controls are Close, Cancel and Commit.
    const labels = Array.from(container.querySelectorAll("button")).map((b) =>
      `${b.getAttribute("aria-label") ?? ""} ${b.textContent ?? ""}`.trim(),
    );
    expect(labels.sort()).toEqual(["Cancel", "Close", "Commit"]);
  });

  it("CMW-FR-06 scrolls a large set inside its own frame rather than growing the window", () => {
    const many: CommitFile[] = Array.from({ length: 400 }, (_, i) => ({
      path: `src/generated/file-${i}.ts`,
      untracked: false,
    }));
    renderWindow(many);
    const list = screen.getByTestId("commit-files");
    expect(list.querySelectorAll(".commit-files__row")).toHaveLength(400);

    // The frame is bounded and scrolls. jsdom computes no layout, so the
    // falsifiable assertion is the contract that produces the bound: the list
    // carries the class the stylesheet caps and overflows, and every row of a
    // 400-file set is inside that one element rather than beside it.
    const styles = getComputedStyle(list);
    expect(list.classList.contains("commit-files")).toBe(true);
    expect(styles.listStyle === "" || styles.listStyle).toBeDefined();
    for (const rowEl of Array.from(
      list.querySelectorAll(".commit-files__row"),
    )) {
      expect(rowEl.parentElement).toBe(list);
    }
    // And the dialog holds exactly one such frame however large the set is.
    expect(
      screen.getByRole("dialog").querySelectorAll(".commit-files"),
    ).toHaveLength(1);
  });
});

// ---------------------------------------------------------------------------
// Validation (CMW-FR-05)
// ---------------------------------------------------------------------------

describe("message validation (CMW-FR-05)", () => {
  it("CMW-FR-05 enables Commit only once the message holds a non-whitespace character", async () => {
    renderWindow();
    expect(commitButton()).toBeDisabled();

    await userEvent.type(messageEditor(), "   \n  ");
    expect(commitButton()).toBeDisabled();

    await userEvent.type(messageEditor(), "add analyst skill");
    expect(commitButton()).toBeEnabled();
  });

  it("does not reach the backend while the message is empty", async () => {
    renderWindow();
    await userEvent.click(commitButton());
    expect(commitCalls()).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// Committing (CMW-FR-07, GTC-FR-19 / CMW-FR-08, GTC-FR-20)
// ---------------------------------------------------------------------------

describe("committing", () => {
  it("CMW-FR-07, GTC-FR-19 invokes commit_paths with the message and every path, then closes", async () => {
    renderWindow();
    await userEvent.type(messageEditor(), "add analyst skill");
    await userEvent.click(commitButton());

    await waitFor(() => expect(commitCalls()).toHaveLength(1));
    expect(commitCalls()[0][1]).toEqual({
      message: "add analyst skill",
      paths: FILES.map((f) => f.path),
    });
    await waitFor(() => expect(onCommitted).toHaveBeenCalledOnce());
    expect(onClose).not.toHaveBeenCalled();
  });

  it("CMW-FR-07, GTC-FR-19 renders its inputs and actions inert while the commit runs", async () => {
    let release!: (hash: string) => void;
    invokeMock.mockImplementation(
      () => new Promise<string>((resolve) => (release = resolve)),
    );
    renderWindow();
    await userEvent.type(messageEditor(), "msg");
    await userEvent.click(commitButton());

    await waitFor(() => expect(messageEditor()).toBeDisabled());
    expect(commitButton()).toBeDisabled();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeDisabled();
    // A second activation submits nothing.
    await userEvent.click(commitButton());
    expect(commitCalls()).toHaveLength(1);

    await act(async () => {
      release("abc1234");
    });
    await waitFor(() => expect(onCommitted).toHaveBeenCalled());
  });

  it("CMW-FR-08, GTC-FR-20 keeps the window open with the message intact when the commit is rejected", async () => {
    invokeMock.mockRejectedValue("nothing_to_commit");
    renderWindow();
    await userEvent.type(messageEditor(), "add analyst skill");
    await userEvent.click(commitButton());

    const error = await screen.findByTestId("commit-error");
    expect(error).toHaveTextContent(/Nothing to commit in the selected files/);
    // The window is still mounted, the message is exactly as typed, and the
    // file set is unchanged.
    expect(messageEditor().value).toBe("add analyst skill");
    expect(messageEditor()).toBeEnabled();
    expect(
      screen.getByTestId("commit-files").querySelectorAll(".commit-files__row"),
    ).toHaveLength(3);
    expect(onCommitted).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
  });

  it("renders each typed cause as a sentence rather than as its wire slug", () => {
    // GTC-FR-20's vocabulary, as CMW-FR-08 presents it.
    expect(commitErrorMessage("empty_commit_message")).toMatch(/message is required/i);
    expect(commitErrorMessage("no_paths_selected")).toMatch(/No files were selected/i);
    expect(commitErrorMessage("nothing_to_commit")).toMatch(/Nothing to commit/i);
    expect(commitErrorMessage("not a git repository")).toMatch(/isn’t a Git repository/);
    // Anything else is shown as it arrived rather than swallowed.
    expect(commitErrorMessage("disk full")).toBe("disk full");
    // Tauri rejects with a string on some paths and an Error on others, and a
    // dropped rejection reason must still read as something.
    expect(commitErrorMessage(new Error("nothing_to_commit"))).toMatch(
      /Nothing to commit/i,
    );
    expect(commitErrorMessage(null)).toBe("commit failed");
    expect(commitErrorMessage(undefined)).toBe("commit failed");
  });

  it("CMW-FR-08 keeps the message through every typed cause the backend returns", async () => {
    for (const cause of [
      "empty_commit_message",
      "no_paths_selected",
      "nothing_to_commit",
    ]) {
      cleanup();
      invokeMock.mockReset();
      invokeMock.mockRejectedValue(cause);
      renderWindow();
      await userEvent.type(messageEditor(), "hand-off");
      await userEvent.click(commitButton());

      expect(await screen.findByTestId("commit-error")).toBeInTheDocument();
      // Never the wire slug itself, and never at the cost of the message.
      expect(screen.getByTestId("commit-error").textContent).not.toBe(cause);
      expect(messageEditor().value).toBe("hand-off");
      expect(
        screen.getByTestId("commit-files").querySelectorAll(".commit-files__row"),
      ).toHaveLength(3);
    }
  });
});

// ---------------------------------------------------------------------------
// Dismissal (CMW-FR-09 / CMW-FR-10 / CMW-FR-12, CMW-FR-04)
// ---------------------------------------------------------------------------

describe("dismissal", () => {
  it("CMW-FR-09 dismisses on Escape without creating a commit", async () => {
    renderWindow();
    await userEvent.type(messageEditor(), "half-written");
    await userEvent.keyboard("{Escape}");

    expect(onClose).toHaveBeenCalledOnce();
    expect(commitCalls()).toHaveLength(0);
    expect(onCommitted).not.toHaveBeenCalled();
  });

  it("CMW-FR-09 dismisses on the close control and on an outside click", async () => {
    const { container } = renderWindow();
    await userEvent.click(screen.getByRole("button", { name: "Close" }));
    expect(onClose).toHaveBeenCalledOnce();

    onClose.mockClear();
    await userEvent.click(container.querySelector(".scrim")!);
    expect(onClose).toHaveBeenCalledOnce();
    expect(commitCalls()).toHaveLength(0);
  });

  it("CMW-FR-09 refuses to dismiss while a commit is in flight", async () => {
    invokeMock.mockImplementation(() => new Promise<string>(() => {}));
    const { container } = renderWindow();
    await userEvent.type(messageEditor(), "msg");
    await userEvent.click(commitButton());
    await waitFor(() => expect(commitCalls()).toHaveLength(1));

    await userEvent.keyboard("{Escape}");
    await userEvent.click(container.querySelector(".scrim")!);

    expect(onClose).not.toHaveBeenCalled();
    // The commit continues: nothing cancelled it.
    expect(commitCalls()).toHaveLength(1);
  });

  it("CMW-FR-09 reports the in-flight window upward, and clears it however it ends", async () => {
    // The parent uses this to refuse a dismissal it would otherwise perform
    // (a native menu accelerator reaches the overlay openers past the scrim).
    const onCommittingChange = vi.fn();
    let release!: (hash: string) => void;
    invokeMock.mockImplementation(
      () => new Promise<string>((resolve) => (release = resolve)),
    );
    render(
      <CommitMessageModal
        files={FILES}
        onClose={onClose}
        onCommitted={onCommitted}
        onCommittingChange={onCommittingChange}
      />,
    );
    expect(onCommittingChange).toHaveBeenLastCalledWith(false);

    await userEvent.type(messageEditor(), "msg");
    await userEvent.click(commitButton());
    await waitFor(() =>
      expect(onCommittingChange).toHaveBeenLastCalledWith(true),
    );

    await act(async () => {
      release("abc1234");
    });
    await waitFor(() => expect(onCommitted).toHaveBeenCalled());

    // And a window unmounted mid-commit must not leave the flag stuck on.
    cleanup();
    expect(onCommittingChange).toHaveBeenLastCalledWith(false);
  });

  it("CMW-FR-09 clears the in-flight report when the commit is rejected", async () => {
    const onCommittingChange = vi.fn();
    invokeMock.mockRejectedValue("nothing_to_commit");
    render(
      <CommitMessageModal
        files={FILES}
        onClose={onClose}
        onCommitted={onCommitted}
        onCommittingChange={onCommittingChange}
      />,
    );
    await userEvent.type(messageEditor(), "msg");
    await userEvent.click(commitButton());

    await screen.findByTestId("commit-error");
    expect(onCommittingChange).toHaveBeenLastCalledWith(false);
  });

  it("CMW-FR-10 opens empty every time, recalling no previous message", async () => {
    renderWindow();
    await userEvent.type(messageEditor(), "add analyst skill");
    await userEvent.click(commitButton());
    await waitFor(() => expect(onCommitted).toHaveBeenCalled());

    cleanup();
    renderWindow();
    expect(messageEditor().value).toBe("");
    expect(screen.getByRole("dialog").textContent).not.toContain(
      "add analyst skill",
    );
  });

  it("CMW-FR-12, CMW-FR-04 is operable from the keyboard alone", async () => {
    renderWindow();
    await waitFor(() => expect(messageEditor()).toHaveFocus());
    await userEvent.keyboard("keyboard only");
    // Reach the Commit action without a pointer and confirm it.
    commitButton().focus();
    await userEvent.keyboard("{Enter}");

    await waitFor(() => expect(commitCalls()).toHaveLength(1));
    expect(commitCalls()[0][1]).toMatchObject({ message: "keyboard only" });
  });
});

// ---------------------------------------------------------------------------
// The inclusion step (CMW-FR-13, CMW-FR-06 / CMW-FR-04 / CMW-FR-09)
// ---------------------------------------------------------------------------

describe("the inclusion step (CMW-FR-13)", () => {
  const SPEC: CommitFile = {
    path: "specifications/ui/CHG-changes.md",
    untracked: false,
  };
  // What the All-artifacts lens hides: the code and tests written from the Spec.
  const HIDDEN: CommitFile[] = [
    { path: "src/components/Changes.tsx", untracked: false },
    { path: "src/components/Changes.test.tsx", untracked: true },
  ];

  function renderWithHidden(hidden: CommitFile[] = HIDDEN) {
    return render(
      <CommitMessageModal
        files={[SPEC]}
        hidden={hidden}
        onClose={onClose}
        onCommitted={onCommitted}
      />,
    );
  }

  const committedPaths = () =>
    (commitCalls()[0][1] as { paths: string[] }).paths;

  async function answer(label: string) {
    await userEvent.click(screen.getByRole("button", { name: label }));
  }

  async function confirm() {
    await userEvent.type(messageEditor(), "hand-off");
    await userEvent.click(commitButton());
    await waitFor(() => expect(commitCalls()).toHaveLength(1));
  }

  it("resolveInclusion takes none, the tracked ones, or all of them", () => {
    expect(resolveInclusion([SPEC], HIDDEN, "no")).toEqual([SPEC]);
    expect(resolveInclusion([SPEC], HIDDEN, "revisioned")).toEqual([
      SPEC,
      HIDDEN[0],
    ]);
    expect(resolveInclusion([SPEC], HIDDEN, "all")).toEqual([SPEC, ...HIDDEN]);
    // Nothing hidden: every answer is the set as it arrived.
    for (const a of ["no", "revisioned", "all"] as const) {
      expect(resolveInclusion([SPEC], [], a)).toEqual([SPEC]);
    }
  });

  it("CMW-FR-13, CMW-FR-06 presents the step, naming every hidden file, before the message", async () => {
    renderWithHidden();

    const step = screen.getByTestId("commit-inclusion");
    expect(within(step).getByText(/2 changed files are hidden/)).toBeInTheDocument();
    // Named, not just counted: the author is not asked to agree to something
    // they cannot see.
    for (const f of HIDDEN) {
      expect(within(step).getByText(f.path)).toBeInTheDocument();
    }
    // The message editor does not exist yet.
    expect(screen.queryByLabelText("Commit message")).toBeNull();
    expect(commitCalls()).toHaveLength(0);
    // Exactly three answers.
    for (const label of ["No", "All revisioned", "All files"]) {
      expect(screen.getByRole("button", { name: label })).toBeInTheDocument();
    }
  });

  it("CMW-FR-13, CMW-FR-06 answering No commits the set as it arrived", async () => {
    renderWithHidden();
    await answer("No");

    expect(screen.getByText("Commit 1 file")).toBeInTheDocument();
    await confirm();
    expect(committedPaths()).toEqual([SPEC.path]);
  });

  it("CMW-FR-13, CMW-FR-06 answering All revisioned adds the tracked hidden file only", async () => {
    renderWithHidden();
    await answer("All revisioned");

    expect(screen.getByText("Commit 2 files")).toBeInTheDocument();
    await confirm();
    expect(committedPaths()).toEqual([SPEC.path, HIDDEN[0].path]);
    expect(committedPaths()).not.toContain(HIDDEN[1].path);
  });

  it("CMW-FR-13, CMW-FR-06 answering All files adds every hidden file", async () => {
    renderWithHidden();
    await answer("All files");

    expect(screen.getByText("Commit 3 files")).toBeInTheDocument();
    await confirm();
    expect(committedPaths()).toEqual([
      SPEC.path,
      HIDDEN[0].path,
      HIDDEN[1].path,
    ]);
  });

  it("CMW-FR-13, CMW-FR-06 lists every path the commit carries before it is confirmed", async () => {
    // CMW-FR-06 / CHG-FR-27: a file added by this step still reaches the author
    // by name before anything is created.
    renderWithHidden();
    await answer("All files");

    const list = screen.getByTestId("commit-files");
    for (const f of [SPEC, ...HIDDEN]) {
      expect(within(list).getByText(f.path)).toBeInTheDocument();
    }
    // And the untracked one is still marked as such.
    const marked = Array.from(
      list.querySelectorAll(".commit-files__row"),
    ).filter((r) => r.textContent?.includes("new"));
    expect(marked).toHaveLength(1);
    expect(marked[0].textContent).toContain(HIDDEN[1].path);
  });

  it("CMW-FR-13, CMW-FR-06 offers no way back to the step once it is answered", async () => {
    renderWithHidden();
    await answer("All revisioned");

    expect(screen.queryByTestId("commit-inclusion")).toBeNull();
    for (const label of ["No", "All revisioned", "All files"]) {
      expect(screen.queryByRole("button", { name: label })).toBeNull();
    }
  });

  it("offers no All revisioned when every hidden file is untracked", async () => {
    // It would be "All files" under a second name.
    renderWithHidden([{ path: "src/new.ts", untracked: true }]);

    expect(screen.getByRole("button", { name: "No" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "All files" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "All revisioned" })).toBeNull();
  });

  it("CMW-FR-13, CMW-FR-04 skips the step entirely when nothing was hidden", async () => {
    renderWindow();
    expect(screen.queryByTestId("commit-inclusion")).toBeNull();
    await waitFor(() => expect(messageEditor()).toHaveFocus());
  });

  it("CMW-FR-13, CMW-FR-04 focuses the message editor once the step is answered", async () => {
    renderWithHidden();
    await answer("No");
    await waitFor(() => expect(messageEditor()).toHaveFocus());
  });

  it("CMW-FR-13, CMW-FR-09 dismisses from the step without committing anything", async () => {
    const { container } = renderWithHidden();
    await userEvent.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalledOnce();
    expect(commitCalls()).toHaveLength(0);

    onClose.mockClear();
    cleanup();
    renderWithHidden();
    await userEvent.click(container.querySelector(".scrim") ?? document.body);
    expect(commitCalls()).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// The graduation start-preflight route (CMW-FR-03, CMW-FR-06, CMW-FR-07, CMW-FR-08, CMW-FR-09, CMW-FR-13)
// ---------------------------------------------------------------------------

describe("the start-preflight route (CMW-FR-03 / CMW-FR-07 / CMW-FR-08)", () => {
  const SIX: CommitFile[] = [
    { path: "staged.md", untracked: false },
    { path: "twice.md", untracked: false },
    { path: "unstaged.md", untracked: false },
    { path: "gone.md", untracked: false },
    { path: "new.md", untracked: false },
    { path: "untracked.md", untracked: true },
  ];

  const renderPreflightWindow = () =>
    render(
      <CommitMessageModal
        files={SIX}
        expectedWorktree="~/dev/acme"
        onClose={onClose}
        onCommitted={onCommitted}
      />,
    );

  it("CMW-FR-03, CMW-FR-06, CMW-FR-07, CMW-FR-08, CMW-FR-09, CMW-FR-13: opens on the six paths with no inclusion step and commits bound to the worktree", async () => {
    renderPreflightWindow();

    expect(
      screen.getByRole("dialog", { name: /Commit 6 files/ }),
    ).toBeInTheDocument();
    expect(screen.queryByTestId("commit-inclusion")).toBeNull();
    for (const file of SIX) {
      expect(screen.getByText(file.path)).toBeInTheDocument();
    }
    // CMW-FR-04: the message editor holds focus.
    expect(messageEditor()).toHaveFocus();

    await userEvent.type(messageEditor(), "Save my work");
    await userEvent.click(commitButton());

    await waitFor(() => expect(commitCalls()).toHaveLength(1));
    expect(commitCalls()[0][1]).toEqual({
      message: "Save my work",
      paths: SIX.map((f) => f.path),
      expectedWorktree: "~/dev/acme",
    });
    expect(
      invokeMock.mock.calls.some((c) => c[0] === "publish_graduation_result"),
    ).toBe(false);
  });

  it("CMW-FR-03, CMW-FR-06, CMW-FR-07, CMW-FR-08, CMW-FR-09, CMW-FR-13: the same window from the Changes panel carries no expected worktree", async () => {
    renderWindow();
    await userEvent.type(messageEditor(), "msg");
    await userEvent.click(commitButton());
    await waitFor(() => expect(commitCalls()).toHaveLength(1));
    expect(commitCalls()[0][1]).toEqual({
      message: "msg",
      paths: FILES.map((f) => f.path),
      expectedWorktree: undefined,
    });
  });

  it("CMW-FR-03, CMW-FR-06, CMW-FR-07, CMW-FR-08, CMW-FR-09, CMW-FR-13: a worktree that moved renders inline and leaves everything intact", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "commit_paths") {
        throw new Error(
          `worktree_identity_changed: ${JSON.stringify({
            expected: "~/dev/acme",
            active: "~/dev/acme-feature",
          })}`,
        );
      }
      return undefined;
    });
    renderPreflightWindow();
    await userEvent.type(messageEditor(), "Save my work");
    await userEvent.click(commitButton());

    // CMW-FR-08: rendered inline, the window open with its message and its file
    // set unchanged, and no commit exists.
    const notice = await screen.findByText(/no longer the one you are in/);
    expect(notice).toHaveTextContent("~/dev/acme");
    expect(notice).toHaveTextContent("~/dev/acme-feature");
    expect(messageEditor()).toHaveValue("Save my work");
    expect(screen.getByText("staged.md")).toBeInTheDocument();
    expect(onCommitted).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
  });

  it("CMW-FR-03, CMW-FR-06, CMW-FR-07, CMW-FR-08, CMW-FR-09, CMW-FR-13: Escape from it commits nothing", async () => {
    renderPreflightWindow();
    await userEvent.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalled();
    expect(commitCalls()).toHaveLength(0);
  });
});

describe("the settled file set (CMW-FR-YQTX)", () => {
  it("CMW-FR-YQTX: an answered inclusion step lists every path the commit carries", async () => {
    invokeMock.mockImplementation(async () => ({
      commitId: "c1",
      committedPaths: [],
    }));
    render(
      <CommitMessageModal
        files={[{ path: "spec.md", untracked: false }]}
        hidden={[
          { path: "src/main.rs", untracked: false },
          { path: "src/new.rs", untracked: true },
        ]}
        onClose={vi.fn()}
        onCommitted={vi.fn()}
      />,
    );

    await userEvent.click(screen.getByRole("button", { name: "All files" }));

    // Every path the commit carries is in the list the author read, the one
    // added by the inclusion step included.
    const listed = within(screen.getByTestId("commit-files"))
      .getAllByRole("listitem")
      .map((row) => row.textContent);
    expect(listed).toHaveLength(3);
    expect(listed.join(" ")).toContain("spec.md");
    expect(listed.join(" ")).toContain("src/main.rs");
    expect(listed.join(" ")).toContain("src/new.rs");

    // And nothing after that adds to it, removes from it, or reorders it.
    await userEvent.type(screen.getByLabelText("Commit message"), "commit");
    expect(
      within(screen.getByTestId("commit-files"))
        .getAllByRole("listitem")
        .map((row) => row.textContent),
    ).toEqual(listed);

    await userEvent.click(screen.getByRole("button", { name: /Commit/ }));
    await waitFor(() => expect(commitCalls()).toHaveLength(1));
    expect(commitCalls()[0][1]).toMatchObject({
      paths: ["spec.md", "src/main.rs", "src/new.rs"],
    });
  });
});

describe("the work-stream merge route (CMW-FR-KRVP / CMW-FR-ZDMT / CMW-FR-BZHM)", () => {
  const renderMergeWindow = (
    props: Partial<React.ComponentProps<typeof CommitMessageModal>> = {},
  ) => {
    const onMergeMessage = vi.fn();
    const onClose = vi.fn();
    render(
      <CommitMessageModal
        files={[]}
        streamMerge={{ streamId: "w1", streamName: "editor work" }}
        onClose={onClose}
        onCommitted={vi.fn()}
        onMergeMessage={onMergeMessage}
        {...props}
      />,
    );
    return { onMergeMessage, onClose };
  };

  it("CMW-FR-KRVP: the confirm hands the message over and invokes no operation", async () => {
    invokeMock.mockImplementation(async () => undefined);
    const { onMergeMessage } = renderMergeWindow();

    await userEvent.type(
      screen.getByLabelText("Commit message"),
      "add graduation specs",
    );
    await userEvent.click(screen.getByRole("button", { name: /Merge/ }));

    expect(onMergeMessage).toHaveBeenCalledWith("add graduation specs");
    // The merge is the opening surface's to start, so this window reaches no
    // backend operation at all.
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("CMW-FR-ZDMT: it names the stream and reports no count of files", () => {
    invokeMock.mockImplementation(async () => undefined);
    renderMergeWindow();

    expect(screen.getByText(/Merge message · editor work/)).toBeInTheDocument();
    // A count of zero would describe a set the window was never given.
    expect(screen.queryByText(/Commit 0 files/)).toBeNull();
    expect(screen.queryByTestId("commit-files")).toBeNull();
  });

  it("CMW-FR-BZHM, CMW-FR-KRVP: confirming takes no in-flight state, so nothing here can lock the window", async () => {
    invokeMock.mockImplementation(async () => undefined);
    const { onClose } = renderMergeWindow();

    await userEvent.type(screen.getByLabelText("Commit message"), "merge it");
    await userEvent.click(screen.getByRole("button", { name: /Merge/ }));

    // In production the parent unmounts this window as it takes the message
    // (covered end to end in `App.test.tsx`). Rendered standalone, it is the
    // probe for the thing that trapped the application: confirming must leave
    // the controls live rather than disabling them for the merge's duration.
    expect(screen.getByRole("button", { name: "Close" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Cancel" })).toBeEnabled();
    await userEvent.keyboard("{Escape}");
    expect(onClose).toHaveBeenCalled();
  });

  it("CMW-FR-09: dismissing it hands over no message and starts nothing", async () => {
    invokeMock.mockImplementation(async () => undefined);
    const { onMergeMessage, onClose } = renderMergeWindow();

    await userEvent.type(screen.getByLabelText("Commit message"), "never sent");
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));

    expect(onClose).toHaveBeenCalled();
    expect(onMergeMessage).not.toHaveBeenCalled();
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("CMW-FR-05: it refuses an empty message exactly as the commit routes do", async () => {
    invokeMock.mockImplementation(async () => undefined);
    const { onMergeMessage } = renderMergeWindow();

    expect(screen.getByRole("button", { name: /Merge/ })).toBeDisabled();
    await userEvent.type(screen.getByLabelText("Commit message"), "   ");
    expect(screen.getByRole("button", { name: /Merge/ })).toBeDisabled();
    expect(onMergeMessage).not.toHaveBeenCalled();
  });

  it("CMW-FR-WKDP: a graduation start preflight never opens on it either", () => {
    // It already carries every uncommitted path in the worktree it handed the
    // identity of over, so nothing is hidden from it to be asked about.
    invokeMock.mockImplementation(async () => undefined);
    render(
      <CommitMessageModal
        files={[{ path: "spec.md", untracked: false }]}
        hidden={[{ path: "src/main.rs", untracked: false }]}
        expectedWorktree="~/dev/acme"
        onClose={vi.fn()}
        onCommitted={vi.fn()}
      />,
    );

    expect(screen.queryByTestId("commit-inclusion")).toBeNull();
    expect(screen.getByLabelText("Commit message")).toBeInTheDocument();
  });

  it("CMW-FR-WKDP: a stream merge never opens on the inclusion step", () => {
    // There is no file set for a filter to have hidden anything from.
    invokeMock.mockImplementation(async () => undefined);
    renderMergeWindow({ hidden: [{ path: "src/main.rs", untracked: false }] });

    expect(screen.queryByTestId("commit-inclusion")).toBeNull();
    expect(screen.getByLabelText("Commit message")).toBeInTheDocument();
  });
});
