import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { useState } from "react";

import { Changes } from "./Changes";
import { Git } from "./Git";
import { PushControl } from "./PushControl";
import { GitTransferProvider, useGitTransferState } from "../hooks/useGitTransfer";
import { GIT_OPERATION_FINISHED, GIT_OUTPUT_LINE } from "../events";
import { flushLogs, resetLogBufferForTest } from "../logging";
import { resetAppPreferencesCache } from "../state/appPreferences";
import { resetPanelReveals } from "../state/panelReveal";
import { createEventBus } from "../test/searchEvents";
import {
  chooseAction,
  makeBackend,
  onOpenGlobalSettings,
  onOpenDiff,
  onRequestCommitMessage,
  onRequestGithubToken,
  onRequestRollback,
  primary,
  resetHandlers,
} from "../test/changesFixtures";

// GIT-FR-QMYB: the top-chrome control, the Git panel, and the Changes panel are
// three ways to start one push. They are rendered here under one provider, as
// the main window renders them.
const bus = createEventBus();
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: (ev: { payload: unknown }) => void) =>
    bus.listen(name, handler),
}));

const backend = makeBackend(invokeMock);
let settlePush: (error?: string) => void;
let pushInvocations: number;

function installBackend() {
  backend({
    sync: { hasRemote: true, hasUpstream: true, ahead: 2, behind: 0 },
  });
  const base = invokeMock.getMockImplementation()!;
  invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
    if (cmd === "push_current_branch") {
      pushInvocations += 1;
      return new Promise<undefined>((resolve, reject) => {
        settlePush = (error) => (error ? reject(error) : resolve(undefined));
      });
    }
    if (cmd === "list_branches")
      return [{ name: "main", kind: "local", isCurrent: true }];
    if (cmd === "get_project_github_token_binding")
      return { tokenId: "t1", resolution: "bound" };
    return base(cmd, args);
  });
}

function Window({ initiallyOpen = true }: { initiallyOpen?: boolean }) {
  const transfer = useGitTransferState({ enabled: true });
  const [gitOpen, setGitOpen] = useState(initiallyOpen);
  return (
    <GitTransferProvider value={transfer}>
      <PushControl
        inRepository
        onRequestGithubToken={onRequestGithubToken}
        onOpenGlobalSettings={onOpenGlobalSettings}
      />
      <button type="button" onClick={() => setGitOpen((o) => !o)}>
        toggle-git
      </button>
      {gitOpen && (
        <div data-testid="git-surface">
          <Git onSwitchWorktree={async () => ({ ok: true })} canCheckOutBranches />
        </div>
      )}
      <div data-testid="changes-surface">
        <Changes
          onOpenDiff={onOpenDiff}
          onRequestCommitMessage={onRequestCommitMessage}
          onRequestRollback={onRequestRollback}
          onRequestGithubToken={onRequestGithubToken}
          onOpenGlobalSettings={onOpenGlobalSettings}
        />
      </div>
    </GitTransferProvider>
  );
}

beforeEach(() => {
  bus.reset();
  resetPanelReveals();
  resetAppPreferencesCache();
  resetHandlers();
  invokeMock.mockReset();
  pushInvocations = 0;
  installBackend();
});

afterEach(cleanup);

const top = () => screen.getByTestId("top-push");
const gitPush = () =>
  within(screen.getByTestId("git-surface")).getByRole("button", { name: "Push" });
/** GIT-FR-PZIE: the Logs section holds the panel's Push and the transcript. */
const openBranches = async () =>
  userEvent.click(within(screen.getByTestId("git-surface")).getByText("Logs"));
const transcript = () => screen.findByTestId("git-transfer-output");

async function selectChangesPush() {
  await screen.findByText(/files selected/);
  await chooseAction("Push");
}

