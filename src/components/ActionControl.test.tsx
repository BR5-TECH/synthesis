import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Editor } from "./Editor";
import { FlowCanvas } from "./Flow";
import { DiffView } from "./DiffView";
import { EditSessionStore } from "../state/editSessions";
import { FlowSessionStore } from "../state/flowSessions";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import {
  resetDiscussionFocus,
  surfaceOwner,
} from "../state/discussionFocus";
import type { Discussion, DiffTarget, ProjectAgent } from "../types";
import {
  artifactDiscussionOrigin,
  expectBackendOrigin,
} from "../test/origins";

/**
 * `ACT-action-control.md`, driven through the real host tabs.
 *
 * The point of the control is that it is ONE control wherever it appears, so
 * these tests exercise it through the Editor, the Flow canvas, and the Diff tab
 * rather than mounting it in isolation — a control that only works when rendered
 * by hand is not the thing the spec describes. What each host contributes (the
 * draft's Graduate and Archive) is covered where that host is covered.
 */
const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const listeners = new Map<string, Set<(e: { payload: unknown }) => void>>();
function emitEvent(name: string, payload: unknown) {
  [...(listeners.get(name) ?? [])].forEach((h) => h({ payload }));
}
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    async (event: string, cb: (e: { payload: unknown }) => void) => {
      const set = listeners.get(event) ?? new Set();
      set.add(cb);
      listeners.set(event, set);
      return () => set.delete(cb);
    },
  ),
}));

const BODY = "# Editor\n\nSteps to run before the first session.\n";
const FLOW_BODY = JSON.stringify({
  version: 1,
  name: "Onboarding review",
  nodes: [],
  edges: [],
});

function human(login: string) {
  return { kind: "human", login } as const;
}

function discussion(over: Partial<Discussion> = {}): Discussion {
  return {
    id: "d1",
    target: { kind: "artifact", artifactId: "a.md" },
    fragmentTarget: null,
    comments: [
      {
        id: "c1",
        author: human("raver119"),
        body: "@arch is this ready?",
        quotes: [],
        attachments: [],
        createdAt: "2026-01-01T00:00:00Z",
      },
    ],
    locked: false,
    resolved: false,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
    ...over,
  };
}

interface Backend {
  discussions: Discussion[];
  /**
   * The enrolment as `list_project_agents` returns it — availability included,
   * because `@all` stands for the agents that can *answer* (AGT-FR-36) and a
   * fixture omitting it would have the handle silently address nobody.
   */
  agents: {
    agent: { id: string; nickname: string };
    availability: ProjectAgent["availability"];
  }[];
  identityError?: string;
  openError?: string;
  calls: { cmd: string; args: unknown }[];
}

/** Returns the handler too, so a test can wrap it for one command. */
function wire(b: Backend): (cmd: string, args: unknown) => Promise<unknown> {
  const handler = async (cmd: string, args: unknown) => {
    b.calls.push({ cmd, args });
    switch (cmd) {
      case "load_artifact_contents_by_id":
        return { body: BODY, checksum: "ck1" };
      case "save_artifact_contents":
        return { checksum: "ck2" };
      case "list_discussions":
        return b.discussions;
      case "list_project_agents":
        return b.agents;
      case "list_ai_api_catalogs":
        return [];
      case "get_active_ai_api_catalog":
        return { resolution: "none_configured", catalog: null };
      case "list_agent_turns":
        return [];
      case "resolve_comment_author_identity":
        if (b.identityError) throw b.identityError;
        return human("raver119");
      case "open_discussion": {
        if (b.openError) throw b.openError;
        const opened = discussion({ id: "d-new" });
        b.discussions = [...b.discussions, opened];
        return opened;
      }
      case "dispatch_agent_turn":
        return {
          id: "turn-1",
          agentId: "a1",
          nickname: "arch",
          origin: expectBackendOrigin((args as { origin: unknown }).origin),
          triggerCommentId: "c1",
          state: "running",
          failure: null,
          startedAt: "2026-01-01T00:00:00Z",
          endedAt: null,
        };
      case "load_project_tree":
        return { id: "", name: "p", path: "", nodeKind: "folder", children: [] };
      case "get_diff":
        return { hunks: [], binary: false, deleted: false };
      default:
        return undefined;
    }
  };
  invokeMock.mockImplementation(handler);
  return handler;
}

function backend(over: Partial<Backend> = {}): Backend {
  return {
    discussions: [],
    agents: [{ agent: { id: "a1", nickname: "arch" }, availability: "ready" }],
    calls: [],
    ...over,
  };
}

