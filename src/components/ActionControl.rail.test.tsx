import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Editor } from "./Editor";
import { EditSessionStore } from "../state/editSessions";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { resetDiscussionFocus } from "../state/discussionFocus";
import type { Discussion } from "../types";

/**
 * `ACT-action-control.md` ACT-FR-BRZK and `CMT-comments.md` CMT-FR-QHTW: on an
 * Editor tab that lends the comment rail a margin, Discuss opens the rail and
 * its whole-target opening composer. No floating composer, no floating panel,
 * and no discussions control appear, whatever the type of the file.
 */
const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));
// No log flush timer may outlive its test.
vi.mock("../logging", () => ({
  logDebug: vi.fn(),
  logInfo: vi.fn(),
  logWarn: vi.fn(),
  logError: vi.fn(),
  flushLogs: vi.fn(),
}));

const BODY = "# Editor\n\nSteps to run before the first session.\n";

function discussion(id: string, artifactId: string): Discussion {
  return {
    id,
    target: { kind: "artifact", artifactId },
    fragmentTarget: null,
    comments: [
      {
        id: "c1",
        author: { kind: "human", login: "raver119" },
        body: "is this ready?",
        quotes: [],
        attachments: [],
        createdAt: "2026-01-01T00:00:00Z",
      },
    ],
    resolved: false,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
  };
}

interface Backend {
  discussions: Discussion[];
  calls: { cmd: string; args: unknown }[];
  identityError?: string;
  openError?: string;
  /** Holds `open_discussion` until the test settles it. */
  openGate?: Promise<void>;
}

const KNOWN = new Set([
  "append_log_records",
  "list_ai_api_catalogs",
  "get_active_ai_api_catalog",
  "list_agent_turns",
  "load_project_tree",
  "save_artifact_contents",
]);

function wire(b: Backend, artifactId: string) {
  invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
    b.calls.push({ cmd, args });
    switch (cmd) {
      case "load_artifact_contents_by_id":
        return { body: BODY, checksum: "ck1" };
      case "list_discussions":
        return b.discussions;
      case "list_project_agents":
        return [{ agent: { id: "a1", nickname: "arch" }, availability: "ready" }];
      case "resolve_comment_author_identity":
        if (b.identityError) throw b.identityError;
        return { kind: "human", login: "raver119" };
      case "open_discussion": {
        if (b.openGate) await b.openGate;
        if (b.openError) throw b.openError;
        const opened = discussion("d-new", artifactId);
        b.discussions = [...b.discussions, opened];
        return opened;
      }
      case "get_active_ai_api_catalog":
        return { resolution: "none_configured", catalog: null };
      default:
        if (KNOWN.has(cmd)) return cmd.startsWith("list") ? [] : undefined;
        throw new Error(`unexpected command ${cmd}`);
    }
  });
}

const calls = (b: Backend, cmd: string) =>
  b.calls.filter((c) => c.cmd === cmd).map((c) => c.args);

async function renderEditor(
  b: Backend,
  opts: { artifactId?: string; artifactType?: "spec" | "doc" } = {},
) {
  const artifactId = opts.artifactId ?? "notes.md";
  wire(b, artifactId);
  render(
    <Editor
      artifactId={artifactId}
      artifactName={artifactId}
      artifactType={opts.artifactType}
      sessions={new EditSessionStore()}
    />,
  );
  await screen.findByRole("button", { name: "Actions" });
}

async function discuss() {
  await userEvent.click(screen.getByRole("button", { name: "Actions" }));
  await userEvent.click(screen.getByRole("menuitem", { name: /Discuss/ }));
}

const railComposer = () => screen.queryByTestId("whole-target-composer");
const floatingComposer = () => document.querySelector(".draft-composer");

beforeEach(() => {
  invokeMock.mockReset();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
});
afterEach(cleanup);

