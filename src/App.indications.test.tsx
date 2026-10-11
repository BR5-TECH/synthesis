/**
 * What the shell raises and withdraws: prompt change proposals and the in-app
 * tab indications.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import App from "./App";
import { resetAppPreferencesCache } from "./state/appPreferences";
import { mintAddress } from "./state/notificationAddress";
import { resetNotifications } from "./state/notifications";
import { resetTabIndications } from "./state/tabIndications";
import { resetPromptProposals } from "./state/promptProposals";
import { resetLayoutPreferencesCache } from "./state/layoutPreferences";
import { resetPanelReveals } from "./state/panelReveal";
import {
  activeWorktreePath,
  createDraftFromPanel,
  defaultInvoke,
  enterIde,
  resetAppFixture,
} from "./test/appFixtures";

const invokeMock = vi.fn();
const confirmMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
// The Library panel (mounted in the IDE) subscribes to the backend
// `"project tree changed"` event, and the File menu reaches the frontend via
// Tauri events. Stub the event bus with a capturing registry so the real Tauri
// runtime is never required and tests can fire menu events deterministically.
const { eventHandlers } = vi.hoisted(() => ({
  eventHandlers: {} as Record<
    string,
    Array<(e: { payload?: unknown }) => void>
  >,
}));
const { emitMock } = vi.hoisted(() => ({ emitMock: vi.fn() }));
vi.mock("@tauri-apps/api/event", () => ({
  // SWN-FR-01: three windows now, so this window both announces and listens.
  emit: (...args: unknown[]) => emitMock(...args),
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
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: async () => null }));
vi.mock("@tauri-apps/api/window", () => {
  class LogicalSize {
    constructor(
      public width: number,
      public height: number,
    ) {}
  }
  const win = {
    setResizable: async () => {},
    setMaximizable: async () => {},
    setSize: async () => {},
    isMaximized: async () => false,
    unmaximize: async () => {},
    maximize: async () => {},
    outerSize: async () => ({ width: 1440, height: 900 }),
    onResized: async () => () => {},
    scaleFactor: async () => 1,
  };
  return {
    LogicalSize,
    getCurrentWindow: () => win,
    availableMonitors: async () => [
      {
        name: "primary",
        size: { width: 2560, height: 1440 },
        position: { x: 0, y: 0 },
        workArea: { position: { x: 0, y: 0 }, size: { width: 2560, height: 1400 } },
        scaleFactor: 1,
      },
    ],
  };
});
function fireBusEvent(name: string, payload?: unknown) {
  for (const h of [...(eventHandlers[name] ?? [])]) h({ payload });
}

beforeEach(() => {
  // Module-level, like the preferences cache: reveal nonces are minted from
  // one counter for the app's life, so a suite that does not reset finds its
  // first request already consumed by the previous one.
  resetPanelReveals();
  resetAppFixture();
  invokeMock.mockReset();
  invokeMock.mockImplementation(
    async (cmd: string, args?: Record<string, unknown>) =>
      defaultInvoke(cmd, args),
  );
  // The app-preferences record is cached at module scope for the application's
  // life (so a theme write can carry the full-screen flag through, GSS-FR-20).
  // Reset it between tests, or the first test's persisted theme is served to
  // every later one.
  resetAppPreferencesCache();
  resetLayoutPreferencesCache();
  confirmMock.mockReset();
  emitMock.mockReset();
  for (const k in eventHandlers) delete eventHandlers[k];
  vi.stubGlobal("confirm", confirmMock);
  vi.stubGlobal("matchMedia", vi.fn().mockReturnValue({ matches: true }));
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

/**
 * PCR-FR-17 / PCR-FR-26 end to end through the shell: the subscriber that
 * raises when a change is proposed to a prompt artifact and **retracts** when
 * one is decided.
 *
 * Mounted at the shell rather than in the Editor tab, so it hears about every
 * artifact in the worktree including files whose tab is closed — which is
 * exactly the case the author most needs telling about (PCR-FR-17).
 */
