// The Notes vertical panel (`specifications/ui/NTS-notes.md`), covering
// NTS-FR-26 … NTS-FR-31. The backend is mocked at `invoke`, so every assertion
// here is about what the panel renders and which operation it invokes — the
// record, its timestamps and the unresolved marking belong to
// `NTC-notes-storage.md` and are covered by its own Rust tests.
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

import { Notes } from "./Notes";
import type { DiscussionReveal } from "../state/revealDiscussion";
import type { Discussion, NoteListItem, NotesPanelState } from "../types";
import {
  ARTIFACT_A,
  entityNote,
  Harness,
  openMenu,
  projectNote,
  rowFor,
  rows,
  TREE,
} from "../test/notesFixtures";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

/**
 * The `"discussion changed"` listeners the panel installs, so a test can
 * deliver a payload the way the backend does (NTS-FR-28).
 */
const threadListeners = new Set<(thread: Discussion) => void>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: (_event: string, handler: (ev: { payload: Discussion }) => void) => {
    const fn = (thread: Discussion) => handler({ payload: thread });
    threadListeners.add(fn);
    return Promise.resolve(() => threadListeners.delete(fn));
  },
}));

function emitThreadChanged(thread: Discussion): void {
  for (const fn of [...threadListeners]) fn(thread);
}

/** What the mocked backend serves, per scope, for the test being run. */
let served: {
  entity: NoteListItem[];
  project: NoteListItem[];
  all: NoteListItem[];
  panelState: NotesPanelState;
};

beforeEach(() => {
  invokeMock.mockReset();
  served = {
    entity: [],
    project: [],
    all: [],
    panelState: { scopePosition: "entity", textFilter: "" },
  };
  invokeMock.mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case "load_notes_panel_state":
        return served.panelState;
      case "save_notes_panel_state":
        return undefined;
      case "list_notes_for_entity":
        return served.entity;
      case "list_project_notes":
        return served.project;
      case "list_all_notes":
        return served.all;
      case "load_project_tree":
        return TREE;
      case "create_note":
        return {
          id: "new",
          scope: { kind: "project" },
          body: "",
          createdAt: "2026-05-09T09:00:00Z",
          updatedAt: "2026-05-09T09:00:00Z",
        };
      case "update_note":
        return undefined;
      case "delete_note":
        return undefined;
      default:
        throw new Error(`unexpected command ${cmd}`);
    }
  });
});

afterEach(() => {
  cleanup();
});

function calls(cmd: string): unknown[][] {
  return invokeMock.mock.calls.filter((c) => c[0] === cmd);
}

// ---------------------------------------------------------------------------
// NTS-FR-26, NTS-FR-27, CVL-FR-03 … NTS-FR-31, NTS-FR-28, AGC-FR-07 — Discuss, and deleting a note that carries one
// ---------------------------------------------------------------------------