describe("one push across the three surfaces (GIT-FR-QMYB)", () => {
  it("GIT-FR-QMYB, GIT-FR-XXLE disables the Git panel's and the Changes panel's Push while the top-chrome push runs", async () => {
    render(<Window />);
    await openBranches();
    await selectChangesPush();
    await waitFor(() => expect(gitPush()).toBeEnabled());
    await waitFor(() => expect(primary()).toBeEnabled());

    await userEvent.click(top());
    await waitFor(() => expect(gitPush()).toBeDisabled());
    expect(primary()).toBeDisabled();
    expect(top()).toHaveAttribute("aria-disabled", "true");
    expect(gitPush()).toHaveAttribute("title", "A push is already running.");

    await act(async () => settlePush());
    await waitFor(() => expect(gitPush()).toBeEnabled());
    expect(primary()).toBeEnabled();
    expect(pushInvocations).toBe(1);
  });

  it("GIT-FR-QMYB disables the top-chrome Push and the Changes panel's Push while the Git panel's push runs", async () => {
    render(<Window />);
    await openBranches();
    await selectChangesPush();
    await waitFor(() => expect(gitPush()).toBeEnabled());

    await userEvent.click(gitPush());
    await waitFor(() => expect(top()).toHaveAttribute("aria-disabled", "true"));
    expect(primary()).toBeDisabled();

    await userEvent.click(top());
    expect(pushInvocations).toBe(1);
    await act(async () => settlePush());
  });

  it("GIT-FR-QMYB, CHG-FR-44 disables the top-chrome Push and the Git panel's Push while the Changes panel's push runs", async () => {
    render(<Window />);
    await openBranches();
    await selectChangesPush();
    await waitFor(() => expect(primary()).toBeEnabled());

    await userEvent.click(primary());
    await waitFor(() => expect(top()).toHaveAttribute("aria-disabled", "true"));
    expect(gitPush()).toBeDisabled();

    await userEvent.click(top());
    await userEvent.click(gitPush());
    expect(pushInvocations).toBe(1);
    await act(async () => settlePush());
  });

  it("GIT-FR-QMYB, CHG-FR-44 makes Commit & Push unavailable in the Changes panel while a push runs", async () => {
    render(<Window />);
    await screen.findByText(/files selected/);
    await chooseAction("Commit & Push");
    const checks = screen
      .getByTestId("changes-surface")
      .querySelectorAll<HTMLInputElement>("input[type=checkbox]");
    await userEvent.click(checks[0]);
    await waitFor(() => expect(primary()).toBeEnabled());

    await userEvent.click(top());
    await waitFor(() => expect(primary()).toBeDisabled());
    expect(primary()).toHaveAttribute("title", "A push is already running.");

    await act(async () => settlePush());
    await waitFor(() => expect(primary()).toBeEnabled());
  });

  it("GIT-FR-QMYB, GIT-FR-PZIE records a push started from the chrome in the output area when the Git panel was closed", async () => {
    render(<Window initiallyOpen={false} />);
    await waitFor(() => expect(top()).toHaveAttribute("aria-disabled", "false"));

    await userEvent.click(top());
    await act(async () => {
      bus.emit(GIT_OUTPUT_LINE, { operation: "push", line: "Pushing main to origin" });
      bus.emit(GIT_OPERATION_FINISHED, { operation: "push", ok: true });
      settlePush();
    });

    await userEvent.click(screen.getByText("toggle-git"));
    await openBranches();
    const area = await transcript();
    expect(area).toHaveTextContent("Pushing main to origin");
    expect(area).toHaveTextContent("push finished");
  });

  it("GIT-FR-IMSH, GIT-FR-PZIE shows a failed chrome push in the output area and nowhere in the Logs panel", async () => {
    // Records an earlier test left pending are not this test's to judge.
    resetLogBufferForTest();
    render(<Window />);
    await openBranches();
    await waitFor(() => expect(top()).toHaveAttribute("aria-disabled", "false"));

    await userEvent.click(top());
    await act(async () => {
      bus.emit(GIT_OPERATION_FINISHED, {
        operation: "push",
        ok: false,
        error: "remote rejected the push",
      });
      settlePush("remote rejected the push");
    });

    expect(await transcript()).toHaveTextContent(
      /push failed: remote rejected the push/,
    );
    await waitFor(() => expect(top()).toHaveAttribute("aria-disabled", "false"));
    // The panel logs its own reads (the Commits section reads its history on
    // mount), and the batch reaches the Logs panel whenever its timer fires.
    // Flush it now, so the check is the same on a slow runner and a fast one,
    // and look for the push: that is what must never be there. A failure is
    // judged by its kind as well as its words, so a record that names the
    // failure in other words is caught too. The panel's own reads at mount
    // are DEBUG records about the local repository.
    await act(async () => {});
    flushLogs();
    const records = invokeMock.mock.calls
      .filter((c) => c[0] === "append_log_records")
      .flatMap(
        (c) =>
          (
            c[1] as {
              records: {
                level: string;
                domains: string[];
                message: string;
                fields: Record<string, unknown>;
              }[];
            }
          ).records,
      );
    const aboutThePush = records.filter(
      (r) =>
        r.level === "WARN" ||
        r.level === "ERROR" ||
        r.domains.includes("remote") ||
        /push|reject|fail/i.test(`${r.message} ${JSON.stringify(r.fields)}`),
    );
    expect(aboutThePush).toEqual([]);
  });

  it("GIT-FR-IMSH writes the typed token failure into the output area in words", async () => {
    render(<Window />);
    await openBranches();
    await waitFor(() => expect(top()).toHaveAttribute("aria-disabled", "false"));
    onRequestGithubToken.mockResolvedValue(false);

    await userEvent.click(top());
    await act(async () => {
      bus.emit(GIT_OPERATION_FINISHED, {
        operation: "push",
        ok: false,
        error: "github_token_selection_required",
      });
      settlePush("github_token_selection_required");
    });

    const area = await transcript();
    expect(area).toHaveTextContent(/Choose which GitHub token/);
    expect(area).not.toHaveTextContent("github_token_selection_required");
    expect(onRequestGithubToken).toHaveBeenCalledOnce();
  });
});
