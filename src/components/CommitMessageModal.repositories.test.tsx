/**
 * The commit message window over a file set of more than one repository
 * (CMW-FR-WZRQ), and the partial commit refusal (GTC-FR-TJPJ, CMW-FR-08).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { partialCommit } from "../api/git";
import { resetLogBufferForTest } from "../logging";
import { otherCommand } from "../test/invokeMocks";
import {
  CommitMessageModal,
  commitErrorMessage,
  commitRepositories,
  groupCommitFiles,
  type CommitFile,
  type CommitRepository,
} from "./CommitMessageModal";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const MAIN: CommitRepository = { path: "", label: "acme" };
const KIT: CommitRepository = { path: "vendor/kit", label: "vendor/kit" };

const MAIN_FILE: CommitFile = {
  path: "src/a.ts",
  untracked: false,
  repositoryChain: [MAIN],
};
const KIT_FILE: CommitFile = {
  path: "vendor/kit/docs/b.md",
  untracked: true,
  repositoryChain: [KIT, MAIN],
};

/** What `commit_paths` answers in the next test. */
let commit: () => Promise<unknown>;

const onClose = vi.fn();
const onCommitted = vi.fn();

function renderWindow(files: CommitFile[]) {
  return render(
    <CommitMessageModal files={files} onClose={onClose} onCommitted={onCommitted} />,
  );
}

beforeEach(() => {
  resetLogBufferForTest();
  onClose.mockReset();
  onCommitted.mockReset();
  commit = async () => ({ commitId: "c1", committedPaths: [], repositories: [] });
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) =>
    cmd === "commit_paths" ? commit() : otherCommand(cmd),
  );
});

afterEach(cleanup);

