// The Notes vertical panel (`specifications/ui/NTS-notes.md`), covering
// NTS-FR-01, NTS-FR-02 … NTS-FR-24. The backend is mocked at `invoke`, so every assertion
// here is about what the panel renders and which operation it invokes — the
// record, its timestamps and the unresolved marking belong to
// `NTC-notes-storage.md` and are covered by its own Rust tests.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Notes } from "./Notes";
import type { Discussion, NoteListItem, NotesPanelState } from "../types";
import {
  ARTIFACT_A,
  ARTIFACT_B,
  entityNote,
  Harness,
  openMenu,
  projectNote,
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
// Emptying a note out
//
// A note may hold no text at all. Save writes whatever the editor holds,
// including nothing (NTS-FR-16), and the note survives it — the row renders the
// empty-note placeholder in place of a body. Removing a note is Delete's job,
// and only Delete's (NTS-FR-21).
// ---------------------------------------------------------------------------

describe("emptying a note (NTS-FR-16)", () => {
  const EMPTY_ROW = "This note is empty";

  /** The blank note a create writes, before anything has been typed into it. */
  const BLANK_NEW = projectNote("new", "", "2026-05-09T09:00:00Z");

  /** When a write lands, the instant the backend stamps onto it (NTC-FR-06). */
  const WRITTEN_AT = "2026-05-20T09:00:00Z";

  /**
   * A backend whose writes actually land: an updated body and a deleted note are
   * both reflected in what the next load returns. Without this the tests would
   * stage the post-write list themselves, and what they assert about the row
   * afterwards would hold under any behaviour at all.
   *
   * A body that actually changes also re-stamps `updatedAt`, as `update_note_in`
   * does — otherwise the ordering a write causes (NTS-FR-15) would be invisible
   * here, and a test could assert an order the real backend never produces.
   */
  function serveMutable(): void {
    invokeMock.mockImplementation(async (cmd: string, payload?: unknown) => {
      const p = payload as
        | { id?: string; fields?: { body?: string } }
        | undefined;
      switch (cmd) {
        case "load_notes_panel_state":
          return served.panelState;
        case "list_notes_for_entity":
          return served.entity;
        case "list_project_notes":
          return served.project;
        case "list_all_notes":
          return served.all;
        case "load_project_tree":
          return TREE;
        case "create_note":
          return BLANK_NEW.note;
        case "update_note": {
          const body = p?.fields?.body;
          if (body === undefined) return undefined;
          const written = (i: NoteListItem) =>
            i.note.id === p?.id
              ? {
                  ...i,
                  note: {
                    ...i.note,
                    body,
                    // Only a real change is stamped, so a re-save of the same
                    // body leaves the order alone.
                    updatedAt:
                      body === i.note.body ? i.note.updatedAt : WRITTEN_AT,
                  },
                }
              : i;
          served.entity = served.entity.map(written);
          served.project = served.project.map(written);
          served.all = served.all.map(written);
          return undefined;
        }
        case "delete_note": {
          const gone = (i: NoteListItem) => i.note.id !== p?.id;
          served.entity = served.entity.filter(gone);
          served.project = served.project.filter(gone);
          served.all = served.all.filter(gone);
          return undefined;
        }
        default:
          return undefined;
      }
    });
  }

  /** Open the named row's editor through the overflow menu and empty it. */
  async function emptyEditorOf(body: string): Promise<void> {
    await userEvent.click(within(await openMenu(body)).getByText("Edit"));
    await userEvent.clear(screen.getByLabelText("Edit note"));
  }

  function save(): Promise<void> {
    return userEvent.click(screen.getByRole("button", { name: "Save note" }));
  }

  it("writes the empty body and keeps the note, saying so on the row", async () => {
    served.project = [
      projectNote("1", "stays as is", "2026-05-02T09:00:00Z"),
      projectNote("2", "about to be emptied", "2026-05-01T09:00:00Z"),
    ];
    serveMutable();
    render(<Harness entity={null} />);
    await screen.findByText("about to be emptied");

    await emptyEditorOf("about to be emptied");
    await save();

    // The empty body is written onto the edited note — and onto that one only,
    // not whichever row happens to be at the top of the list.
    await waitFor(() => expect(calls("update_note")).toHaveLength(1));
    expect(calls("update_note")[0][1]).toEqual({ id: "2", fields: { body: "" } });
    // Emptying is not deleting: nothing is removed.
    expect(calls("delete_note")).toHaveLength(0);

    // Both rows are still listed. The emptied one says it is empty, in place of
    // the body it no longer has; the other is untouched.
    await waitFor(() => expect(rows()).toHaveLength(2));
    expect(screen.getByText("stays as is")).toBeInTheDocument();
    const emptied = document.querySelector('[data-note-id="2"]') as HTMLElement;
    // The placeholder is marked as such, which is what centres it and sets it
    // apart from a note whose body happens to read like this.
    expect(within(emptied).getByText(EMPTY_ROW)).toHaveClass("note__text--empty");
    // The editor closed onto the row rather than staying open over it.
    expect(screen.queryByLabelText("Edit note")).not.toBeInTheDocument();
    expect(emptied).toHaveAttribute("data-editing", "false");
    // Emptying is an edit, so the note is stamped and leads the list
    // (NTS-FR-15 / NTC-FR-06) rather than staying where it was.
    expect(rows()[0]).toBe(emptied);
  });

  it("does not reorder anything when an already-empty note is saved again", async () => {
    // NTC-FR-06: the stamp follows a change, not a submission. Re-committing the
    // same empty body must leave the row where it is.
    served.project = [
      projectNote("1", "newer", "2026-05-02T09:00:00Z"),
      projectNote("2", "", "2026-05-01T09:00:00Z"),
    ];
    serveMutable();
    render(<Harness entity={null} />);
    await screen.findByText(EMPTY_ROW);
    expect(rows()[1]).toHaveAttribute("data-note-id", "2");

    await userEvent.click(
      within(rows()[1]).getByLabelText("Note actions"),
    );
    await userEvent.click(
      within(document.querySelector(".menu") as HTMLElement).getByText("Edit"),
    );
    await save();

    await waitFor(() => expect(calls("update_note")).toHaveLength(1));
    expect(calls("update_note")[0][1]).toEqual({ id: "2", fields: { body: "" } });
    await waitFor(() =>
      expect(screen.queryByLabelText("Edit note")).not.toBeInTheDocument(),
    );
    expect(rows()[1]).toHaveAttribute("data-note-id", "2");
  });

  it("leaves the emptied note editable and deletable through its menu", async () => {
    // The placeholder is text the panel renders, not a state the note is stuck
    // in: the row keeps its overflow menu, so it can be typed into again or
    // removed for real.
    served.project = [projectNote("1", "about to be emptied", "2026-05-01T09:00:00Z")];
    serveMutable();
    render(<Harness entity={null} />);
    await screen.findByText("about to be emptied");

    await emptyEditorOf("about to be emptied");
    await save();
    await waitFor(() => expect(screen.getByText(EMPTY_ROW)).toBeInTheDocument());

    // Reopened, the editor is empty — the placeholder is not its content.
    await userEvent.click(screen.getByLabelText("Note actions"));
    await userEvent.click(
      within(document.querySelector(".menu") as HTMLElement).getByText("Edit"),
    );
    expect(screen.getByLabelText("Edit note")).toHaveValue("");

    // And typing into it brings the note back to an ordinary one.
    await userEvent.type(screen.getByLabelText("Edit note"), "words again");
    await save();
    await waitFor(() =>
      expect(screen.getByText("words again")).toBeInTheDocument(),
    );
    expect(calls("update_note")[1][1]).toEqual({
      id: "1",
      fields: { body: "words again" },
    });
    expect(screen.queryByText(EMPTY_ROW)).not.toBeInTheDocument();
  });

  it("is removed for real through the menu's Delete, which is the only way", async () => {
    // Delete is the escape hatch the whole design leans on now that Save never
    // removes anything, so it has to work from an empty row.
    served.project = [projectNote("1", "about to be emptied", "2026-05-01T09:00:00Z")];
    serveMutable();
    render(<Harness entity={null} />);
    await screen.findByText("about to be emptied");

    await emptyEditorOf("about to be emptied");
    await save();
    await waitFor(() => expect(screen.getByText(EMPTY_ROW)).toBeInTheDocument());

    // NTS-FR-19: the empty row's menu is the same five entries as any other's.
    await userEvent.click(screen.getByLabelText("Note actions"));
    const menu = document.querySelector(".menu") as HTMLElement;
    expect(
      Array.from(menu.querySelectorAll(".menu-item")).map((el) => el.textContent),
    ).toEqual(["Discuss", "Edit", "Move…", "Set Reminder…", "Delete"]);

    // NTS-FR-21: confirmed, and then actually gone.
    await userEvent.click(within(menu).getByText("Delete"));
    expect(screen.getByText(/Delete “this empty note”\?/)).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Delete" }));

    await waitFor(() => expect(calls("delete_note")).toHaveLength(1));
    expect(calls("delete_note")[0][1]).toEqual({ id: "1" });
    await waitFor(() => expect(rows()).toHaveLength(0));
  });

  it("is an ordinary note afterwards, not a never-written one", async () => {
    // The reap exists for a note created and abandoned without ever being typed
    // into. Committing an empty body is not that — so every path that reaps must
    // leave this note alone. If the reap ever keyed off the stored body instead
    // of the marker, this is what would catch it.
    served.project = [projectNote("1", "about to be emptied", "2026-05-01T09:00:00Z")];
    serveMutable();
    const { unmount } = render(<Harness entity={null} />);
    await screen.findByText("about to be emptied");

    await emptyEditorOf("about to be emptied");
    await save();
    await waitFor(() => expect(screen.getByText(EMPTY_ROW)).toBeInTheDocument());

    // Reopened and cancelled: nothing removed.
    await userEvent.click(screen.getByLabelText("Note actions"));
    await userEvent.click(
      within(document.querySelector(".menu") as HTMLElement).getByText("Edit"),
    );
    await userEvent.click(screen.getByRole("button", { name: "Cancel edit" }));
    expect(calls("delete_note")).toHaveLength(0);
    expect(screen.getByText(EMPTY_ROW)).toBeInTheDocument();

    // Reopened and filtered out from under the editor: still nothing removed,
    // even though an empty body matches no filter text.
    await userEvent.click(screen.getByLabelText("Note actions"));
    await userEvent.click(
      within(document.querySelector(".menu") as HTMLElement).getByText("Edit"),
    );
    await userEvent.type(screen.getByLabelText("Filter notes"), "nothing");
    await waitFor(() =>
      expect(screen.queryByLabelText("Edit note")).not.toBeInTheDocument(),
    );
    expect(calls("delete_note")).toHaveLength(0);

    // And the panel going away takes nothing with it.
    unmount();
    expect(calls("delete_note")).toHaveLength(0);
  });

  it("stores a whitespace-only body as typed, and still calls the row empty", async () => {
    // What the store holds is what the user committed; whether the row reads as
    // empty is a rendering decision, and spaces and newlines are not content.
    served.panelState = { scopePosition: "entity", textFilter: "" };
    served.entity = [entityNote("1", "specs/a.md", "on A", "2026-05-01T09:00:00Z")];
    serveMutable();
    render(<Harness entity={ARTIFACT_A} />);
    await screen.findByText("on A");
    const before = calls("list_notes_for_entity").length;

    await emptyEditorOf("on A");
    await userEvent.type(screen.getByLabelText("Edit note"), "  {Enter} ");
    expect(screen.getByLabelText("Edit note")).toHaveValue("  \n ");
    await save();

    await waitFor(() => expect(calls("update_note")).toHaveLength(1));
    expect(calls("update_note")[0][1]).toEqual({
      id: "1",
      fields: { body: "  \n " },
    });
    expect(calls("delete_note")).toHaveLength(0);
    expect(await screen.findByText(EMPTY_ROW)).toBeInTheDocument();
    // The reload follows the position the panel is in rather than defaulting
    // elsewhere, so the surviving row is the one this scope actually holds.
    await waitFor(() =>
      expect(calls("list_notes_for_entity").length).toBeGreaterThan(before),
    );
    expect(calls("list_project_notes")).toHaveLength(0);
  });

  it("keeps the note's group in the all-notes position", async () => {
    // The note is still attached to its entity, so its group stands — an empty
    // note is not a note that has gone anywhere.
    served.panelState = { scopePosition: "all", textFilter: "" };
    served.all = [
      entityNote("1", "specs/a.md", "on A", "2026-05-02T09:00:00Z"),
      entityNote("2", "specs/b.md", "on B", "2026-05-01T09:00:00Z"),
    ];
    serveMutable();
    render(<Harness entity={null} />);
    await screen.findByText("on B");

    await emptyEditorOf("on B");
    await save();

    await waitFor(() => expect(calls("update_note")).toHaveLength(1));
    expect(calls("update_note")[0][1]).toEqual({ id: "2", fields: { body: "" } });
    await waitFor(() => expect(screen.getByText(EMPTY_ROW)).toBeInTheDocument());
    expect(rows()).toHaveLength(2);
    // Both groups stand. B's leads now, because emptying stamped its note and
    // groups are ordered by their most-recently-edited one (NTS-FR-15).
    expect(
      Array.from(document.querySelectorAll(".note-group__header")).map(
        (el) => el.textContent,
      ),
    ).toEqual(["b.md", "a.md"]);
  });

  it("keeps an unresolved note unresolved when it is emptied (NTS-FR-23)", async () => {
    served.panelState = { scopePosition: "all", textFilter: "" };
    served.all = [
      entityNote("1", "specs/gone.md", "orphan", "2026-05-01T09:00:00Z", {
        unresolved: true,
      }),
    ];
    serveMutable();
    render(<Harness entity={null} />);
    await screen.findByText("orphan");

    await emptyEditorOf("orphan");
    await save();

    await waitFor(() => expect(screen.getByText(EMPTY_ROW)).toBeInTheDocument());
    // The row still names the entity it lost, so it is still reattachable
    // through Move…, and it is still in the Unresolved group.
    expect(screen.getByText("specs/gone.md")).toBeInTheDocument();
    expect(
      Array.from(document.querySelectorAll(".note-group__header")).map(
        (el) => el.textContent,
      ),
    ).toEqual(["Unresolved"]);
    expect(calls("delete_note")).toHaveLength(0);
  });

  it("keeps a reminder and a revision badge on an emptied row", async () => {
    // The write carries only the body, so everything else about the note stands —
    // and the row goes on rendering it beside the placeholder.
    served.project = [
      {
        note: {
          id: "1",
          scope: { kind: "project" },
          body: "about to be emptied",
          createdAt: "2026-01-01T09:00:00Z",
          updatedAt: "2026-05-01T09:00:00Z",
          reminder: "2026-06-01T09:00:00Z",
          revision: "abc1234",
        },
        unresolved: false,
      },
    ];
    serveMutable();
    render(<Harness entity={null} />);
    await screen.findByText("about to be emptied");

    await emptyEditorOf("about to be emptied");
    await save();

    await waitFor(() => expect(screen.getByText(EMPTY_ROW)).toBeInTheDocument());
    const row = rows()[0];
    expect(row.querySelector(".note__reminder")).not.toBeNull();
    expect(within(row).getByText("@ abc1234")).toBeInTheDocument();
    // NTS-FR-07: still marked as attached to a historical revision.
    expect(row).toHaveAttribute("data-historical", "true");
    // NTS-FR-20: the menu still offers to clear the reminder it still carries.
    await userEvent.click(within(row).getByLabelText("Note actions"));
    expect(
      within(document.querySelector(".menu") as HTMLElement).getByText(
        "Clear Reminder",
      ),
    ).toBeInTheDocument();
  });

  it("is not an empty panel just because every note in it is empty", async () => {
    // `visible` counts empty notes, so the panel must not claim it holds nothing
    // while holding two notes — nor centre its no-notes message over them.
    served.project = [
      projectNote("1", "", "2026-05-02T09:00:00Z"),
      projectNote("2", "   ", "2026-05-01T09:00:00Z"),
    ];
    render(<Harness entity={null} />);

    await waitFor(() => expect(rows()).toHaveLength(2));
    expect(screen.getAllByText(EMPTY_ROW)).toHaveLength(2);
    expect(screen.queryByText("No project-wide notes yet.")).not.toBeInTheDocument();
    expect(document.querySelector(".notes__body--empty")).toBeNull();
  });

  it("commits a brand-new note saved without a body, and stops reaping it", async () => {
    // A create writes the note before anything is typed into it, and the reap
    // takes an abandoned one away again. Saving is not abandoning: the note is
    // committed empty, and nothing chases it afterwards — including the unmount.
    served.project = [];
    serveMutable();
    const { unmount } = render(<Harness entity={null} />);
    await screen.findByLabelText("Notes scope");
    served.project = [BLANK_NEW];
    await userEvent.click(screen.getByRole("button", { name: "New note" }));
    await screen.findByLabelText("Edit note");

    await save();

    await waitFor(() => expect(calls("update_note")).toHaveLength(1));
    expect(calls("update_note")[0][1]).toEqual({
      id: "new",
      fields: { body: "" },
    });
    expect(await screen.findByText(EMPTY_ROW)).toBeInTheDocument();
    expect(calls("delete_note")).toHaveLength(0);

    unmount();

    // The unmount reap is the last of the six editor-closing paths; a committed
    // note must not be caught by it.
    expect(calls("delete_note")).toHaveLength(0);
  });

  it("hides an emptied note from an active filter without removing it", async () => {
    // An empty body matches no filter text, so the row leaves the filtered view
    // the moment it is committed. The note itself is untouched, and clearing the
    // filter brings it back.
    served.panelState = { scopePosition: "project", textFilter: "beta" };
    served.project = [
      projectNote("1", "alpha note", "2026-05-02T09:00:00Z"),
      projectNote("2", "beta note", "2026-05-01T09:00:00Z"),
    ];
    serveMutable();
    render(<Harness entity={null} />);
    await screen.findByText("beta note");

    await emptyEditorOf("beta note");
    await save();

    await waitFor(() => expect(rows()).toHaveLength(0));
    expect(calls("delete_note")).toHaveLength(0);
    expect(screen.getByText("No note matches the filter.")).toBeInTheDocument();

    await userEvent.clear(screen.getByLabelText("Filter notes"));
    await waitFor(() => expect(rows()).toHaveLength(2));
    expect(screen.getByText(EMPTY_ROW)).toBeInTheDocument();
    expect(screen.getByText("alpha note")).toBeInTheDocument();
  });

  it("keeps an emptied note visible under a filter its entity name matches", async () => {
    // NTS-FR-12: the all-notes position matches the entity name too, so an empty
    // body is not the only thing the filter has to go on there. The rule above is
    // the project position's, not a general one.
    served.panelState = { scopePosition: "all", textFilter: "a.md" };
    served.all = [entityNote("1", "specs/a.md", "on A", "2026-05-01T09:00:00Z")];
    serveMutable();
    render(<Harness entity={null} />);
    await screen.findByText("on A");

    await emptyEditorOf("on A");
    await save();

    await waitFor(() => expect(screen.getByText(EMPTY_ROW)).toBeInTheDocument());
    expect(rows()).toHaveLength(1);
  });

  it("keeps the stored body when the cleared editor is cancelled", async () => {
    // Only Save commits an emptying, so an accidental clear costs nothing.
    served.project = [projectNote("1", "still here", "2026-05-01T09:00:00Z")];
    render(<Harness entity={null} />);
    await screen.findByText("still here");

    await emptyEditorOf("still here");
    await userEvent.click(screen.getByRole("button", { name: "Cancel edit" }));

    expect(calls("update_note")).toHaveLength(0);
    expect(calls("delete_note")).toHaveLength(0);
    // A rendered row has no textarea, so the text being here proves both that
    // the editor closed and that the stored body survived.
    expect(screen.getByText("still here")).toBeInTheDocument();
    expect(screen.queryByText(EMPTY_ROW)).not.toBeInTheDocument();

    // The same on Escape, which is Cancel by another key.
    await emptyEditorOf("still here");
    await userEvent.type(screen.getByLabelText("Edit note"), "{Escape}");
    expect(calls("update_note")).toHaveLength(0);
    expect(calls("delete_note")).toHaveLength(0);
    expect(screen.getByText("still here")).toBeInTheDocument();

    // Reopening shows the stored body, not the emptied draft.
    await userEvent.click(within(await openMenu("still here")).getByText("Edit"));
    expect(screen.getByLabelText("Edit note")).toHaveValue("still here");
  });

  it("keeps the editor open, error on the row, when the write fails — and retries", async () => {
    // The row stays actionable so the user can try again, rather than closing
    // over a note whose emptying never landed.
    served.project = [projectNote("1", "stubborn", "2026-05-01T09:00:00Z")];
    serveMutable();
    render(<Harness entity={null} />);
    await screen.findByText("stubborn");

    await emptyEditorOf("stubborn");
    const landing = invokeMock.getMockImplementation()!;
    let fail = true;
    invokeMock.mockImplementation(async (cmd: string, payload?: unknown) => {
      if (cmd === "update_note" && fail) throw "note is locked";
      return landing(cmd, payload);
    });
    await save();

    await waitFor(() => expect(calls("update_note")).toHaveLength(1));
    const row = document.querySelector('[data-note-id="1"]') as HTMLElement;
    // The error belongs to the row, not to the create affordance below the list.
    expect(within(row).getByText("note is locked")).toBeInTheDocument();
    expect(within(row).getByLabelText("Edit note")).toBeInTheDocument();
    // The stored note is untouched by a failed write, and nothing was deleted.
    expect(calls("delete_note")).toHaveLength(0);

    // Retrying from the still-open editor works, and clears the error with it.
    fail = false;
    await save();

    await waitFor(() => expect(screen.getByText(EMPTY_ROW)).toBeInTheDocument());
    expect(calls("update_note")).toHaveLength(2);
    expect(screen.queryByText("note is locked")).not.toBeInTheDocument();
    expect(screen.queryByLabelText("Edit note")).not.toBeInTheDocument();
  });

  it("writes once from a double click, with both actions unavailable in flight", async () => {
    served.project = [projectNote("1", "about to be emptied", "2026-05-01T09:00:00Z")];
    serveMutable();
    const landing = invokeMock.getMockImplementation()!;
    let settleWrite!: () => void;
    invokeMock.mockImplementation(async (cmd: string, payload?: unknown) => {
      if (cmd === "update_note") {
        await new Promise<void>((resolve) => {
          settleWrite = resolve;
        });
      }
      return landing(cmd, payload);
    });
    render(<Harness entity={null} />);
    await screen.findByText("about to be emptied");

    await emptyEditorOf("about to be emptied");
    fireEvent.click(screen.getByRole("button", { name: "Save note" }));

    // While the write is in flight neither action is available: a second Save
    // would write twice, and a Cancel would close the editor from under the
    // continuation that is about to reset it. (A click on a disabled button is
    // not dispatched at all, so these two prove only that the buttons are what
    // stops it — the Escape below is what reaches the guard behind them.)
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Save note" })).toBeDisabled(),
    );
    expect(screen.getByRole("button", { name: "Cancel edit" })).toBeDisabled();

    // Escape is the one door a disabled button does not close: the textarea
    // stays enabled, so the guard has to hold there too.
    fireEvent.keyDown(screen.getByLabelText("Edit note"), { key: "Escape" });
    expect(screen.getByLabelText("Edit note")).toBeInTheDocument();

    settleWrite();
    await waitFor(() => expect(screen.getByText(EMPTY_ROW)).toBeInTheDocument());
    expect(calls("update_note")).toHaveLength(1);
    expect(calls("delete_note")).toHaveLength(0);
  });

  /**
   * A brand-new note in edit, with its Save held open: the window in which the
   * note exists on disk, is reapable if abandoned, and is being committed.
   * `settleWrite` closes it.
   */
  async function newNoteWithWriteInFlight(): Promise<{ settleWrite: () => void }> {
    const blank = entityNote("new", "specs/a.md", "", "2026-05-09T09:00:00Z");
    let settleWrite!: () => void;
    served.panelState = { scopePosition: "entity", textFilter: "" };
    served.entity = [];
    invokeMock.mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case "load_notes_panel_state":
          return served.panelState;
        case "list_notes_for_entity":
          return served.entity;
        case "create_note":
          return blank.note;
        case "update_note":
          return new Promise<void>((resolve) => {
            settleWrite = resolve;
          });
        default:
          return undefined;
      }
    });
    served.entity = [blank];
    await userEvent.click(screen.getByRole("button", { name: "New note" }));
    await screen.findByLabelText("Edit note");
    await save();
    await waitFor(() => expect(calls("update_note")).toHaveLength(1));
    return { settleWrite: () => settleWrite() };
  }

  it("does not let the leave-the-list reap delete a note whose write is in flight", async () => {
    // A brand-new note is reapable until it is committed, and committing it is
    // not instant. If its row leaves the list mid-write — here the active tab
    // rebinding — nothing may reap it: the user asked for it to be kept.
    served.entity = [];
    const { rerender } = render(<Harness entity={ARTIFACT_A} />);
    await screen.findByLabelText("Notes scope");
    const { settleWrite } = await newNoteWithWriteInFlight();

    served.entity = [];
    rerender(<Harness entity={ARTIFACT_B} />);
    await waitFor(() => expect(rows()).toHaveLength(0));
    expect(calls("delete_note")).toHaveLength(0);

    settleWrite();
    await waitFor(() =>
      expect(screen.queryByLabelText("Edit note")).not.toBeInTheDocument(),
    );
    // Committed, so out of the reap's reach for good.
    expect(calls("delete_note")).toHaveLength(0);
  });

  it("does not let the unmount reap delete a note whose write is in flight", async () => {
    // The panel can be closed the instant after Save is clicked — a vertical-panel
    // switch, a project change — and the unmount reap runs with no render left to
    // gate it. A note being committed must not go with it.
    served.entity = [];
    const { unmount } = render(<Harness entity={ARTIFACT_A} />);
    await screen.findByLabelText("Notes scope");
    const { settleWrite } = await newNoteWithWriteInFlight();

    unmount();

    expect(calls("delete_note")).toHaveLength(0);
    settleWrite();
    await waitFor(() => expect(calls("update_note")).toHaveLength(1));
    expect(calls("delete_note")).toHaveLength(0);
  });

  it("does not let the reap chase an abandoned note's delete already in flight", async () => {
    // The other side of the same window: Cancel on a never-written note deletes
    // it, and if the row leaves the list before that delete lands, the reap must
    // not issue a second one for the same id.
    const blank = entityNote("new", "specs/a.md", "", "2026-05-09T09:00:00Z");
    let settleDelete!: () => void;
    served.panelState = { scopePosition: "entity", textFilter: "" };
    served.entity = [];
    invokeMock.mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case "load_notes_panel_state":
          return served.panelState;
        case "list_notes_for_entity":
          return served.entity;
        case "create_note":
          return blank.note;
        case "delete_note":
          return new Promise<void>((resolve) => {
            settleDelete = resolve;
          });
        default:
          return undefined;
      }
    });
    const { rerender } = render(<Harness entity={ARTIFACT_A} />);
    await screen.findByLabelText("Notes scope");
    served.entity = [blank];
    await userEvent.click(screen.getByRole("button", { name: "New note" }));
    await screen.findByLabelText("Edit note");

    fireEvent.click(screen.getByRole("button", { name: "Cancel edit" }));
    await waitFor(() => expect(calls("delete_note")).toHaveLength(1));

    served.entity = [];
    rerender(<Harness entity={ARTIFACT_B} />);
    await waitFor(() => expect(rows()).toHaveLength(0));

    settleDelete();
    await waitFor(() =>
      expect(screen.queryByLabelText("Edit note")).not.toBeInTheDocument(),
    );
    expect(calls("delete_note")).toHaveLength(1);
  });
});
