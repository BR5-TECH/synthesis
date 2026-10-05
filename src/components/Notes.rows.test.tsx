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

import {
  Notes,
  artifactTargets,
  fromLocalInput,
  toLocalInput,
} from "./Notes";
import { pickSelector } from "../test/selectors";
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
// NTS-FR-06, NTS-FR-20 — reminders
// ---------------------------------------------------------------------------

describe("reminders (NTS-FR-06 / NTS-FR-20 / NTS-FR-06)", () => {
  it("offers Set Reminder… on a note carrying none and commits the picked instant", async () => {
    served.project = [projectNote("1", "remind me", "2026-05-01T09:00:00Z")];
    render(<Harness entity={null} />);
    await screen.findByText("remind me");

    const menu = await openMenu("remind me");
    expect(within(menu).getByText("Set Reminder…")).toBeInTheDocument();
    await userEvent.click(within(menu).getByText("Set Reminder…"));

    fireEvent.change(screen.getByLabelText("Reminder"), {
      target: { value: "2026-06-01T09:00" },
    });
    // What the reload will return once the write lands.
    served.project = [
      {
        ...served.project[0],
        note: { ...served.project[0].note, reminder: "2026-06-01T09:00:00Z" },
      },
    ];
    await userEvent.click(screen.getByRole("button", { name: "Set" }));

    await waitFor(() => expect(calls("update_note")).toHaveLength(1));
    const [, args] = calls("update_note")[0] as [string, { id: string; fields: { reminder: string } }];
    expect(args.id).toBe("1");
    expect(new Date(args.fields.reminder).getTime()).toBe(
      new Date("2026-06-01T09:00").getTime(),
    );
    // The row now renders a reminder indicator, and the menu entry has flipped.
    expect(
      rowFor("remind me").querySelector(".note__reminder"),
    ).not.toBeNull();
    const reopened = await openMenu("remind me");
    expect(within(reopened).getByText("Clear Reminder")).toBeInTheDocument();
  });

  it("invokes nothing when the picker is committed with no instant chosen", async () => {
    served.project = [projectNote("1", "remind me", "2026-05-01T09:00:00Z")];
    render(<Harness entity={null} />);
    await screen.findByText("remind me");

    await userEvent.click(
      within(await openMenu("remind me")).getByText("Set Reminder…"),
    );
    await userEvent.click(screen.getByRole("button", { name: "Set" }));

    expect(calls("update_note")).toHaveLength(0);
    expect(document.querySelector(".menu")).toBeNull();
  });

  it("offers Clear Reminder on a note carrying one and clears it without a picker", async () => {
    served.project = [
      {
        ...projectNote("1", "has a reminder", "2026-05-01T09:00:00Z"),
        note: {
          ...projectNote("1", "has a reminder", "2026-05-01T09:00:00Z").note,
          reminder: "2026-06-01T09:00:00Z",
        },
      },
    ];
    render(<Harness entity={null} />);
    await screen.findByText("has a reminder");

    const menu = await openMenu("has a reminder");
    expect(within(menu).queryByText("Set Reminder…")).not.toBeInTheDocument();
    await userEvent.click(within(menu).getByText("Clear Reminder"));

    expect(screen.queryByLabelText("Reminder")).not.toBeInTheDocument();
    await waitFor(() => expect(calls("update_note")).toHaveLength(1));
    expect(calls("update_note")[0][1]).toEqual({
      id: "1",
      fields: { reminder: null },
    });
  });
});

// ---------------------------------------------------------------------------
// NTS-FR-07 — historical notes
// ---------------------------------------------------------------------------

