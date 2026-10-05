import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { PushControl } from "./PushControl";
import { GIT_OPERATION_FINISHED, GIT_OUTPUT_LINE } from "../events";
import { flushLogs } from "../logging";
import { createEventBus } from "../test/searchEvents";
import type { UpstreamSyncState } from "../types";

// The top-chrome Push control (GIT-FR-XXLE). It reaches the backend through
// `invoke` and follows the transfer channels on the event bus.
const bus = createEventBus();
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (name: string, handler: (ev: { payload: unknown }) => void) =>
    bus.listen(name, handler),
}));

let sync: UpstreamSyncState;
let pushImpl: () => Promise<undefined>;
const onRequestGithubToken = vi.fn<() => Promise<boolean>>();
const onOpenGlobalSettings = vi.fn();

const AHEAD: UpstreamSyncState = {
  hasRemote: true,
  hasUpstream: true,
  ahead: 2,
  behind: 0,
};

beforeEach(() => {
  bus.reset();
  sync = AHEAD;
  pushImpl = async () => undefined;
  onRequestGithubToken.mockReset();
  onRequestGithubToken.mockResolvedValue(true);
  onOpenGlobalSettings.mockReset();
  invokeMock.mockReset();
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "get_upstream_sync_state") return sync;
    if (cmd === "push_current_branch") return pushImpl();
    return undefined;
  });
});

afterEach(cleanup);

const pushCalls = () =>
  invokeMock.mock.calls.filter((c) => c[0] === "push_current_branch").length;

const renderControl = (inRepository = true) =>
  render(
    <PushControl
      inRepository={inRepository}
      onRequestGithubToken={onRequestGithubToken}
      onOpenGlobalSettings={onOpenGlobalSettings}
    />,
  );

const button = () => screen.getByRole("button", { name: "Push" });

const waitAvailable = () =>
  waitFor(() => expect(button()).toHaveAttribute("aria-disabled", "false"));
const waitUnavailable = () =>
  waitFor(() => expect(button()).toHaveAttribute("aria-disabled", "true"));

/** A push whose invocation settles only when the test says so. */
function deferredPush() {
  let settle!: (outcome: { error?: string }) => void;
  pushImpl = () =>
    new Promise<undefined>((resolve, reject) => {
      settle = ({ error }) => (error ? reject(error) : resolve(undefined));
    });
  return { settle: (o: { error?: string } = {}) => settle(o) };
}

describe("visibility (GIT-FR-XXLE)", () => {
  it("GIT-FR-XXLE is present inside a Git repository", async () => {
    renderControl(true);
    expect(await screen.findByTestId("top-push")).toBeInTheDocument();
  });

  it("GIT-FR-XXLE is absent outside a Git repository and reads no state", async () => {
    renderControl(false);
    expect(screen.queryByTestId("top-push")).toBeNull();
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "get_upstream_sync_state"),
    ).toHaveLength(0);
  });
});

describe("enablement from the upstream state (GIT-FR-XXLE, CHG-FR-37)", () => {
  it("GIT-FR-XXLE, CHG-FR-37 is available when the branch is ahead of its upstream", async () => {
    renderControl();
    await waitAvailable();
  });

  it("GIT-FR-XXLE, CHG-FR-37 is available for a branch with a remote and no upstream yet", async () => {
    sync = { hasRemote: true, hasUpstream: false, ahead: null, behind: null };
    renderControl();
    await waitAvailable();
  });

  it.each([
    [
      "level with its upstream",
      { hasRemote: true, hasUpstream: true, ahead: 0, behind: 0 },
      "Nothing to push — the branch is level with its remote.",
    ],
    [
      "behind its upstream only",
      { hasRemote: true, hasUpstream: true, ahead: 0, behind: 3 },
      "Nothing to push — the branch is level with its remote.",
    ],
    [
      "with no remote",
      { hasRemote: false, hasUpstream: false, ahead: null, behind: null },
      "This repository has no remote configured.",
    ],
  ] as [string, UpstreamSyncState, string][])(
    "GIT-FR-XXLE, GIT-FR-CNQO is unavailable %s and states why",
    async (_name, state, reason) => {
      sync = state;
      renderControl();
      // Wait for the read: the unread state is also unavailable but reads
      // differently, so the reason is what proves the state arrived.
      await waitFor(() =>
        expect(screen.getByTestId("top-push-reason")).toHaveTextContent(reason),
      );
      expect(button()).toHaveAttribute("aria-disabled", "true");
      // One tooltip only: the reason bubble, with no native `title` over it.
      expect(button()).not.toHaveAttribute("title");
      expect(button()).toHaveAccessibleDescription(reason);

      await userEvent.click(button());
      expect(pushCalls()).toBe(0);
    },
  );

  it("GIT-FR-XXLE re-reads the state when a commit lands and when a push finishes", async () => {
    sync = { hasRemote: true, hasUpstream: true, ahead: 0, behind: 0 };
    renderControl();
    await waitUnavailable();

    sync = AHEAD;
    await act(async () => bus.emit("changes-updated", {}));
    await waitAvailable();

    sync = { hasRemote: true, hasUpstream: true, ahead: 0, behind: 0 };
    await act(async () =>
      bus.emit(GIT_OPERATION_FINISHED, { operation: "push", ok: true }),
    );
    await waitUnavailable();
  });
});

