import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";

import { MainBranchWriteHost } from ".";
import {
  ensureMainBranchWrite,
  runMainBranchWrite,
  WRITE_CANCELLED,
} from "../../state/mainBranchWrite";
import { onNewWorktreeRequested } from "../../state/newWorktreeRequests";
import {
  mainGuard,
  scriptedReads,
  unconfirmedRefusal,
} from "../../test/mainBranchWriteFixtures";

const logged = vi.hoisted(() => [] as { level: string; message: string; fields: unknown }[]);
vi.mock("../../logging", async (original) => ({
  ...(await original<typeof import("../../logging")>()),
  logInfo: (_d: unknown, message: string, fields: unknown) =>
    logged.push({ level: "INFO", message, fields }),
  logWarn: (_d: unknown, message: string, fields: unknown) =>
    logged.push({ level: "WARN", message, fields }),
}));

beforeEach(() => {
  logged.length = 0;
});
afterEach(cleanup);

function host() {
  return render(
    <>
      <button>opener</button>
      <MainBranchWriteHost />
    </>,
  );
}

describe("the main branch write confirmation (MBW-main-branch-write.md)", () => {
  it("MBW-FR-RMCO, MBW-FR-RXAR, MBW-FR-XBJS, MBW-FR-DBGA: a main branch with no confirmation shows the confirmation first, and Confirm records it before the write", async () => {
    const reads = scriptedReads(mainGuard());
    host();
    const write = vi.fn(async () => "written");
    let result: unknown;
    const run = runMainBranchWrite("create_draft", write).then((value) => {
      result = value;
    });

    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("main is the default branch of this repository.");
    expect(dialog).toHaveTextContent("You asked to create a draft. The draft is committed to main.");
    expect(write).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Confirm write to main" }));
    await act(async () => {
      await run;
    });

    expect(reads.confirms).toEqual(["main"]);
    expect(write).toHaveBeenCalledTimes(1);
    expect(result).toBe("written");
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("MBW-FR-RMCO: a branch that is not a main branch, or one already confirmed, writes with no confirmation", async () => {
    scriptedReads(mainGuard({ isDefault: false, isMain: false, branch: "feature" }));
    host();
    await expect(ensureMainBranchWrite("commit")).resolves.toBe(true);
    scriptedReads(mainGuard({ confirmed: true }));
    await expect(ensureMainBranchWrite("commit")).resolves.toBe(true);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("MBW-FR-XBJS, MBW-FR-GYVD: Cancel, Escape, and the backdrop close it and invoke no write", async () => {
    scriptedReads(mainGuard());
    host();
    const write = vi.fn(async () => undefined);
    for (const dismiss of ["cancel", "escape", "backdrop"] as const) {
      let result: unknown;
      const run = runMainBranchWrite("commit", write).then((value) => {
        result = value;
      });
      const dialog = await screen.findByRole("dialog");
      if (dismiss === "cancel") fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
      if (dismiss === "escape") fireEvent.keyDown(document, { key: "Escape" });
      if (dismiss === "backdrop") fireEvent.mouseDown(dialog.parentElement!);
      await act(async () => {
      await run;
    });
      expect(result).toBe(WRITE_CANCELLED);
    }
    expect(write).not.toHaveBeenCalled();
  });

  it("MBW-FR-XBJS: a failed confirmation shows its error inline and invokes no write", async () => {
    const reads = scriptedReads(mainGuard());
    reads.confirmError = new Error("github_unreachable");
    host();
    const write = vi.fn(async () => undefined);
    void runMainBranchWrite("push", write);
    await screen.findByRole("dialog");
    fireEvent.click(screen.getByRole("button", { name: "Confirm write to main" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("github_unreachable");
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    expect(write).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  });

  it("MBW-FR-GYVD, MBW-FR-WCBQ: focus starts on Cancel, stays inside, and returns to the opener", async () => {
    scriptedReads(mainGuard());
    host();
    const opener = screen.getByRole("button", { name: "opener" });
    opener.focus();
    let answer: unknown;
    const run = ensureMainBranchWrite("commit").then((value) => {
      answer = value;
    });
    await screen.findByRole("dialog");
    const cancel = screen.getByRole("button", { name: "Cancel" });
    const confirm = screen.getByRole("button", { name: "Confirm write to main" });
    expect(cancel).toHaveFocus();
    confirm.focus();
    fireEvent.keyDown(document, { key: "Tab" });
    expect(cancel).toHaveFocus();
    fireEvent.keyDown(document, { key: "Tab", shiftKey: true });
    expect(confirm).toHaveFocus();
    expect(screen.getByRole("dialog")).toHaveAccessibleDescription(/main is the default branch/);
    fireEvent.keyDown(document, { key: "Escape" });
    await act(async () => {
      await run;
    });
    expect(answer).toBe(false);
    expect(opener).toHaveFocus();
  });

  it("MBW-FR-RXAR: a long branch name is cut in the middle with its whole value kept", async () => {
    const long = `feature/${"x".repeat(60)}/end`;
    scriptedReads(mainGuard({ branch: long }));
    host();
    void ensureMainBranchWrite("commit");
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveAccessibleName(`Write to ${long}?`);
    expect(screen.getByRole("heading").textContent).toContain("…");
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  });

  it("MBW-FR-JZME, MBW-FR-GYVD: New worktree leaves focus in the New worktree dialog, and Cancel gives it back to the opener", async () => {
    scriptedReads(mainGuard({ protection: "protected" }));
    render(
      <>
        <button>opener</button>
        <input aria-label="Branch" />
        <MainBranchWriteHost />
      </>,
    );
    const opener = screen.getByRole("button", { name: "opener" });
    const branchField = screen.getByLabelText("Branch");
    // The New worktree dialog takes focus on its Branch field when it opens.
    const stop = onNewWorktreeRequested(() => branchField.focus());
    const open = async () => {
      opener.focus();
      const run = runMainBranchWrite("create_draft", vi.fn(async () => undefined));
      await screen.findByRole("dialog");
      return { run };
    };

    const { run: first } = await open();
    fireEvent.click(screen.getByRole("button", { name: "New worktree" }));
    await act(async () => {
      await first;
    });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(branchField).toHaveFocus();

    const { run: second } = await open();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await act(async () => {
      await second;
    });
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
    expect(opener).toHaveFocus();
    stop();
  });

  it("MBW-FR-JZME, MBW-FR-RXAR, MBW-FR-PJTG: a protected branch shows the confirmation with the GitHub line, keeps Confirm and Cancel, and offers New worktree as a secondary action", async () => {
    scriptedReads(mainGuard({ protection: "protected" }));
    host();
    const requested = vi.fn();
    const stop = onNewWorktreeRequested(requested);
    const write = vi.fn(async () => undefined);
    let result: unknown;
    const run = runMainBranchWrite("push", write).then((value) => {
      result = value;
    });
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("main is the default branch of this repository, and GitHub protects it.");
    expect(dialog).toHaveTextContent("GitHub protects main and may refuse a push.");
    expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Confirm write to main" })).toBeEnabled();
    expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus();
    fireEvent.click(screen.getByRole("button", { name: "New worktree" }));
    await act(async () => {
      await run;
    });
    stop();
    expect(requested).toHaveBeenCalledTimes(1);
    expect(result).toBe(WRITE_CANCELLED);
    expect(write).not.toHaveBeenCalled();
    expect(logged.map((r) => r.message)).not.toContain("main branch write blocked");
    expect(logged).toContainEqual({
      level: "INFO",
      message: "main branch write confirmation shown",
      fields: { branch: "main", write: "push", surface: "dialog" },
    });
  });

  it("MBW-FR-JZME, MBW-FR-ILFN, MBW-FR-XBJS: a confirm of a protected branch runs the write, and the guard then reports the branch as confirmed", async () => {
    const reads = scriptedReads(mainGuard({ protection: "protected" }));
    host();
    const write = vi.fn(async () => "pushed");
    const run = runMainBranchWrite("push", write);
    fireEvent.click(await screen.findByRole("button", { name: "Confirm write to main" }));
    await act(async () => {
      await expect(run).resolves.toBe("pushed");
    });
    expect(reads.confirms).toEqual(["main"]);
    expect(write).toHaveBeenCalledTimes(1);
    await expect(ensureMainBranchWrite("push")).resolves.toBe(true);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("MBW-FR-UNAN: an unknown protection of the default branch says why, Check again reads with refresh, and Confirm stays available", async () => {
    const reads = scriptedReads(
      mainGuard({ protection: "unknown", protectionError: "github_unreachable" }),
    );
    host();
    void ensureMainBranchWrite("commit");
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("The branch protection could not be read: GitHub is unreachable.");
    expect(screen.getByRole("button", { name: "Confirm write to main" })).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: "Check again" }));
    await waitFor(() => expect(reads.reads[reads.reads.length - 1]).toEqual({ branch: "main", refresh: true }));
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  });

  it("MBW-FR-UNAN: a guard read that fails shows the error with Try again and invokes no write until the read succeeds", async () => {
    const reads = scriptedReads(new Error("no project open"));
    host();
    const write = vi.fn(async () => "done");
    let result: unknown;
    const run = runMainBranchWrite("delete_draft", write).then((value) => {
      result = value;
    });
    expect(await screen.findByRole("alert")).toHaveTextContent("no project open");
    expect(screen.getByRole("button", { name: "Confirm write to the branch" })).toBeDisabled();
    expect(write).not.toHaveBeenCalled();
    reads.answer = mainGuard();
    fireEvent.click(screen.getByRole("button", { name: "Try again" }));
    fireEvent.click(await screen.findByRole("button", { name: "Confirm write to main" }));
    await act(async () => {
      await run;
    });
    expect(write).toHaveBeenCalledTimes(1);
    expect(result).toBe("done");
  });

  it("MBW-FR-YSMC: a refusal of the backend shows the confirmation with the guard it carries, and a confirm invokes the write once more", async () => {
    const reads = scriptedReads(mainGuard({ isMain: false, isDefault: false }));
    host();
    let calls = 0;
    const write = vi.fn(async () => {
      calls += 1;
      if (calls === 1) throw unconfirmedRefusal(mainGuard({ branch: "trunk" }));
      return "second";
    });
    let result: unknown;
    const run = runMainBranchWrite("commit", write).then((value) => {
      result = value;
    });
    const dialog = await screen.findByRole("dialog");
    expect(dialog).toHaveTextContent("trunk is the default branch");
    fireEvent.click(screen.getByRole("button", { name: "Confirm write to trunk" }));
    await act(async () => {
      await run;
    });
    expect(reads.confirms).toEqual(["trunk"]);
    expect(write).toHaveBeenCalledTimes(2);
    expect(result).toBe("second");
  });

  it("MBW-FR-ILFN, MBW-FR-PJTG: after a confirmation no further confirmation shows for that branch on any surface, and each step is logged with no content", async () => {
    // The backend holds the confirmation, so every later read answers it.
    scriptedReads(mainGuard());
    host();
    const first = ensureMainBranchWrite("commit");
    await screen.findByRole("dialog");
    fireEvent.click(screen.getByRole("button", { name: "Confirm write to main" }));
    await act(async () => {
      await first;
    });
    await expect(ensureMainBranchWrite("push")).resolves.toBe(true);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(logged.map((r) => r.message)).toEqual([
      "main branch write confirmation shown",
      "main branch write confirmation confirmed",
    ]);
    expect(logged[0].fields).toEqual({ branch: "main", write: "commit", surface: "dialog" });
  });

  it("MBW-FR-RXAR, MBW-FR-JZME: the reason says GitHub protects the branch, alone or with the default branch", async () => {
    const { reasonSentence } = await import("../../state/mainBranchWrite");
    expect(reasonSentence(mainGuard({ protection: "protected" }))).toBe(
      "main is the default branch of this repository, and GitHub protects it.",
    );
    expect(reasonSentence(mainGuard({ isDefault: false, protection: "protected", branch: "release" }))).toBe(
      "GitHub protects release.",
    );
  });

  it("MBW-FR-PJTG: a cancel is logged with the branch and the kind of write", async () => {
    scriptedReads(mainGuard());
    host();
    const run = ensureMainBranchWrite("delete_draft");
    await screen.findByRole("dialog");
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await act(async () => {
      await run;
    });
    expect(logged).toContainEqual({
      level: "INFO",
      message: "main branch write confirmation cancelled",
      fields: { branch: "main", write: "delete_draft", surface: "dialog" },
    });
  });

  it("MBW-FR-UNAN, MBW-FR-JZME: Check again that finds the branch protected adds the GitHub line and keeps Confirm available", async () => {
    const reads = scriptedReads(mainGuard({ protection: "unknown", protectionError: "github_unreachable" }));
    host();
    const run = ensureMainBranchWrite("commit");
    await screen.findByRole("dialog");
    reads.answer = mainGuard({ protection: "protected" });
    fireEvent.click(screen.getByRole("button", { name: "Check again" }));
    expect(await screen.findByTestId("main-branch-write-protected")).toHaveTextContent(
      "GitHub protects main and may refuse a push.",
    );
    expect(screen.getByRole("button", { name: "Confirm write to main" })).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await act(async () => {
      await expect(run).resolves.toBe(false);
    });
  });

  it("MBW-FR-YSMC: the write is invoked again once only, and a second refusal goes back to the caller", async () => {
    scriptedReads(mainGuard({ isMain: false, isDefault: false }));
    host();
    const write = vi.fn(async () => {
      throw unconfirmedRefusal();
    });
    const run = runMainBranchWrite("commit", write);
    await screen.findByRole("dialog");
    fireEvent.click(screen.getByRole("button", { name: "Confirm write to main" }));
    await act(async () => {
      await expect(run).rejects.toMatch("main_branch_write_unconfirmed");
    });
    expect(write).toHaveBeenCalledTimes(2);
  });
});
