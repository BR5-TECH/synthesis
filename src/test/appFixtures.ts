/**
 * The shared harness the `App.*.test.tsx` suites build on: the backend fixture
 * that `invoke` is pointed at, the two steps every test starts with, and the
 * tab-strip read they all make.
 *
 * The `vi.mock` calls themselves stay in each test file, because they are
 * hoisted and file-scoped. Vitest gives each test file its own module
 * instance, so the state below is per file.
 */

import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect } from "vitest";

let saveAppPrefsImpl: () => unknown = () => undefined;
/** Which worktree the backend currently reports as rooting the project. */
export let activeWorktreePath: string = "~/dev/acme";

/** The one agent the open project has enrolled, for the chrome roster. */
export const ROSTER_AGENT_ID = "a1";

function worktreeEntry(path: string) {
  return {
    path,
    name: path.split("/").pop(),
    branch: path.endsWith("-main") ? "main" : "feature/x",
    headShortHash: "4f2a10c",
    isDetached: false,
    isActive: path === activeWorktreePath,
    isPrimary: path === "~/dev/acme",
    isMissing: false,
  };
}

/** NTD-FR-07: ids are unique for the run and never reused. */
let postCounter = 0;

/**
 * The drafts this test has created, in creation order.
 *
 * `list_drafts` is served from here so a created draft actually appears in the
 * Drafts panel — DRP-FR-36's inline name field opens on the new draft's ROW, so
 * a fixture that always listed nothing would make every assertion about that
 * field pass whether the feature worked or not. Reset per test.
 */
let createdDrafts: {
  id: string;
  name: string;
  status: string;
  folder: string;
  updatedAt: string;
}[] = [];

