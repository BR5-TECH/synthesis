import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import App from "./App";
import { MENU_NEW_FILE, MENU_NEW_FOLDER } from "./events";
import { resetAppPreferencesCache } from "./state/appPreferences";
import { resetLayoutPreferencesCache } from "./state/layoutPreferences";
import { pickSelector, selectorValue, selectorValues } from "./test/selectors";
import type { CommitOutcome } from "./types";

/**
 * App-level wiring for the GitHub token picker
 * (`specifications/ui/GHA-github-authentication.md`).
 *
 * The picker is mounted by `App` and asked for by two different surfaces, so
 * the seams these tests cover — which token is preselected, which overlay wins,
 * and whether a waiting caller is ever left unsettled — exist only here. A
 * component-level test of `GithubTokenPicker` cannot see any of them: it is
 * handed its props directly, so it passes whether or not the app supplies them.
 */

const invokeMock = vi.fn();
const confirmMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const { eventHandlers } = vi.hoisted(() => ({
  eventHandlers: {} as Record<
    string,
    Array<(e: { payload?: unknown }) => void>
  >,
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    async (name: string, handler: (e: { payload?: unknown }) => void) => {
      (eventHandlers[name] ??= []).push(handler);
      return () => {
        eventHandlers[name] = (eventHandlers[name] ?? []).filter(
          (h) => h !== handler,
        );
      };
    },
  ),
}));
function fireBusEvent(name: string, payload?: unknown) {
  for (const h of [...(eventHandlers[name] ?? [])]) h({ payload });
}
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: async () => null }));
vi.mock("@tauri-apps/api/window", () => {
  class LogicalSize {
    constructor(
      public width: number,
      public height: number,
    ) {}
  }
  return {
    LogicalSize,
    getCurrentWindow: () => ({
      setResizable: async () => {},
      setMaximizable: async () => {},
      setSize: async () => {},
      isMaximized: async () => false,
      unmaximize: async () => {},
      isFullscreen: async () => false,
      setFullscreen: async () => {},
      onResized: async () => () => {},
      outerSize: async () => new LogicalSize(1600, 1000),
      scaleFactor: async () => 1,
    }),
  };
});

const WORK = {
  id: "t1",
  label: "work laptop",
  accountLogin: "raver119",
  scopes: ["repo"],
  maskedHint: "a3f9",
  addedAt: "2026-03-12T10:00:00Z",
  lastVerifiedAt: null,
  state: "valid" as const,
};
const PERSONAL = { ...WORK, id: "t2", label: "personal", maskedHint: "1b04" };

/** What `get_project_github_token_binding` answers; a test may change it. */
let binding: { tokenId: string | null; resolution: string };
/** Set to a rejection to make `commit_paths` fail (GTC-FR-20). */
let commitResult: string | undefined;

function defaultInvoke(cmd: string) {
  switch (cmd) {
    case "list_recent_projects":
      return [
        { name: "acme", path: "~/dev/acme", lastOpenedAt: "2026-05-15T10:00:00Z" },
      ];
    case "open_project_at_path":
      return { name: "acme", path: "~/dev/acme", activeWorktreePath: "~/dev/acme" };
    case "get_active_worktree":
      return {
        path: "~/dev/acme",
        branch: "main",
        isPrimary: true,
        isActive: true,
        headShortHash: "aaa1111",
        missing: false,
      };
    case "list_github_tokens":
      return [WORK, PERSONAL];
    case "get_project_github_token_binding":
      return binding;
    case "set_project_github_token_binding":
      return binding;
    case "load_app_preferences":
      return { theme: "system", mainWindowFullscreen: false };
    case "list_branches":
      return [{ name: "main", kind: "local", isCurrent: true }];
    // The Changes panel's own reads (CHG-FR-03 / CHG-FR-38), so its footer's
    // Push is offered — the branch is ahead of its upstream.
    case "load_changes_panel_state":
      return { mode: "uncommitted" };
    case "get_default_branch":
      return "main";
    case "list_uncommitted_changes":
      return {
        comparison: { kind: "uncommitted" },
        entries: [
          {
            id: "src/App.tsx",
            path: "src/App.tsx",
            name: "App.tsx",
            changeStatus: "modified",
            addedLines: 4,
            removedLines: 1,
            isBinary: false,
          },
          // A second changed file, so a commit of the first can be shown to
          // leave the Diff tab on the other one standing (TAB-FR-22).
          {
            id: "src/Other.tsx",
            path: "src/Other.tsx",
            name: "Other.tsx",
            changeStatus: "modified",
            addedLines: 1,
            removedLines: 0,
            isBinary: false,
          },
        ],
      };
    // DFV-FR-25 / DFV-FR-42: a Diff tab reads the ORIGINAL here and the target
    // through the artifact's own editing session.
    case "get_file_revisions":
      return { old: "was\n", new: "is now\n", isBinary: false };
    case "load_artifact_contents_by_id":
      return { body: "is now\n", checksum: "ck1" };
    case "get_upstream_sync_state":
      return { hasRemote: true, hasUpstream: true, ahead: 2, behind: 0 };
    case "list_drafts":
      return { folders: [], drafts: [] };
    case "create_draft":
    case "open_draft":
    case "rename_draft":
    case "set_draft_status":
      return {
        id: "d1",
        name: "Untitled",
        promptPath: "Untitled.md",
        status: "active",
        createdAt: "2026-07-31T10:00:00Z",
        updatedAt: "2026-07-31T10:00:00Z",
      };
    case "list_draft_files":
      return [];
    case "load_draft_file_contents":
      return { body: "", checksum: "dck" };
    case "save_draft_file_contents":
      return { checksum: "dck2" };
    default:
      return undefined;
  }
}

