import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { WorktreeSelector, type SwitchOutcome } from "./WorktreeSelector";
import type { WorktreeContext } from "../types";
import { ACTIVE, defaultWorktreeContext } from "../test/worktreeFixtures";

// WTS-FR-UKSX: the Branch field of the New worktree dialog is normalized as the
// user types. The backend applies the same rule again (WTC-FR-XLXY), so what is
// pinned here is what the field shows and what the dialog sends.
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
}));

let context: WorktreeContext;
let handedOperation: (() => Promise<WorktreeContext>) | null;

beforeEach(() => {
  context = defaultWorktreeContext();
  context.branches.push({
    name: "Feature/X",
    kind: "local",
    headShortHash: "ddd4444",
  });
  handedOperation = null;
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string, args?: { branch?: string }) => {
    if (cmd === "list_worktrees_and_branches") return context;
    if (cmd === "propose_worktree_path") return `/dev/acme-platform-${args?.branch ?? ""}`;
    if (cmd === "append_log_records") return undefined;
    throw new Error(`unexpected command ${cmd}`);
  });
});

afterEach(cleanup);

function renderSelector() {
  const onSwitch = vi.fn(async (op: () => Promise<WorktreeContext>) => {
    handedOperation = op;
    return { ok: true } as SwitchOutcome;
  });
  render(<WorktreeSelector active={ACTIVE} onSwitch={onSwitch} />);
}

async function openEmptyDialog() {
  renderSelector();
  await userEvent.click(screen.getByTestId("worktree-selector"));
  await screen.findByTestId("worktree-group");
  await userEvent.click(screen.getByTestId("worktree-new"));
  return screen.findByLabelText("Branch");
}

describe("WorktreeSelector Branch field normalization (WTS-FR-UKSX)", () => {
  it("WTS-FR-UKSX: shows the normalized branch while the user types", async () => {
    const branch = await openEmptyDialog();
    await userEvent.type(branch, "More fixes");
    expect(branch).toHaveValue("more-fixes");
  });

  it("WTS-FR-UKSX: keeps a trailing hyphen so the next word can follow", async () => {
    const branch = await openEmptyDialog();
    await userEvent.type(branch, "More ");
    expect(branch).toHaveValue("more-");
    await userEvent.type(branch, "Fixes");
    expect(branch).toHaveValue("more-fixes");
  });

  it("WTS-FR-UKSX: an edit in the middle keeps the caret after what was typed", async () => {
    const branch = (await openEmptyDialog()) as HTMLInputElement;
    await userEvent.type(branch, "team/spike");
    branch.setSelectionRange(5, 5);
    await userEvent.type(branch, "Big ", {
      initialSelectionStart: 5,
      initialSelectionEnd: 5,
    });
    expect(branch).toHaveValue("team/big-spike");
    expect(branch.selectionStart).toBe(9);
  });

  it("WTS-FR-UKSX, WTS-FR-19: derives the location from the normalized branch", async () => {
    const branch = await openEmptyDialog();
    await userEvent.type(branch, "Team/Big Spike");
    await waitFor(() =>
      expect(screen.getByLabelText("Location")).toHaveValue(
        "/dev/acme-platform-team/big-spike",
      ),
    );
    const asked = invokeMock.mock.calls
      .filter(([cmd]) => cmd === "propose_worktree_path")
      .map(([, args]) => (args as { branch: string }).branch);
    expect(asked).not.toContain("Team/Big Spike");
    expect(asked[asked.length - 1]).toBe("team/big-spike");
  });

  it("WTS-FR-UKSX, WTS-FR-21: Create sends the normalized branch", async () => {
    const branch = await openEmptyDialog();
    await userEvent.type(branch, "More fixes");
    await waitFor(() =>
      expect(screen.getByLabelText("Location")).toHaveValue(
        "/dev/acme-platform-more-fixes",
      ),
    );
    const create = screen.getByRole("button", { name: "Create" });
    expect(create).toBeEnabled();
    await userEvent.click(create);

    expect(handedOperation).not.toBeNull();
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === "create_worktree" ? context : undefined,
    );
    await handedOperation!();
    expect(invokeMock).toHaveBeenCalledWith("create_worktree", {
      branch: "more-fixes",
      path: "/dev/acme-platform-more-fixes",
    });
  });

  it("WTS-FR-UKSX, WTS-FR-16: a pre-filled branch stays as reported until it is edited", async () => {
    renderSelector();
    await userEvent.click(screen.getByTestId("worktree-selector"));
    await userEvent.click(await screen.findByText("Feature/X"));
    const choice = screen
      .getByRole("button", { name: "Check out here" })
      .closest("div")!;
    await userEvent.click(
      within(choice).getByRole("button", { name: "New worktree…" }),
    );

    const branch = await screen.findByLabelText("Branch");
    expect(branch).toHaveValue("Feature/X");
    expect(invokeMock).toHaveBeenCalledWith("propose_worktree_path", {
      branch: "Feature/X",
    });

    await userEvent.type(branch, "Y");
    expect(branch).toHaveValue("feature/xy");
  });

  it("WTS-FR-UKSX, WTS-FR-21: an unedited pre-filled branch is created as reported", async () => {
    renderSelector();
    await userEvent.click(screen.getByTestId("worktree-selector"));
    await userEvent.click(await screen.findByText("Feature/X"));
    const choice = screen
      .getByRole("button", { name: "Check out here" })
      .closest("div")!;
    await userEvent.click(
      within(choice).getByRole("button", { name: "New worktree…" }),
    );
    await waitFor(() =>
      expect(screen.getByLabelText("Location")).toHaveValue(
        "/dev/acme-platform-Feature/X",
      ),
    );
    await userEvent.click(screen.getByRole("button", { name: "Create" }));

    expect(handedOperation).not.toBeNull();
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === "create_worktree" ? context : undefined,
    );
    await handedOperation!();
    expect(invokeMock).toHaveBeenCalledWith("create_worktree", {
      branch: "Feature/X",
      path: "/dev/acme-platform-Feature/X",
    });
  });

  it("WTS-FR-UKSX, WTS-FR-21: a name with nothing a branch keeps still reaches the backend", async () => {
    // The backend refuses it with the typed "unknown branch" (WTC-FR-XLXY),
    // which the dialog renders inline (WTS-FR-24).
    const branch = await openEmptyDialog();
    await userEvent.type(branch, "!");
    expect(branch).toHaveValue("-");
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Create" })).toBeEnabled(),
    );
  });
});