describe("historical notes (NTS-FR-07)", () => {
  it("renders a note on a historical revision distinctly from one on the current version", async () => {
    served.entity = [
      entityNote("1", "specs/a.md", "current version", "2026-05-02T09:00:00Z"),
      entityNote("2", "specs/a.md", "old version", "2026-05-01T09:00:00Z", {
        revision: "7e3f1a2",
      }),
    ];
    render(<Harness entity={ARTIFACT_A} />);
    await screen.findByText("old version");

    expect(rowFor("old version")).toHaveAttribute("data-historical", "true");
    expect(rowFor("current version")).toHaveAttribute(
      "data-historical",
      "false",
    );
    expect(within(rowFor("old version")).getByText(/7e3f1a2/)).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// NTS-FR-05, NTS-FR-16 / NTS-FR-17 — the inline editor
// ---------------------------------------------------------------------------

describe("inline editing (NTS-FR-16 / NTS-FR-17)", () => {
  beforeEach(() => {
    served.project = [
      projectNote("1", "first note", "2026-05-02T09:00:00Z"),
      projectNote("2", "second note", "2026-05-01T09:00:00Z"),
    ];
  });

  it("saves the edited body and returns the row to its rendered form (NTS-FR-05, NTS-FR-16)", async () => {
    render(<Harness entity={null} />);
    await screen.findByText("first note");

    await userEvent.click(within(await openMenu("first note")).getByText("Edit"));
    const editor = screen.getByLabelText("Edit note");
    expect(editor).toHaveValue("first note");
    await userEvent.clear(editor);
    await userEvent.type(editor, "edited");

    served.project = [
      projectNote("1", "edited", "2026-05-03T09:00:00Z"),
      projectNote("2", "second note", "2026-05-01T09:00:00Z"),
    ];
    await userEvent.click(screen.getByRole("button", { name: "Save note" }));

    await waitFor(() => expect(calls("update_note")).toHaveLength(1));
    expect(calls("update_note")[0][1]).toEqual({
      id: "1",
      fields: { body: "edited" },
    });
    expect(await screen.findByText("edited")).toBeInTheDocument();
    expect(screen.queryByLabelText("Edit note")).not.toBeInTheDocument();
  });

  it("keeps a single footer line and swaps its contents when edit opens", async () => {
    // The *rendered height* of the row is a CSS property — jsdom lays nothing
    // out, so it cannot be asserted here and this test does not claim to. What
    // it does pin is the structure that makes the height stable: the editor
    // replaces the body in place, and the actions replace the timestamp inside
    // the footer line that is already there, rather than adding a third row.
    render(<Harness entity={null} />);
    await screen.findByText("first note");

    const row = rowFor("first note");
    expect(row.querySelector(".note__meta > .note__when")).not.toBeNull();
    expect(row).toHaveAttribute("data-editing", "false");

    await userEvent.click(within(await openMenu("first note")).getByText("Edit"));

    const editing = screen.getByLabelText("Edit note").closest(".note")!;
    expect(editing).toHaveAttribute("data-editing", "true");
    expect(editing.querySelectorAll(".note__meta")).toHaveLength(1);
    const footer = editing.querySelector(".note__meta") as HTMLElement;
    expect(footer.querySelector(".note__when")).toBeNull();
    expect(within(footer).getByLabelText("Cancel edit")).toBeInTheDocument();
    expect(within(footer).getByLabelText("Save note")).toBeInTheDocument();
    // The editor is the panel's own box-less control. Whether it actually
    // carries no border is a CSS-review matter (vitest runs with `css: false`).
    expect(screen.getByLabelText("Edit note")).toHaveClass("note__editor");
  });

  it("keeps the editor and the typed text open when the save fails", async () => {
    render(<Harness entity={null} />);
    await screen.findByText("first note");

    await userEvent.click(within(await openMenu("first note")).getByText("Edit"));
    await userEvent.clear(screen.getByLabelText("Edit note"));
    await userEvent.type(screen.getByLabelText("Edit note"), "hard-won prose");

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "update_note") throw "note not found";
      if (cmd === "list_project_notes") return served.project;
      if (cmd === "load_notes_panel_state") return served.panelState;
      return undefined;
    });
    await userEvent.click(screen.getByRole("button", { name: "Save note" }));

    // The row reports the failure AND keeps what the user typed, so the edit
    // is retryable rather than lost.
    expect(await screen.findByText("note not found")).toBeInTheDocument();
    expect(screen.getByLabelText("Edit note")).toHaveValue("hard-won prose");
  });

  it("discards the edit on Cancel and on Escape, invoking nothing (NTS-FR-05, NTS-FR-16)", async () => {
    render(<Harness entity={null} />);
    await screen.findByText("first note");

    await userEvent.click(within(await openMenu("first note")).getByText("Edit"));
    await userEvent.type(screen.getByLabelText("Edit note"), " and more");
    await userEvent.click(screen.getByRole("button", { name: "Cancel edit" }));

    expect(calls("update_note")).toHaveLength(0);
    // And emphatically nothing is removed: the blank-note reap must never
    // reach a note that has a body.
    expect(calls("delete_note")).toHaveLength(0);
    expect(screen.getByText("first note")).toBeInTheDocument();

    await userEvent.click(within(await openMenu("first note")).getByText("Edit"));
    await userEvent.type(screen.getByLabelText("Edit note"), " again{Escape}");

    expect(calls("update_note")).toHaveLength(0);
    expect(calls("delete_note")).toHaveLength(0);
    expect(screen.queryByLabelText("Edit note")).not.toBeInTheDocument();
    expect(screen.getByText("first note")).toBeInTheDocument();
  });

  it("survives an outside click and blocks a second editor (NTS-FR-17)", async () => {
    render(<Harness entity={null} />);
    await screen.findByText("first note");

    await userEvent.click(within(await openMenu("first note")).getByText("Edit"));
    await userEvent.clear(screen.getByLabelText("Edit note"));
    await userEvent.type(screen.getByLabelText("Edit note"), "unsaved work");

    // A click elsewhere in the panel neither saves nor discards.
    await userEvent.click(screen.getByLabelText("Filter notes"));
    expect(screen.getByLabelText("Edit note")).toHaveValue("unsaved work");
    expect(calls("update_note")).toHaveLength(0);

    // And Edit on another row does nothing while that editor is open.
    await userEvent.click(within(await openMenu("second note")).getByText("Edit"));
    expect(screen.getAllByLabelText("Edit note")).toHaveLength(1);
    expect(screen.getByLabelText("Edit note")).toHaveValue("unsaved work");
  });

  it("closes and discards when its note leaves the rendered list (NTS-FR-17)", async () => {
    render(<Harness entity={null} />);
    await screen.findByText("first note");

    await userEvent.click(within(await openMenu("first note")).getByText("Edit"));
    await userEvent.type(screen.getByLabelText("Edit note"), " unsaved");

    // The selector moves to a position that excludes it — while still
    // rendering another note, so "the editor closed because its note left"
    // cannot be confused with "the editor closed because the list emptied".
    served.all = [projectNote("3", "a different note", "2026-05-05T09:00:00Z")];
    await pickSelector("Notes scope", "all");
    expect(await screen.findByText("a different note")).toBeInTheDocument();

    await waitFor(() =>
      expect(screen.queryByLabelText("Edit note")).not.toBeInTheDocument(),
    );
    expect(calls("update_note")).toHaveLength(0);
    // The reap shares its delete with the emptied-Save path, so it has to be
    // pinned that it reaches only a note that was never written into.
    expect(calls("delete_note")).toHaveLength(0);
  });

  it("closes and discards when the filter excludes it (NTS-FR-17)", async () => {
    render(<Harness entity={null} />);
    await screen.findByText("first note");

    await userEvent.click(within(await openMenu("first note")).getByText("Edit"));
    await userEvent.type(screen.getByLabelText("Filter notes"), "second");

    await waitFor(() =>
      expect(screen.queryByLabelText("Edit note")).not.toBeInTheDocument(),
    );
    expect(calls("update_note")).toHaveLength(0);
    expect(calls("delete_note")).toHaveLength(0);
  });

  it("keeps a note whose editor was emptied and then reaped rather than saved", async () => {
    // Emptying the draft is not what deletes a note — committing it is. A row
    // that leaves the list mid-clear keeps its stored body.
    render(<Harness entity={null} />);
    await screen.findByText("first note");

    await userEvent.click(within(await openMenu("first note")).getByText("Edit"));
    await userEvent.clear(screen.getByLabelText("Edit note"));
    await userEvent.type(screen.getByLabelText("Filter notes"), "second");

    await waitFor(() =>
      expect(screen.queryByLabelText("Edit note")).not.toBeInTheDocument(),
    );
    expect(calls("delete_note")).toHaveLength(0);
    expect(calls("update_note")).toHaveLength(0);

    // Clearing the filter brings it back with its body intact.
    await userEvent.clear(screen.getByLabelText("Filter notes"));
    expect(await screen.findByText("first note")).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// NTS-FR-18, NTS-FR-19 / NTS-FR-05, NTS-FR-21 — the overflow menu and delete
// ---------------------------------------------------------------------------

describe("the overflow menu (NTS-FR-18 / NTS-FR-19 / NTS-FR-21)", () => {
  beforeEach(() => {
    served.project = [projectNote("1", "a note", "2026-05-01T09:00:00Z")];
  });

  it("offers exactly Discuss, Edit, Move…, the reminder entry and Delete (NTS-FR-18, NTS-FR-19)", async () => {
    render(<Harness entity={null} />);
    await screen.findByText("a note");

    const menu = await openMenu("a note");
    expect(
      Array.from(menu.querySelectorAll(".menu-item")).map((el) => el.textContent),
    ).toEqual(["Discuss", "Edit", "Move…", "Set Reminder…", "Delete"]);
  });

  it("confirms before deleting, and a dismissal invokes nothing (NTS-FR-05, NTS-FR-21)", async () => {
    render(<Harness entity={null} />);
    await screen.findByText("a note");

    await userEvent.click(within(await openMenu("a note")).getByText("Delete"));
    // The confirmation names the note and nothing has been invoked yet.
    expect(screen.getByText(/Delete “a note”\?/)).toBeInTheDocument();
    expect(calls("delete_note")).toHaveLength(0);

    // The confirmation's own Cancel, which is a labelled button rather than
    // the editor's icon pair.
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(calls("delete_note")).toHaveLength(0);
    expect(screen.getByText("a note")).toBeInTheDocument();

    await userEvent.click(within(await openMenu("a note")).getByText("Delete"));
    served.project = [];
    await userEvent.click(screen.getByRole("button", { name: "Delete" }));

    await waitFor(() => expect(calls("delete_note")).toHaveLength(1));
    expect(calls("delete_note")[0][1]).toEqual({ id: "1" });
    await waitFor(() => expect(rows()).toHaveLength(0));
  });

  it("names a note with no body in the confirmation", async () => {
    // A create writes the note before it has a body, so a bodiless row is
    // reachable and the confirmation still has to say what it is about.
    served.project = [projectNote("1", "", "2026-05-09T09:00:00Z")];
    render(<Harness entity={null} />);
    await waitFor(() => expect(rows()).toHaveLength(1));

    // Addressed through the row's own button rather than by its text, which a
    // bodiless row does not have.
    await userEvent.click(screen.getByLabelText("Note actions"));
    await userEvent.click(
      within(document.querySelector(".menu") as HTMLElement).getByText("Delete"),
    );

    expect(screen.getByText(/Delete “this empty note”\?/)).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// NTS-FR-22 / NTS-FR-23 — moving and unresolved notes
// ---------------------------------------------------------------------------

describe("moving a note (NTS-FR-22)", () => {
  beforeEach(() => {
    served.entity = [entityNote("1", "specs/a.md", "misfiled", "2026-05-01T09:00:00Z")];
  });

  it("moves it onto an artifact chosen from the filtered picker", async () => {
    render(<Harness entity={ARTIFACT_A} />);
    await screen.findByText("misfiled");

    await userEvent.click(within(await openMenu("misfiled")).getByText("Move…"));
    const picker = document.querySelector(".notes-move") as HTMLElement;
    await waitFor(() =>
      expect(within(picker).getByText("b.md")).toBeInTheDocument(),
    );
    // Only artifacts and Flows are offered.
    expect(within(picker).queryByText("notes.txt")).not.toBeInTheDocument();

    await userEvent.type(within(picker).getByLabelText("Filter artifacts"), "b.");
    expect(within(picker).queryByText("a.md")).not.toBeInTheDocument();
    await userEvent.click(within(picker).getByText("b.md"));

    await waitFor(() => expect(calls("update_note")).toHaveLength(1));
    expect(calls("update_note")[0][1]).toEqual({
      id: "1",
      fields: { scope: { kind: "entity", entityId: "specs/b.md", entityPath: "specs/b.md" } },
    });
    expect(document.querySelector(".notes-move")).toBeNull();
  });

  it("moves it to project level from the picker's head entry", async () => {
    render(<Harness entity={ARTIFACT_A} />);
    await screen.findByText("misfiled");

    await userEvent.click(within(await openMenu("misfiled")).getByText("Move…"));
    await userEvent.click(
      within(document.querySelector(".notes-move") as HTMLElement).getByText(
        "Project level",
      ),
    );

    await waitFor(() => expect(calls("update_note")).toHaveLength(1));
    expect(calls("update_note")[0][1]).toEqual({
      id: "1",
      fields: { scope: { kind: "project" } },
    });
  });
});

describe("unresolved notes (NTS-FR-23 / NTS-FR-22)", () => {
  it("renders in the Unresolved group with its last-known path and its menu", async () => {
    served.panelState = { scopePosition: "all", textFilter: "" };
    served.all = [
      entityNote("1", "specs/old.md", "orphan", "2026-05-01T09:00:00Z", {
        unresolved: true,
      }),
    ];
    render(<Harness entity={null} />);
    await screen.findByText("orphan");

    expect(
      Array.from(document.querySelectorAll(".note-group__header")).map(
        (el) => el.textContent,
      ),
    ).toEqual(["Unresolved"]);
    const row = rowFor("orphan");
    expect(within(row).getByText("specs/old.md")).toBeInTheDocument();
    expect(within(row).getByLabelText("Note actions")).toBeInTheDocument();

    // And Move… is how it is reattached.
    await userEvent.click(within(await openMenu("orphan")).getByText("Move…"));
    const picker = document.querySelector(".notes-move") as HTMLElement;
    await waitFor(() =>
      expect(within(picker).getByText("a.md")).toBeInTheDocument(),
    );
    served.all = [entityNote("1", "specs/a.md", "orphan", "2026-05-02T09:00:00Z")];
    await userEvent.click(within(picker).getByText("a.md"));

    await waitFor(() => expect(calls("update_note")).toHaveLength(1));
    await waitFor(() =>
      expect(
        Array.from(document.querySelectorAll(".note-group__header")).map(
          (el) => el.textContent,
        ),
      ).toEqual(["a.md"]),
    );
  });

  // NTS-FR-11, NTS-FR-23, SNV-FR-57, CMP-FR-12, second half: the marker is spelled and treated exactly as the
  // Comments panel spells and treats it, so one state reads one way.
  it("carries the shared Unresolved marker on the row (NTS-FR-23)", async () => {
    served.panelState = { scopePosition: "all", textFilter: "" };
    served.all = [
      entityNote("1", "specs/old.md", "orphan", "2026-05-01T09:00:00Z", {
        unresolved: true,
      }),
    ];
    render(<Harness entity={null} />);
    await screen.findByText("orphan");

    const marker = within(rowFor("orphan")).getByText("Unresolved", {
      selector: ".unresolved-marker",
    });
    expect(marker).toBeInTheDocument();
    // The path beside it truncates; the marker never does.
    expect(within(rowFor("orphan")).getByText("specs/old.md")).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// NTS-FR-25, NTS-FR-16 — the body is Markdown, rendered rich and edited as source
// ---------------------------------------------------------------------------

describe("note bodies render Markdown (NTS-FR-25 / NTS-FR-16)", () => {
  it("renders emphasis as rich text and shows no source characters", async () => {
    served.panelState = { scopePosition: "project", textFilter: "" };
    served.project = [
      projectNote("1", "Ship the **drafts panel** first", "2026-05-01T09:00:00Z"),
    ];
    render(<Harness entity={null} />);

    const emphasised = await screen.findByText("drafts panel");
    expect(emphasised.tagName).toBe("STRONG");
    // The asterisks are gone from what the row renders.
    const row = emphasised.closest(".note")!;
    expect(row.textContent).toContain("Ship the drafts panel first");
    expect(row.textContent).not.toContain("**");
  });

  it("sets the body at the panel's UI scale, not the document scale", async () => {
    served.panelState = { scopePosition: "project", textFilter: "" };
    served.project = [
      projectNote("1", "Ship the **drafts panel** first", "2026-05-01T09:00:00Z"),
    ];
    render(<Harness entity={null} />);
    await screen.findByText("drafts panel");

    // `.doc` alone is the Editor page's type scale; `.doc--ui` is what brings a
    // body down to the scale the rest of the panel's rows run at.
    const body = document.querySelector(".note__text .comment__body")!;
    expect(body).toHaveClass("doc");
    expect(body).toHaveClass("doc--ui");
  });

  it("edits the Markdown source verbatim and stores exactly what was typed", async () => {
    served.panelState = { scopePosition: "project", textFilter: "" };
    served.project = [
      projectNote("1", "Ship the **drafts panel** first", "2026-05-01T09:00:00Z"),
    ];
    render(<Harness entity={null} />);
    await screen.findByText("drafts panel");

    await userEvent.click(within(await openMenu("drafts panel")).getByText("Edit"));
    const editor = document.querySelector(".note__editor") as HTMLTextAreaElement;
    // NTS-FR-25: the editor holds the literal source, asterisks included.
    expect(editor.value).toBe("Ship the **drafts panel** first");

    await userEvent.click(screen.getByRole("button", { name: "Save note" }));
    await waitFor(() => expect(calls("update_note")).toHaveLength(1));
    expect(
      (calls("update_note")[0][1] as { fields: { body: string } }).fields.body,
    ).toBe("Ship the **drafts panel** first");
  });
});

// ---------------------------------------------------------------------------
// NTS-FR-24 — the single-overlay invariant
// ---------------------------------------------------------------------------

describe("one overlay at a time (NTS-FR-24)", () => {
  beforeEach(() => {
    served.project = [projectNote("1", "a note", "2026-05-01T09:00:00Z")];
  });

  it("replaces the dropdown with the Move picker, and Escape closes it invoking nothing", async () => {
    render(<Harness entity={null} />);
    await screen.findByText("a note");

    await userEvent.click(within(await openMenu("a note")).getByText("Move…"));
    expect(document.querySelectorAll(".menu")).toHaveLength(1);
    expect(document.querySelector(".notes-move")).not.toBeNull();

    fireEvent.keyDown(window, { key: "Escape" });
    expect(document.querySelector(".menu")).toBeNull();
    expect(calls("update_note")).toHaveLength(0);
  });

  it("dismisses an open overlay on an outside click without invoking anything", async () => {
    render(<Harness entity={null} />);
    await screen.findByText("a note");

    await userEvent.click(within(await openMenu("a note")).getByText("Delete"));
    expect(screen.getByText(/Delete “a note”\?/)).toBeInTheDocument();

    await userEvent.click(document.body);
    expect(document.querySelector(".menu")).toBeNull();
    expect(calls("delete_note")).toHaveLength(0);
  });
});

// ---------------------------------------------------------------------------
// The pure helpers behind the Move picker and the reminder picker
// ---------------------------------------------------------------------------

describe("artifactTargets (NTS-FR-22)", () => {
  it("flattens the tree to artifact and Flow FILES, carrying each type", () => {
    // The type is what decides the surface an artifact opens in (LIB-FR-03),
    // so it has to survive the flattening. The `flows` folder carries a type of
    // its own and must not be offered; `notes.txt` carries none.
    expect(artifactTargets(TREE)).toEqual([
      { id: "specs/a.md", name: "a.md", artifactType: "spec" },
      { id: "specs/b.md", name: "b.md", artifactType: "spec" },
      { id: "specs/README.md", name: "README.md", artifactType: "spec" },
      {
        id: "flows/release.flow",
        name: "release.flow",
        artifactType: "flow",
      },
      { id: "docs/README.md", name: "README.md", artifactType: "spec" },
    ]);
  });

  it("returns nothing for a tree with no artifact in it", () => {
    expect(
      artifactTargets({
        id: "",
        name: "empty",
        path: "",
        nodeKind: "folder",
        children: [],
      }),
    ).toEqual([]);
  });
});

describe("reminder instants (NTS-FR-20)", () => {
  it("round-trips a picked local time through the stored instant", () => {
    const picked = "2026-06-01T09:30";
    const stored = fromLocalInput(picked);
    expect(stored).not.toBeNull();
    expect(new Date(stored!).getTime()).toBe(new Date(picked).getTime());
    // And the stored instant seeds the picker back with the same wall clock,
    // so reopening it does not silently shift the reminder by the UTC offset.
    expect(toLocalInput(stored!)).toBe(picked);
  });

  it("treats an empty or unparseable value as no reminder", () => {
    expect(fromLocalInput("")).toBeNull();
    expect(fromLocalInput("not a date")).toBeNull();
    expect(toLocalInput(undefined)).toBe("");
    expect(toLocalInput("not a date")).toBe("");
  });
});