beforeEach(() => {
  binding = { tokenId: null, resolution: "selection_required" };
  commitResult = undefined;
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => defaultInvoke(cmd));
  resetAppPreferencesCache();
  resetLayoutPreferencesCache();
  confirmMock.mockReset();
  for (const k in eventHandlers) delete eventHandlers[k];
  vi.stubGlobal("confirm", confirmMock);
  vi.stubGlobal("matchMedia", vi.fn().mockReturnValue({ matches: true }));
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

async function enterIde() {
  await userEvent.click(await screen.findByText("acme"));
  await screen.findByRole("button", { name: "Global settings" });
}

/**
 * The Git panel's own **Push**. The top chrome carries a control of the same
 * accessible name (GIT-FR-CNQO), so the panel's is addressed by its zone.
 */
async function gitPanelPush() {
  const panel = await screen.findByTestId("bottom-panel-title");
  return within(panel.closest(".bottom-panel") as HTMLElement).findByRole(
    "button",
    { name: "Push" },
  );
}

/** Show the bottom panel, route it to Git, open its Logs section, and push. */
async function pushFromGitPanel() {
  // SNV-FR-46: the bottom panel starts hidden; its Git toggle opens it straight
  // onto the Git surface.
  await userEvent.click(screen.getByRole("button", { name: "Git" }));
  // GIT-FR-PZIE: the transfer controls live in the Logs section, alongside the
  // push/pull output area every transfer streams into.
  const panel = await screen.findByTestId("bottom-panel-title");
  await userEvent.click(
    await within(panel.closest(".bottom-panel") as HTMLElement).findByRole("tab", {
      name: "Logs",
    }),
  );
  await userEvent.click(await gitPanelPush());
}

describe("GitHub token picker at the App seam", () => {
  // GHA-FR-20 — the picker opened from the Project section — is no longer at
  // this seam: GHA-FR-20 requires it presented **within the Project settings
  // window**, that window being modal to this one (SWN-FR-02), so this window
  // could not show it even if it wanted to. It is covered in
  // `SettingsWindowApp.test.tsx`, against the window that actually holds it.

  it("GHA-FR-21 is mutually exclusive with the other overlays, in both directions", async () => {
    // GHA-FR-21. The outward direction was there from the start; the inward one
    // is what makes the invariant hold — every other opener predates the picker
    // and knows nothing about it unless told.
    render(<App />);
    await enterIde();
    await pushFromGitPanel();
    await screen.findByTestId("github-token-picker");

    // Another overlay opening must close the picker rather than stack on it.
    fireBusEvent(MENU_NEW_FILE);

    await waitFor(() =>
      expect(screen.queryByTestId("github-token-picker")).not.toBeInTheDocument(),
    );
    expect(screen.getByRole("dialog")).toHaveTextContent("New File");
  });

  it("GHA-FR-21: the New Folder modal closes the picker too, and the picker closes it", async () => {
    // Every overlay opener has to know about every other one, in both directions
    // (GHA-FR-21 / NFW-FR-01), and a new opener is exactly what silently breaks
    // the invariant — nothing else in the app would notice.
    render(<App />);
    await enterIde();
    await pushFromGitPanel();
    await screen.findByTestId("github-token-picker");

    fireBusEvent(MENU_NEW_FOLDER);

    await waitFor(() =>
      expect(screen.queryByTestId("github-token-picker")).not.toBeInTheDocument(),
    );
    expect(screen.getByRole("dialog")).toHaveTextContent("New Folder");

    // And the other direction: the picker, opening after the modal, closes it.
    await userEvent.click(await gitPanelPush());
    await screen.findByTestId("github-token-picker");
    expect(screen.queryByText("New Folder")).not.toBeInTheDocument();
  });

  it("never leaves a waiting operation unsettled when the picker is dismissed by another overlay", async () => {
    // The Git panel awaits the picker's answer. A dismissal that dropped the
    // pending `settle` would leave that promise unresolved forever: no note, no
    // push, and no way for the author to tell anything had happened.
    render(<App />);
    await enterIde();
    await pushFromGitPanel();
    await screen.findByTestId("github-token-picker");

    fireBusEvent(MENU_NEW_FILE);

    await waitFor(() =>
      expect(screen.getByTestId("git-auth-note")).toHaveTextContent(
        /Push cancelled — no GitHub token was selected/,
      ),
    );
  });

  it("binds the project and lets the blocked operation proceed once a token is chosen", async () => {
    // GHA-FR-16 end to end: the Git panel's push is what opened the picker, and
    // it resumes on confirm.
    render(<App />);
    await enterIde();
    await pushFromGitPanel();

    const picker = await screen.findByTestId("github-token-picker");
    await userEvent.click(within(picker).getByRole("radio", { name: "personal" }));
    await userEvent.click(within(picker).getByRole("button", { name: "Use" }));

    await waitFor(() =>
      expect(screen.queryByTestId("github-token-picker")).not.toBeInTheDocument(),
    );
    expect(invokeMock).toHaveBeenCalledWith("set_project_github_token_binding", {
      tokenId: "t2",
    });
    // GIT-FR-04: the push that was waiting on a token actually ran.
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some((c) => c[0] === "push_current_branch"),
      ).toBe(true),
    );
  });
});

