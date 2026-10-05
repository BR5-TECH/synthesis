/**
 * The Library panel's filesystem tree: how it renders, what the lens and the
 * text filter admit, and how it reloads (`../../specifications/ui/LIB-library.md`).
 *
 * The context-menu actions, the persisted panel state, the reveal, and the
 * lens row each live in a `Library.<topic>.test.tsx` sibling.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  cleanup,
  fireEvent,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
const unlistenMock = vi.fn();
// Captured event subscribers, keyed by event name, so a test can fire the
// backend `"project tree changed"` event by hand.
let listeners: Record<string, (event: { payload: TreeChangedPayload }) => void> =
  {};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    async (
      event: string,
      cb: (event: { payload: TreeChangedPayload }) => void,
    ) => {
      listeners[event] = cb;
      return unlistenMock;
    },
  ),
}));

import {
  baseTree,
  allFolderIds,
  expandedAll,
  file,
  folder,
  makeTreeStubs,
  panelState,
  renderLibrary,
} from "../test/libraryFixtures";
import type { TreeChangedPayload } from "../types";
import { PROJECT_TREE_CHANGED } from "../events";
import { ARTIFACT_TYPES } from "../artifactTypes";
import {
  pickSelector,
  selectorNames,
  selectorValues,
} from "../test/selectors";
import { resetPanelReveals } from "../state/panelReveal";

const { mockInvoke, setLoadTree } = makeTreeStubs(invokeMock);

beforeEach(() => {
  // Module-level, like the preferences cache: reveal nonces are minted from
  // one counter for the app's life, so a suite that does not reset finds its
  // first request already consumed by the previous one.
  resetPanelReveals();
  invokeMock.mockReset();
  unlistenMock.mockReset();
  listeners = {};
});

afterEach(() => {
  cleanup();
});

describe("Library filesystem tree", () => {
  // LIB-FR-07: tree mirrors on-disk nesting; no separate "Flows" group.
  it("renders folders and files nested as on disk, with no Flows group", async () => {
    setLoadTree(baseTree());
    renderLibrary();

    // Folders + files appear. Children show because the persisted state this
    // fixture supplies has every folder expanded (LIB-FR-15) — folders are not
    // expanded by default.
    expect(await screen.findByText(".claude")).toBeInTheDocument();
    expect(screen.getByText("skills")).toBeInTheDocument();
    expect(screen.getByText("onboarding.md")).toBeInTheDocument();
    expect(screen.getByText("specifications")).toBeInTheDocument();
    expect(screen.getByText("AGENTS.md")).toBeInTheDocument();
    // The Flow appears at its natural location — there is no "Flows" group.
    expect(screen.getByText("review.flow")).toBeInTheDocument();
    expect(screen.queryByText("Flows")).not.toBeInTheDocument();
  });

  // LIB-FR-03: clicking a non-Flow file asks to open it (App routes it to
  // an Editor tab via the resolved artifact type).
  it("opens a non-Flow file with its resolved type", async () => {
    setLoadTree(baseTree());
    const { onOpenArtifact } = renderLibrary();

    fireEvent.click(await screen.findByText("onboarding.md"));
    expect(onOpenArtifact).toHaveBeenCalledWith(
      expect.objectContaining({ name: "onboarding.md", artifactType: "skill" }),
    );
  });

  // LIB-FR-03: clicking a Flow file asks to open it (App routes a Flow to a
  // Flow tab). What makes it a Flow is its resolved type, not its name — a Flow
  // the user assigned onto a file called anything opens on the canvas too, and
  // a test whose fixture always spelled `.flow` would keep passing if the
  // routing went back to reading the filename.
  it("opens a Flow file by its resolved type, whatever it is named", async () => {
    setLoadTree({
      ...baseTree(),
      children: [
        ...(baseTree().children ?? []),
        file("workflows/pipeline", "flow", "assigned"),
      ],
    });
    const { onOpenArtifact } = renderLibrary();

    fireEvent.click(await screen.findByText("review.flow"));
    expect(onOpenArtifact).toHaveBeenCalledWith(
      expect.objectContaining({ name: "review.flow", artifactType: "flow" }),
    );

    fireEvent.click(await screen.findByText("pipeline"));
    expect(onOpenArtifact).toHaveBeenCalledWith(
      expect.objectContaining({ name: "pipeline", artifactType: "flow" }),
    );
  });

  // LIB-FR-05, LIB-FR-06: type filter hides non-matching nodes; expand state preserved.
  it("filters by artifact type, hiding non-matching nodes", async () => {
    setLoadTree(baseTree());
    renderLibrary();
    await screen.findByText("onboarding.md");

    await pickSelector("Filter by type", "skill");

    // The skill survives; an agent + unclassified files are hidden.
    expect(screen.getByText("onboarding.md")).toBeInTheDocument();
    expect(screen.queryByText("AGENTS.md")).not.toBeInTheDocument();
    expect(screen.queryByText("notes.txt")).not.toBeInTheDocument();
    // The skill's ancestor folders remain expanded (state preserved).
    expect(screen.getByText(".claude")).toBeInTheDocument();
    expect(screen.getByText("skills")).toBeInTheDocument();
  });

  // LIB-FR-08, ASC-FR-18: classified file shows its type tag; unclassified shows none.
  // The unclassified file only surfaces under the "All files" lens (LIB-FR-12),
  // so switch to it before asserting the no-tag case.
  it("shows a type tag for classified files and none for unclassified", async () => {
    setLoadTree(baseTree());
    renderLibrary();

    const specRow = (await screen.findByText("specifications/LIB-library.md".split("/").pop()!))
      .closest(".tree-row")!;
    expect(within(specRow as HTMLElement).getByText("SPC")).toBeInTheDocument();

    await pickSelector("Filter by type", "files");
    const plainRow = screen.getByText("notes.txt").closest(".tree-row")!;
    expect(
      (plainRow as HTMLElement).querySelector(".chip-type"),
    ).toBeNull();
  });

  // LIB-FR-05, LIB-FR-09: a folder whose subtree has no artifacts (only unclassified files)
  // is hidden under the default "All artifacts" lens; "All files" reveals the
  // folder together with its unclassified files.
  it("hides artifact-less folders by default and reveals them under All files", async () => {
    setLoadTree(baseTree());
    renderLibrary();
    await screen.findByText("AGENTS.md");

    // `misc` holds only the unclassified `todo.txt` -> hidden by default.
    expect(screen.queryByText("misc")).not.toBeInTheDocument();
    expect(screen.queryByText("todo.txt")).not.toBeInTheDocument();

    await pickSelector("Filter by type", "files");
    expect(screen.getByText("misc")).toBeInTheDocument();
    expect(screen.getByText("todo.txt")).toBeInTheDocument();
  });

  // LCM-FR-04: assign a folder type via the context menu.
  it("assigns a folder type and re-tags its files without a manual reload", async () => {
    // After assignment, the backend returns a tree where misc's file is a spec.
    const reassigned = folder("", true, [
      folder("misc", true, [file("misc/todo.txt", "spec", "inherited")]),
    ]);
    mockInvoke(
      {
        load_project_tree: () => baseTree(),
        assign_artifact_type: () => reassigned,
      },
      expandedAll(baseTree()),
    );
    renderLibrary();
    await screen.findByText("AGENTS.md");

    // Reveal the artifact-less folder (via the "All files" lens) so it can be
    // right-clicked.
    await pickSelector("Filter by type", "files");
    fireEvent.contextMenu(screen.getByText("misc"));

    // Choose "Artifact Type -> Spec" from the nested submenu (LCM-FR-04), scoped
    // to the menu to avoid the filter dropdown's own "Spec" option.
    const menu = document.querySelector(".menu") as HTMLElement;
    fireEvent.click(within(menu).getByText("Artifact Type"));
    fireEvent.click(within(menu).getByText("Spec"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("assign_artifact_type", {
        path: "misc",
        artifactType: "spec",
        scope: "folder",
      }),
    );
    // The re-tagged file now renders with a Spec tag.
    const fileRow = (await screen.findByText("todo.txt")).closest(".tree-row")!;
    expect(within(fileRow as HTMLElement).getByText("SPC")).toBeInTheDocument();
  });

  // LIB-FR-06, LIB-FR-10: a `"project tree changed"` event reloads the tree, preserving
  // expand/collapse state.
  it("reloads on the project-tree-changed event, preserving expand state", async () => {
    const withNewFile = folder("", true, [
      folder(".claude", true, [
        folder(".claude/skills", true, [
          file(".claude/skills/onboarding.md", "skill", "inferred"),
        ]),
      ]),
      file("created-externally.md", "spec", "inferred"),
    ]);
    mockInvoke(
      {
        load_project_tree: () =>
          // First load = base tree; subsequent loads (after the event) include
          // the externally-created file.
          invokeMock.mock.calls.filter((c) => c[0] === "load_project_tree")
            .length === 1
            ? baseTree()
            : withNewFile,
      },
      expandedAll(baseTree()),
    );
    renderLibrary();
    await screen.findByText("onboarding.md");

    // Click `.claude`: this both selects it and collapses it, so we can prove
    // the reload preserves BOTH selection and expand state (LIB-FR-06).
    fireEvent.click(screen.getByText(".claude"));
    expect(screen.queryByText("onboarding.md")).not.toBeInTheDocument();
    expect(
      screen.getByText(".claude").closest(".tree-row"),
    ).toHaveAttribute("data-selected", "true");

    // Fire the backend event.
    listeners[PROJECT_TREE_CHANGED]?.({ payload: { changeCount: 1 } });

    // New file appears…
    expect(await screen.findByText("created-externally.md")).toBeInTheDocument();
    // …`.claude` is still collapsed (expand state preserved)…
    expect(screen.queryByText("onboarding.md")).not.toBeInTheDocument();
    // …and still selected (selection preserved).
    expect(
      screen.getByText(".claude").closest(".tree-row"),
    ).toHaveAttribute("data-selected", "true");
  });

  // LIB-FR-05 corner (AND-combination): under the "All files" lens a text filter
  // surfaces a matching unclassified file inside an otherwise-hidden folder,
  // while non-matching top-level files are hidden by the same text filter.
  it("AND-combines the All files lens with a text filter to reveal one file", async () => {
    setLoadTree(baseTree());
    renderLibrary();
    await screen.findByText("AGENTS.md");

    // `misc` holds only the unclassified `todo.txt` and is hidden by default.
    expect(screen.queryByText("misc")).not.toBeInTheDocument();

    await pickSelector("Filter by type", "files");
    await userEvent.type(screen.getByLabelText("Filter tree"), "todo");

    // The unclassified file is now reachable and its folder is shown for it.
    expect(screen.getByText("misc")).toBeInTheDocument();
    expect(screen.getByText("todo.txt")).toBeInTheDocument();
    // Non-matching top-level files are hidden by the AND-combined text filter.
    expect(screen.queryByText("AGENTS.md")).not.toBeInTheDocument();
  });

  // LIB-FR-12: the default "All artifacts" lens shows classified artifacts and
  // hides unclassified tooling/config files (the reported bug fix, LIB-FR-12).
  it("hides unclassified files under the default All artifacts lens", async () => {
    setLoadTree(baseTree());
    renderLibrary();

    // Classified artifacts at the root are shown…
    expect(await screen.findByText("AGENTS.md")).toBeInTheDocument();
    expect(screen.getByText("review.flow")).toBeInTheDocument();
    // …while the unclassified root file is hidden by default.
    expect(screen.queryByText("notes.txt")).not.toBeInTheDocument();
  });

  // LIB-FR-08: the "All files" lens shows every file, unclassified ones included
  // and rendered without a type tag (LIB-FR-08).
  it("shows every file under the All files lens, unclassified without a tag", async () => {
    setLoadTree(baseTree());
    renderLibrary();
    await screen.findByText("AGENTS.md");

    await pickSelector("Filter by type", "files");

    // Previously-hidden unclassified files and their folders now appear…
    const plainRow = screen.getByText("notes.txt").closest(".tree-row")!;
    expect(plainRow).toBeInTheDocument();
    expect(screen.getByText("todo.txt")).toBeInTheDocument();
    // …with no type tag on the unclassified file.
    expect((plainRow as HTMLElement).querySelector(".chip-type")).toBeNull();
    // Classified files still render with their tag.
    const agentRow = screen.getByText("AGENTS.md").closest(".tree-row")!;
    expect(within(agentRow as HTMLElement).getByText("AGT")).toBeInTheDocument();
  });

  // LIB-FR-05: a specific-type lens AND-combines with the text filter — a
  // name-matching file of the WRONG type is excluded (proves AND, not OR).
  it("AND-combines a specific-type lens with text, excluding wrong-type matches", async () => {
    // Two files whose names both contain "lib": one spec, one agent.
    const tree = folder("", true, [
      folder("specifications", true, [
        file("specifications/LIB-library.md", "spec", "inferred"),
      ]),
      file("lib-agent.md", "agent", "inferred"),
    ]);
    setLoadTree(tree);
    renderLibrary();
    await screen.findByText("LIB-library.md");

    await pickSelector("Filter by type", "spec");
    await userEvent.type(screen.getByLabelText("Filter tree"), "lib");

    // The spec named "lib…" survives; the agent named "lib…" is excluded by the
    // type half of the AND.
    expect(screen.getByText("LIB-library.md")).toBeInTheDocument();
    expect(screen.queryByText("lib-agent.md")).not.toBeInTheDocument();
  });

  // LIB-FR-09: under a specific-type lens, a folder whose only descendants are
  // a DIFFERENT classified type is hidden.
  it("hides a folder of a different classified type under a specific lens", async () => {
    setLoadTree(baseTree());
    renderLibrary();
    await screen.findByText("onboarding.md");

    await pickSelector("Filter by type", "skill");

    // The skill survives; `specifications/` (only a spec inside) is hidden.
    expect(screen.getByText("onboarding.md")).toBeInTheDocument();
    expect(screen.queryByText("specifications")).not.toBeInTheDocument();
    expect(screen.queryByText("LIB-library.md")).not.toBeInTheDocument();
  });

  // LIB-FR-09 v1 consequence: a directory with no files at any depth has no
  // visible descendant and is not surfaced under ANY lens, including All files.
  // LIB-FR-09: an empty folder is surfaced rather than swallowed. A
  // typed one is visible under **All artifacts** and its own type lens from the
  // moment it exists; an untyped one is visible under **All files**. Without
  // this, creating a folder (NFW-FR-11) would look like nothing happened.
  it("surfaces an empty folder by its own type, and an untyped one under All files", async () => {
    // `typed` carries a folder-scope `spec` assignment (ASC-FR-18) and holds
    // nothing; `hollow` carries no assignment and holds nothing at any depth.
    const typed = folder("typed", true, []);
    typed.artifactType = "spec";
    typed.typeSource = "assigned";
    const tree = folder("", true, [
      file("AGENTS.md", "agent", "inferred"),
      // Gives the Prompt lens a button to exist under (LIB-FR-19); it is the
      // lens below that matches neither `typed` nor `hollow`.
      file("ask.md", "prompt", "inferred"),
      typed,
      folder("hollow", false, [folder("hollow/sub", false, [])]),
    ]);
    setLoadTree(tree);
    renderLibrary();
    await screen.findByText("AGENTS.md");

    // All artifacts (the default): the typed folder shows, the untyped one does
    // not.
    expect(screen.getByText("typed")).toBeInTheDocument();
    expect(screen.queryByText("hollow")).not.toBeInTheDocument();

    // Its own type lens: still shown.
    await pickSelector("Filter by type", "spec");
    expect(screen.getByText("typed")).toBeInTheDocument();
    expect(screen.queryByText("hollow")).not.toBeInTheDocument();

    // A different type lens: the folder's own assignment does not match, and it
    // has no descendant that does, so it is hidden.
    await pickSelector("Filter by type", "agent");
    expect(screen.queryByText("typed")).not.toBeInTheDocument();

    // A lens matching neither the folder's own type nor anything inside it hides
    // both. It admits `ask.md`, which is why the button exists at all — the
    // point here is the two folders, not an empty tree.
    await pickSelector("Filter by type", "prompt");
    expect(screen.getByText("ask.md")).toBeInTheDocument();
    expect(screen.queryByText("typed")).not.toBeInTheDocument();
    expect(screen.queryByText("hollow")).not.toBeInTheDocument();

    // All files constrains nothing, so both appear.
    await pickSelector("Filter by type", "files");
    expect(screen.getByText("typed")).toBeInTheDocument();
    expect(screen.getByText("hollow")).toBeInTheDocument();
  });

  // LIB-FR-09: a folder is also eligible on its own NAME under the text filter, so
  // a folder whose name matches is surfaced even when nothing inside it does.
  it("surfaces a folder whose own name matches the text filter", async () => {
    const tree = folder("", true, [
      folder("specifications", true, [file("specifications/x.md", "spec", "inferred")]),
      file("AGENTS.md", "agent", "inferred"),
    ]);
    setLoadTree(tree, panelState({ expandedPaths: allFolderIds(tree) }));
    renderLibrary();
    await screen.findByText("specifications");

    await pickSelector("Filter by type", "files");
    await userEvent.type(screen.getByLabelText("Filter tree"), "specif");

    // The folder matches by name; its child does not match, so it is filtered out
    // of the folder's rendered children.
    expect(screen.getByText("specifications")).toBeInTheDocument();
    expect(screen.queryByText("x.md")).not.toBeInTheDocument();
    expect(screen.queryByText("AGENTS.md")).not.toBeInTheDocument();
  });

  // LIB-FR-08: a folder carrying its own folder-scope assignment wears the tag,
  // so the type it imposes on its contents is legible from the tree. A folder
  // with none is untagged whatever its contents resolve to.
  it("tags a folder with the type it carries, and leaves an unassigned one untagged", async () => {
    const typed = folder("drafts", true, []);
    typed.artifactType = "scenario";
    typed.typeSource = "assigned";
    const tree = folder("", true, [
      typed,
      // Holds an `agent` file but carries no assignment of its own.
      folder("src", true, [file("src/AGENTS.md", "agent", "inferred")]),
    ]);
    setLoadTree(tree);
    renderLibrary();

    const draftsRow = (await screen.findByText("drafts")).closest(".tree-row")!;
    const chip = draftsRow.querySelector(".chip-type")!;
    expect(chip).toHaveAttribute("data-type", "scenario");
    expect(chip).toHaveAttribute("title", "scenario (assigned)");

    const srcRow = screen.getByText("src").closest(".tree-row")!;
    expect(srcRow.querySelector(".chip-type")).toBeNull();
  });

  // LIB-FR-04 / LIB-FR-19: the lens positions, in order — All Artifacts
  // leading, the types the tree actually holds, All Files trailing — each
  // reading with its words capitalised like every other label in the app.
  it("offers All Artifacts, the present types, then All Files — title-cased", async () => {
    // `baseTree` holds a skill, a spec, an agent, and a flow, and nothing of
    // the other four types.
    setLoadTree(baseTree());
    renderLibrary();
    await screen.findByText("AGENTS.md");

    expect(selectorNames("Filter by type")).toEqual([
      "All Artifacts",
      "Skill",
      "Agent",
      "Spec",
      "Flow",
      "All Files",
    ]);
    // The order is ARTIFACT_TYPES' own, filtered — not the order the tree
    // happened to mention them in.
    expect(selectorValues("Filter by type")).toEqual([
      "artifacts",
      ...ARTIFACT_TYPES.filter((t) =>
        ["skill", "agent", "spec", "flow"].includes(t.value),
      ).map((t) => t.value),
      "files",
    ]);
  });

  // LIB-FR-06: expand/collapse state and selection are preserved across a lens
  // change (not just across tree reloads).
  it("preserves collapse state and selection across a lens change", async () => {
    setLoadTree(baseTree());
    renderLibrary();
    await screen.findByText("onboarding.md");

    // Collapse `.claude` (also selects it).
    fireEvent.click(screen.getByText(".claude"));
    expect(screen.queryByText("onboarding.md")).not.toBeInTheDocument();
    expect(
      screen.getByText(".claude").closest(".tree-row"),
    ).toHaveAttribute("data-selected", "true");

    // Switch the lens to All files and back to All artifacts.
    await pickSelector("Filter by type", "files");
    await pickSelector("Filter by type", "artifacts");

    // `.claude` is still collapsed (onboarding.md still hidden) and still selected.
    expect(screen.queryByText("onboarding.md")).not.toBeInTheDocument();
    expect(
      screen.getByText(".claude").closest(".tree-row"),
    ).toHaveAttribute("data-selected", "true");
  });

  // The empty-state message reflects the active lens, so "All files" / a
  // specific type don't misreport an empty result as "No artifacts found."
  it("varies the empty-state message by the active lens", async () => {
    // A tree with only an unclassified file: empty under "All artifacts" and any
    // specific type, non-empty under "All files".
    setLoadTree(folder("", false, [file("notes.txt")]));
    renderLibrary();

    // Default "All artifacts" lens: nothing classified -> artifacts message.
    expect(await screen.findByText("No artifacts found.")).toBeInTheDocument();

    // The type-specific message is covered where a type lens is reachable at
    // all — LIB-FR-19 renders a per-type button only for a type the tree holds,
    // so this tree offers none. See "keeps a lens whose last file left" below.

    // "All files" -> the unclassified file shows, so no empty message at all.
    await pickSelector("Filter by type", "files");
    expect(screen.getByText("notes.txt")).toBeInTheDocument();
    expect(screen.queryByText(/No .*found\./)).not.toBeInTheDocument();
  });

  // LIB error path: load_project_tree rejecting (e.g. "no project open")
  // surfaces an inline message instead of crashing.
  it("shows an inline error when the tree fails to load", async () => {
    mockInvoke(
      {
        load_project_tree: () => {
          throw "no project open";
        },
      },
      panelState(),
    );
    renderLibrary();
    expect(await screen.findByText(/no project open/)).toBeInTheDocument();
  });

  // LIB-FR-11: the manual rescan affordance re-renders the returned tree.
  it("re-renders the tree when the manual rescan affordance is used", async () => {
    const rescanned = folder("", true, [file("fresh.md", "spec", "inferred")]);
    mockInvoke(
      {
        load_project_tree: () => baseTree(),
        rescan_project_tree: () => rescanned,
      },
      expandedAll(baseTree()),
    );
    renderLibrary();
    await screen.findByText("AGENTS.md");

    await userEvent.click(screen.getByLabelText("Rescan project tree"));

    await waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("rescan_project_tree"),
    );
    expect(await screen.findByText("fresh.md")).toBeInTheDocument();
  });
});