const calls = (b: Backend, cmd: string) =>
  b.calls.filter((c) => c.cmd === cmd).map((c) => c.args);

async function renderEditor(
  b: Backend,
  opts: { artifactType?: "spec" | "prompt" } = {},
) {
  wire(b);
  render(
    <Editor
      artifactId="a.md"
      artifactName="a.md"
      artifactType={opts.artifactType}
      sessions={new EditSessionStore()}
    />,
  );
  await screen.findByRole("button", { name: "Actions" });
}

const toggle = () => screen.getByRole("button", { name: "Actions" });
async function openActions() {
  await userEvent.click(toggle());
  return screen.getByRole("menu");
}
const items = (menu: HTMLElement) =>
  within(menu)
    .getAllByRole("menuitem")
    .map((el) => el.textContent?.trim());

beforeEach(() => {
  invokeMock.mockReset();
  listeners.clear();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
});
afterEach(cleanup);

describe("the action set is decided by the item (ACT-FR-04, ACT-FR-05)", () => {
  it("ACT-FR-01, ACT-FR-03, ACT-FR-04, ACT-FR-05, ACT-FR-09: a file with no special type affords Discuss alone", async () => {
    const b = backend();
    await renderEditor(b, { artifactType: "prompt" });

    expect(toggle()).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();

    const menu = await openActions();
    expect(items(menu)).toEqual(["Discuss"]);
    // Expanded, the control itself becomes the dismissal.
    await userEvent.click(
      screen.getByRole("button", { name: "Close actions" }),
    );
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });

  it("ACT-FR-04, ACT-FR-05: a specification affords Discuss alone", async () => {
    const b = backend();
    await renderEditor(b, { artifactType: "spec" });
    expect(items(await openActions())).toEqual(["Discuss"]);
  });

  it("ACT-FR-04, ACT-FR-05: a Flow affords Discuss alone", async () => {
    const b = backend();
    wire(b);
    const flows = new FlowSessionStore();
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_artifact_contents_by_id")
        return { body: FLOW_BODY, checksum: "ck1" };
      if (cmd === "list_discussions") return b.discussions;
      if (cmd === "list_project_agents") return b.agents;
      if (cmd === "resolve_comment_author_identity") return human("raver119");
      return undefined;
    });
    await act(async () => {
      flows.openTab("review.flow");
      await flows.get("review.flow")?.pendingLoad;
    });
    render(<FlowCanvas flowId="review.flow" flows={flows} />);
    await screen.findByRole("button", { name: "Actions" });
    expect(items(await openActions())).toEqual(["Discuss"]);
  });

  it("ACT-FR-04, ACT-FR-05: a read-only Diff tab on a spec affords Discuss alone", async () => {
    const b = backend();
    wire(b);
    const target: DiffTarget = {
      path: "specs/spec.md",
      name: "spec.md",
      scope: { kind: "path", path: "specs/spec.md" },
      comparisonLabel: "Uncommitted",
      // The file IS a spec — Implement is absent because the tab is one reading
      // of one comparison of a file rather than the file's own tab (ACT-FR-04),
      // not because the item is not a specification.
      artifactType: "spec",
    };
    // DFV-FR-42: the tab's target is the artifact's editing session, so it
    // needs the store that holds it.
    render(<DiffView target={target} sessions={new EditSessionStore()} />);
    await screen.findByRole("button", { name: "Actions" });
    const menu = await openActions();
    expect(items(menu)).toEqual(["Discuss"]);
    expect(
      within(menu).queryByRole("menuitem", { name: /Implement/ }),
    ).not.toBeInTheDocument();
  });
});

describe("the control is the tab's own chrome (ACT-FR-01, ACT-FR-03)", () => {
  it("ACT-FR-01, ACT-FR-08: it is present in both editing modes, and there is exactly one", async () => {
    const b = backend();
    await renderEditor(b, { artifactType: "spec" });
    expect(screen.getAllByRole("button", { name: "Actions" })).toHaveLength(1);

    await userEvent.click(
      screen.getByRole("button", { name: "Edit as Markdown source" }),
    );
    // ACT-FR-01: still there, and still exactly one — starting a conversation
    // about the file is not a thing the shape of the text on screen bears on.
    expect(screen.getAllByRole("button", { name: "Actions" })).toHaveLength(1);
    expect(items(await openActions())).toEqual(["Discuss"]);
  });

  it("ACT-FR-01, ACT-FR-03, ACT-FR-04, ACT-FR-05, ACT-FR-09: it collapses on Escape and on a press elsewhere in the tab", async () => {
    const b = backend();
    await renderEditor(b);

    await openActions();
    await act(async () => {
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    });
    await waitFor(() =>
      expect(screen.queryByRole("menu")).not.toBeInTheDocument(),
    );

    await openActions();
    await act(async () => {
      window.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
    });
    await waitFor(() =>
      expect(screen.queryByRole("menu")).not.toBeInTheDocument(),
    );

    // Nothing any of that did reached the backend.
    expect(calls(b, "open_discussion")).toHaveLength(0);
    expect(calls(b, "dispatch_agent_turn")).toHaveLength(0);
  });
});

