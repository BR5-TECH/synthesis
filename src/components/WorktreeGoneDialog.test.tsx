import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { WorktreeGoneDialog, WorktreeGoneGate } from "./WorktreeGoneDialog";
import {
  applyActiveRootState,
  isActiveRootGone,
  resetActiveRootHealth,
} from "../state/activeRootHealth";
import { resetLogBufferForTest } from "../logging";
import type { ActiveRootState, WorktreeEntry } from "../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: () => Promise.resolve(() => {}),
  emit: () => Promise.resolve(),
}));

const linked: WorktreeEntry = {
  path: "/data/w/w1",
  name: "w1",
  branch: "synthesis/stream/one",
  headShortHash: "abc1234",
  isDetached: false,
  isActive: true,
  isPrimary: false,
  isMissing: false,
};

const gone: ActiveRootState = {
  state: "gone",
  primaryPath: "/dev/acme",
  primaryBranch: "main",
};

function answering(answer: ActiveRootState) {
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "active_root_state") return Promise.resolve(answer);
    if (cmd === "append_log_records") return Promise.resolve(undefined);
    return Promise.reject(new Error(`unexpected command: ${cmd}`));
  });
}

function dialogWith(over: Partial<Parameters<typeof WorktreeGoneDialog>[0]> = {}) {
  const onSwitch = vi.fn(async () => null as string | null);
  const onCloseProject = vi.fn();
  render(
    <WorktreeGoneDialog
      worktreeName="w1"
      primaryBranch="main"
      onSwitch={onSwitch}
      onCloseProject={onCloseProject}
      {...over}
    />,
  );
  return { onSwitch, onCloseProject };
}

function gate() {
  const onOpened = vi.fn();
  const onSwitch = vi.fn(async () => null as string | null);
  const onCloseProject = vi.fn();
  render(
    <WorktreeGoneGate
      activeWorktree={linked}
      contentRootEpoch={0}
      onSwitch={onSwitch}
      onCloseProject={onCloseProject}
      onOpened={onOpened}
    />,
  );
  return { onOpened, onSwitch, onCloseProject };
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockImplementation((cmd: string) =>
    cmd === "append_log_records"
      ? Promise.resolve(undefined)
      : Promise.reject(new Error(`unexpected command: ${cmd}`)),
  );
  resetLogBufferForTest();
  resetActiveRootHealth();
});
afterEach(cleanup);

