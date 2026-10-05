// The Notes vertical panel (`specifications/ui/NTS-notes.md`), covering
// NTS-FR-01, NTS-FR-02 … NTS-FR-24. The backend is mocked at `invoke`, so every assertion
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
import { selectorValue } from "../test/selectors";
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
// NTS-FR-04, NTS-FR-10 — creating
// ---------------------------------------------------------------------------

describe("creating a note (NTS-FR-04 / NTS-FR-10)", () => {
  /** The note the mocked backend hands back, and then serves in the list. */
  function created(id: string, entityId?: string) {
    return entityId
      ? entityNote(id, entityId, "", "2026-05-09T09:00:00Z")
      : projectNote(id, "", "2026-05-09T09:00:00Z");
  }

  it("writes the note straight away and opens it for editing (NTS-FR-04, NTS-FR-10)", async () => {
    render(<Harness entity={ARTIFACT_A} />);
    await screen.findByLabelText("Notes scope");

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "create_note") return created("9", "specs/a.md").note;
      if (cmd === "list_notes_for_entity") return served.entity;
      if (cmd === "load_notes_panel_state") return served.panelState;
      if (cmd === "update_note") return undefined;
      return undefined;
    });
    served.entity = [created("9", "specs/a.md")];
    await userEvent.click(screen.getByRole("button", { name: "New note" }));

    // The note exists the moment it is asked for — attached to the active
    // tab's entity — and its row is in the list, already editable.
    await waitFor(() => expect(calls("create_note")).toHaveLength(1));
    expect(calls("create_note")[0][1]).toMatchObject({
      scope: { kind: "entity", entityId: "specs/a.md" },
      body: "",
    });
    const editor = await screen.findByLabelText("Edit note");
    expect(rows()).toHaveLength(1);
    expect(editor).toHaveValue("");

    // Typing into that row and saving writes the body onto the note that is
    // already there, rather than creating a second one.
    await userEvent.type(editor, "X");
    served.entity = [entityNote("9", "specs/a.md", "X", "2026-05-09T09:01:00Z")];
    await userEvent.click(screen.getByRole("button", { name: "Save note" }));

    await waitFor(() => expect(calls("update_note")).toHaveLength(1));
    expect(calls("update_note")[0][1]).toEqual({ id: "9", fields: { body: "X" } });
    expect(calls("create_note")).toHaveLength(1);
    expect(await screen.findByText("X")).toBeInTheDocument();
  });

  it("still attaches to the active tab's entity from the all-notes position", async () => {
    served.panelState = { scopePosition: "all", textFilter: "" };
    served.all = [entityNote("1", "specs/a.md", "existing", "2026-05-01T09:00:00Z")];
    render(<Harness entity={ARTIFACT_B} />);
    await waitFor(() => expect(selectorValue("Notes scope")).toBe("all"));

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "create_note") return created("9", "specs/b.md").note;
      if (cmd === "list_all_notes") return served.all;
      if (cmd === "load_notes_panel_state") return served.panelState;
      return undefined;
    });
    served.all = [created("9", "specs/b.md"), ...served.all];
    await userEvent.click(screen.getByRole("button", { name: "New note" }));

    await waitFor(() => expect(calls("create_note")).toHaveLength(1));
    expect(calls("create_note")[0][1]).toMatchObject({
      scope: { kind: "entity", entityId: "specs/b.md" },
    });
    // NTS-FR-04, NTS-FR-10's second half: the note appears under B's group, and it is the
    // row that took the caret — not the pre-existing note beside it.
    await waitFor(() =>
      expect(
        Array.from(document.querySelectorAll(".note-group__header")).map(
          (el) => el.textContent,
        ),
      ).toContain("b.md"),
    );
    const editing = screen.getByLabelText("Edit note").closest(".note");
    expect(editing).toHaveAttribute("data-note-id", "9");
    expect(editing).toHaveAttribute("data-editing", "true");
  });

  it("attaches to the project when no tab is entity-bound", async () => {
    render(<Harness entity={null} />);
    await screen.findByLabelText("Notes scope");
    served.project = [created("new")];

    await userEvent.click(screen.getByRole("button", { name: "New note" }));

    await waitFor(() => expect(calls("create_note")).toHaveLength(1));
    expect(calls("create_note")[0][1]).toMatchObject({
      scope: { kind: "project" },
    });
    const editing = (await screen.findByLabelText("Edit note")).closest(".note");
    expect(editing).toHaveAttribute("data-note-id", "new");
  });

  it("creates a project-wide note from the project-wide position", async () => {
    // A deliberate divergence from NTS-FR-04, which asks for the active tab's
    // entity in every position: the note is written before it is typed into,
    // so it has to land somewhere this list can show — otherwise the caret
    // goes to a row that does not exist.
    served.panelState = { scopePosition: "project", textFilter: "" };
    served.project = [created("new")];
    render(<Harness entity={ARTIFACT_A} />);
    await waitFor(() => expect(selectorValue("Notes scope")).toBe("project"));

    await userEvent.click(screen.getByRole("button", { name: "New note" }));

    await waitFor(() => expect(calls("create_note")).toHaveLength(1));
    expect(calls("create_note")[0][1]).toMatchObject({
      scope: { kind: "project" },
    });
    expect(await screen.findByLabelText("Edit note")).toBeInTheDocument();
  });

  it("takes an abandoned blank note back out of the list", async () => {
    // Because the note is written up front, walking away from it has to remove
    // it — otherwise every stray click on + Note leaves a blank row behind.
    render(<Harness entity={null} />);
    await screen.findByLabelText("Notes scope");
    served.project = [created("new")];
    await userEvent.click(screen.getByRole("button", { name: "New note" }));
    await screen.findByLabelText("Edit note");

    served.project = [];
    await userEvent.click(screen.getByRole("button", { name: "Cancel edit" }));

    await waitFor(() => expect(calls("delete_note")).toHaveLength(1));
    expect(calls("delete_note")[0][1]).toEqual({ id: "new" });
    await waitFor(() => expect(rows()).toHaveLength(0));
  });

  it("opens the new note for editing even while a filter is active", async () => {
    // A blank body matches no filter text, so without an exemption the row
    // would be created and hidden in the same breath — the user would see
    // nothing happen and a blank note would be written to disk.
    served.panelState = { scopePosition: "project", textFilter: "alpha" };
    served.project = [projectNote("1", "alpha note", "2026-05-01T09:00:00Z")];
    render(<Harness entity={null} />);
    await screen.findByText("alpha note");

    served.project = [created("new"), ...served.project];
    await userEvent.click(screen.getByRole("button", { name: "New note" }));

    // The new row is visible and editable despite the filter…
    expect(await screen.findByLabelText("Edit note")).toBeInTheDocument();
    // …and it was not quietly deleted by the leave-the-list reap.
    expect(calls("delete_note")).toHaveLength(0);
    // The filter itself is untouched, so the user does not lose it.
    expect(screen.getByLabelText("Filter notes")).toHaveValue("alpha");
  });

  it("discards a new note on Cancel even after something was typed into it", async () => {
    // The sharp edge of writing the note up front: Cancel on one that has never
    // been *saved* takes the note away along with the text in the editor. That is
    // "never mind, I did not want a note", not the NTS-FR-16 discard that leaves
    // a stored body standing — there is no stored body yet.
    render(<Harness entity={null} />);
    await screen.findByLabelText("Notes scope");
    served.project = [created("new")];
    await userEvent.click(screen.getByRole("button", { name: "New note" }));
    await userEvent.type(await screen.findByLabelText("Edit note"), "second thoughts");

    served.project = [];
    await userEvent.click(screen.getByRole("button", { name: "Cancel edit" }));

    await waitFor(() => expect(calls("delete_note")).toHaveLength(1));
    expect(calls("delete_note")[0][1]).toEqual({ id: "new" });
    // The typed text was never written, so nothing carried it.
    expect(calls("update_note")).toHaveLength(0);
    await waitFor(() => expect(rows()).toHaveLength(0));
  });

  it("keeps a new note saved without a body, though a filter then hides it", async () => {
    // Committing a note with nothing in it takes away the exemption that kept it
    // visible while it was being created: an empty body matches no filter text.
    // The note is kept — clearing the filter is what brings it back.
    served.panelState = { scopePosition: "project", textFilter: "alpha" };
    served.project = [projectNote("1", "alpha note", "2026-05-01T09:00:00Z")];
    render(<Harness entity={null} />);
    await screen.findByText("alpha note");

    const blank = created("new");
    served.project = [blank, ...served.project];
    await userEvent.click(screen.getByRole("button", { name: "New note" }));
    await screen.findByLabelText("Edit note");
    await userEvent.click(screen.getByRole("button", { name: "Save note" }));

    await waitFor(() => expect(calls("update_note")).toHaveLength(1));
    expect(calls("update_note")[0][1]).toEqual({ id: "new", fields: { body: "" } });
    // Kept, not reaped, even though its row is no longer shown.
    await waitFor(() => expect(rows()).toHaveLength(1));
    expect(screen.getByText("alpha note")).toBeInTheDocument();
    expect(calls("delete_note")).toHaveLength(0);

    await userEvent.clear(screen.getByLabelText("Filter notes"));
    await waitFor(() => expect(rows()).toHaveLength(2));
    expect(screen.getByText("This note is empty")).toBeInTheDocument();
  });

  it("cannot steal the editor from a row that is already being edited", async () => {
    // NTS-FR-17: `+ Note` is a second door into the one-editor-at-a-time rule.
    served.project = [projectNote("1", "first note", "2026-05-01T09:00:00Z")];
    render(<Harness entity={null} />);
    await screen.findByText("first note");

    await userEvent.click(within(await openMenu("first note")).getByText("Edit"));
    await userEvent.clear(screen.getByLabelText("Edit note"));
    await userEvent.type(screen.getByLabelText("Edit note"), "hard-won prose");

    expect(screen.getByRole("button", { name: "New note" })).toBeDisabled();
    // Even if the click gets through, nothing is created and nothing typed is
    // lost.
    fireEvent.click(screen.getByRole("button", { name: "New note" }));
    expect(calls("create_note")).toHaveLength(0);
    expect(screen.getByLabelText("Edit note")).toHaveValue("hard-won prose");
  });

  it("reaps the blank note when its row leaves the list under it", async () => {
    // The row can go out from under the editor for reasons no button handler
    // sees — here, the active tab rebinding the entity position. The note was
    // never written into, so it must not be left behind.
    served.panelState = { scopePosition: "entity", textFilter: "" };
    served.entity = [];
    const { rerender } = render(<Harness entity={ARTIFACT_A} />);
    await screen.findByLabelText("Notes scope");

    served.entity = [created("new", "specs/a.md")];
    await userEvent.click(screen.getByRole("button", { name: "New note" }));
    await screen.findByLabelText("Edit note");

    // Switch tabs: the panel rebinds to B, whose list does not hold the note.
    served.entity = [];
    rerender(<Harness entity={ARTIFACT_B} />);

    await waitFor(() => expect(calls("delete_note")).toHaveLength(1));
    expect(calls("delete_note")[0][1]).toEqual({ id: "new" });
    expect(screen.queryByLabelText("Edit note")).not.toBeInTheDocument();
  });

  it("reaps the blank note when the panel is unmounted under it", async () => {
    // A vertical-panel surface switch unmounts `Notes` outright; the blank
    // note has to go with it rather than outliving the panel that wrote it.
    served.project = [];
    const { unmount } = render(<Harness entity={null} />);
    await screen.findByLabelText("Notes scope");
    served.project = [created("new")];
    await userEvent.click(screen.getByRole("button", { name: "New note" }));
    await screen.findByLabelText("Edit note");

    unmount();

    await waitFor(() => expect(calls("delete_note")).toHaveLength(1));
    expect(calls("delete_note")[0][1]).toEqual({ id: "new" });
  });

  it("keeps the editor open when the blank note cannot be removed", async () => {
    // Same policy as a failed Save: closing over a row the user cannot act on
    // is worse than leaving them somewhere they can retry.
    served.project = [];
    render(<Harness entity={null} />);
    await screen.findByLabelText("Notes scope");
    served.project = [created("new")];
    await userEvent.click(screen.getByRole("button", { name: "New note" }));
    await screen.findByLabelText("Edit note");

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "delete_note") throw "note is locked";
      if (cmd === "list_project_notes") return served.project;
      if (cmd === "load_notes_panel_state") return served.panelState;
      return undefined;
    });
    await userEvent.click(screen.getByRole("button", { name: "Cancel edit" }));

    const row = document.querySelector('[data-note-id="new"]') as HTMLElement;
    // On the row, not under the create affordance at the foot of the panel.
    expect(await within(row).findByText("note is locked")).toBeInTheDocument();
    expect(within(row).getByLabelText("Edit note")).toBeInTheDocument();
  });

  it("removes a brand-new blank note on Escape, as Cancel does", async () => {
    served.project = [];
    render(<Harness entity={null} />);
    await screen.findByLabelText("Notes scope");
    served.project = [created("new")];
    await userEvent.click(screen.getByRole("button", { name: "New note" }));
    await screen.findByLabelText("Edit note");

    served.project = [];
    await userEvent.type(screen.getByLabelText("Edit note"), "{Escape}");

    await waitFor(() => expect(calls("delete_note")).toHaveLength(1));
    await waitFor(() => expect(rows()).toHaveLength(0));
  });

  it("re-enables the affordance after a failed create", async () => {
    render(<Harness entity={null} />);
    await screen.findByLabelText("Notes scope");

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "create_note") throw "no project open";
      if (cmd === "list_project_notes") return served.project;
      if (cmd === "load_notes_panel_state") return served.panelState;
      return undefined;
    });
    await userEvent.click(screen.getByRole("button", { name: "New note" }));
    await screen.findByText("no project open");

    // A failed create must not leave the panel unable to try again.
    expect(screen.getByRole("button", { name: "New note" })).not.toBeDisabled();
  });

  it("cannot write the same note twice from a double click", async () => {
    render(<Harness entity={null} />);
    await screen.findByLabelText("Notes scope");

    const add = screen.getByRole("button", { name: "New note" });
    fireEvent.click(add);
    fireEvent.click(add);

    await waitFor(() => expect(calls("create_note")).toHaveLength(1));
  });

  it("reports a failed create below the affordance", async () => {
    render(<Harness entity={null} />);
    await screen.findByLabelText("Notes scope");

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "create_note") throw "no project open";
      if (cmd === "list_project_notes") return served.project;
      if (cmd === "load_notes_panel_state") return served.panelState;
      return undefined;
    });
    await userEvent.click(screen.getByRole("button", { name: "New note" }));

    expect(await screen.findByText("no project open")).toBeInTheDocument();
    expect(screen.queryByLabelText("Edit note")).not.toBeInTheDocument();
  });
});