describe("Discuss opens a conversation about the item (ACT-FR-13 … ACT-FR-17)", () => {
  it("ACT-FR-15, ACT-FR-16, AGC-FR-05, AGT-FR-36, AGT-FR-39, CVP-FR-64: posting targets the artifact and dispatches the artifact origin", async () => {
    const b = backend();
    await renderEditor(b, { artifactType: "spec" });

    // ACT-FR-13: closed rather than always present.
    expect(
      screen.queryByRole("dialog", { name: /Discuss this/ }),
    ).not.toBeInTheDocument();

    await openActions();
    await userEvent.click(screen.getByRole("menuitem", { name: /Discuss/ }));
    const composer = await screen.findByRole("textbox", {
      name: "Discuss this specification",
    });
    // ACT-FR-12: opening the composer collapsed the control.
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();

    await userEvent.type(composer, "@arch is this ready?");
    // ACT-FR-13: typing invokes nothing.
    expect(calls(b, "open_discussion")).toHaveLength(0);

    await userEvent.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() =>
      expect(calls(b, "open_discussion")).toHaveLength(1),
    );
    const [opened] = calls(b, "open_discussion") as [
      { target: unknown; body: string },
    ];
    // CMS-FR-57: the target names the file, taken entire.
    expect(opened.target).toEqual({ kind: "artifact", artifactId: "a.md" });
    expect(opened.body).toBe("@arch is this ready?");

    // ACT-FR-16 / AGC-FR-05: the origin the target decides.
    await waitFor(() => expect(calls(b, "dispatch_agent_turn")).toHaveLength(1));
    const [turn] = calls(b, "dispatch_agent_turn") as [{ origin: unknown }];
    expect(turn.origin).toEqual(artifactDiscussionOrigin("d-new", "a.md"));
  });

  it("ACT-FR-15, ACT-FR-16, AGC-FR-05, AGT-FR-36, AGT-FR-39, CVP-FR-64: @all leads the composer's picker and fans the message out", async () => {
    // ACT-FR-15 / ACT-FR-16, on the terms AGT-FR-36 and AGT-FR-39 set. Driven
    // through the real composer because this surface dispatches on its own
    // (`useDiscussionControl`), so the handle reaching two agents here is a
    // separate wiring from the rail's.
    const b = backend({
      agents: [
        { agent: { id: "a1", nickname: "arch" }, availability: "ready" },
        { agent: { id: "a2", nickname: "sec" }, availability: "ready" },
        {
          agent: { id: "a3", nickname: "scribe" },
          availability: "provider_unverified",
        },
      ],
    });
    await renderEditor(b, { artifactType: "spec" });

    await openActions();
    await userEvent.click(screen.getByRole("menuitem", { name: /Discuss/ }));
    const composer = await screen.findByRole("textbox", {
      name: "Discuss this specification",
    });

    // AGT-FR-39: the handle leads, naming only the agents that can answer.
    await userEvent.type(composer, "@a");
    const options = await screen.findAllByTestId("mention-picker-option");
    expect(options[0]).toHaveAttribute("data-entry", "all");
    expect(options[0]).toHaveTextContent("@arch, @sec");
    expect(options[0]).not.toHaveTextContent("@scribe");

    // AGT-FR-40: Tab inserts the handle itself.
    await userEvent.keyboard("{Tab}");
    await waitFor(() => expect(composer).toHaveValue("@all "));
    await userEvent.type(composer, "is this ready?");

    await userEvent.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() =>
      expect(calls(b, "open_discussion")).toHaveLength(1),
    );
    // One discussion, and one turn per agent that can answer.
    await waitFor(() => expect(calls(b, "dispatch_agent_turn")).toHaveLength(2));
    expect(
      (calls(b, "dispatch_agent_turn") as { nickname: string }[]).map(
        (c) => c.nickname,
      ),
    ).toEqual(["arch", "sec"]);
  });

  it("ACT-FR-13, ACT-FR-14 / ACT-FR-16, ACT-FR-17, ACT-FR-28: the composer keeps its text through any dismissal short of a successful post", async () => {
    const b = backend();
    await renderEditor(b);

    await openActions();
    await userEvent.click(screen.getByRole("menuitem", { name: /Discuss/ }));
    await userEvent.type(
      await screen.findByRole("textbox", { name: "Discuss this file" }),
      "half a thought",
    );

    // No close control of its own — Escape is the explicit dismissal. Its
    // default is prevented so nothing past this handler — a native shell
    // binding Escape to something of its own, fullscreen exit among them —
    // also acts on the same keystroke.
    const notPrevented = fireEvent.keyDown(window, { key: "Escape" });
    expect(notPrevented).toBe(false);
    await waitFor(() =>
      expect(
        screen.queryByRole("textbox", { name: "Discuss this file" }),
      ).not.toBeInTheDocument(),
    );
    // ACT-FR-13: the composer autofocuses its field, so a dismissal that put
    // focus nowhere would drop it on the document body. Escape puts it back on
    // the control the composer was opened from.
    expect(toggle()).toHaveFocus();

    // ACT-FR-14: the text lives in the session store, so reopening Discuss
    // restores what was typed.
    await openActions();
    await userEvent.click(screen.getByRole("menuitem", { name: /Discuss/ }));
    expect(
      await screen.findByRole("textbox", { name: "Discuss this file" }),
    ).toHaveValue("half a thought");
    expect(calls(b, "open_discussion")).toHaveLength(0);
  });

  it("ACT-FR-12, ACT-FR-13, ACT-FR-14: a host surface taking the composer's place keeps its text", async () => {
    const b = backend({ discussions: [discussion()] });
    await renderEditor(b);
    await screen.findByTestId("discussion-section");

    await openActions();
    await userEvent.click(screen.getByRole("menuitem", { name: /Discuss/ }));
    await userEvent.type(
      await screen.findByRole("textbox", { name: "Discuss this file" }),
      "half a thought",
    );

    // ACT-FR-12: a card's overflow menu opening dismisses whatever this control
    // had open — not an explicit close, but the composer goes all the same.
    await userEvent.click(
      screen.getByRole("button", { name: /^Thread actions for/ }),
    );
    await waitFor(() =>
      expect(
        screen.queryByRole("textbox", { name: "Discuss this file" }),
      ).not.toBeInTheDocument(),
    );

    await openActions();
    await userEvent.click(screen.getByRole("menuitem", { name: /Discuss/ }));
    expect(
      await screen.findByRole("textbox", { name: "Discuss this file" }),
    ).toHaveValue("half a thought");
  });

  it("ACT-FR-17: no identity disables the composer and opens nothing", async () => {
    const b = backend({ identityError: "identity_none_stored" });
    await renderEditor(b);

    await openActions();
    await userEvent.click(screen.getByRole("menuitem", { name: /Discuss/ }));
    const composer = await screen.findByRole("textbox", {
      name: "Discuss this file",
    });
    expect(composer).toBeDisabled();
    expect(screen.getByRole("button", { name: "Post" })).toBeDisabled();
    expect(calls(b, "open_discussion")).toHaveLength(0);
    // ACT-FR-13: a disabled field cannot take focus itself, so it lands on
    // the composer's stated reason instead — announced and discoverable
    // rather than silently unreachable.
    expect(screen.getByRole("status")).toHaveFocus();

    // And it is left the same way an enabled one is: a press elsewhere
    // dismisses it, there being nothing typed here that a dismissal could cost.
    await act(async () => {
      document
        .querySelector(".editor-tab")!
        .dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
    });
    await waitFor(() =>
      expect(
        screen.queryByRole("textbox", { name: "Discuss this file" }),
      ).not.toBeInTheDocument(),
    );
  });

  it("ACT-FR-17: a refused post keeps the body and opens nothing", async () => {
    const b = backend({ openError: "unsupported_media_type" });
    await renderEditor(b);

    await openActions();
    await userEvent.click(screen.getByRole("menuitem", { name: /Discuss/ }));
    const composer = await screen.findByRole("textbox", {
      name: "Discuss this file",
    });
    await userEvent.type(composer, "look at this");
    await userEvent.click(screen.getByRole("button", { name: "Post" }));

    await screen.findByRole("alert");
    // ACT-FR-17: the body is intact and no turn was dispatched.
    expect(composer).toHaveValue("look at this");
    expect(calls(b, "dispatch_agent_turn")).toHaveLength(0);
  });

  it("ACT-FR-28, CVP-FR-40, CVP-FR-41: Ctrl/Cmd+Enter posts, and is a no-op while empty, disabled, or mid-mention-completion", async () => {
    const b = backend();
    await renderEditor(b, { artifactType: "spec" });

    await openActions();
    await userEvent.click(screen.getByRole("menuitem", { name: /Discuss/ }));
    const composer = await screen.findByRole("textbox", {
      name: "Discuss this specification",
    });

    // Empty: the accelerator does nothing.
    fireEvent.keyDown(composer, { key: "Enter", ctrlKey: true });
    expect(calls(b, "open_discussion")).toHaveLength(0);

    // The mention picker's own Enter/Tab handling is untouched: completing a
    // nickname must not also post whatever the field held at that moment.
    await userEvent.type(composer, "@ar");
    await screen.findByRole("listbox");
    fireEvent.keyDown(composer, { key: "Enter", metaKey: true });
    await waitFor(() => expect(composer).toHaveValue("@arch "));
    expect(calls(b, "open_discussion")).toHaveLength(0);

    await userEvent.type(composer, "is this ready?");
    fireEvent.keyDown(composer, { key: "Enter", metaKey: true });
    await waitFor(() =>
      expect(calls(b, "open_discussion")).toHaveLength(1),
    );
    // No newline landed in the message the accelerator posted.
    const [opened] = calls(b, "open_discussion") as [{ body: string }];
    expect(opened.body).not.toContain("\n");
  });

  it("ACT-FR-28, CVP-FR-40, CVP-FR-41: the accelerator does nothing while the composer is disabled for want of an identity", async () => {
    const b = backend({ identityError: "identity_none_stored" });
    await renderEditor(b);

    await openActions();
    await userEvent.click(screen.getByRole("menuitem", { name: /Discuss/ }));
    const composer = await screen.findByRole("textbox", {
      name: "Discuss this file",
    });
    fireEvent.change(composer, { target: { value: "is this ready?" } });
    fireEvent.keyDown(composer, { key: "Enter", ctrlKey: true });
    expect(calls(b, "open_discussion")).toHaveLength(0);
  });

  it("ACT-FR-16, ACT-FR-17, ACT-FR-28: a second Post while the first is still in flight invokes the operation once", async () => {
    const b = backend();
    await renderEditor(b);

    await openActions();
    await userEvent.click(screen.getByRole("menuitem", { name: /Discuss/ }));
    const composer = await screen.findByRole("textbox", {
      name: "Discuss this file",
    });
    await userEvent.type(composer, "is this ready?");

    // Swap in a controlled promise for `open_discussion_thread` alone, now
    // that mount-time setup (identity, agents, threads) has already resolved
    // through the standard mock — every other command still answers as it
    // did.
    const answering = wire(b);
    let resolveOpen!: (thread: Discussion) => void;
    invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
      if (cmd === "open_discussion") {
        b.calls.push({ cmd, args });
        return new Promise<Discussion>((resolve) => {
          resolveOpen = resolve;
        });
      }
      return answering(cmd, args);
    });

    const post = screen.getByRole("button", { name: "Post" });
    await userEvent.click(post);
    expect(post).toBeDisabled();
    // A second click, and the accelerator too, while the first request is
    // still unresolved.
    await userEvent.click(post);
    fireEvent.keyDown(composer, { key: "Enter", ctrlKey: true });
    expect(calls(b, "open_discussion")).toHaveLength(1);

    resolveOpen(discussion({ id: "d-new" }));
    // Still exactly one call once it settles, and the composer is gone.
    await waitFor(() =>
      expect(
        screen.queryByRole("textbox", { name: "Discuss this file" }),
      ).not.toBeInTheDocument(),
    );
    expect(calls(b, "open_discussion")).toHaveLength(1);
  });
});