describe("the worktree gone dialog", () => {
  it("WTS-FR-WIFO: it says that the worktree is gone and that unsaved changes are discarded", () => {
    dialogWith();

    const dialog = screen.getByTestId("worktree-gone-dialog");
    expect(dialog).toHaveTextContent("w1");
    expect(dialog).toHaveTextContent(/no longer on disk/);
    expect(dialog).toHaveTextContent(/Unsaved changes in open tabs are discarded/);
    expect(screen.getByRole("button", { name: "Close project" })).toBeEnabled();
  });

  it("WTS-FR-LLBR: the Switch action names the primary worktree's branch", () => {
    dialogWith();

    expect(screen.getByRole("button", { name: "Switch to main" })).toBeInTheDocument();
  });

  it("WTS-FR-LLBR: where the primary worktree holds no branch, the action names the worktree", () => {
    dialogWith({ primaryBranch: null });

    expect(
      screen.getByRole("button", { name: "Switch to primary worktree" }),
    ).toBeInTheDocument();
  });

  it("WTS-FR-FEEK: Escape and a press outside do not close it", () => {
    const { onSwitch, onCloseProject } = dialogWith();

    fireEvent.keyDown(document.body, { key: "Escape" });
    fireEvent.mouseDown(document.querySelector(".scrim")!);

    expect(screen.getByTestId("worktree-gone-dialog")).toBeInTheDocument();
    expect(onSwitch).not.toHaveBeenCalled();
    expect(onCloseProject).not.toHaveBeenCalled();
  });

  it("WTS-FR-FEEK: Escape reaches nothing beneath the dialog", () => {
    // Registered first, on the surfaces' own level, so the order is not the proof.
    const beneath = vi.fn();
    document.addEventListener("keydown", beneath);
    dialogWith();

    fireEvent.keyDown(document.body, { key: "Escape" });
    document.removeEventListener("keydown", beneath);

    expect(beneath).not.toHaveBeenCalled();
  });

  it("WTS-FR-ZHLG: focus starts on Switch, and Tab and Shift+Tab stay inside the dialog", () => {
    dialogWith();
    const switchButton = screen.getByTestId("worktree-gone-switch");
    const closeButton = screen.getByTestId("worktree-gone-close");
    expect(switchButton).toHaveFocus();

    fireEvent.keyDown(switchButton, { key: "Tab" });
    expect(closeButton).toHaveFocus();
    fireEvent.keyDown(closeButton, { key: "Tab" });
    expect(switchButton).toHaveFocus();
    fireEvent.keyDown(switchButton, { key: "Tab", shiftKey: true });
    expect(closeButton).toHaveFocus();
  });

  it("WTS-FR-ZHLG: a key that starts behind the dialog goes nowhere, and focus comes back", () => {
    const behind = document.createElement("input");
    document.body.appendChild(behind);
    const reached = vi.fn();
    document.addEventListener("keydown", reached, true);
    window.addEventListener("keydown", reached);
    dialogWith();

    fireEvent.keyDown(behind, { key: "k", metaKey: true });
    document.removeEventListener("keydown", reached, true);
    window.removeEventListener("keydown", reached);
    behind.remove();

    expect(reached).not.toHaveBeenCalled();
    expect(screen.getByTestId("worktree-gone-switch")).toHaveFocus();
  });

  it("WTS-FR-ZHLG: an accelerator pressed inside the dialog reaches no surface", () => {
    const reached = vi.fn();
    document.addEventListener("keydown", reached, true);
    window.addEventListener("keydown", reached);
    dialogWith();

    fireEvent.keyDown(screen.getByTestId("worktree-gone-switch"), {
      key: "k",
      metaKey: true,
    });
    document.removeEventListener("keydown", reached, true);
    window.removeEventListener("keydown", reached);

    expect(reached).not.toHaveBeenCalled();
  });

  it("WTS-FR-ZHLG: Enter and Space keep their default on a button", () => {
    dialogWith();
    const button = screen.getByTestId("worktree-gone-switch");

    expect(fireEvent.keyDown(button, { key: "Enter" })).toBe(true);
    expect(fireEvent.keyDown(button, { key: " " })).toBe(true);
  });

  it("WTS-FR-ZHLG: a surface that takes the focus does not keep it", () => {
    const behind = document.createElement("input");
    document.body.appendChild(behind);
    dialogWith();

    behind.focus();

    expect(screen.getByTestId("worktree-gone-switch")).toHaveFocus();
    behind.remove();
  });

  it("WTS-FR-ZHLG: a press on the scrim keeps the focus where it is", () => {
    dialogWith();

    const kept = fireEvent.mouseDown(document.querySelector(".scrim")!);

    expect(kept).toBe(false);
  });

  it("WTS-FR-YKSZ: a failed switch gives the focus back to the action", async () => {
    dialogWith({ onSwitch: vi.fn(async () => "not a git repository") });

    await userEvent.click(screen.getByTestId("worktree-gone-switch"));

    await screen.findByTestId("worktree-gone-error");
    expect(screen.getByTestId("worktree-gone-switch")).toHaveFocus();
  });

  it("WTS-FR-YKSZ: the Switch action calls the switch, and both actions wait for it", async () => {
    let finish: (error: string | null) => void = () => {};
    const onSwitch = vi.fn(
      () => new Promise<string | null>((resolve) => (finish = resolve)),
    );
    dialogWith({ onSwitch });

    await userEvent.click(screen.getByTestId("worktree-gone-switch"));

    expect(onSwitch).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId("worktree-gone-switch")).toBeDisabled();
    expect(screen.getByTestId("worktree-gone-close")).toBeDisabled();
    await act(async () => finish(null));
  });

  it("WTS-FR-YKSZ: a typed error renders inline, the dialog stays, and the action opens again", async () => {
    dialogWith({ onSwitch: vi.fn(async () => "not a git repository") });

    await userEvent.click(screen.getByTestId("worktree-gone-switch"));

    expect(await screen.findByTestId("worktree-gone-error")).toHaveTextContent(
      "not a git repository",
    );
    expect(screen.getByTestId("worktree-gone-dialog")).toBeInTheDocument();
    expect(screen.getByTestId("worktree-gone-switch")).toBeEnabled();
  });

  it("WTS-FR-BTMD: the Close project action closes the project", async () => {
    const { onCloseProject } = dialogWith();

    await userEvent.click(screen.getByTestId("worktree-gone-close"));

    expect(onCloseProject).toHaveBeenCalledTimes(1);
  });
});

describe("the gate", () => {
  it("WTS-FR-WIFO, SNV-FR-56: a gone answer opens the dialog and closes every other overlay", async () => {
    answering(gone);
    const { onOpened } = gate();

    expect(await screen.findByTestId("worktree-gone-dialog")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Switch to main" })).toBeInTheDocument();
    expect(onOpened).toHaveBeenCalledTimes(1);
  });

  it("WTS-FR-FEEK: a healthy answer closes the dialog and discards nothing", async () => {
    answering(gone);
    const { onSwitch, onCloseProject } = gate();
    await screen.findByTestId("worktree-gone-dialog");

    answering({ ...gone, state: "healthy" });
    fireEvent(window, new Event("focus"));

    await waitFor(() => expect(screen.queryByTestId("worktree-gone-dialog")).toBeNull());
    expect(isActiveRootGone()).toBe(false);
    expect(onSwitch).not.toHaveBeenCalled();
    expect(onCloseProject).not.toHaveBeenCalled();
  });

  it("WTS-FR-FEEK: an unknown answer opens no dialog", async () => {
    answering({ ...gone, state: "unknown" });
    gate();

    await waitFor(() =>
      expect(invokeMock.mock.calls.some((c) => c[0] === "active_root_state")).toBe(true),
    );
    await act(async () => {});

    expect(screen.queryByTestId("worktree-gone-dialog")).toBeNull();
  });

  it("WTS-FR-FEEK: an unknown answer closes no dialog", async () => {
    answering(gone);
    gate();
    await screen.findByTestId("worktree-gone-dialog");

    answering({ ...gone, state: "unknown" });
    await act(async () => {
      fireEvent(window, new Event("focus"));
    });

    expect(screen.getByTestId("worktree-gone-dialog")).toBeInTheDocument();
  });
});

describe("the state a teardown reads (WTS-FR-OSKY)", () => {
  it("WTS-FR-OSKY: it holds on a gone answer and clears on a healthy one", () => {
    expect(isActiveRootGone()).toBe(false);
    applyActiveRootState(gone);
    expect(isActiveRootGone()).toBe(true);
    applyActiveRootState({ ...gone, state: "unknown" });
    expect(isActiveRootGone()).toBe(true);
    applyActiveRootState({ ...gone, state: "healthy" });
    expect(isActiveRootGone()).toBe(false);
  });
});
