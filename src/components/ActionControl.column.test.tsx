import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Editor } from "./Editor";
import { EditSessionStore } from "../state/editSessions";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { resetDiscussionFocus } from "../state/discussionFocus";
import { resetLayoutPreferencesCache } from "../state/layoutPreferences";
import { CONTEXT_ESTIMATE } from "../test/invokeMocks";
import {
  discussionColumnState,
  fileColumn,
  restoreDiscussionColumns,
} from "../state/discussionColumn";
import type { Discussion } from "../types";

/**
 * `ACT-action-control.md` ACT-FR-BRZK and ACT-FR-19, with
 * `DDS-draft-discussion.md` DDS-FR-JWNC: on an Editor tab the discussions are
 * read in the discussion column beside the document. Discuss opens no
 * composer of its own. While the column shows, Discuss is disabled and says
 * why. While the column is hidden, Discuss shows it and puts the caret in its
 * composer. No floating composer, no floating panel, and no discussions
 * control appear, whatever the type of the file and whatever the mode.
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
  "load_layout_preferences",
  "save_layout_preferences",
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
      case "estimate_discussion_context":
        return CONTEXT_ESTIMATE;
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

async function openActions() {
  await userEvent.click(screen.getByRole("button", { name: "Actions" }));
}

const discussItem = () => screen.getByRole("menuitem", { name: /Discuss/ });

async function discuss() {
  await openActions();
  await userEvent.click(discussItem());
}

const column = () => screen.getByTestId("editor-discussion-column");
const columnToggle = () => screen.getByTestId("editor-discussion-toggle");
const openingComposer = () => screen.queryByTestId("draft-open-discussion");
const floatingComposer = () => document.querySelector(".draft-composer");

/** DDS-FR-XQMF: hide the column with the toolbar's own control. */
async function hideColumn() {
  await userEvent.click(columnToggle());
  await waitFor(() => expect(column()).not.toBeVisible());
}

/** ACT-FR-19 / ACT-FR-20: nothing of the floating route is in the tab. */
function expectNoFloatingRoute() {
  expect(floatingComposer()).toBeNull();
  expect(screen.queryByRole("dialog", { name: "Discussion" })).toBeNull();
  expect(screen.queryByRole("button", { name: /Show discussion/ })).toBeNull();
  expect(screen.queryByRole("button", { name: /Hide discussion$/ })).toBeNull();
}