describe("prompt change proposals reach the shell (PCR-FR-17 / PCR-FR-26)", () => {
  const ARTIFACT = "prompts/review.md";

  const promptProposal = (
    over: Record<string, unknown> = {},
  ): Record<string, unknown> => ({
    id: "pp1",
    artifactId: ARTIFACT,
    path: ARTIFACT,
    agent: { kind: "agent", agentId: "a1", handle: "arch", model: "m" },
    rationale: "The instructions bury the important step.",
    threadId: "t1",
    commentId: "c1",
    state: "pending",
    candidateEdited: false,
    commentOwed: false,
    createdAt: "2026-08-01T10:00:00Z",
    ...over,
  });

  const deliver = (over: Record<string, unknown> = {}, artifactId = ARTIFACT) =>
    fireBusEvent("prompt-change-proposals-changed", {
      artifactId,
      proposal: promptProposal({ artifactId, path: artifactId, ...over }),
    });

  const posts = () =>
    invokeMock.mock.calls
      .filter((c) => c[0] === "post_notification")
      .map((c) => (c[1] as { request: Record<string, unknown> }).request);
  const withdrawals = () =>
    invokeMock.mock.calls.filter((c) => c[0] === "withdraw_notification");

  beforeEach(() => {
    resetTabIndications();
    resetNotifications();
    resetPromptProposals();
    // NTF-FR-08: an unfocused window posts everything, which is the situation
    // a proposal recorded while the author is elsewhere is actually in.
    vi.spyOn(document, "hasFocus").mockReturnValue(false);
  });

  it("raises with the proposal's own key and the project-file address", async () => {
    render(<App />);
    await enterIde();

    deliver();

    await waitFor(() => expect(posts()).toHaveLength(1));
    const posted = posts()[0];
    // PCR-FR-17: derived from the proposal's **own id**, so two proposals
    // against one artifact never share a key.
    expect(posted.key).toBe("prompt-proposal:pp1");
    expect(posted.title).toContain("@arch");
    expect(posted.title).toContain("review.md");
    expect(posted.body).toBe(ARTIFACT);
    // NTF-FR-03: the project-file address, so activating it opens or focuses
    // that artifact's Editor tab (PCR-FR-18).
    expect(posted.payload).toBe(
      mintAddress("~/dev/acme", activeWorktreePath, {
        kind: "file",
        path: ARTIFACT,
      }),
    );
  });

  it("raises with every Editor tab closed", async () => {
    // PCR-FR-17: the subscriber is the shell's, so a proposal against a file
    // nothing has open still calls the author back.
    render(<App />);
    await enterIde();
    expect(screen.queryByText("review.md")).toBeNull();

    deliver();

    await waitFor(() => expect(posts()).toHaveLength(1));
  });

  it("retracts on a decision of either kind, and only that proposal's", async () => {
    // NTF-FR-38, NTF-FR-24 / PCP-FR-17 / PCR-FR-26.
    render(<App />);
    await enterIde();
    deliver();
    deliver({ id: "pp2" }, "prompts/other.md");
    await waitFor(() => expect(posts()).toHaveLength(2));

    deliver({ state: "accepted", decidedAt: "2026-08-01T11:00:00Z" });

    await waitFor(() => expect(withdrawals()).toHaveLength(1));
    // The second proposal's notification is untouched.
    expect(withdrawals()[0][1]).toEqual({ id: "n1" });
    // …and nothing is posted for the decision itself (NTF-FR-24).
    expect(posts()).toHaveLength(2);

    deliver({ id: "pp2", state: "rejected" }, "prompts/other.md");
    await waitFor(() => expect(withdrawals()).toHaveLength(2));
    expect(withdrawals()[1][1]).toEqual({ id: "n2" });
    expect(posts()).toHaveLength(2);
  });

  it("retracts an acceptance whose comment is still owed", async () => {
    // NTF-FR-38, NTF-FR-24, PCP-FR-17 second clause / PCR-FR-26, PCP-FR-14: the proposal is decided whatever its
    // conversation is still owed.
    render(<App />);
    await enterIde();
    deliver();
    await waitFor(() => expect(posts()).toHaveLength(1));

    deliver({ state: "accepted", commentOwed: true });

    await waitFor(() => expect(withdrawals()).toHaveLength(1));
  });

  it("retracts to a no-op where the raise was suppressed", async () => {
    // NTF-FR-38, NTF-FR-29, NTF-FR-10: nothing was posted and no tab was marked, so the retraction
    // errors on nothing and changes nothing.
    render(<App />);
    await enterIde();

    deliver({ state: "rejected" });

    await waitFor(() => expect(screen.getByText("Dashboard")).toBeInTheDocument());
    expect(posts()).toHaveLength(0);
    expect(withdrawals()).toHaveLength(0);
  });
});

/**
 * NTF-FR-26 … NTF-FR-37 end to end through the shell: the half no unit test
 * reaches, which is a real raise finding a real tab in a real strip and the
 * emphasis actually appearing on it.
 *
 * The raiser is the one the application actually has (DCR-FR-18): a change an
 * agent proposed to a draft. That matters — a test that raised through a
 * synthetic hook would prove the store works and nothing about whether anything
 * is wired to it.
 */