describe("EDT-FR-03: a Document is edited in the one Editor", () => {
  it("EDT-FR-03: a Markdown file of the Document type opens editable, in WYSIWYG, with the raw-mode toggle", async () => {
    const b: Backend = { discussions: [], calls: [] };
    await renderEditor(b, { artifactType: "doc" });
    expect(await screen.findByText("Steps to run before the first session.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Edit as Markdown source" })).toBeInTheDocument();
    expect(document.querySelector('[contenteditable="true"]')).not.toBeNull();
  });
});

describe("ACT-FR-BRZK: Discuss opens the rail's whole-target composer", () => {
  it("ACT-FR-BRZK, ACT-FR-19, CMT-FR-QHTW, CMT-FR-53: a file with no type opens the composer at the head of the rail, with no floating composer, panel, or control", async () => {
    const b: Backend = { discussions: [], calls: [] };
    await renderEditor(b);
    expect(document.querySelector(".comment-rail")).toBeNull();

    await discuss();

    const composer = await screen.findByTestId("whole-target-composer");
    const section = screen.getByTestId("discussion-section");
    expect(section).toContainElement(composer);
    expect(within(composer).getByRole("textbox", { name: "Discuss this file" })).toHaveFocus();
    expect(floatingComposer()).toBeNull();
    expect(screen.queryByRole("dialog", { name: "Discussion" })).toBeNull();
    expect(screen.queryByRole("button", { name: /Show discussion/ })).toBeNull();
  });

  it("ACT-FR-BRZK, CMT-FR-QHTW: a typed artifact takes the same route, named for its type", async () => {
    const b: Backend = { discussions: [discussion("d1", "a.spec.md")], calls: [] };
    await renderEditor(b, { artifactId: "a.spec.md", artifactType: "spec" });

    await discuss();

    const composer = await screen.findByTestId("whole-target-composer");
    const field = within(composer).getByRole("textbox", { name: "Discuss this specification" });
    expect(field).toHaveAttribute("placeholder", "What should be done with this specification?");
    expect(floatingComposer()).toBeNull();
    // CMT-FR-QHTW: above the item's discussions.
    const section = screen.getByTestId("discussion-section");
    expect(section.firstElementChild?.nextElementSibling).toBe(composer);
  });

  it("ACT-FR-BRZK, ESH-FR-SSDV: a source file, whose one surface carries the rail, takes the same route", async () => {
    const b: Backend = { discussions: [], calls: [] };
    await renderEditor(b, { artifactId: "build.sh" });
    await discuss();
    expect(await screen.findByTestId("whole-target-composer")).toBeInTheDocument();
    expect(floatingComposer()).toBeNull();
  });

  it("ACT-FR-16, ACT-FR-LXKD, CMT-FR-QHTW: a post opens a whole-target discussion and shows it in the rail with the composer gone", async () => {
    const b: Backend = { discussions: [], calls: [] };
    await renderEditor(b, { artifactType: "doc" });
    await discuss();
    const composer = await screen.findByTestId("whole-target-composer");
    await userEvent.type(within(composer).getByRole("textbox"), "what next?");
    await userEvent.click(within(composer).getByRole("button", { name: "Post" }));

    const rail = document.querySelector(".comment-rail") as HTMLElement;
    expect(await within(rail).findByTestId("comment-thread-d-new")).toBeInTheDocument();
    await waitFor(() => expect(railComposer()).toBeNull());
    expect(calls(b, "open_discussion")).toHaveLength(1);
    expect(calls(b, "open_discussion")[0]).toMatchObject({
      target: { kind: "artifact", artifactId: "notes.md" },
      fragmentTarget: null,
    });
    expect(calls(b, "dispatch_agent_turn")).toHaveLength(0);
    expect(floatingComposer()).toBeNull();
    // ACT-FR-16: focus lands in the reply composer of the new discussion.
    await waitFor(() =>
      expect(screen.getByRole("textbox", { name: "Reply to thread d-new" })).toHaveFocus(),
    );
    // ACT-FR-14: a successful post clears the draft.
    await discuss();
    const again = within(await screen.findByTestId("whole-target-composer")).getByRole("textbox");
    expect(again).toHaveValue("");
  });

  it("ACT-FR-16: a second Post while the first is in flight invokes the operation once", async () => {
    let release = () => {};
    const b: Backend = {
      discussions: [],
      calls: [],
      openGate: new Promise<void>((r) => {
        release = r;
      }),
    };
    await renderEditor(b);
    await discuss();
    const composer = await screen.findByTestId("whole-target-composer");
    await userEvent.type(within(composer).getByRole("textbox"), "once only");
    const post = within(composer).getByRole("button", { name: "Post" });
    await userEvent.click(post);
    await userEvent.click(post);
    fireEvent.keyDown(within(composer).getByRole("textbox"), { key: "Enter", ctrlKey: true });
    expect(calls(b, "open_discussion")).toHaveLength(1);
    release();
    await waitFor(() => expect(railComposer()).toBeNull());
    expect(calls(b, "open_discussion")).toHaveLength(1);
  });

  it("ACT-FR-17, CMT-FR-QHTW: a refused post keeps the composer, the body, and the error, and a press outside while it is in flight leaves them", async () => {
    let release = () => {};
    const b: Backend = {
      discussions: [],
      calls: [],
      openError: "artifact_not_found",
      openGate: new Promise<void>((r) => {
        release = r;
      }),
    };
    await renderEditor(b);
    await discuss();
    const composer = await screen.findByTestId("whole-target-composer");
    await userEvent.type(within(composer).getByRole("textbox"), "keep me");
    await userEvent.click(within(composer).getByRole("button", { name: "Post" }));

    fireEvent.mouseDown(document.querySelector(".editor-tab") as HTMLElement);
    expect(railComposer()).not.toBeNull();
    release();

    await waitFor(() =>
      expect(within(composer).getByRole("button", { name: "Post" })).not.toBeDisabled(),
    );
    expect(railComposer()).not.toBeNull();
    expect(within(composer).getByRole("textbox")).toHaveValue("keep me");
    expect(within(composer).getByRole("alert")).toBeInTheDocument();
  });

  it("ACT-FR-17, CMT-FR-32: with no identity the composer is disabled, takes focus on its surface, and Escape closes it", async () => {
    const b: Backend = { discussions: [], calls: [], identityError: "github_token_selection_required" };
    await renderEditor(b);
    await discuss();
    const composer = await screen.findByTestId("whole-target-composer");
    expect(within(composer).getByRole("textbox")).toBeDisabled();
    await waitFor(() => expect(composer).toHaveFocus());

    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(railComposer()).toBeNull());
    expect(calls(b, "open_discussion")).toHaveLength(0);
  });

  it("CMT-FR-32, ACT-FR-14: Escape and a press outside close the composer, and the typed text comes back", async () => {
    const b: Backend = { discussions: [], calls: [] };
    await renderEditor(b);
    await discuss();
    const field = within(await screen.findByTestId("whole-target-composer")).getByRole("textbox");
    await userEvent.type(field, "half a thought");

    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() => expect(railComposer()).toBeNull());

    await discuss();
    const again = within(await screen.findByTestId("whole-target-composer")).getByRole("textbox");
    expect(again).toHaveValue("half a thought");

    fireEvent.mouseDown(document.querySelector(".editor-tab") as HTMLElement);
    await waitFor(() => expect(railComposer()).toBeNull());
    expect(calls(b, "open_discussion")).toHaveLength(0);
  });

  it("ACT-FR-13, ACT-FR-20, ACT-FR-14: raw-Markdown mode keeps the floating composer, holding the same draft", async () => {
    const b: Backend = { discussions: [], calls: [] };
    await renderEditor(b);
    await discuss();
    const field = within(await screen.findByTestId("whole-target-composer")).getByRole("textbox");
    await userEvent.type(field, "kept across modes");
    fireEvent.keyDown(window, { key: "Escape" });

    await userEvent.click(screen.getByRole("button", { name: "Edit as Markdown source" }));
    await discuss();

    expect(floatingComposer()).not.toBeNull();
    expect(railComposer()).toBeNull();
    expect(screen.getByRole("textbox", { name: "Discuss this file" })).toHaveValue(
      "kept across modes",
    );
  });
});