describe("keyboard and accessible states (GIT-FR-CNQO)", () => {
  it("GIT-FR-CNQO carries the accessible name Push and is reachable and operable by keyboard", async () => {
    renderControl();
    await waitAvailable();
    const user = userEvent.setup();

    await user.tab();
    // Focus lands on the control itself, not on a wrapper.
    expect(document.activeElement).toBe(button());
    await user.keyboard("{Enter}");
    await waitFor(() => expect(pushCalls()).toBe(1));

    await user.keyboard(" ");
    await waitFor(() => expect(pushCalls()).toBe(2));
  });

  it("GIT-FR-CNQO keeps an unavailable control focusable and makes it start nothing", async () => {
    sync = { hasRemote: true, hasUpstream: true, ahead: 0, behind: 0 };
    renderControl();
    await waitUnavailable();
    const user = userEvent.setup();

    await user.tab();
    expect(document.activeElement).toBe(button());
    await user.keyboard("{Enter}");
    await user.keyboard(" ");
    expect(pushCalls()).toBe(0);
  });

  it("GIT-FR-CNQO keeps the same icon, grayed out, while unavailable", async () => {
    sync = { hasRemote: true, hasUpstream: true, ahead: 0, behind: 0 };
    renderControl();
    await waitUnavailable();
    // The state attribute drives the grayed-out icon in the stylesheet; the
    // accessible state and the words carry it for assistive technology.
    expect(button()).toHaveAttribute("data-state", "unavailable");
    expect(button()).toHaveAttribute("aria-disabled", "true");
    expect(screen.getByTestId("top-push-reason")).toBeInTheDocument();
    const icons = button().querySelectorAll("svg");
    expect(icons).toHaveLength(1);
    expect(icons[0]).toHaveAttribute("data-icon", "push-up");
  });

  it("GIT-FR-CNQO keeps the native tooltip and shows no reason bubble while available", async () => {
    renderControl();
    await waitAvailable();
    expect(button()).toHaveAttribute(
      "title",
      "Push the branch's commits to its remote",
    );
    expect(button()).not.toHaveAttribute("aria-describedby");
    expect(screen.queryByTestId("top-push-reason")).toBeNull();
    expect(document.querySelector(".push-control__reason")).toBeNull();
  });

  it("GIT-FR-CNQO renders the reason bubble where the hover and focus styles find it", async () => {
    sync = { hasRemote: false, hasUpstream: false, ahead: null, behind: null };
    renderControl();
    await waitUnavailable();
    const visible = document.querySelector(".push-control__reason");
    expect(visible).toHaveTextContent("This repository has no remote configured.");
    // The visible copy is a repeat of the description, so a screen reader does
    // not read it twice.
    expect(visible).toHaveAttribute("aria-hidden", "true");
    // The stylesheet shows the bubble through `button + .sr-only + bubble`.
    const description = button().nextElementSibling;
    expect(description).toBe(screen.getByTestId("top-push-reason"));
    expect(description?.nextElementSibling).toBe(visible);
  });

  it("GIT-FR-CNQO is unavailable and states why when the standing is unknown", async () => {
    const reason = "The branch's standing against its remote is unknown.";
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_upstream_sync_state") throw "read failed";
      if (cmd === "push_current_branch") return pushImpl();
      return undefined;
    });
    renderControl();
    await waitFor(() =>
      expect(screen.getByTestId("top-push-reason")).toHaveTextContent(reason),
    );
    expect(button()).toHaveAttribute("aria-disabled", "true");
    expect(button()).not.toHaveAttribute("title");
    expect(button()).toHaveAccessibleDescription(reason);

    await userEvent.click(button());
    expect(pushCalls()).toBe(0);
    // The failed read logs a record in a batch. Write it out now, so it does
    // not arrive during a later test that counts log writes.
    flushLogs();
  });

  it("GIT-FR-CNQO keeps its usual icon while busy", async () => {
    deferredPush();
    renderControl();
    await waitAvailable();
    const usual = button().querySelector("svg")?.getAttribute("data-icon");
    expect(usual).toBe("push-up");
    await userEvent.click(button());
    await waitFor(() => expect(button()).toHaveAttribute("data-state", "busy"));
    const icons = button().querySelectorAll("svg");
    expect(icons).toHaveLength(1);
    expect(icons[0]).toHaveAttribute("data-icon", usual!);
  });

  it("GIT-FR-CNQO keeps a busy control focusable and makes it start nothing more", async () => {
    deferredPush();
    renderControl();
    await waitAvailable();
    await userEvent.click(button());
    await waitFor(() => expect(button()).toHaveAttribute("data-state", "busy"));
    expect(pushCalls()).toBe(1);
    const user = userEvent.setup();

    button().blur();
    await user.tab();
    expect(document.activeElement).toBe(button());
    await user.keyboard("{Enter}");
    await user.keyboard(" ");
    expect(pushCalls()).toBe(1);
  });

  it("GIT-FR-CNQO, GIT-FR-QMYB is busy and states that a push is running while one runs", async () => {
    const push = deferredPush();
    renderControl();
    await waitAvailable();

    await userEvent.click(button());
    await waitFor(() => expect(button()).toHaveAttribute("aria-busy", "true"));
    expect(button()).toHaveAttribute("aria-disabled", "true");
    expect(button()).toHaveAttribute("data-state", "busy");
    expect(button()).toHaveAccessibleDescription("A push is already running.");
    expect(button()).not.toHaveAttribute("title");

    await act(async () => push.settle());
    await waitFor(() => expect(button()).toHaveAttribute("aria-busy", "false"));
  });
});