export function defaultInvoke(cmd: string, args?: Record<string, unknown>) {
  switch (cmd) {
    // NTD-FR-02: the platform's disposition, read at startup (NTF-FR-13).
    // Granted, so the post policy of NTF-FR-08/NTF-FR-11 is the only thing
    // deciding whether anything posts — without this every `post_notification`
    // assertion in this file passes for the wrong reason.
    case "get_notification_permission":
      return "granted";
    // NTD-FR-07: the id a withdrawal is later keyed by (NTF-FR-14). Returning
    // nothing here would leave the facility unable to withdraw anything, and
    // every "was it withdrawn?" assertion would fail for a reason that has
    // nothing to do with what it is testing.
    case "post_notification":
      return { id: `n${(postCounter += 1)}` };
    case "list_recent_projects":
      return [
        {
          name: "acme",
          path: "~/dev/acme",
          lastOpenedAt: "2026-05-15T10:00:00Z",
        },
      ];
    case "open_project_at_path":
      return {
        name: "acme",
        path: "~/dev/acme",
        activeWorktreePath: activeWorktreePath,
      };
    // WTS-FR-02: a project inside a Git repository, so the chrome's worktree
    // selector renders. Its own suite covers the dropdown's behaviour; here it
    // is the entry point for the end-to-end switch.
    case "get_active_worktree":
      return worktreeEntry(activeWorktreePath);
    case "list_worktrees_and_branches":
      return {
        repositoryRoot: "~/dev/acme",
        activeWorktreePath: activeWorktreePath,
        worktrees: [
          worktreeEntry("~/dev/acme"),
          worktreeEntry("~/dev/acme-main"),
        ],
        // WTC-FR-06: offered only from the repository's own checkout.
        branches:
          activeWorktreePath === "~/dev/acme"
            ? [{ name: "develop", kind: "local", headShortHash: "bbb2222" }]
            : [],
      };
    case "activate_worktree":
      activeWorktreePath = "~/dev/acme-main";
      return {
        repositoryRoot: "~/dev/acme",
        activeWorktreePath: activeWorktreePath,
        worktrees: [
          worktreeEntry("~/dev/acme"),
          worktreeEntry("~/dev/acme-main"),
        ],
        branches: [],
      };
    case "load_layout_preferences":
      return null;
    case "save_layout_preferences":
      return undefined;
    case "load_app_preferences":
      return { theme: "dark" };
    case "save_app_preferences":
      return saveAppPrefsImpl();
    case "list_installed_plugins":
    case "list_agent_adapters":
      return [];
    // A minimal Library tree holding the one artifact the Changes fixtures
    // also report, so the Editor-tab half of CHG-FR-20 has something to open.
    case "load_project_tree":
    case "rescan_project_tree":
      return {
        id: "",
        name: "acme",
        path: "",
        nodeKind: "folder",
        hasArtifacts: true,
        children: [
          {
            id: "src",
            name: "src",
            path: "src",
            nodeKind: "folder",
            // Unclassified: visible only under the Library's "All files" lens
            // (LIB-FR-12), which is TAB-FR-02, TAB-FR-04, TAB-FR-05's second entry point.
            hasArtifacts: false,
            children: [
              {
                id: "src/main.rs",
                name: "main.rs",
                path: "src/main.rs",
                nodeKind: "file",
              },
            ],
          },
          {
            id: "specifications",
            name: "specifications",
            path: "specifications",
            nodeKind: "folder",
            hasArtifacts: true,
            children: [
              {
                id: "specifications/ui",
                name: "ui",
                path: "specifications/ui",
                nodeKind: "folder",
                hasArtifacts: true,
                children: [
                  {
                    id: "specifications/ui/CHG-changes.md",
                    name: "CHG-changes.md",
                    path: "specifications/ui/CHG-changes.md",
                    nodeKind: "file",
                    artifactType: "spec",
                    typeSource: "inferred",
                  },
                ],
              },
            ],
          },
        ],
      };
    case "validate_flow_document":
      // FGV-FR-02 / FLO-FR-46: the backend judges a Flow body before the canvas
      // renders it. The rules themselves are covered by the Rust suite.
      return { valid: true, violations: [] };
    case "load_artifact_contents_by_id":
      // A Flow's body has to deserialize as a Flow document (FLO-FR-03); an
      // empty one is the empty graph a freshly-created Flow opens on
      // (FLO-FR-04). Everything else is Markdown.
      if (String(args?.id ?? "").endsWith(".flow"))
        return { body: "", checksum: "ck1" };
      // DFV-FR-42: a Diff tab's target is the artifact's own contents, so the
      // file the Changes fixture reports as changed has to hold the new
      // revision the comparison is against.
      if (args?.id === "specifications/ui/CHG-changes.md")
        return { body: "first line\na new line\n", checksum: "ck1" };
      return { body: "# spec", checksum: "ck1" };
    // Library panel state (PSS-FR-18 / LIB-FR-14). Folders render collapsed
    // unless the persisted set says otherwise (LIB-FR-15), so the fixture
    // tree's folders are listed as expanded here: these App-level tests are
    // about tabs and panels, and reaching a nested file through the tree is
    // setup for them rather than the thing under test. The Library's own suite
    // covers the expansion default and the persistence itself.
    case "load_library_panel_state":
      return {
        expandedPaths: ["src", "specifications", "specifications/ui"],
        artifactTypeFilter: "all_artifacts",
        textFilter: "",
      };
    case "save_library_panel_state":
      return undefined;
    // Notes panel (NTS-notes.md). Its own suite covers the behaviour; these
    // App-level tests only need the panel to mount and load without erroring.
    case "load_notes_panel_state":
      return { scopePosition: "entity", textFilter: "" };
    case "save_notes_panel_state":
      return undefined;
    case "list_notes_for_entity":
    case "list_project_notes":
    case "list_all_notes":
      return [];
    // Changes panel (CHG-changes.md). The App-level tests only need it to
    // mount without erroring; its own suite covers the behaviour.
    case "load_changes_panel_state":
      return { mode: "uncommitted" };
    case "get_default_branch":
      return "main";
    case "list_comparison_branches":
      return [{ name: "main", isCurrent: true, isDefault: true }];
    case "list_branches":
      return [
        { name: "main", kind: "local", isCurrent: true },
        { name: "develop", kind: "local", isCurrent: false },
      ];
    case "list_uncommitted_changes":
      return {
        comparison: { kind: "uncommitted" },
        entries: [
          {
            id: "specifications/ui/CHG-changes.md",
            path: "specifications/ui/CHG-changes.md",
            name: "CHG-changes.md",
            changeStatus: "modified",
            addedLines: 91,
            removedLines: 0,
            isBinary: false,
            artifactType: "spec",
            typeSource: "inferred",
          },
        ],
      };
    // DFV-FR-25 / DFV-FR-43: a Diff tab reads the ORIGINAL here and the target
    // through the artifact's own editing session, then derives the comparison
    // between the two itself.
    case "get_file_revisions":
      return {
        old: "first line\n",
        new: "first line\na new line\n",
        isBinary: false,
      };
    // DRP-FR-05 / DRP-FR-36: the listing reflects what this session created, so
    // a draft created from the panel has a ROW — which is what the inline name
    // field of DRP-FR-36 opens on. A listing frozen at empty would make every
    // assertion about that field vacuously true.
    case "list_drafts":
      return { folders: [], drafts: createdDrafts.map((d) => ({ ...d })) };
    // DRS-FR-06: `create_draft` hands back the record *and* the one Markdown
    // file the draft was created holding, which the tab opens on.
    case "create_draft": {
      // DRS-FR-27: an unnamed draft is `Untitled`, or the first `Untitled N`
      // free in the worktree — which is what DRP-FR-06, DRP-FR-36, NAW-FR-03, NAW-FR-04's second creation turns
      // on.
      const ordinal = createdDrafts.length + 1;
      const name = ordinal === 1 ? "Untitled" : `Untitled ${ordinal}`;
      const record = {
        id: `d${ordinal}`,
        name,
        promptPath: `${name}.md`,
        status: "active",
        createdAt: "2026-07-31T10:00:00Z",
        updatedAt: "2026-07-31T10:00:00Z",
      };
      createdDrafts.push({
        id: record.id,
        name,
        status: "active",
        folder: String((args as Record<string, unknown>)?.folder ?? ""),
        updatedAt: record.updatedAt,
      });
      return { draft: record, file: record.promptPath };
    }
    case "open_draft":
    case "rename_draft":
    case "set_draft_status":
      return {
        id: "d1",
        name: "Untitled",
        promptPath: "Untitled.md",
        status: "active",
        createdAt: "2026-07-31T10:00:00Z",
        updatedAt: "2026-07-31T10:00:00Z",
      };
    case "list_draft_files":
      return [{ path: "Untitled.md", name: "Untitled.md", nodeKind: "file" }];
    case "load_draft_file_contents":
      return { body: "", checksum: "c0" };
    case "save_draft_file_contents":
      return { checksum: "c1" };
    case "search_drafts":
      return [];
    // AGT-FR-03 / AGT-FR-22: one agent enrolled in the open project, so the
    // chrome roster has a row to activate (AGT-FR-06, AGT-FR-07).
    case "list_project_agents":
      return [
        {
          agent: {
            id: ROSTER_AGENT_ID,
            nickname: "arch",
            title: "Architect",
            provider: "anthropic",
            model: "claude-sonnet-5",
            reasoning: "medium",
            instructions: "",
          },
          availability: { state: "available" },
        },
      ];
    // DSH-FR-09 / DSH-FR-13 / DSH-FR-14: the Dashboard's widget loaders. The
    // Dashboard is the tab that opens on project open, so a fixture that served
    // nothing here would render the whole-Dashboard empty state (DSH-FR-08) and
    // every shell assertion that names a widget would fail for a reason that has
    // nothing to do with what it is testing.
    case "list_recently_edited_artifacts":
      return [
        {
          id: "specs/checkout.spec.md",
          name: "checkout.spec.md",
          kind: "markdown",
          modifiedAt: "2026-05-15T09:00:00.000Z",
        },
      ];
    case "list_active_drafts":
      return [];
    case "list_recent_agent_runs":
      return [
        {
          runId: "run-7e3",
          draftId: "d1",
          draftName: "design-review",
          state: "published",
          stage: "acceptance",
          stageCondition: "complete",
          updatedAt: "2026-05-15T08:00:00.000Z",
        },
      ];
    case "list_pending_git_activity":
      return {
        modifiedArtifacts: 3,
        modifiedSourceFiles: 1,
        unpushedCommits: 2,
        fetchableCommits: 0,
      };
    case "list_agents":
      return [];
    case "list_agent_turns":
    case "list_recoverable_agent_turn_failures":
    case "list_agent_turn_image_notices":
      return [];
    default:
      return undefined;
  }
}
/** Put the fixture back to the state a fresh test finds it in. */
export function resetAppFixture() {
  createdDrafts = [];
  saveAppPrefsImpl = () => undefined;
  activeWorktreePath = "~/dev/acme";
  postCounter = 0;
}