describe("no item affords an implementation (ACT-FR-04, ACT-FR-05)", () => {
  it("ACT-FR-04, ACT-FR-05: no entry offers to hand a specification to an implementing agent", async () => {
    const b = backend();
    await renderEditor(b, { artifactType: "spec" });

    // One run does the whole of the work, so there is no second half for any
    // surface to trigger. The entry is absent rather than disabled, which is
    // ACT-FR-05's rule for an action the item does not afford.
    await openActions();
    expect(
      screen.queryByRole("menuitem", { name: /Implement/ }),
    ).not.toBeInTheDocument();
    expect(calls(b, "start_implementation")).toHaveLength(0);
  });
});

describe("where the discussions are read (ACT-FR-19, ACT-FR-20)", () => {
  it("ACT-FR-16, ACT-FR-19, CMT-FR-53, CVP-FR-SDMQ, CVP-FR-64: an Editor in WYSIWYG reads them in the rail and carries no panel", async () => {
    const b = backend({ discussions: [discussion()] });
    await renderEditor(b);

    // ACT-FR-19: pinned at the rail's head, so there is no discussions control
    // and no floating panel anywhere in the tab.
    await waitFor(() =>
      expect(calls(b, "list_discussions")).toHaveLength(1),
    );
    expect(calls(b, "list_discussions")[0]).toEqual({
      target: { kind: "artifact", artifactId: "a.md" },
    });
    expect(
      screen.queryByRole("button", { name: /Show discussion/ }),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("dialog", { name: "Discussion" }),
    ).not.toBeInTheDocument();
  });

  it("ACT-FR-16, ACT-FR-19, CMT-FR-53, CVP-FR-SDMQ, CVP-FR-64/CMT-FR-61: the Editor rail actually renders the discussion card", async () => {
    const b = backend({ discussions: [discussion()] });
    await renderEditor(b);

    // CMT-FR-61 previously FORBADE this. The positive claim is the whole of the
    // change, so it is asserted directly rather than inferred from the absence
    // of a panel: the card is in the rail's pinned Discussion section, above the
    // aligned cards, carrying its opening comment and no anchor quote.
    await screen.findByTestId("discussion-section");
    const rail = document.querySelector(".comment-rail") as HTMLElement;
    expect(within(rail).getByText(/is this ready\?/)).toBeInTheDocument();
  });

  it("ACT-FR-16, ACT-FR-19, CMT-FR-53, CVP-FR-SDMQ / CVP-FR-64: posting from an Editor shows the new discussion in the rail through the shared surface", async () => {
    const b = backend();
    await renderEditor(b);

    await openActions();
    await userEvent.click(screen.getByRole("menuitem", { name: /Discuss/ }));
    await userEvent.type(
      await screen.findByRole("textbox", { name: "Discuss this file" }),
      "is this ready?",
    );
    await userEvent.click(screen.getByRole("button", { name: "Post" }));

    // ACT-FR-16: the rail reveals itself and renders the new discussion in the
    // shared surface, the only place it is ever shown.
    await screen.findByTestId("discussion-section");
    const rail = document.querySelector(".comment-rail") as HTMLElement;
    expect(
      await within(rail).findByTestId("comment-thread-d-new"),
    ).toBeInTheDocument();
    // The composer emptied and closed behind it.
    expect(
      screen.queryByRole("textbox", { name: "Discuss this file" }),
    ).not.toBeInTheDocument();
  });

  it("ACT-FR-20, ACT-FR-16, ACT-FR-24 / CVP-FR-64: posting from a panel host opens the panel on the shared surface", async () => {
    const b = backend();
    await renderEditor(b);
    // Raw-text mode has no margin, so this exercises the OTHER branch of the
    // post path — the one Flow and Diff tabs always take (ACT-FR-20).
    await userEvent.click(
      screen.getByRole("button", { name: "Edit as Markdown source" }),
    );

    await openActions();
    await userEvent.click(screen.getByRole("menuitem", { name: /Discuss/ }));
    await userEvent.type(
      await screen.findByRole("textbox", { name: "Discuss this file" }),
      "is this ready?",
    );
    await userEvent.click(screen.getByRole("button", { name: "Post" }));

    const panel = await screen.findByRole("dialog", { name: "Discussion" });
    expect(
      await within(panel).findByTestId("comment-thread-d-new"),
    ).toBeInTheDocument();
    expect(surfaceOwner("d-new")).toBe("panel");
    // CVP-FR-02: one surface, so the reply composer is in the panel alone.
    expect(screen.getAllByLabelText("Reply to thread d-new")).toHaveLength(1);
    // ACT-FR-20: and the counter appeared with it.
    expect(
      screen.getByRole("button", { name: "Hide discussion" }),
    ).toBeInTheDocument();
  });

  it("ACT-FR-12: the control's surfaces and the rail's card menus are one set", async () => {
    const b = backend({ discussions: [discussion()] });
    await renderEditor(b);
    await screen.findByTestId("discussion-section");

    // Opening a card's overflow menu dismisses whatever the control had open.
    await openActions();
    expect(screen.getByRole("menu")).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: /^Thread actions for/ }),
    );
    await waitFor(() =>
      // The control's own column is gone; what is open now is the card's menu.
      expect(
        screen.queryByRole("menuitem", { name: /Discuss/ }),
      ).not.toBeInTheDocument(),
    );
  });

  it("ACT-FR-13: a press elsewhere in the tab dismisses the composer, one inside it does not", async () => {
    const b = backend();
    await renderEditor(b);

    await openActions();
    await userEvent.click(screen.getByRole("menuitem", { name: /Discuss/ }));
    const composer = await screen.findByRole("textbox", {
      name: "Discuss this file",
    });

    // A press on the composer itself is not "elsewhere": the field, its mention
    // picker, and its attach menu all live inside the control's own element.
    await act(async () => {
      composer.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
    });
    expect(composer).toBeInTheDocument();

    // The picker is the case that matters, because it is the one part of the
    // composer that renders as a surface of its own — pressing an agent's row
    // to complete a nickname must not discard the sentence it is in.
    await userEvent.type(composer, "ask @a");
    const option = (await screen.findAllByTestId("mention-picker-option"))[0];
    await act(async () => {
      option.dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
    });
    expect(composer).toBeInTheDocument();
    expect(composer).toHaveValue("ask @a");

    // ACT-FR-03 / ACT-FR-13: the expansion and the composer are left the same
    // way — one small surface over the tab, dismissed by a press past its edge.
    await act(async () => {
      document
        .querySelector(".editor-tab")!
        .dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
    });
    await waitFor(() =>
      expect(
        screen.queryByRole("textbox", { name: "Discuss this file" }),
      ).not.toBeInTheDocument(),
    );

    // Unlike Escape: the press has itself put focus where the author chose, and
    // pulling it back to the control would take it off whatever they reached for.
    expect(toggle()).not.toHaveFocus();

    // ACT-FR-14: what was typed survives the dismissal, and nothing any of it
    // did reached the backend.
    await openActions();
    await userEvent.click(screen.getByRole("menuitem", { name: /Discuss/ }));
    expect(
      await screen.findByRole("textbox", { name: "Discuss this file" }),
    ).toHaveValue("ask @a");
    expect(calls(b, "open_discussion")).toHaveLength(0);
  });

  it("ACT-FR-13: a press elsewhere while a post is in flight leaves the composer, and its refusal, standing", async () => {
    const b = backend();
    await renderEditor(b);

    await openActions();
    await userEvent.click(screen.getByRole("menuitem", { name: /Discuss/ }));
    const composer = await screen.findByRole("textbox", {
      name: "Discuss this file",
    });
    await userEvent.type(composer, "is this ready?");

    // The same controlled-promise swap ACT-FR-16, ACT-FR-17, ACT-FR-28 uses, refusing instead of
    // resolving: mount-time setup has already settled through the standard mock.
    const answering = wire(b);
    let rejectOpen!: (reason: unknown) => void;
    invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
      if (cmd === "open_discussion") {
        b.calls.push({ cmd, args });
        return new Promise<Discussion>((_, reject) => {
          rejectOpen = reject;
        });
      }
      return answering(cmd, args);
    });

    await userEvent.click(screen.getByRole("button", { name: "Post" }));
    await act(async () => {
      document
        .querySelector(".editor-tab")!
        .dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
    });
    // ACT-FR-17: the composer is the only surface the refusal has to render in,
    // so a stray click while the request is out must not unmount it.
    expect(composer).toBeInTheDocument();
    expect(composer).toHaveValue("is this ready?");

    await act(async () => {
      rejectOpen(new Error("refused"));
      await Promise.resolve();
    });
    await screen.findByRole("alert");
    // The body is still here, so the message is not retyped.
    expect(composer).toHaveValue("is this ready?");
  });

  it("ACT-FR-19, ACT-FR-20: raw-text mode reads them in the floating panel instead", async () => {
    const b = backend({ discussions: [discussion()] });
    await renderEditor(b);
    await waitFor(() =>
      expect(calls(b, "list_discussions")).toHaveLength(1),
    );

    await userEvent.click(
      screen.getByRole("button", { name: "Edit as Markdown source" }),
    );

    // ACT-FR-20: the discussions control appears, carrying the unresolved count.
    const show = await screen.findByRole("button", { name: "Show discussion (1)" });
    expect(
      screen.queryByRole("dialog", { name: "Discussion" }),
    ).not.toBeInTheDocument();

    await userEvent.click(show);
    await screen.findByRole("dialog", { name: "Discussion" });
    // A second activation closes it again.
    await userEvent.click(
      screen.getByRole("button", { name: "Hide discussion" }),
    );
    await waitFor(() =>
      expect(
        screen.queryByRole("dialog", { name: "Discussion" }),
      ).not.toBeInTheDocument(),
    );
    // The list was read once for the tab, not once per mode.
    expect(calls(b, "list_discussions")).toHaveLength(1);
  });

  it("ACT-FR-12: a press elsewhere in the tab leaves the discussion panel open", async () => {
    const b = backend({ discussions: [discussion()] });
    await renderEditor(b);
    await userEvent.click(
      screen.getByRole("button", { name: "Edit as Markdown source" }),
    );
    await userEvent.click(
      await screen.findByRole("button", { name: "Show discussion (1)" }),
    );
    const panel = await screen.findByRole("dialog", { name: "Discussion" });

    // Unlike the expansion and the composer: a conversation is held open to be
    // read, and a press in the tab while reading one is as likely to be
    // scrolling what it is about as it is to be leaving it.
    await act(async () => {
      document
        .querySelector(".editor-tab")!
        .dispatchEvent(new MouseEvent("mousedown", { bubbles: true }));
    });
    expect(panel).toBeInTheDocument();
  });

  it("ACT-FR-20, ACT-FR-16, ACT-FR-24, CVP-FR-64: an item with no discussion carries one mark of chrome, not two", async () => {
    const b = backend();
    await renderEditor(b);
    await userEvent.click(
      screen.getByRole("button", { name: "Edit as Markdown source" }),
    );
    // ACT-FR-20: the control renders only once there is something to show.
    expect(
      screen.queryByRole("button", { name: /Show discussion/ }),
    ).not.toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: "Actions" })).toHaveLength(1);
  });

  it("ACT-FR-21, ACT-FR-22, CMT-FR-55: a discussion redraws from the event without a second read", async () => {
    const b = backend({ discussions: [discussion()] });
    await renderEditor(b);
    await userEvent.click(
      screen.getByRole("button", { name: "Edit as Markdown source" }),
    );
    await screen.findByRole("button", { name: "Show discussion (1)" });

    // ACT-FR-22: the surface follows `"discussion changed"` and re-reads for
    // none of it — here the discussion is resolved elsewhere.
    await act(async () => {
      emitEvent(
        "discussion-changed",
        discussion({ resolved: true, updatedAt: "2026-01-02T00:00:00Z" }),
      );
    });
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Show discussion (0)" }),
      ).toBeInTheDocument(),
    );
    expect(calls(b, "list_discussions")).toHaveLength(1);
  });

  it("a discussion is never merged into the rail's anchored threads", async () => {
    // A regression guard. An artifact's discussion carries the SAME `artifactId`
    // its anchored threads do, so the rail's `discussion-changed` handler
    // matched it and merged it into the anchored list — one thread in two lists.
    // Once resolved it appeared in the resolved disclosure twice, under one React
    // key, and the unresolved count changed on the first event rather than being
    // right from the open.
    const b = backend({ discussions: [discussion()] });
    await renderEditor(b);
    await screen.findByTestId("discussion-section");

    // CMT-FR-56: counted from the open, not from the first event.
    expect(
      screen.getByRole("button", { name: /Hide comments \(1 unresolved\)/ }),
    ).toBeInTheDocument();

    await act(async () => {
      emitEvent(
        "discussion-changed",
        discussion({ resolved: true, updatedAt: "2026-01-02T00:00:00Z" }),
      );
    });

    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: /Hide comments \(0 unresolved\)/ }),
      ).toBeInTheDocument(),
    );
    // One thread, counted once. Before the fix this read "2 resolved threads"
    // and rendered the same card twice under one React key.
    const disclosure = screen.getByRole("button", { name: /resolved thread/ });
    expect(disclosure).toHaveTextContent("1 resolved thread");
    await userEvent.click(disclosure);
    const rail = document.querySelector(".comment-rail") as HTMLElement;
    expect(within(rail).queryAllByText(/is this ready\?/)).toHaveLength(1);
  });

  it("another file's discussion is ignored", async () => {
    const b = backend();
    await renderEditor(b);
    await userEvent.click(
      screen.getByRole("button", { name: "Edit as Markdown source" }),
    );

    await act(async () => {
      emitEvent("discussion-changed", discussion({ target: { kind: "artifact", artifactId: "other.md" } }));
    });
    // Nothing of another file's conversation reaches this tab.
    expect(
      screen.queryByRole("button", { name: /Show discussion/ }),
    ).not.toBeInTheDocument();
  });
});