describe("Discuss (NTS-FR-26 … NTS-FR-31)", () => {
  beforeEach(() => {
    served.project = [projectNote("1", "the rail's floor is wrong", "2026-05-01T09:00:00Z")];
  });

  const discuss = async (body: string) =>
    userEvent.click(within(await openMenu(body)).getByText("Discuss"));

  it("CVP-FR-57: hands the route the note's label with its Markdown read as text", async () => {
    served.project = [
      projectNote("1", "Ship the **drafts panel** before the handoff", "2026-05-01T09:00:00Z"),
    ];
    const onReveal = vi.fn();
    render(<Harness entity={null} onReveal={onReveal} />);
    await screen.findByText(/Ship the/);

    await discuss("Ship the");

    // The label leads with the note rather than with whatever it is filed
    // against, and carries no asterisks — a tab name and a spoken announcement
    // cannot render marks.
    expect(onReveal).toHaveBeenCalledTimes(1);
    const reveal: DiscussionReveal = onReveal.mock.calls[0][0];
    expect(reveal.ownerLabel).toContain("Ship the drafts panel");
    expect(reveal.ownerLabel).not.toContain("**");
    expect(reveal.target).toEqual({ kind: "note", noteId: "1" });
  });

  it("NTS-FR-28: learns the association the moment the discussion is created", async () => {
    // Without this the panel's list still says the note has no discussion after
    // its opening composer posted — so choosing Discuss again opens a *fresh*
    // composer and the message typed into it is silently dropped, because
    // `get_or_create` is idempotent and appends nothing (CMS-FR-62).
    const onReveal = vi.fn();
    render(<Harness entity={null} onReveal={onReveal} />);
    await screen.findByText("the rail's floor is wrong");

    await discuss("the rail's floor is wrong");
    expect(onReveal.mock.calls[0][0].discussionId).toBeUndefined();

    // The conversation tab's composer posts; the backend announces the write.
    act(() => {
      emitThreadChanged({
        id: "nt1",
        target: { kind: "note", noteId: "1" },
        fragmentTarget: null,
        comments: [],
        locked: false,
        resolved: false,
        createdAt: "2026-05-01T10:00:00Z",
        updatedAt: "2026-05-01T10:00:00Z",
      });
    });

    // Choosing Discuss again now reaches that conversation rather than a new one.
    await discuss("the rail's floor is wrong");
    expect(onReveal).toHaveBeenCalledTimes(2);
    expect(onReveal.mock.calls[1][0].discussionId).toBe("nt1");
    expect(calls("list_project_notes").length).toBeLessThanOrEqual(2);
  });

  it("ignores a thread-changed payload that is not this note's", async () => {
    const onReveal = vi.fn();
    render(<Harness entity={null} onReveal={onReveal} />);
    await screen.findByText("the rail's floor is wrong");

    act(() => {
      // An artifact thread, and a note thread for a different note.
      emitThreadChanged({
        id: "at1",
        target: { kind: "artifact", artifactId: "specs/a.md" },
        fragmentTarget: null,
        comments: [],
        locked: false,
        resolved: false,
        createdAt: "2026-05-01T10:00:00Z",
        updatedAt: "2026-05-01T10:00:00Z",
      });
      emitThreadChanged({
        id: "nt9",
        target: { kind: "note", noteId: "other-note" },
        fragmentTarget: null,
        comments: [],
        locked: false,
        resolved: false,
        createdAt: "2026-05-01T10:00:00Z",
        updatedAt: "2026-05-01T10:00:00Z",
      });
    });

    await discuss("the rail's floor is wrong");
    // Still no discussion of its own, so still the opening composer.
    expect(onReveal.mock.calls[0][0].discussionId).toBeUndefined();
  });

  it("NTS-FR-26, NTS-FR-27: calls the route once for a note with no discussion and creates nothing", async () => {
    const onReveal = vi.fn();
    render(<Harness entity={null} onReveal={onReveal} />);
    await screen.findByText("the rail's floor is wrong");

    await discuss("the rail's floor is wrong");

    expect(onReveal).toHaveBeenCalledTimes(1);
    expect(onReveal.mock.calls[0][0]).toEqual(
      expect.objectContaining({
        discussionId: undefined,
        target: { kind: "note", noteId: "1" },
      }),
    );
    // NTS-FR-27: nothing was created — no discussion, no association, no
    // opening comment, and no agent turn.
    expect(calls("get_or_create_note_discussion")).toHaveLength(0);
    expect(calls("open_discussion")).toHaveLength(0);
    expect(calls("dispatch_agent_turn")).toHaveLength(0);
  });

  it("NTS-FR-26, NTS-FR-28: names the existing conversation without creating anything", async () => {
    served.project = [
      {
        ...projectNote("1", "the rail's floor is wrong", "2026-05-01T09:00:00Z"),
        discussionThreadId: "nt1",
      },
    ];
    const onReveal = vi.fn();
    render(<Harness entity={null} onReveal={onReveal} />);
    await screen.findByText("the rail's floor is wrong");

    await discuss("the rail's floor is wrong");

    // NTS-FR-28: the row already knew, so nothing was invoked to find out.
    expect(onReveal).toHaveBeenCalledWith(
      expect.objectContaining({ discussionId: "nt1" }),
    );
    expect(calls("get_or_create_note_discussion")).toHaveLength(0);
    expect(calls("dispatch_agent_turn")).toHaveLength(0);
  });

  it("NTS-FR-26, NTS-FR-23: is offered on every note the panel renders", async () => {
    served.all = [
      entityNote("1", "specs/a.md", "on an artifact", "2026-05-01T09:00:00Z"),
      projectNote("2", "project-wide", "2026-05-01T08:00:00Z"),
      entityNote("3", "specs/gone.md", "unresolved", "2026-05-01T07:00:00Z", {
        unresolved: true,
      }),
    ];
    served.panelState = { scopePosition: "all", textFilter: "" };
    render(<Harness entity={ARTIFACT_A} />);
    await screen.findByText("unresolved");

    for (const body of ["on an artifact", "project-wide", "unresolved"]) {
      const menu = await openMenu(body);
      expect(within(menu).getByText("Discuss")).toBeInTheDocument();
      await userEvent.keyboard("{Escape}");
    }
  });

  it("NTS-FR-29, CVP-FR-59: renders no conversation anywhere in the panel", async () => {
    served.project = [
      {
        ...projectNote("1", "the rail's floor is wrong", "2026-05-01T09:00:00Z"),
        discussionThreadId: "nt1",
      },
    ];
    const { container } = render(<Harness entity={null} onReveal={vi.fn()} />);
    await screen.findByText("the rail's floor is wrong");

    await discuss("the rail's floor is wrong");

    // NTS-FR-29: no card, no placeholder, no composer, no textarea beyond the
    // panel's own, and no conversation of any kind inside the panel — the row
    // reads exactly as any other row does.
    expect(container.querySelector(".comment-card")).toBeNull();
    expect(container.querySelector(".comment-card--placeholder")).toBeNull();
    expect(container.querySelector(".comment-card__reply")).toBeNull();
    expect(container.querySelector(".comment-composer")).toBeNull();
    expect(container.querySelector("[data-testid='note-discussion-opener']")).toBeNull();
    expect(container.querySelector("[data-testid^='comment-thread-']")).toBeNull();
    // NTS-FR-29: and no row indicates its conversation's unread or pending state.
    expect(rowFor("the rail's floor is wrong").textContent).not.toMatch(/unread|waiting/i);
    expect(rows()).toHaveLength(1);
  });

  it("NTS-FR-21, NTS-FR-30, NTC-FR-21: deletion reports the note to the shell on success alone", async () => {
    served.project = [
      {
        ...projectNote("1", "the rail's floor is wrong", "2026-05-01T09:00:00Z"),
        discussionThreadId: "nt1",
      },
    ];
    const onNoteDeleted = vi.fn();
    render(<Harness entity={null} onReveal={vi.fn()} onNoteDeleted={onNoteDeleted} />);
    await screen.findByText("the rail's floor is wrong");

    await userEvent.click(
      within(await openMenu("the rail's floor is wrong")).getByText("Delete"),
    );
    served.project = [];
    await userEvent.click(screen.getByRole("button", { name: "Delete" }));

    await waitFor(() => expect(calls("delete_note")).toHaveLength(1));
    // CVP-FR-61: the conversation tab is closed on the backend's reported success.
    await waitFor(() => expect(onNoteDeleted).toHaveBeenCalledWith("1"));
    await waitFor(() => expect(rows()).toHaveLength(0));
  });

  it("NTS-FR-21, NTS-FR-30, NTC-FR-21: a failed cleanup keeps the row and the error, and closes nothing", async () => {
    served.project = [
      {
        ...projectNote("1", "the rail's floor is wrong", "2026-05-01T09:00:00Z"),
        discussionThreadId: "nt1",
      },
    ];
    const onNoteDeleted = vi.fn();
    render(<Harness entity={null} onReveal={vi.fn()} onNoteDeleted={onNoteDeleted} />);
    await screen.findByText("the rail's floor is wrong");

    // NTC-FR-21: the transaction refuses, having removed nothing.
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "delete_note") throw "discussion_cleanup_failed";
      if (cmd === "list_project_notes") return served.project;
      if (cmd === "load_notes_panel_state") return served.panelState;
      return undefined;
    });

    await userEvent.click(
      within(await openMenu("the rail's floor is wrong")).getByText("Delete"),
    );
    await userEvent.click(screen.getByRole("button", { name: "Delete" }));

    await waitFor(() => expect(calls("delete_note")).toHaveLength(1));
    // NTS-FR-30: the typed error renders inline, the row stays, and nothing
    // reported the note or its conversation deleted.
    await waitFor(() =>
      expect(document.querySelector(".note__error")).toBeInTheDocument(),
    );
    expect(rows()).toHaveLength(1);
    expect(onNoteDeleted).not.toHaveBeenCalled();
  });

  it("NTS-FR-24: the panel's overlays work after Discuss, a tab being no overlay of the panel", async () => {
    served.project = [
      {
        ...projectNote("1", "the rail's floor is wrong", "2026-05-01T09:00:00Z"),
        discussionThreadId: "nt1",
      },
    ];
    render(<Harness entity={null} onReveal={vi.fn()} />);
    await screen.findByText("the rail's floor is wrong");

    await discuss("the rail's floor is wrong");

    await openMenu("the rail's floor is wrong");
    await userEvent.click(screen.getByText("Move…"));
    expect(await screen.findByText("Project level")).toBeInTheDocument();
    await userEvent.keyboard("{Escape}");

    await userEvent.click(
      within(await openMenu("the rail's floor is wrong")).getByText("Delete"),
    );
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(rows()).toHaveLength(1);
  });

  it("NTS-FR-31, NTS-FR-28, AGC-FR-07: everything else about a note is untouched by having one", async () => {
    served.project = [
      {
        ...projectNote("1", "the rail's floor is wrong", "2026-05-01T09:00:00Z"),
        discussionThreadId: "nt1",
      },
    ];
    render(<Harness entity={null} />);
    await screen.findByText("the rail's floor is wrong");

    // NTS-FR-31: editing behaves identically and invokes no discussion operation.
    await userEvent.click(
      within(await openMenu("the rail's floor is wrong")).getByText("Edit"),
    );
    const field = screen.getByLabelText("Edit note");
    await userEvent.clear(field);
    await userEvent.type(field, "edited");
    served.project = [
      {
        ...projectNote("1", "edited", "2026-05-01T10:00:00Z"),
        discussionThreadId: "nt1",
      },
    ];
    await userEvent.click(screen.getByRole("button", { name: "Save note" }));

    await waitFor(() => expect(calls("update_note")).toHaveLength(1));
    expect(calls("get_or_create_note_discussion")).toHaveLength(0);
    expect(calls("delete_note")).toHaveLength(0);
    await screen.findByText("edited");

    // NTS-FR-31: and so do move, the reminder pair, and the filter — none of
    // them invokes a discussion operation, and the association is unchanged
    // throughout.
    await userEvent.click(within(await openMenu("edited")).getByText("Move…"));
    await userEvent.click(await screen.findByText("Project level"));
    await waitFor(() => expect(calls("update_note")).toHaveLength(2));

    await userEvent.click(
      within(await openMenu("edited")).getByText("Set Reminder…"),
    );
    fireEvent.change(screen.getByLabelText("Reminder"), {
      target: { value: "2026-06-01T09:00" },
    });
    await userEvent.click(screen.getByRole("button", { name: "Set" }));
    await waitFor(() => expect(calls("update_note")).toHaveLength(3));

    await userEvent.type(screen.getByLabelText("Filter notes"), "edit");
    expect(rows()).toHaveLength(1);

    expect(calls("get_or_create_note_discussion")).toHaveLength(0);
    expect(calls("open_discussion")).toHaveLength(0);
    expect(calls("delete_note")).toHaveLength(0);
  });
});