describe("in-app tab indications (NTF-FR-26 / NTF-FR-32 / NTF-FR-33)", () => {
  const DRAFT = {
    id: "d1",
    name: "Untitled",
    status: "active",
    folder: "",
    updatedAt: "2026-07-31T10:00:00Z",
  };

  function withOneDraft() {
    invokeMock.mockImplementation(
      async (cmd: string, args?: Record<string, unknown>) => {
        if (cmd === "list_drafts") return { folders: [], drafts: [DRAFT] };
        return defaultInvoke(cmd, args as Record<string, unknown>);
      },
    );
  }

  /** A pending proposal against `d1` — what DCR-FR-18 raises about. */
  const proposalArrived = () =>
    fireBusEvent("draft-change-proposals-changed", {
      draftId: "d1",
      proposal: {
        id: "p1",
        draftId: "d1",
        path: "prompt.md",
        agent: { kind: "agent", handle: "arch", name: "Arch" },
        rationale: "Tightened the second paragraph.",
        threadId: "t1",
        commentId: "c1",
        state: "pending",
        candidateEdited: false,
        createdAt: "2026-08-01T10:00:00Z",
      },
    });

  /**
   * The facility and the indication store are module state, so a test that
   * left an indication or a posted-notification id behind would hand it to the
   * next one. Nothing here should depend on what ran before it.
   */
  beforeEach(() => {
    resetTabIndications();
    resetNotifications();
    // jsdom reports the document as unfocused, and NTF-FR-08 posts everything
    // from an unfocused window — which would make every test here quietly
    // about the *unfocused* case regardless of what it says it is about. The
    // situation each test is in is stated rather than inherited.
    setWindowFocused(true);
  });

  let focusSpy: { mockRestore: () => void } | null = null;
  const setWindowFocused = (focused: boolean) => {
    focusSpy?.mockRestore();
    focusSpy = vi.spyOn(document, "hasFocus").mockReturnValue(focused);
  };
  afterEach(() => {
    focusSpy?.mockRestore();
    focusSpy = null;
  });

  const strip = () => screen.getByTestId("tabstrip");
  const posts = () =>
    invokeMock.mock.calls.filter((c) => c[0] === "post_notification");
  /** The tab elements carrying a needs-attention emphasis, by label. */
  const marked = () =>
    Array.from(strip().querySelectorAll(".tab[data-attention]")).map(
      (el) => el.querySelector(".tab__label")?.textContent ?? null,
    );

  /**
   * Open the draft's New Artifact tab, then leave it for the Dashboard — the
   * situation the whole feature is about: a tab open, holding work, and not the
   * one the author is reading.
   */
  async function openDraftThenLeave() {
    await createDraftFromPanel();
    await screen.findByRole("button", { name: "Draft actions" });
    await userEvent.click(within(strip()).getByText("Dashboard"));
    await waitFor(() =>
      expect(
        strip().querySelector('.tab[data-active="true"] .tab__label')
          ?.textContent,
      ).toBe("Dashboard"),
    );
  }

  // NTF-FR-08, NTF-FR-26, NTF-FR-37: the window is focused on another tab, and the affected tab is
  // marked without the focus moving.
  it("marks a background tab and leaves the active tab where it is", async () => {
    withOneDraft();
    render(<App />);
    await enterIde();
    await openDraftThenLeave();
    expect(marked()).toEqual([]);

    proposalArrived();

    await waitFor(() => expect(marked()).toEqual(["Untitled"]));
    // NTF-FR-37: nothing was activated and nothing moved on account of it.
    expect(
      strip().querySelector('.tab[data-active="true"] .tab__label')?.textContent,
    ).toBe("Dashboard");
    // NTF-FR-36: and it says so to a reader who cannot see the treatment.
    expect(
      strip().querySelector(".tab[data-attention]")?.textContent,
    ).toContain("needs attention");
    // NTF-FR-08, NTF-FR-26, NTF-FR-WMBD: the main window holds focus, so the
    // toast is the channel and nothing went to the OS. Both in-window surfaces,
    // one raise.
    expect(posts()).toHaveLength(0);
    expect(screen.getAllByTestId("notification-toast")).toHaveLength(1);
    // NTF-FR-35: reduced motion is on in this suite, so the emphasis is static
    // from the moment it appears rather than pulsing.
    expect(
      strip()
        .querySelector(".tab[data-attention]")
        ?.getAttribute("data-attention"),
    ).toBe("on");
  });

  /**
   * NTF-FR-14, NTF-FR-32, NTF-FR-37: activating the tab clears it — really clears it, rather than the
   * strip merely declining to draw an emphasis on the active tab.
   *
   * The strip refuses `data-attention` on whichever tab is active
   * (`TabStrip.tsx`), so asserting "unmarked while active" would pass even with
   * the clearing path deleted outright. Leaving the tab again is what tells the
   * two apart: if `notifyArrived` never cleared, the emphasis comes straight
   * back the moment the tab stops being the active one.
   */
  it("clears the emphasis when the author activates the tab", async () => {
    withOneDraft();
    render(<App />);
    await enterIde();
    await openDraftThenLeave();
    proposalArrived();
    await waitFor(() => expect(marked()).toEqual(["Untitled"]));

    await userEvent.click(within(strip()).getByText("Untitled"));
    await waitFor(() => expect(marked()).toEqual([]));

    // Away again — and it stays clear.
    await userEvent.click(within(strip()).getByText("Dashboard"));
    await waitFor(() =>
      expect(
        strip().querySelector('.tab[data-active="true"] .tab__label')
          ?.textContent,
      ).toBe("Dashboard"),
    );
    expect(marked()).toEqual([]);
    // NTF-FR-14: reaching the target removed its toast too.
    expect(screen.queryByTestId("notification-toast")).toBeNull();
  });

  /**
   * NTF-FR-14: the window regaining OS focus reaches only what its
   * already-active tab is a view onto.
   *
   * A marked background tab keeps its emphasis and its notification: focus
   * alone brought the author no nearer to it.
   */
  it("keeps a background tab marked when the window regains focus", async () => {
    withOneDraft();
    render(<App />);
    await enterIde();
    await openDraftThenLeave();
    proposalArrived();
    await waitFor(() => expect(marked()).toEqual(["Untitled"]));

    await act(async () => {
      window.dispatchEvent(new Event("focus"));
    });

    expect(marked()).toEqual(["Untitled"]);
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "withdraw_notification"),
    ).toEqual([]);
  });

  // NTF-FR-29, NTF-FR-11, NTF-FR-13: the switch governs the operating system alone.
  it("marks the tab with notifications switched off, posting nothing", async () => {
    invokeMock.mockImplementation(
      async (cmd: string, args?: Record<string, unknown>) => {
        if (cmd === "list_drafts") return { folders: [], drafts: [DRAFT] };
        if (cmd === "load_app_preferences") {
          const prefs = (await defaultInvoke(
            cmd,
            args as Record<string, unknown>,
          )) as Record<string, unknown>;
          return { ...prefs, notificationsEnabled: false };
        }
        if (cmd === "get_notification_permission") return "denied";
        return defaultInvoke(cmd, args as Record<string, unknown>);
      },
    );
    render(<App />);
    await enterIde();
    await openDraftThenLeave();

    proposalArrived();

    await waitFor(() => expect(marked()).toEqual(["Untitled"]));
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "post_notification"),
    ).toEqual([]);
    // NTF-FR-29: and no prompt appeared because a background tab needed one.
    expect(
      invokeMock.mock.calls.filter(
        (c) => c[0] === "request_notification_permission",
      ),
    ).toEqual([]);
  });

  // NTF-FR-33, NTF-FR-15, NTF-FR-30: an indication does not outlive its tab, and does not come back
  // with it.
  it("drops the emphasis when the tab closes and does not restore it", async () => {
    withOneDraft();
    render(<App />);
    await enterIde();
    await openDraftThenLeave();
    proposalArrived();
    await waitFor(() => expect(marked()).toEqual(["Untitled"]));

    await userEvent.click(screen.getByTestId("close-draft:d1"));
    await waitFor(() =>
      expect(within(strip()).queryByText("Untitled")).toBeNull(),
    );

    // Reopening the same draft opens an unmarked tab. The Drafts panel is
    // already the active surface — the tab that just closed took it there
    // (SNV-FR-64) and closing a tab does not move it back — so no toggle is
    // clicked here; clicking one would *hide* the panel (SNV-FR-45).
    await userEvent.click(
      await screen.findByRole("treeitem", { name: /^Draft Untitled/ }),
    );
    await waitFor(() =>
      expect(within(strip()).queryByText("Untitled")).toBeInTheDocument(),
    );
    expect(marked()).toEqual([]);
  });

  /**
   * NTF-FR-29: a raise naming the tab the author is already on marks nothing —
   * and the mark does not appear the moment they look away either, because
   * nothing was ever recorded.
   *
   * The strip declines to draw an emphasis on the active tab whatever it is
   * told, so "unmarked while active" proves nothing on its own. Switching away
   * *without raising again* is the assertion that does: had the suppressed
   * raise wrongly marked, the emphasis would surface here.
   */
  it("marks nothing while the affected tab is the active one", async () => {
    withOneDraft();
    render(<App />);
    await enterIde();
    await createDraftFromPanel();
    await screen.findByRole("button", { name: "Draft actions" });

    proposalArrived();
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.some((c) => c[0] === "list_drafts"),
      ).toBe(true),
    );
    expect(marked()).toEqual([]);
    // NTF-FR-08: nor was anything posted, the author being on the tab already —
    // the window is focused and permission is granted, so nothing else is
    // stopping it.
    expect(posts()).toEqual([]);

    await userEvent.click(within(strip()).getByText("Dashboard"));
    await waitFor(() =>
      expect(
        strip().querySelector('.tab[data-active="true"] .tab__label')
          ?.textContent,
      ).toBe("Dashboard"),
    );
    expect(marked()).toEqual([]);

    // The plumbing was live all along: the same event, with the author now
    // elsewhere, does mark.
    proposalArrived();
    await waitFor(() => expect(marked()).toEqual(["Untitled"]));
  });

  /**
   * NTF-FR-26, NTF-FR-29: with the window unfocused the notification posts whatever the
   * active tab is — but the active tab is still never the marked one.
   */
  it("posts but marks nothing for the active tab while the window is unfocused", async () => {
    withOneDraft();
    render(<App />);
    await enterIde();
    await createDraftFromPanel();
    await screen.findByRole("button", { name: "Draft actions" });

    setWindowFocused(false);
    proposalArrived();

    await waitFor(() => expect(posts()).toHaveLength(1));
    expect(marked()).toEqual([]);
  });

  /**
   * NTF-FR-26, TAB-FR-19, TAB-FR-26, TAB-FR-15: the emphasis belongs to the target, not to a position in the
   * strip.
   *
   * Closing a tab *ahead* of the marked one re-renders the strip and re-runs
   * reconciliation; the same tab must still be the marked one, and no
   * neighbour may inherit it.
   */
  it("keeps the emphasis on the same tab when a tab ahead of it closes", async () => {
    withOneDraft();
    render(<App />);
    await enterIde();

    // An Editor tab ahead of the draft's tab, so there is something to close
    // that is neither the marked tab nor the active one.
    const panel = within(document.querySelector(".vpanel") as HTMLElement);
    await userEvent.click(await panel.findByText("CHG-changes.md"));
    await waitFor(() =>
      expect(within(strip()).queryByText("CHG-changes.md")).toBeInTheDocument(),
    );
    await openDraftThenLeave();
    proposalArrived();
    await waitFor(() => expect(marked()).toEqual(["Untitled"]));

    // Reached through the tab rather than by a testid: an Editor tab's id is
    // built from the artifact's full path while its label is the basename
    // (`useShellSession`), so the id is not the label and guessing it is how
    // this query breaks the next time the fixture path changes.
    const editorTab = within(strip())
      .getByText("CHG-changes.md")
      .closest(".tab") as HTMLElement;
    await userEvent.click(
      editorTab.querySelector(".tab__close") as HTMLElement,
    );
    await waitFor(() =>
      expect(within(strip()).queryByText("CHG-changes.md")).toBeNull(),
    );

    // Still the draft's tab, and only it.
    expect(marked()).toEqual(["Untitled"]);
  });

  /**
   * NTF-FR-26, TAB-FR-19, TAB-FR-26, TAB-FR-15's last clause: a marked tab is closed like any other, and the
   * Dashboard that replaces it opens unmarked (TAB-FR-15, TAB-FR-26).
   */
  it("closes a marked tab on the ordinary terms and reopens the Dashboard unmarked", async () => {
    withOneDraft();
    render(<App />);
    await enterIde();
    await openDraftThenLeave();
    proposalArrived();
    await waitFor(() => expect(marked()).toEqual(["Untitled"]));

    // Leave the marked tab alone in the strip, then close it.
    await userEvent.click(screen.getByTestId("close-dashboard"));
    await waitFor(() =>
      expect(within(strip()).queryByText("Dashboard")).toBeNull(),
    );
    await userEvent.click(screen.getByTestId("close-draft:d1"));

    await waitFor(() =>
      expect(within(strip()).queryByText("Dashboard")).toBeInTheDocument(),
    );
    expect(within(strip()).queryByText("Untitled")).toBeNull();
    expect(marked()).toEqual([]);
  });
});