describe("one push at a time (GIT-FR-QMYB)", () => {
  it("GIT-FR-QMYB invokes the push once for two activations before any output arrives", async () => {
    deferredPush();
    renderControl();
    await waitAvailable();

    await userEvent.dblClick(button());
    expect(pushCalls()).toBe(1);
  });

  it("GIT-FR-QMYB is unavailable while a push started elsewhere streams output", async () => {
    renderControl();
    await waitAvailable();

    await act(async () =>
      bus.emit(GIT_OUTPUT_LINE, { operation: "push", line: "Pushing x to origin" }),
    );
    await waitUnavailable();
    await userEvent.click(button());
    expect(pushCalls()).toBe(0);

    await act(async () =>
      bus.emit(GIT_OPERATION_FINISHED, { operation: "push", ok: true }),
    );
    await waitAvailable();
  });

  it("GIT-FR-XXLE invokes the push command and no other writing command", async () => {
    renderControl();
    await waitAvailable();
    await userEvent.click(button());
    await waitFor(() => expect(pushCalls()).toBe(1));

    const commands = invokeMock.mock.calls.map((c) => c[0]);
    expect(commands).not.toContain("refresh_worktrees_and_branches");
    expect(commands).not.toContain("commit_paths");
    expect(commands).not.toContain("pull_current_branch");
    expect(commands).not.toContain("activate_worktree");
    expect(commands).not.toContain("append_log_records");
  });
});

