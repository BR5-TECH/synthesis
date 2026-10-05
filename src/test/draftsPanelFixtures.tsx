/**
 * Shared fixtures for the Drafts panel test files (`DraftsPanel.test.tsx` and
 * its `DraftsPanel.<topic>.test.tsx` siblings).
 *
 * Everything here is independent of the `@tauri-apps` module mocks, which are
 * hoisted and file-scoped: each test file declares its own `invokeMock` and
 * `listenMock` beside its own copy of the `beforeEach` that installs them.
 */
import { fireEvent, render, screen } from "@testing-library/react";
import { vi } from "vitest";
import type { Mock } from "vitest";
import { useRef, useState } from "react";

import { DraftsPanel, type DraftFilter } from "../components/DraftsPanel";
import type {
  DraftHierarchy,
  DraftSummary,
  PanelRevealRequest,
} from "../types";


export const DRAFTS: DraftSummary[] = [
  {
    id: "d-archived",
    name: "artifact-window",
    status: "archived",
    folder: "",
    updatedAt: "2026-07-31T08:00:00Z",
  },
  {
    id: "d-active",
    name: "editor-tweaks",
    status: "active",
    folder: "",
    updatedAt: "2026-07-31T07:00:00Z",
  },
  {
    id: "d-old",
    name: "library-lens",
    status: "active",
    folder: "",
    updatedAt: "2026-07-28T07:00:00Z",
  },
];

/** The callbacks every render below supplies, all of them spies. */
export interface PanelCallbacks {
  onOpenDraft: Mock<(draft: DraftSummary) => void>;
  onCreateDraft: Mock<
    (folder?: string) => Promise<{ id: string; name: string } | null> | void
  >;
  onDraftDeleted: Mock<(id: string) => void>;
  onDraftRenamed: Mock<(id: string, name: string) => void>;
  onDraftChanged: Mock<() => void>;
  /** DRP-FR-35: the route a row's graduation marker offers. */
  onOpenRun: Mock<(runId: string) => void>;
  /** DRP-FR-YYZU: a GitHub-shadow row's Graduate…. */
  onGraduateDraft: Mock<(draftId: string, draftName: string) => void>;
}

export const flat = (drafts: DraftSummary[] = DRAFTS): DraftHierarchy => ({
  folders: [],
  drafts,
});

/**
 * A worktree with a shape: `UI` holding `Components`, `backend` beside it, and
 * a draft filed in each level plus one at the root.
 */
export function nested(): DraftHierarchy {
  return {
    folders: [
      { path: "UI", parent: "" },
      { path: "UI/Components", parent: "UI" },
      { path: "backend", parent: "" },
    ],
    drafts: [
      {
        id: "d-button",
        name: "button-lens",
        status: "active",
        folder: "UI/Components",
        updatedAt: "2026-07-31T09:00:00Z",
      },
      {
        id: "d-window",
        name: "artifact-window",
        status: "active",
        folder: "UI",
        updatedAt: "2026-07-31T08:00:00Z",
      },
      {
        id: "d-editor",
        name: "editor-tweaks",
        status: "active",
        folder: "backend",
        updatedAt: "2026-07-31T07:00:00Z",
      },
      {
        id: "d-scratch",
        name: "scratch",
        status: "active",
        folder: "",
        updatedAt: "2026-07-28T07:00:00Z",
      },
    ],
  };
}

/**
 * The panel's own state, held for real rather than stubbed: expansion is what
 * the tree renders from (DRP-FR-14), so a test that toggles a folder has to see
 * the toggle land.
 */
export function Harness({
  revision,
  filter,
  text,
  expanded,
  props,
  onExpandedChange,
  reveal,
}: {
  revision: number;
  filter: DraftFilter;
  text: string;
  expanded: string[];
  props: PanelCallbacks;
  onExpandedChange?: (paths: string[]) => void;
  reveal?: PanelRevealRequest | null;
}) {
  const [f, setF] = useState<DraftFilter>(filter);
  const [t, setT] = useState(text);
  const [open, setOpen] = useState<Set<string>>(new Set(expanded));
  // Mirrored synchronously, exactly as the real hook does: two expansion
  // updates can land in one tick — a move remaps the moved folder's key and
  // then reveals its destination — and a setter reading render-scope state
  // would lose the first.
  const mirror = useRef(open);
  const publish = (next: Set<string>) => {
    mirror.current = next;
    setOpen(next);
    onExpandedChange?.([...next]);
  };
  return (
    <DraftsPanel
      panel={{
        filter: f,
        setFilter: setF,
        text: t,
        setText: setT,
        expanded: open,
        toggleExpanded: (path) => {
          const next = new Set(mirror.current);
          if (next.has(path)) next.delete(path);
          else next.add(path);
          publish(next);
        },
        expandAll: (paths) => {
          const next = new Set(mirror.current);
          for (const p of paths.filter((p) => p !== "")) next.add(p);
          publish(next);
        },
        reparentExpanded: (from, to) => {
          if (from === "" || from === to) return;
          const next = new Set(mirror.current);
          for (const path of mirror.current)
            if (path === from || path.startsWith(`${from}/`)) {
              next.delete(path);
              next.add(`${to}${path.slice(from.length)}`);
            }
          publish(next);
        },
      }}
      revision={revision}
      reveal={reveal ?? null}
      {...props}
    />
  );
}

