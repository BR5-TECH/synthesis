// The Notes vertical panel (`specifications/ui/NTS-notes.md`), covering
// NTS-FR-01, NTS-FR-02 … NTS-FR-24. The backend is mocked at `invoke`, so every assertion
// here is about what the panel renders and which operation it invokes — the
// record, its timestamps and the unresolved marking belong to
// `NTC-notes-storage.md` and are covered by its own Rust tests.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Notes } from "./Notes";
import {
  pickSelector,
  selectorButton,
  selectorValue,
} from "../test/selectors";
import type { Discussion, NoteListItem, NotesPanelState } from "../types";
import {
  ARTIFACT_A,
  ARTIFACT_B,
  entityNote,
  Harness,
  openMenu,
  projectNote,
  rowFor,
  rows,
  scopeSelect,
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
// NTS-FR-01, NTS-FR-02 / NTS-FR-03 — the entity and project-wide positions
// ---------------------------------------------------------------------------

describe("scope positions (NTS-FR-02 / NTS-FR-03 / NTS-FR-01 / NTS-FR-03)", () => {
  it("binds to the active tab's entity and shows its notes", async () => {
    served.entity = [entityNote("1", "specs/a.md", "on A", "2026-05-01T09:00:00Z")];
    render(<Harness entity={ARTIFACT_A} />);

    expect(await screen.findByText("on A")).toBeInTheDocument();
    expect(calls("list_notes_for_entity")[0][1]).toEqual({
      entityId: "specs/a.md",
    });
    expect(selectorValue("Notes scope")).toBe("entity");
  });

  it("renders project-wide with the entity position unavailable when no tab is entity-bound", async () => {
    served.project = [projectNote("1", "project-wide", "2026-05-01T09:00:00Z")];
    render(<Harness entity={null} />);

    expect(await screen.findByText("project-wide")).toBeInTheDocument();
    expect(calls("list_project_notes")).toHaveLength(1);
    expect(calls("list_notes_for_entity")).toHaveLength(0);
    expect(selectorValue("Notes scope")).toBe("project");
    // The entity position is rendered but cannot be chosen — there is no
    // entity-scoped tab for it to bind to (NTS-FR-03 / NTS-FR-09).
    expect(selectorButton("Notes scope", "entity")).toBeDisabled();
    expect(selectorButton("Notes scope", "entity")).toHaveAttribute(
      "aria-label",
      "Current artifact",
    );
  });
});

// ---------------------------------------------------------------------------
// NTS-FR-08 — reacting to a tab change
// ---------------------------------------------------------------------------

describe("tab changes (NTS-FR-08)", () => {
  it("rebinds to the new tab's entity in the entity position", async () => {
    served.entity = [entityNote("1", "specs/a.md", "on A", "2026-05-01T09:00:00Z")];
    const { rerender } = render(<Harness entity={ARTIFACT_A} />);
    await screen.findByText("on A");

    served.entity = [entityNote("2", "specs/b.md", "on B", "2026-05-01T09:00:00Z")];
    rerender(<Harness entity={ARTIFACT_B} />);

    expect(await screen.findByText("on B")).toBeInTheDocument();
    expect(calls("list_notes_for_entity").map((c) => c[1])).toEqual([
      { entityId: "specs/a.md" },
      { entityId: "specs/b.md" },
    ]);
  });

  it("leaves the rendered list alone in the all-notes position", async () => {
    served.panelState = { scopePosition: "all", textFilter: "" };
    served.all = [projectNote("1", "unaffected", "2026-05-01T09:00:00Z")];
    const { rerender } = render(<Harness entity={ARTIFACT_A} />);
    await screen.findByText("unaffected");
    expect(calls("list_all_notes")).toHaveLength(1);

    rerender(<Harness entity={ARTIFACT_B} />);

    expect(screen.getByText("unaffected")).toBeInTheDocument();
    expect(calls("list_all_notes")).toHaveLength(1);
    expect(calls("list_notes_for_entity")).toHaveLength(0);
  });

  it("falls back to project-wide when the new tab is not entity-bound", async () => {
    served.entity = [entityNote("1", "specs/a.md", "on A", "2026-05-01T09:00:00Z")];
    served.project = [projectNote("2", "project-wide", "2026-05-01T09:00:00Z")];
    const { rerender } = render(<Harness entity={ARTIFACT_A} />);
    await screen.findByText("on A");

    rerender(<Harness entity={null} />);

    expect(await screen.findByText("project-wide")).toBeInTheDocument();
    expect(selectorValue("Notes scope")).toBe("project");
  });
});

// ---------------------------------------------------------------------------
// NTS-FR-09 / NTS-FR-13 — stickiness
// ---------------------------------------------------------------------------

describe("sticky selector and filter (NTS-FR-09 / NTS-FR-13)", () => {
  it("mounts in the persisted position (NTS-FR-09)", async () => {
    served.panelState = { scopePosition: "all", textFilter: "" };
    served.all = [projectNote("1", "everything", "2026-05-01T09:00:00Z")];
    render(<Harness entity={ARTIFACT_A} />);

    expect(await screen.findByText("everything")).toBeInTheDocument();
    expect(selectorValue("Notes scope")).toBe("all");
    expect(calls("list_all_notes")).toHaveLength(1);
  });

  it("persists a change of position", async () => {
    render(<Harness entity={ARTIFACT_A} />);
    await screen.findByLabelText("Notes scope");

    await pickSelector("Notes scope", "all");

    await waitFor(
      () =>
        expect(calls("save_notes_panel_state")[0][1]).toEqual({
          state: { scopePosition: "all", textFilter: "" },
        }),
      { timeout: 2000 },
    );
  });

  it("renders project-wide over a persisted entity position without rewriting it (NTS-FR-09)", async () => {
    served.panelState = { scopePosition: "entity", textFilter: "" };
    served.project = [projectNote("1", "project-wide", "2026-05-01T09:00:00Z")];
    const { rerender } = render(<Harness entity={null} />);
    await screen.findByText("project-wide");

    expect(selectorValue("Notes scope")).toBe("project");
    // Nothing was persisted: the stored position is still `entity`…
    expect(calls("save_notes_panel_state")).toHaveLength(0);

    // …so opening an artifact tab returns the panel to that entity's notes.
    served.entity = [entityNote("2", "specs/a.md", "on A", "2026-05-01T09:00:00Z")];
    rerender(<Harness entity={ARTIFACT_A} />);
    expect(await screen.findByText("on A")).toBeInTheDocument();
    expect(selectorValue("Notes scope")).toBe("entity");
  });

  it("coalesces a run of filter keystrokes into one write", async () => {
    served.project = [projectNote("1", "alpha", "2026-05-01T09:00:00Z")];
    render(<Harness entity={null} />);
    await screen.findByText("alpha");

    // Typed with no inter-key delay, so a slow CI machine cannot split the
    // burst across the 300 ms debounce window and turn one write into two.
    await userEvent.type(screen.getByLabelText("Filter notes"), "alph", {
      delay: null,
    });

    await waitFor(
      () => expect(calls("save_notes_panel_state")).toHaveLength(1),
      { timeout: 2000 },
    );
    // The write carries the *persisted* position, not the rendered fallback:
    // typing in the filter must not quietly downgrade a stored `entity` to the
    // `project` the panel happens to be showing (NTS-FR-09).
    expect(calls("save_notes_panel_state")[0][1]).toEqual({
      state: { scopePosition: "entity", textFilter: "alph" },
    });
  });

  it("restores the persisted filter text and narrows the list with it (NTS-FR-13)", async () => {
    served.panelState = { scopePosition: "project", textFilter: "alpha" };
    served.project = [
      projectNote("1", "alpha note", "2026-05-02T09:00:00Z"),
      projectNote("2", "beta note", "2026-05-01T09:00:00Z"),
    ];
    render(<Harness entity={null} />);

    expect(await screen.findByText("alpha note")).toBeInTheDocument();
    expect(screen.getByLabelText("Filter notes")).toHaveValue("alpha");
    expect(screen.queryByText("beta note")).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// NTS-FR-10, NTS-FR-15 / NTS-FR-11 — the all-notes position
// ---------------------------------------------------------------------------

describe("the all-notes position (NTS-FR-10 / NTS-FR-11 / NTS-FR-15)", () => {
  beforeEach(() => {
    served.panelState = { scopePosition: "all", textFilter: "" };
    served.all = [
      entityNote("1", "specs/a.md", "A newer", "2026-05-04T09:00:00Z"),
      entityNote("2", "specs/a.md", "A older", "2026-05-01T09:00:00Z"),
      entityNote("3", "specs/b.md", "B note", "2026-05-03T09:00:00Z"),
      projectNote("4", "project note", "2026-05-02T09:00:00Z"),
    ];
  });

  it("groups by entity, ordering groups and rows most-recently-edited first (NTS-FR-10, NTS-FR-15)", async () => {
    render(<Harness entity={null} />);
    await screen.findByText("A newer");

    expect(calls("list_all_notes")).toHaveLength(1);
    const headers = Array.from(
      document.querySelectorAll(".note-group__header"),
    ).map((el) => el.textContent);
    expect(headers).toEqual(["a.md", "b.md", "Project"]);
    expect(rows().map((r) => r.getAttribute("data-note-id"))).toEqual([
      "1",
      "2",
      "3",
      "4",
    ]);
  });

  it("follows an entity group header to its artifact, and no other header (NTS-FR-11)", async () => {
    const onOpenArtifact = vi.fn();
    render(<Harness entity={null} onOpenArtifact={onOpenArtifact} />);
    await screen.findByText("A newer");

    await userEvent.click(screen.getByText("a.md"));
    // The artifact's TYPE travels with it: `openArtifact` routes on it, so
    // without it every entity would open in an Editor tab (LIB-FR-03).
    await waitFor(() =>
      expect(onOpenArtifact).toHaveBeenCalledWith({
        id: "specs/a.md",
        name: "a.md",
        artifactType: "spec",
      }),
    );

    onOpenArtifact.mockClear();
    // The Project *group header*, not the scope selector's Project position —
    // both read "Project", and it is the header that must not be a link.
    await userEvent.click(
      screen
        .getAllByText("Project")
        .find((el) => el.classList.contains("note-group__header"))!,
    );
    expect(onOpenArtifact).not.toHaveBeenCalled();
  });

  it("routes a Flow group header to its own surface (NTS-FR-11)", async () => {
    served.all = [
      entityNote("9", "flows/release.flow", "on the flow", "2026-05-05T09:00:00Z"),
    ];
    const onOpenArtifact = vi.fn();
    render(<Harness entity={null} onOpenArtifact={onOpenArtifact} />);
    await screen.findByText("on the flow");

    await userEvent.click(screen.getByText("release.flow"));

    await waitFor(() =>
      expect(onOpenArtifact).toHaveBeenCalledWith({
        id: "flows/release.flow",
        name: "release.flow",
        // A Flow opens in a Flow tab, not an Editor one.
        artifactType: "flow",
      }),
    );
  });

  it("keeps two entities sharing a basename in separate groups (NTS-FR-10)", async () => {
    served.all = [
      entityNote("7", "specs/README.md", "spec readme", "2026-05-06T09:00:00Z"),
      entityNote("8", "docs/README.md", "docs readme", "2026-05-05T09:00:00Z"),
    ];
    const onOpenArtifact = vi.fn();
    render(<Harness entity={null} onOpenArtifact={onOpenArtifact} />);
    await screen.findByText("spec readme");

    // Two groups, not one — and each header carries the path that tells them
    // apart, since both render the same basename.
    const headers = Array.from(
      document.querySelectorAll(".note-group__header"),
    ) as HTMLElement[];
    expect(headers.map((h) => h.textContent)).toEqual(["README.md", "README.md"]);
    expect(headers.map((h) => h.getAttribute("title"))).toEqual([
      "specs/README.md",
      "docs/README.md",
    ]);
    expect(document.querySelectorAll(".note-group")).toHaveLength(2);

    // And each header routes to its own artifact.
    await userEvent.click(headers[1]);
    await waitFor(() =>
      expect(onOpenArtifact).toHaveBeenCalledWith({
        id: "docs/README.md",
        name: "README.md",
        artifactType: "spec",
      }),
    );
  });

  it("filters client-side, hiding a group left with nothing (NTS-FR-12)", async () => {
    render(<Harness entity={null} />);
    await screen.findByText("A newer");
    const before = invokeMock.mock.calls.length;

    await userEvent.type(screen.getByLabelText("Filter notes"), "A newer");

    await waitFor(() => expect(rows()).toHaveLength(1));
    expect(
      Array.from(document.querySelectorAll(".note-group__header")).map(
        (el) => el.textContent,
      ),
    ).toEqual(["a.md"]);
    // No backend call was made to narrow the list; the only ones since are the
    // debounced persistence of the filter text itself.
    expect(
      invokeMock.mock.calls
        .slice(before)
        .filter((c) => c[0] !== "save_notes_panel_state"),
    ).toEqual([]);

    // And a note also matches on its entity's name.
    await userEvent.clear(screen.getByLabelText("Filter notes"));
    await userEvent.type(screen.getByLabelText("Filter notes"), "b.md");
    await waitFor(() => expect(rows()).toHaveLength(1));
    expect(screen.getByText("B note")).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// NTS-FR-14 — timestamps
// ---------------------------------------------------------------------------

describe("timestamps (NTS-FR-14)", () => {
  it("shows a relative last-edited time that discloses both absolutes on hover", async () => {
    const twoHoursAgo = new Date(Date.now() - 2 * 3600 * 1000).toISOString();
    served.project = [projectNote("1", "recent", twoHoursAgo)];
    served.project[0].note.createdAt = "2026-01-01T09:00:00Z";
    render(<Harness entity={null} />);
    await screen.findByText("recent");

    const when = rowFor("recent").querySelector(".note__when") as HTMLElement;
    expect(when.textContent).toBe("2h ago");
    const title = when.getAttribute("title") ?? "";
    expect(title).toContain("Created");
    expect(title).toContain("Last edited");
    expect(title).toContain(new Date("2026-01-01T09:00:00Z").toLocaleString());
    expect(title).toContain(new Date(twoHoursAgo).toLocaleString());
  });
});

// ---------------------------------------------------------------------------
// NTS-FR-11, NTS-FR-23, SNV-FR-57, CMP-FR-12 — a group header naming an entity keeps the file's on-disk case
// ---------------------------------------------------------------------------

describe("entity group headers keep their case (SNV-FR-57 / NTS-FR-11, NTS-FR-23, CMP-FR-12)", () => {
  it("does not case-transform a filename, while label headers keep the eyebrow", async () => {
    served.panelState = { scopePosition: "all", textFilter: "" };
    served.all = [
      entityNote("1", "specs/LIB-library.md", "a", "2026-05-02T09:00:00Z"),
      projectNote("2", "b", "2026-05-01T09:00:00Z"),
    ];
    render(<Harness entity={null} />);
    await screen.findByText("a");

    const headers = Array.from(
      document.querySelectorAll<HTMLElement>(".note-group__header"),
    );
    const entityHeader = headers.find((h) => h.textContent === "LIB-library.md")!;
    const projectHeader = headers.find((h) => h.textContent === "Project")!;

    // The entity header carries the modifier that turns the eyebrow's
    // uppercasing off; the Project header, being a label, does not.
    expect(entityHeader).toHaveClass("note-group__header--entity");
    expect(projectHeader).not.toHaveClass("note-group__header--entity");
    // And the rendered text is the file's own case, not LIB-LIBRARY.MD.
    expect(entityHeader.textContent).toBe("LIB-library.md");
  });
});

// ---------------------------------------------------------------------------
// The empty state
// ---------------------------------------------------------------------------

describe("the empty state", () => {
  it("centres its message in the space the list would fill", async () => {
    render(<Harness entity={null} />);

    expect(await screen.findByText("No project-wide notes yet.")).toBeInTheDocument();
    // The scroll container does the centring, so the message cannot overflow
    // that container's padding and raise a scrollbar over an empty panel.
    expect(document.querySelector(".notes__body--empty")).not.toBeNull();
  });

  it("centres the filtered-to-nothing message too", async () => {
    // `items` is non-empty here — the list is empty for a different reason,
    // and the panel must not look broken in that case either.
    served.panelState = { scopePosition: "project", textFilter: "zzz" };
    served.project = [projectNote("1", "a note", "2026-05-01T09:00:00Z")];
    render(<Harness entity={null} />);

    expect(await screen.findByText("No note matches the filter.")).toBeInTheDocument();
    expect(document.querySelector(".notes__body--empty")).not.toBeNull();
  });

  it("stops centring once the list has rows", async () => {
    served.project = [projectNote("1", "a note", "2026-05-01T09:00:00Z")];
    render(<Harness entity={null} />);
    await screen.findByText("a note");

    expect(document.querySelector(".notes__body--empty")).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// Failure paths
// ---------------------------------------------------------------------------

describe("errors", () => {
  it("attaches a failed row operation to the row that triggered it", async () => {
    served.project = [projectNote("1", "a note", "2026-05-01T09:00:00Z")];
    render(<Harness entity={null} />);
    await screen.findByText("a note");

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "delete_note") throw "note not found";
      if (cmd === "list_project_notes") return served.project;
      if (cmd === "load_notes_panel_state") return served.panelState;
      return undefined;
    });
    await userEvent.click(within(await openMenu("a note")).getByText("Delete"));
    await userEvent.click(screen.getByRole("button", { name: "Delete" }));

    expect(
      await within(rowFor("a note")).findByText("note not found"),
    ).toBeInTheDocument();
  });

  it("reports a load failure instead of rendering an empty panel", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "load_notes_panel_state")
        return { scopePosition: "project", textFilter: "" };
      if (cmd === "list_project_notes") throw "no project open";
      return undefined;
    });
    render(<Harness entity={null} />);

    expect(await screen.findByText("no project open")).toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// SNV-FR-58
// ---------------------------------------------------------------------------

describe("filter-row arrangement (SNV-FR-58)", () => {
  it("puts the text filter above the scope selector, both above the list", async () => {
    served.panelState = { scopePosition: "all", textFilter: "" };
    served.all = [projectNote("1", "a note", "2026-05-01T09:00:00Z")];
    render(<Harness entity={null} />);
    await screen.findByText("a note");

    const controls = document.querySelector(".panel-controls")!;
    const scope = scopeSelect();
    const filter = screen.getByLabelText("Filter notes");
    const body = document.querySelector(".vpanel__body")!;

    expect(controls).toContainElement(scope);
    expect(controls).toContainElement(filter);
    expect(
      filter.compareDocumentPosition(scope) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    expect(
      controls.compareDocumentPosition(body) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });
});

// ---------------------------------------------------------------------------
// SNV-FR-62: the scope selector's form — a row of toggle buttons
// rather than a dropdown, with the full name reachable behind a shortened tag.
// ---------------------------------------------------------------------------

describe("the scope selector's form (SNV-FR-62)", () => {
  it("is three toggle buttons, one active, and opens nothing on activation", async () => {
    render(<Harness entity={ARTIFACT_A} />);
    await screen.findByRole("radiogroup", { name: "Notes scope" });

    const buttons = within(scopeSelect()).getAllByRole("radio");
    expect(buttons).toHaveLength(3);
    expect(buttons.every((b) => b.tagName === "BUTTON")).toBe(true);
    expect(scopeSelect().querySelector("select")).toBeNull();
    expect(
      buttons.filter((b) => b.getAttribute("aria-checked") === "true"),
    ).toHaveLength(1);
    expect(selectorValue("Notes scope")).toBe("entity");

    // Activating a button that is not the active one makes it the active one,
    // and the previously active button is no longer marked.
    await pickSelector("Notes scope", "all");
    expect(selectorValue("Notes scope")).toBe("all");
    expect(selectorButton("Notes scope", "entity")).toHaveAttribute(
      "aria-checked",
      "false",
    );
    // Nothing opened: there is no listbox, menu, or dropdown in the panel.
    expect(screen.queryByRole("listbox")).not.toBeInTheDocument();
  });

  it("shortens the entity position's tag and discloses its full name", async () => {
    render(<Harness entity={ARTIFACT_A} />);
    await screen.findByRole("radiogroup", { name: "Notes scope" });

    const entity = selectorButton("Notes scope", "entity");
    // The tag stays fixed so the control does not resize with the active tab…
    expect(entity.querySelector(".chip-type")).toHaveTextContent("This file");
    // …and the artifact's own name is what the tooltip and the accessible name
    // carry, which is the half a shortened tag would otherwise lose.
    expect(entity.querySelector(".selector-row__tip")).toHaveTextContent("a.md");
    expect(entity).toHaveAttribute("aria-label", "a.md");

    expect(
      selectorButton("Notes scope", "project").querySelector(
        ".selector-row__tip",
      ),
    ).toHaveTextContent("Project-Wide Notes");
    expect(
      selectorButton("Notes scope", "all").querySelector(".selector-row__tip"),
    ).toHaveTextContent("All Notes");
  });
});

// ---------------------------------------------------------------------------
// NTS-FR-09 / SNV-FR-62: the entity position binds to whatever tab is active,
// and the tooltip is the only thing that says which — the tag is fixed so the
// control does not resize every time the author changes tab.
// ---------------------------------------------------------------------------

describe("the entity position's disclosure follows the active tab", () => {
  it("renames the tooltip on a tab change without moving the row", async () => {
    const { rerender } = render(<Harness entity={ARTIFACT_A} />);
    await screen.findByRole("radiogroup", { name: "Notes scope" });

    const before = selectorButton("Notes scope", "entity");
    expect(before.querySelector(".selector-row__tip")).toHaveTextContent("a.md");

    rerender(<Harness entity={ARTIFACT_B} />);

    const after = selectorButton("Notes scope", "entity");
    expect(after.querySelector(".selector-row__tip")).toHaveTextContent("b.md");
    expect(after).toHaveAttribute("aria-label", "b.md");
    // The visible tag did not change with it — that is what keeps a long
    // filename from resizing the control (NTS-FR-09).
    expect(after.querySelector(".chip-type")).toHaveTextContent("This file");
  });
});