/** The labels currently in the tab strip (the strip renders `.tab`, not ARIA tabs). */
export function tabLabels(): string[] {
  return Array.from(
    document.querySelectorAll(".tabstrip .tab .tab__label"),
  ).map((el) => el.textContent ?? "");
}

export async function enterIde() {
  // The picker is the startup surface; open a recent to reach the IDE shell.
  const recent = await screen.findByText("acme");
  await userEvent.click(recent);
  // Activity bar Global settings button only exists in the IDE shell.
  await screen.findByRole("button", { name: "Global settings" });
}

/**
 * DRP-FR-06 / NAW-FR-03: create a draft the one way a draft can be created —
 * from the Drafts panel's create affordance — and wait for its New Artifact tab.
 *
 * The File menu's **New Artifact** no longer reaches this: it opens the
 * typed-artifact window instead, which creates a project file and no draft
 * (NTA-FR-14 / SNV-FR-24).
 */
export async function createDraftFromPanel(
  // False for the two tests whose backend deliberately lists no drafts at all,
  // so the created draft has no row for the field of DRP-FR-36 to open on.
  opts: { listed?: boolean } = {},
) {
  const listed = opts.listed ?? true;
  await userEvent.click(screen.getByRole("button", { name: "Drafts" }));
  // DRP-FR-15: an empty worktree offers the affordance in its empty state; one
  // holding anything offers the pinned one below the tree.
  const create =
    screen.queryByRole("button", { name: "New draft" }) ??
    screen.getByRole("button", { name: /Draft$/ });
  await userEvent.click(create);
  await screen.findByRole("button", { name: "Draft actions" });
  // DRP-FR-36: the new row opens in rename mode with focus in its name field.
  // Asserted rather than assumed — without it the Escape below would pass on a
  // field that never opened. These tests are about what happens after the draft
  // exists, so the field is abandoned with Escape, which leaves the created name
  // standing and the row in its ordinary form (DRP-FR-36, DRP-FR-11, DRP-FR-16).
  if (!listed) return;
  await screen.findByLabelText("Draft name");
  await userEvent.keyboard("{Escape}");
  await waitFor(() =>
    expect(screen.queryByLabelText("Draft name")).toBeNull(),
  );
}