beforeEach(() => {
  // The record restored here is cached per project, and must not reach the
  // next test.
  resetLayoutPreferencesCache();
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

describe("ACT-FR-BRZK: Discuss acts on the discussion column", () => {
  it("ACT-FR-BRZK, ACT-FR-19, ACT-FR-23, CMT-FR-53: with the column shown, Discuss is disabled and says why, and the column holds the opening composer", async () => {
    const b: Backend = { discussions: [], calls: [] };
    await renderEditor(b);

    // CMT-FR-30: the column of a file shows until the author hides it.
    expect(column()).toBeVisible();
    const composer = within(column()).getByTestId("draft-open-discussion");
    expect(
      within(composer).getByRole("textbox", { name: "Discuss this file" }),
    ).toHaveAttribute("placeholder", "Ask an agent about this file…");

    await openActions();
    expect(discussItem()).toBeDisabled();
    expect(discussItem()).toHaveAttribute(
      "title",
      "The discussion is already beside the file",
    );
    expectNoFloatingRoute();
  });

  it("ACT-FR-BRZK: a specification takes the same route, named for its type", async () => {
    const b: Backend = { discussions: [], calls: [] };
    await renderEditor(b, { artifactId: "a.spec.md", artifactType: "spec" });

    const field = within(column()).getByRole("textbox", {
      name: "Discuss this specification",
    });
    expect(field).toHaveAttribute(
      "placeholder",
      "Ask an agent about this specification…",
    );
    await openActions();
    expect(discussItem()).toHaveAttribute(
      "title",
      "The discussion is already beside the specification",
    );
    expectNoFloatingRoute();
  });

  it("ACT-FR-BRZK, DDS-FR-JWNC, DDS-FR-XQMF: with the column hidden, Discuss shows it and puts the caret in its opening composer", async () => {
    const b: Backend = { discussions: [], calls: [] };
    // A project is open, so the column's state is written to its record.
    wire(b, "notes.md");
    await restoreDiscussionColumns("/p");
    await renderEditor(b);
    await hideColumn();
    expect(screen.getByTestId("editor-discussion-split")).toHaveAttribute(
      "data-discussion",
      "hidden",
    );

    await openActions();
    expect(discussItem()).toBeEnabled();
    await userEvent.click(discussItem());

    await waitFor(() => expect(column()).toBeVisible());
    await waitFor(() =>
      expect(
        within(column()).getByRole("textbox", { name: "Discuss this file" }),
      ).toHaveFocus(),
    );
    // The toolbar's control and the store agree: one state, not two.
    expect(columnToggle()).toHaveAttribute("aria-pressed", "true");
    expect(discussionColumnState(fileColumn("notes.md")).hidden).toBe(false);
    // DDS-FR-JWNC: the same persisted result as the toolbar's control.
    await waitFor(() => {
      const saves = calls(b, "save_layout_preferences") as {
        preferences: { fileDiscussionHidden?: Record<string, boolean> };
      }[];
      expect(saves).toHaveLength(2);
      expect(saves[1].preferences.fileDiscussionHidden).toEqual({ "notes.md": false });
    });
    expectNoFloatingRoute();
    expect(calls(b, "open_discussion")).toHaveLength(0);

    // The column stands again, so the action has nothing left to do.
    await openActions();
    expect(discussItem()).toBeDisabled();
  });

  it("ACT-FR-BRZK, DDS-FR-JWNC: with a discussion, Discuss puts the caret in its reply composer", async () => {
    const b: Backend = { discussions: [discussion("d1", "notes.md")], calls: [] };
    await renderEditor(b);
    await within(column()).findByTestId("comment-thread-d1");
    await hideColumn();

    await discuss();

    await waitFor(() => expect(column()).toBeVisible());
    await waitFor(() =>
      expect(screen.getByRole("textbox", { name: "Reply to thread d1" })).toHaveFocus(),
    );
    expect(openingComposer()).toBeNull();
    expectNoFloatingRoute();
  });

  it("ACT-FR-BRZK, ESH-FR-SSDV: a source file, whose one surface carries the column, takes the same route", async () => {
    const b: Backend = { discussions: [], calls: [] };
    await renderEditor(b, { artifactId: "build.sh" });

    expect(within(column()).getByTestId("draft-open-discussion")).toBeInTheDocument();
    await openActions();
    expect(discussItem()).toBeDisabled();
    await userEvent.keyboard("{Escape}");

    await hideColumn();
    await discuss();
    await waitFor(() =>
      expect(
        within(column()).getByRole("textbox", { name: "Discuss this file" }),
      ).toHaveFocus(),
    );
    expectNoFloatingRoute();
  });

  it("ACT-FR-BRZK, ACT-FR-19, EDT-FR-61, EDT-FR-64: raw-Markdown mode takes the same route, with no floating composer or panel", async () => {
    // A discussion, so the floating discussions control would render if the
    // tab still had one.
    const b: Backend = { discussions: [discussion("d1", "notes.md")], calls: [] };
    await renderEditor(b);
    await within(column()).findByTestId("comment-thread-d1");
    await userEvent.click(screen.getByRole("button", { name: "Edit as Markdown source" }));

    // EDT-FR-61: a switch of editing mode never hides the column.
    expect(column()).toBeVisible();
    await openActions();
    expect(discussItem()).toBeDisabled();
    await userEvent.keyboard("{Escape}");

    await hideColumn();
    await discuss();
    await waitFor(() =>
      expect(screen.getByRole("textbox", { name: "Reply to thread d1" })).toHaveFocus(),
    );
    expectNoFloatingRoute();
  });

  it("DDS-FR-XQMF, DDS-FR-JWNC: text typed before the column is hidden is still there when Discuss shows it again", async () => {
    const b: Backend = { discussions: [], calls: [] };
    await renderEditor(b);
    await userEvent.type(
      within(column()).getByRole("textbox", { name: "Discuss this file" }),
      "half a thought",
    );
    await hideColumn();

    await discuss();

    const field = within(column()).getByRole("textbox", { name: "Discuss this file" });
    await waitFor(() => expect(field).toHaveFocus());
    expect(field).toHaveValue("half a thought");
    // A second cycle lands the caret again.
    await hideColumn();
    await discuss();
    await waitFor(() => expect(field).toHaveFocus());
  });

  it("ACT-FR-14, EDT-FR-61: the column's opening composer keeps its text across a switch of mode", async () => {
    const b: Backend = { discussions: [], calls: [] };
    await renderEditor(b);
    await userEvent.type(
      within(column()).getByRole("textbox", { name: "Discuss this file" }),
      "kept across modes",
    );

    await userEvent.click(screen.getByRole("button", { name: "Edit as Markdown source" }));

    expect(
      within(column()).getByRole("textbox", { name: "Discuss this file" }),
    ).toHaveValue("kept across modes");
    expect(calls(b, "open_discussion")).toHaveLength(0);
  });
});

describe("ACT-FR-16, ACT-FR-17: posting the column's opening composer", () => {
  it("ACT-FR-16, ACT-FR-LXKD, ACT-FR-19: a post opens a whole-target discussion and shows it in the column, with focus in its reply composer", async () => {
    const b: Backend = { discussions: [], calls: [] };
    await renderEditor(b, { artifactType: "doc" });
    const composer = within(column()).getByTestId("draft-open-discussion");
    await userEvent.type(within(composer).getByRole("textbox"), "what next?");
    await userEvent.click(within(composer).getByRole("button", { name: "Post" }));

    expect(await within(column()).findByTestId("comment-thread-d-new")).toBeInTheDocument();
    await waitFor(() => expect(openingComposer()).toBeNull());
    expect(calls(b, "open_discussion")).toHaveLength(1);
    expect(calls(b, "open_discussion")[0]).toMatchObject({
      target: { kind: "artifact", artifactId: "notes.md" },
      fragmentTarget: null,
    });
    // A message that addresses nobody dispatches no turn.
    expect(calls(b, "dispatch_agent_turn")).toHaveLength(0);
    expectNoFloatingRoute();
    // ACT-FR-16: focus lands in the reply composer of the new discussion.
    await waitFor(() =>
      expect(screen.getByRole("textbox", { name: "Reply to thread d-new" })).toHaveFocus(),
    );
    // The new discussion is one tab of the chooser.
    expect(
      within(within(column()).getByRole("tablist", { name: "Discussions" })).getAllByRole(
        "tab",
      ),
    ).toHaveLength(1);
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
    const composer = within(column()).getByTestId("draft-open-discussion");
    await userEvent.type(within(composer).getByRole("textbox"), "once only");
    const post = within(composer).getByRole("button", { name: "Post" });
    await userEvent.click(post);
    await userEvent.click(post);
    fireEvent.keyDown(within(composer).getByRole("textbox"), { key: "Enter", ctrlKey: true });
    expect(calls(b, "open_discussion")).toHaveLength(1);
    release();
    await waitFor(() => expect(openingComposer()).toBeNull());
    expect(calls(b, "open_discussion")).toHaveLength(1);
  });

  it("ACT-FR-17, CMT-FR-34: a refused post keeps the composer, the body, and the error, and a press outside while it is in flight leaves them", async () => {
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
    const composer = within(column()).getByTestId("draft-open-discussion");
    await userEvent.type(within(composer).getByRole("textbox"), "keep me");
    await userEvent.click(within(composer).getByRole("button", { name: "Post" }));

    fireEvent.mouseDown(document.querySelector(".editor-tab") as HTMLElement);
    expect(openingComposer()).not.toBeNull();
    release();

    await waitFor(() =>
      expect(within(composer).getByRole("button", { name: "Post" })).not.toBeDisabled(),
    );
    expect(openingComposer()).not.toBeNull();
    expect(within(composer).getByRole("textbox")).toHaveValue("keep me");
    expect(within(composer).getByRole("alert")).toBeInTheDocument();
  });

  it("ACT-FR-17, CMT-FR-24: with no identity the opening composer is disabled and opens nothing", async () => {
    const b: Backend = {
      discussions: [],
      calls: [],
      identityError: "github_token_selection_required",
    };
    await renderEditor(b);
    const composer = within(column()).getByTestId("draft-open-discussion");
    await waitFor(() => expect(within(composer).getByRole("textbox")).toBeDisabled());
    expect(within(composer).getByRole("button", { name: "Post" })).toBeDisabled();
    fireEvent.keyDown(within(composer).getByRole("textbox"), { key: "Enter", ctrlKey: true });
    expect(calls(b, "open_discussion")).toHaveLength(0);
  });
});