export function renderPanel(
  overrides: {
    filter?: DraftFilter;
    text?: string;
    expanded?: string[];
    onOpenDraft?: PanelCallbacks["onOpenDraft"];
    onCreateDraft?: PanelCallbacks["onCreateDraft"];
    onDraftDeleted?: PanelCallbacks["onDraftDeleted"];
    onDraftRenamed?: PanelCallbacks["onDraftRenamed"];
    onDraftChanged?: PanelCallbacks["onDraftChanged"];
    onOpenRun?: PanelCallbacks["onOpenRun"];
    onGraduateDraft?: PanelCallbacks["onGraduateDraft"];
    onExpandedChange?: (paths: string[]) => void;
  } = {},
) {
  let revealSeq = 0;
  const props: PanelCallbacks = {
    onOpenDraft: overrides.onOpenDraft ?? vi.fn(),
    onCreateDraft: overrides.onCreateDraft ?? vi.fn(),
    onDraftDeleted: overrides.onDraftDeleted ?? vi.fn(),
    onDraftRenamed: overrides.onDraftRenamed ?? vi.fn(),
    onDraftChanged: overrides.onDraftChanged ?? vi.fn(),
    onOpenRun: overrides.onOpenRun ?? vi.fn(),
    onGraduateDraft: overrides.onGraduateDraft ?? vi.fn(),
  };
  let currentReveal: PanelRevealRequest | null = null;
  let currentRevision = 0;
  const tree = (revision: number) => (
    <Harness
      revision={revision}
      filter={overrides.filter ?? "all"}
      text={overrides.text ?? ""}
      expanded={overrides.expanded ?? []}
      onExpandedChange={overrides.onExpandedChange}
      props={props}
      reveal={currentReveal}
    />
  );
  const view = render(tree(0));
  /**
   * Re-render the SAME panel with a bumped revision, which is what the shell
   * does on every `"drafts changed"` event (DRP-FR-05). Deliberately not a fresh
   * `render`: what is under test is a mounted panel re-listing.
   */
  const bump = (revision: number) => {
    currentRevision = revision;
    view.rerender(tree(revision));
  };
  /**
   * DRP-FR-34: hand the mounted panel a reveal request, as the shell does when
   * the active tab changes (SNV-FR-64). Each call mints a fresh nonce, so asking
   * twice for the same draft is two requests rather than one.
   */
  const revealDraft = (id: string, opts: { focus?: boolean } = {}) => {
    currentReveal = {
      panel: "drafts",
      id,
      nonce: ++revealSeq,
      // A follow (SNV-FR-64): the draft either exists now or not at all.
      optimistic: false,
      // DRP-FR-34: a reveal the author asked for directly lands focus on the
      // row; a tab-follow selects it and leaves focus where the author put it.
      focus: opts.focus ?? false,
    };
    view.rerender(tree(currentRevision));
  };
  return { ...props, view, bump, revealDraft };
}

/** A folder row, by the name it renders. */
export const folderRow = (name: string) =>
  screen.getByRole("treeitem", { name: `Folder ${name}` });

/** A draft row, by the name it renders. */
export const draftRow = (name: string) =>
  screen.getByRole("treeitem", { name: new RegExp(`^Draft ${name}(,|$)`) });

/**
 * DRP-FR-22: a row's menu is opened by right-clicking the row, the way the
 * Project panel's is — there is no per-row button to press.
 */
export const openFolderMenu = (name: string) => fireEvent.contextMenu(folderRow(name));
export const openDraftMenu = (name: string) => fireEvent.contextMenu(draftRow(name));