describe("typed token errors (GIT-FR-IMSH)", () => {
  it("GIT-FR-IMSH opens the token picker on a selection-required error and pushes once more on confirmation", async () => {
    let calls = 0;
    pushImpl = async () => {
      calls += 1;
      if (calls === 1) throw "github_token_selection_required";
      return undefined;
    };
    renderControl();
    await waitAvailable();

    await userEvent.click(button());
    await waitFor(() => expect(onRequestGithubToken).toHaveBeenCalledOnce());
    await waitFor(() => expect(pushCalls()).toBe(2));
    expect(screen.queryByTestId("top-push-note")).toBeNull();
  });

  it("GIT-FR-IMSH abandons the push when the picker is cancelled", async () => {
    pushImpl = async () => {
      throw "github_token_selection_required";
    };
    onRequestGithubToken.mockResolvedValue(false);
    renderControl();
    await waitAvailable();

    await userEvent.click(button());
    expect(await screen.findByTestId("top-push-note")).toHaveTextContent(
      "Push cancelled — no GitHub token was selected.",
    );
    expect(pushCalls()).toBe(1);
  });

  it("GIT-FR-IMSH, GIT-FR-QMYB, GIT-FR-CNQO says a push is already running when one starts while the picker is open", async () => {
    let calls = 0;
    pushImpl = async () => {
      calls += 1;
      if (calls === 1) throw "github_token_selection_required";
      return undefined;
    };
    onRequestGithubToken.mockImplementation(async () => {
      // Another control begins a push while the picker is up.
      bus.emit(GIT_OUTPUT_LINE, { operation: "push", line: "Pushing x to origin" });
      return true;
    });
    renderControl();
    await waitAvailable();

    await userEvent.click(button());
    const note = await screen.findByTestId("top-push-note");
    expect(note).toHaveTextContent("A push is already running.");
    expect(note).toHaveAttribute("role", "status");
    expect(pushCalls()).toBe(1);
    // GIT-FR-CNQO: the note and the reason bubble use one place, so only the
    // note shows. The reason stays the accessible description.
    expect(button()).toHaveAttribute("data-state", "busy");
    expect(document.querySelector(".push-control__reason")).toBeNull();
    expect(button()).toHaveAccessibleDescription("A push is already running.");

    await userEvent.click(screen.getByTestId("top-push-note-dismiss"));
    expect(document.querySelector(".push-control__reason")).toHaveTextContent(
      "A push is already running.",
    );
  });

  it("GIT-FR-IMSH asks for a token at most once per activation", async () => {
    pushImpl = async () => {
      throw "github_token_selection_required";
    };
    renderControl();
    await waitAvailable();

    await userEvent.click(button());
    await waitFor(() => expect(pushCalls()).toBe(2));
    expect(onRequestGithubToken).toHaveBeenCalledOnce();
  });

  it("GIT-FR-IMSH offers the Global settings route on a missing-token error and dismisses its note", async () => {
    pushImpl = async () => {
      throw "github_token_missing";
    };
    renderControl();
    await waitAvailable();

    await userEvent.click(button());
    expect(await screen.findByTestId("top-push-note")).toHaveTextContent(
      "Push needs a GitHub token.",
    );
    expect(onRequestGithubToken).not.toHaveBeenCalled();

    await userEvent.click(screen.getByTestId("top-push-note-settings"));
    expect(onOpenGlobalSettings).toHaveBeenCalledOnce();
    expect(screen.queryByTestId("top-push-note")).toBeNull();
  });

  it("GIT-FR-IMSH renders a note that can be dismissed", async () => {
    pushImpl = async () => {
      throw "github_token_missing";
    };
    renderControl();
    await waitAvailable();
    await userEvent.click(button());
    await screen.findByTestId("top-push-note");

    await userEvent.click(screen.getByTestId("top-push-note-dismiss"));
    expect(screen.queryByTestId("top-push-note")).toBeNull();
    expect(onOpenGlobalSettings).not.toHaveBeenCalled();
  });

  it("GIT-FR-IMSH writes no push failure to the diagnostic Logs panel", async () => {
    pushImpl = async () => {
      throw "remote rejected the push";
    };
    renderControl();
    await waitAvailable();
    await userEvent.click(button());
    await waitFor(() => expect(pushCalls()).toBe(1));
    await waitAvailable();

    await act(async () => {
      await new Promise((r) => setTimeout(r, 300));
    });
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "append_log_records"),
    ).toHaveLength(0);
    // The failure is the transcript's to carry, so the control adds no note.
    expect(screen.queryByTestId("top-push-note")).toBeNull();
  });
});