describe("a file set of more than one repository (CMW-FR-WZRQ)", () => {
  it("CMW-FR-WZRQ: states each repository the commit is made in, above the list, and groups the list by repository", () => {
    renderWindow([KIT_FILE, MAIN_FILE]);
    const statement = screen.getByTestId("commit-repositories");
    expect(statement).toHaveTextContent("One commit is made in each of 2 repositories");
    const names = within(statement)
      .getAllByRole("listitem")
      .map((li) => li.textContent);
    expect(names).toEqual(["acme", "vendor/kit"]);

    const list = screen.getByTestId("commit-files");
    // The statement renders above the list.
    expect(
      statement.compareDocumentPosition(list) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    const groups = list.querySelectorAll<HTMLElement>(".commit-files__group");
    expect([...groups].map((g) => g.dataset.repository)).toEqual(["", "vendor/kit"]);
    expect(
      [...list.querySelectorAll(".commit-files__group-label")].map((l) => l.textContent),
    ).toEqual(["acme", "vendor/kit"]);
    expect(within(groups[0]).getByText("src/a.ts")).toBeInTheDocument();
    expect(within(groups[1]).getByText("vendor/kit/docs/b.md")).toBeInTheDocument();
    // CMW-FR-06: the untracked mark stays on its row inside the group.
    expect(within(groups[1]).getByText("new")).toBeInTheDocument();
  });

  it("CMW-FR-WZRQ, GSM-FR-MPWK: a submodule file alone names the parent that records its pointer", () => {
    renderWindow([KIT_FILE]);
    const statement = screen.getByTestId("commit-repositories");
    expect(within(statement).getAllByRole("listitem").map((li) => li.textContent)).toEqual([
      "acme",
      "vendor/kit",
    ]);
  });

  it("CMW-FR-WZRQ, CMW-FR-06: a file set of one repository renders the flat list and no statement", () => {
    renderWindow([MAIN_FILE, { path: "src/c.ts", untracked: false }]);
    expect(screen.queryByTestId("commit-repositories")).toBeNull();
    expect(document.querySelector(".commit-files__group")).toBeNull();
    expect(within(screen.getByTestId("commit-files")).getByText("src/c.ts")).toBeInTheDocument();
  });

  it("CMW-FR-WZRQ: a pointer path is grouped under its parent, beside a submodule file", () => {
    renderWindow([KIT_FILE, { path: "vendor/kit", untracked: false, repositoryChain: [MAIN] }]);
    const groups = screen
      .getByTestId("commit-files")
      .querySelectorAll<HTMLElement>(".commit-files__group");
    expect(within(groups[0]).getByText("vendor/kit")).toBeInTheDocument();
    expect(within(groups[1]).getByText("vendor/kit/docs/b.md")).toBeInTheDocument();
  });

  it("CMW-FR-WZRQ, CMW-FR-13: a hidden submodule file taken in adds its repository to the statement and the commit", async () => {
    render(
      <CommitMessageModal
        files={[MAIN_FILE]}
        hidden={[{ ...KIT_FILE, untracked: false }]}
        onClose={onClose}
        onCommitted={onCommitted}
      />,
    );
    await userEvent.click(screen.getByRole("button", { name: "All revisioned" }));
    const statement = screen.getByTestId("commit-repositories");
    expect(within(statement).getAllByRole("listitem").map((li) => li.textContent)).toEqual([
      "acme",
      "vendor/kit",
    ]);
    await userEvent.type(screen.getByLabelText("Commit message"), "m");
    await userEvent.click(screen.getByRole("button", { name: /^Commit$/ }));
    await waitFor(() => expect(onCommitted).toHaveBeenCalledOnce());
    const commitCall = invokeMock.mock.calls.find((c) => c[0] === "commit_paths");
    expect(commitCall?.[1].paths).toEqual(["src/a.ts", "vendor/kit/docs/b.md"]);
  });

  it("CMW-FR-WZRQ: orders the repositories main first, then by path", () => {
    const z: CommitRepository = { path: "z", label: "z" };
    expect(
      commitRepositories([
        { path: "z/a", untracked: false, repositoryChain: [z, MAIN] },
        KIT_FILE,
      ]),
    ).toEqual([MAIN, KIT, z]);
    expect(groupCommitFiles([KIT_FILE, MAIN_FILE]).map((g) => g.repository)).toEqual([
      MAIN,
      KIT,
    ]);
  });
});

describe("the partial commit refusal (GTC-FR-TJPJ, CMW-FR-08)", () => {
  const refusal = `partial_commit: ${JSON.stringify({
    committed: [{ repository: "vendor/kit", commitId: "k1", committedPaths: ["vendor/kit/docs/b.md"] }],
    failed: "",
    cause: "nothing_to_commit",
  })}`;

  it("GTC-FR-TJPJ: reads the commits made, the repository that failed, and the cause", () => {
    expect(partialCommit(refusal)).toEqual({
      committed: [
        { repository: "vendor/kit", commitId: "k1", committedPaths: ["vendor/kit/docs/b.md"] },
      ],
      failed: "",
      cause: "nothing_to_commit",
    });
    expect(partialCommit(new Error(refusal))?.failed).toBe("");
    expect(partialCommit("nothing_to_commit")).toBeNull();
    expect(partialCommit("partial_commit: not json")).toBeNull();
  });

  it("CMW-FR-08, GTC-FR-TJPJ: states the repository that failed and the commits that stay", () => {
    const labels = new Map([
      ["", "acme"],
      ["vendor/kit", "vendor/kit"],
    ]);
    expect(commitErrorMessage(refusal, labels)).toBe(
      "The commit failed in acme: nothing_to_commit. The commits already made in vendor/kit stay.",
    );
    // A payload the window cannot read still names the partial state.
    expect(commitErrorMessage("partial_commit")).toContain("The commits already made stay.");
    // A repository the labels do not hold is named by its path, or as the main one.
    expect(commitErrorMessage(refusal)).toBe(
      "The commit failed in the main repository: nothing_to_commit. The commits already made in vendor/kit stay.",
    );
  });

  it("CMW-FR-08, GTC-FR-TJPJ: a partial commit renders inline and keeps the window open with the message", async () => {
    commit = () => Promise.reject(refusal);
    renderWindow([KIT_FILE, MAIN_FILE]);
    await userEvent.type(screen.getByLabelText("Commit message"), "Update tokens");
    await userEvent.click(screen.getByRole("button", { name: /^Commit$/ }));
    expect(await screen.findByTestId("commit-error")).toHaveTextContent(
      "The commit failed in acme: nothing_to_commit. The commits already made in vendor/kit stay.",
    );
    expect(onCommitted).not.toHaveBeenCalled();
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByLabelText("Commit message")).toHaveValue("Update tokens");
    await waitFor(() => expect(screen.getByLabelText("Commit message")).toBeEnabled());
  });
});