// ---------------------------------------------------------------------------
// The Changes panel reaches the same picker (GHA-FR-16, GHA-FR-17, CHG-FR-45 / OVW-FR-06, OVW-FR-07, GIT-FR-10)
// ---------------------------------------------------------------------------

describe("the Changes panel's push at the App seam (GHA-FR-16, GHA-FR-17, CHG-FR-45)", () => {
  /** Route the vertical panel to Changes and push from its footer control. */
  async function pushFromChangesPanel() {
    await userEvent.click(screen.getByRole("button", { name: "Changes" }));
    await screen.findByText(/files selected/);
    // CHG-FR-33: pick Push from the footer's attached dropdown, then run it.
    await userEvent.click(screen.getByRole("button", { name: "Choose action" }));
    const menu = screen.getByRole("listbox", { name: "Commit action" });
    await userEvent.click(within(menu).getByRole("option", { name: "Push" }));
    const primary = document
      .querySelector(".changes-actions__split")!
      .querySelector("button")!;
    await waitFor(() => expect(primary).toBeEnabled());
    await userEvent.click(primary);
  }

  it("GHA-FR-16, GHA-FR-17, CHG-FR-45 / OVW-FR-06, OVW-FR-07, GIT-FR-10 opens the same modal the Git panel opens", async () => {
    // CHG-FR-45 / GHA-FR-16: `selection_required` prompts, wherever the push
    // was started from — it is one picker, owned by the shell.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "push_current_branch") throw "github_token_selection_required";
      return defaultInvoke(cmd);
    });
    render(<App />);
    await enterIde();
    await pushFromChangesPanel();

    const picker = await screen.findByTestId("github-token-picker");
    expect(picker).toBeInTheDocument();
    // GHA-FR-15: exactly one picker exists, however many surfaces can ask.
    expect(screen.getAllByTestId("github-token-picker")).toHaveLength(1);
  });

  it("GHA-FR-17 abandons the push when the picker is cancelled", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "push_current_branch") throw "github_token_selection_required";
      return defaultInvoke(cmd);
    });
    render(<App />);
    await enterIde();
    await pushFromChangesPanel();

    const picker = await screen.findByTestId("github-token-picker");
    await userEvent.click(within(picker).getByRole("button", { name: /Cancel/ }));

    expect(
      await screen.findByText(/no GitHub token was selected/),
    ).toBeInTheDocument();
    // One attempt only: the abandoned push was not retried.
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "push_current_branch"),
    ).toHaveLength(1);
  });
});

// ---------------------------------------------------------------------------
// The commit message window at the App seam
// (CMW-FR-01 single-overlay, CHG-FR-42 commit-then-push ordering)
// ---------------------------------------------------------------------------

