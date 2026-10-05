import { screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { Notes } from "../components/Notes";
import type { DiscussionReveal } from "../state/revealDiscussion";
import { useNotesPanelState } from "../hooks/useNotesPanelState";
import type {
  NoteListItem,
  NotesEntity,
  OpenableArtifact,
  TreeNode,
} from "../types";

// ---------------------------------------------------------------------------
// Fixtures
// ---------------------------------------------------------------------------

export function entityNote(
  id: string,
  entityId: string,
  body: string,
  updatedAt: string,
  extra: Partial<NoteListItem["note"]> & { unresolved?: boolean } = {},
): NoteListItem {
  const { unresolved, ...note } = extra;
  return {
    note: {
      id,
      scope: { kind: "entity", entityId, entityPath: entityId },
      body,
      createdAt: "2026-01-01T09:00:00Z",
      updatedAt,
      ...note,
    },
    entityName: unresolved ? undefined : entityId.split("/").pop(),
    unresolved: !!unresolved,
  };
}

export function projectNote(
  id: string,
  body: string,
  updatedAt: string,
): NoteListItem {
  return {
    note: {
      id,
      scope: { kind: "project" },
      body,
      createdAt: "2026-01-01T09:00:00Z",
      updatedAt,
    },
    unresolved: false,
  };
}

export const ARTIFACT_A: NotesEntity = { id: "specs/a.md", name: "a.md" };
export const ARTIFACT_B: NotesEntity = { id: "specs/b.md", name: "b.md" };

export const TREE: TreeNode = {
  id: "",
  name: "acme",
  path: "",
  nodeKind: "folder",
  hasArtifacts: true,
  children: [
    {
      id: "specs",
      name: "specs",
      path: "specs",
      nodeKind: "folder",
      hasArtifacts: true,
      children: [
        {
          id: "specs/a.md",
          name: "a.md",
          path: "specs/a.md",
          nodeKind: "file",
          artifactType: "spec",
          typeSource: "inferred",
        },
        {
          id: "specs/b.md",
          name: "b.md",
          path: "specs/b.md",
          nodeKind: "file",
          artifactType: "spec",
          typeSource: "inferred",
        },
        {
          // Not an artifact, so the Move picker does not offer it.
          id: "specs/notes.txt",
          name: "notes.txt",
          path: "specs/notes.txt",
          nodeKind: "file",
        },
        {
          // Shares a basename with `docs/README.md` below, so grouping, keying
          // and the Move picker all have to keep the two apart.
          id: "specs/README.md",
          name: "README.md",
          path: "specs/README.md",
          nodeKind: "file",
          artifactType: "spec",
          typeSource: "inferred",
        },
      ],
    },
    {
      id: "flows",
      name: "flows",
      path: "flows",
      nodeKind: "folder",
      hasArtifacts: true,
      // A folder carrying an artifact type of its own (ASC-FR-05, folder
      // scope): a note attaches to an artifact, so it must not be offered.
      artifactType: "flow",
      children: [
        {
          id: "flows/release.flow",
          name: "release.flow",
          path: "flows/release.flow",
          nodeKind: "file",
          artifactType: "flow",
          typeSource: "inferred",
        },
      ],
    },
    {
      id: "docs",
      name: "docs",
      path: "docs",
      nodeKind: "folder",
      hasArtifacts: true,
      children: [
        {
          id: "docs/README.md",
          name: "README.md",
          path: "docs/README.md",
          nodeKind: "file",
          artifactType: "spec",
          typeSource: "inferred",
        },
      ],
    },
  ],
};

/**
 * The panel as `VPanel` mounts it: with the persisted-state hook that owns the
 * selector position and the filter text, so the persistence scenarios exercise
 * the real thing rather than a stub.
 */
export function Harness({
  entity,
  onOpenArtifact = () => {},
  onReveal,
  onNoteDeleted,
}: {
  entity: NotesEntity | null;
  onOpenArtifact?: (artifact: OpenableArtifact) => void;
  onReveal?: (reveal: DiscussionReveal) => void;
  onNoteDeleted?: (noteId: string) => void;
}) {
  const panel = useNotesPanelState();
  return (
    <Notes
      panel={panel}
      entity={entity}
      onOpenArtifact={onOpenArtifact}
      onReveal={onReveal}
      onNoteDeleted={onNoteDeleted}
    />
  );
}

export function scopeSelect(): HTMLElement {
  return screen.getByRole("radiogroup", { name: "Notes scope" });
}

export function rows(): HTMLElement[] {
  return screen.queryAllByTestId("note-row");
}

export function rowFor(body: string): HTMLElement {
  const row = rows().find((r) => r.textContent?.includes(body));
  if (!row) throw new Error(`no row rendering ${body!}`);
  return row;
}

export async function openMenu(body: string) {
  await userEvent.click(
    within(rowFor(body)).getByLabelText("Note actions"),
  );
  return document.querySelector(".menu") as HTMLElement;
}