describe("the commit message window at the App seam", () => {
  /** Route the vertical panel to Changes and tick the one changed file. */
  async function openChangesAndTick() {
    await userEvent.click(screen.getByRole("button", { name: "Changes" }));
    await screen.findByText(/files? selected/);
    await pickSelector("Filter by type", "files");
    await userEvent.click(await screen.findByLabelText("Include src/App.tsx"));
  }

  const changesPrimary = () =>
    document
      .querySelector(".changes-actions__split")!
      .querySelector("button")!;

  async function chooseFooterAction(label: string) {
    await userEvent.click(screen.getByRole("button", { name: "Choose action" }));
    const menu = screen.getByRole("listbox", { name: "Commit action" });
    await userEvent.click(within(menu).getByRole("option", { name: label }));
  }

  it("CHG-FR-42 commits, then pushes, in that order", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "commit_paths")
        return { commitId: "abc1234", committedPaths: [] };
      if (cmd === "push_current_branch") return undefined;
      return defaultInvoke(cmd);
    });
    render(<App />);
    await enterIde();
    await openChangesAndTick();
    await chooseFooterAction("Commit & Push");

    await userEvent.click(changesPrimary());

    // CMW-FR-03 / CMW-FR-06: the real window, carrying the real commit set.
    const dialog = await screen.findByRole("dialog");
    expect(within(dialog).getByText("Commit 1 file")).toBeInTheDocument();
    expect(within(dialog).getByText("src/App.tsx")).toBeInTheDocument();

    await userEvent.type(
      within(dialog).getByLabelText("Commit message"),
      "hand-off",
    );
    await userEvent.click(within(dialog).getByRole("button", { name: /^Commit/ }));

    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some((c) => c[0] === "push_current_branch"),
      ).toBe(true),
    );
    const order = invokeMock.mock.calls
      .map((c) => c[0] as string)
      .filter((c) => c === "commit_paths" || c === "push_current_branch");
    expect(order).toEqual(["commit_paths", "push_current_branch"]);
    // The window closed on success (CMW-FR-07).
    await waitFor(() => expect(screen.queryByRole("dialog")).toBeNull());
  });

  it("CHG-FR-41, TAB-FR-22 / TAB-FR-21, CMW-FR-07 closes exactly the Diff tabs the commit recorded", async () => {
    // The seam CMW-FR-07 → CHG-FR-41 → TAB-FR-22 runs through: the window
    // reports the paths the commit RECORDED, the panel hands them on, and the
    // strip closes those Diff tabs and no others.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "commit_paths")
        // A rename: the commit records it at BOTH locations, which is why the
        // strip acts on what landed rather than on what was submitted.
        return {
          commitId: "abc1234",
          committedPaths: ["src/App.tsx", "src/Was.tsx"],
        };
      return defaultInvoke(cmd);
    });
    render(<App />);
    await enterIde();
    await openChangesAndTick();

    // Two Diff tabs: one on the file about to be committed, one on another.
    // The panel stays mounted behind the viewport, so both rows are reachable
    // without routing back to it.
    const panel = () => within(document.querySelector(".vpanel") as HTMLElement);
    await userEvent.click(await panel().findByText("App.tsx"));
    await screen.findByText("src/App.tsx", { selector: ".diff-view__path" });
    await userEvent.click(await panel().findByText("Other.tsx"));
    await screen.findByText("src/Other.tsx", { selector: ".diff-view__path" });

    const tabLabels = () =>
      Array.from(document.querySelectorAll(".tabstrip .tab__label")).map(
        (n) => n.textContent,
      );
    expect(tabLabels()).toContain("Diff: App.tsx");
    expect(tabLabels()).toContain("Diff: Other.tsx");

    await userEvent.click(changesPrimary());
    const dialog = await screen.findByRole("dialog");
    await userEvent.type(
      within(dialog).getByLabelText("Commit message"),
      "hand-off",
    );
    await userEvent.click(within(dialog).getByRole("button", { name: /^Commit/ }));

    await waitFor(() => expect(tabLabels()).not.toContain("Diff: App.tsx"));
    // The Diff tab the commit did not include stays open.
    expect(tabLabels()).toContain("Diff: Other.tsx");
  });

  it("CHG-FR-42 pushes nothing when the backend rejects the commit", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "commit_paths") throw "nothing_to_commit";
      return defaultInvoke(cmd);
    });
    render(<App />);
    await enterIde();
    await openChangesAndTick();
    await chooseFooterAction("Commit & Push");
    await userEvent.click(changesPrimary());

    const dialog = await screen.findByRole("dialog");
    await userEvent.type(
      within(dialog).getByLabelText("Commit message"),
      "hand-off",
    );
    await userEvent.click(within(dialog).getByRole("button", { name: /^Commit/ }));

    // CMW-FR-08: the window stays open with its message and the typed cause.
    expect(await screen.findByTestId("commit-error")).toHaveTextContent(
      /Nothing to commit/,
    );
    expect(
      within(screen.getByRole("dialog")).getByLabelText<HTMLTextAreaElement>(
        "Commit message",
      ).value,
    ).toBe("hand-off");
    // CHG-FR-42: no push followed a commit that never happened.
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "push_current_branch"),
    ).toHaveLength(0);
  });

  it("CMW-FR-01 closes any other overlay when it opens, and is closed by one opening after it", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "commit_paths")
        return { commitId: "abc1234", committedPaths: [] };
      return defaultInvoke(cmd);
    });
    render(<App />);
    await enterIde();

    await openChangesAndTick();

    // Another overlay first: the New File modal. New Artifact is deliberately
    // not used here — it is a tab now, not an overlay (SNV-FR-56).
    fireBusEvent(MENU_NEW_FILE);
    expect(await screen.findByText("New File")).toBeInTheDocument();

    await userEvent.click(changesPrimary());

    // Opening the commit window closed it; exactly one overlay is mounted.
    expect(await screen.findByText("Commit 1 file")).toBeInTheDocument();
    expect(screen.queryByText("New File")).toBeNull();
    expect(screen.getAllByRole("dialog")).toHaveLength(1);

    // And the other direction — the one that deadlocks the footer if a call
    // site forgets to dismiss: the New File modal opens over it.
    fireBusEvent(MENU_NEW_FILE);
    await waitFor(() =>
      expect(screen.queryByText("Commit 1 file")).toBeNull(),
    );
    // The Changes panel's pending promise settled, so its footer is live again.
    await userEvent.keyboard("{Escape}");
    await waitFor(() => expect(changesPrimary()).toBeEnabled());
  });

  it("CMW-FR-01: the New Folder modal closes the commit window and settles its promise", async () => {
    // The deadlock case, for the newer of the two creation modals: the Changes
    // panel awaits the commit window's answer, so an opener that tears the window
    // down without settling leaves the footer disabled forever (CMW-FR-01 /
    // NFW-FR-01).
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "commit_paths")
        return { commitId: "abc1234", committedPaths: [] };
      return defaultInvoke(cmd);
    });
    render(<App />);
    await enterIde();

    await openChangesAndTick();
    await userEvent.click(changesPrimary());
    expect(await screen.findByText("Commit 1 file")).toBeInTheDocument();

    fireBusEvent(MENU_NEW_FOLDER);

    await waitFor(() => expect(screen.queryByText("Commit 1 file")).toBeNull());
    expect(screen.getAllByRole("dialog")).toHaveLength(1);
    expect(screen.getByRole("dialog")).toHaveTextContent("New Folder");
    // The panel's pending promise settled, so its footer is live again — the
    // assertion that actually rules out the deadlock.
    await userEvent.keyboard("{Escape}");
    await waitFor(() => expect(changesPrimary()).toBeEnabled());
  });

  it("CMW-FR-09 refuses to be torn down while its commit is in flight", async () => {
    // A native menu accelerator reaches the overlay openers straight past the
    // modal's scrim. Tearing the window down here would answer the panel with
    // "nothing was committed" while the commit was landing, and silently skip
    // the push half of Commit & Push.
    let release!: (outcome: CommitOutcome) => void;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "commit_paths")
        return new Promise<CommitOutcome>((resolve) => (release = resolve));
      if (cmd === "push_current_branch") return undefined;
      return defaultInvoke(cmd);
    });
    render(<App />);
    await enterIde();
    await openChangesAndTick();
    await chooseFooterAction("Commit & Push");
    await userEvent.click(changesPrimary());

    const dialog = await screen.findByRole("dialog");
    await userEvent.type(
      within(dialog).getByLabelText("Commit message"),
      "hand-off",
    );
    await userEvent.click(within(dialog).getByRole("button", { name: /^Commit/ }));
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some((c) => c[0] === "commit_paths"),
      ).toBe(true),
    );

    // The File menu fires while the commit is still running.
    fireBusEvent(MENU_NEW_FILE);
    expect(screen.getByText("Commit 1 file")).toBeInTheDocument();

    // The commit lands, the window closes, and the push it was gating runs.
    await act(async () => {
      release({ commitId: "abc1234", committedPaths: [] });
    });
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some((c) => c[0] === "push_current_branch"),
      ).toBe(true),
    );
  });
});
