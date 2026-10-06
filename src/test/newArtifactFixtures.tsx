/**
 * Shared fixtures for the New Artifact workspace test files
 * (`NewArtifactWorkspace.test.tsx` and its `NewArtifactWorkspace.<topic>` siblings).
 *
 * Everything here is independent of the `@tauri-apps` module mocks, which are
 * hoisted and file-scoped: each test file declares its own `invokeMock` and
 * hands it to `makeStubs` to get the two backend stubs back.
 */
import { readFileSync } from "node:fs";

import { useState } from "react";

import { expect, vi } from "vitest";
import type { Mock } from "vitest";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { NewArtifactWorkspace } from "../components/NewArtifactWorkspace";
import type {
  AgentTurn,
  Discussion,
  DraftHistoryEntry,
  DraftHistoryList,
  DraftPublicationView,
  DraftStatus,
  MetadataList,
  Participant,
  PublicationErrorCode,
  PublicationMetadata,
  PublicationRecord,
  PublicationRemoteResolution,
} from "../types";
import { DraftSessionStore } from "../state/draftSessions";
import { readStylesheet } from "./readStylesheet";
import { expectBackendOrigin } from "./origins";


/** NAW-FR-06: the draft's one prompt, which the tab opens on. */
export const PROMPT = "artifact-window.md";

/** GHP-FR-CWTG: a draft nobody has published, and nothing standing in its way. */
export function noPublication(): DraftPublicationView {
  return {
    current: null,
    history: [],
    attempt: null,
    eligibility: {
      publishable: true,
      reasonCode: null,
      reason: null,
      localAssets: [],
    },
  };
}

/** GHP-FR-CWTG: a publication the action is refused for, with its reason. */
export function publicationRefused(
  reasonCode: PublicationErrorCode,
  reason: string,
  localAssets: string[] = [],
): DraftPublicationView {
  return {
    ...noPublication(),
    eligibility: { publishable: false, reasonCode, reason, localAssets },
  };
}

/** GHP-FR-JAWD: one successful publication record. */
export function publicationRecord(
  issueNumber: number,
  marker: string,
  publishedAt = "2026-09-12T10:04:00.000Z",
): PublicationRecord {
  return {
    provider: "github",
    repositoryOwner: "acme",
    repositoryName: "widgets",
    issueNumber,
    issueUrl: `https://github.com/acme/widgets/issues/${issueNumber}`,
    publishedAt,
    marker,
  };
}

/** GHP-FR-DZLB: a metadata list that read, holding `items` (possibly none). */
export function loadedList<T>(items: T[]): MetadataList<T> {
  return { state: "loaded", items, errorCode: null, error: null };
}

/** GHP-FR-DZLB: a metadata list whose read failed with a typed error. */
export function failedList<T>(
  errorCode: PublicationErrorCode,
  error: string,
): MetadataList<T> {
  return { state: "failed", items: [], errorCode, error };
}

/**
 * GHP-FR-MDLD: what `load_publication_metadata` answers by default — two Feature
 * parents, three Types, two open milestones, and the default settings.
 */
export function publicationMetadata(
  overrides: Partial<PublicationMetadata> = {},
): PublicationMetadata {
  return {
    repositoryOwner: "acme",
    repositoryName: "widgets",
    settings: {
      parentIssueTypes: ["Feature"],
      subIssueType: "Task",
      subIssueMilestonePolicy: "inherit_parent",
    },
    parents: loadedList([
      {
        number: 412,
        title: "Window chrome",
        issueType: "Feature",
        url: "https://github.com/acme/widgets/issues/412",
        milestone: { number: 7, title: "v1.2" },
      },
      {
        number: 398,
        title: "Release pipeline",
        issueType: "Feature",
        url: "https://github.com/acme/widgets/issues/398",
        milestone: null,
      },
    ]),
    issueTypes: loadedList([{ name: "Feature" }, { name: "Task" }, { name: "Bug" }]),
    milestones: loadedList([
      { number: 7, title: "v1.2" },
      { number: 8, title: "v1.3" },
    ]),
    subIssueType: { name: "Task", resolved: "Task" },
    ...overrides,
  };
}

/** The publication chooser's dialog, found by its root option. */
export async function publicationChooser(): Promise<HTMLElement> {
  const root = await screen.findByRole("radio", { name: /Publish as a root issue/ });
  return root.closest('[role="dialog"]') as HTMLElement;
}

/** NAW-FR-RBTE: confirm the chooser as it stands, once its read has landed. */
export async function confirmPublicationChooser() {
  const dialog = await publicationChooser();
  const publish = within(dialog).getByRole("button", { name: "Publish" });
  await waitFor(() => expect(publish).toBeEnabled());
  await userEvent.click(publish);
}

/** GHP-FR-XAUP: one configured remote, eligible, chosen automatically. */
export function oneEligibleRemote(): PublicationRemoteResolution {
  return {
    remotes: [
      {
        name: "origin",
        url: "github.com/acme/widgets",
        kind: "github",
        repositoryOwner: "acme",
        repositoryName: "widgets",
        eligibility: "eligible",
        reason: null,
      },
    ],
    selection: "origin",
    origin: "automatic",
    persistedChoice: null,
  };
}

/**
 * GHP-FR-XAUP: four configured remotes, one per cause the picker explains — one
 * eligible, one GitHub remote the token cannot create issues in, one whose
 * repository accepts no issue, and one that is not GitHub at all. The picker is
 * shown for this, and every entry carries its own reason.
 */
export function fourRemotes(): PublicationRemoteResolution {
  return {
    remotes: [
      {
        name: "origin",
        url: "github.com/acme/widgets",
        kind: "github",
        repositoryOwner: "acme",
        repositoryName: "widgets",
        eligibility: "eligible",
        reason: null,
      },
      {
        name: "upstream",
        url: "github.com/other/widgets",
        kind: "github",
        repositoryOwner: "other",
        repositoryName: "widgets",
        eligibility: "issues_create_forbidden",
        reason: "The token cannot create issues in this repository.",
      },
      {
        name: "fork",
        url: "github.com/acme/fork",
        kind: "github",
        repositoryOwner: "acme",
        repositoryName: "fork",
        eligibility: "issues_disabled",
        reason:
          "This repository does not accept new issues. Check that Issues are " +
          "turned on and that the repository is neither archived nor disabled.",
      },
      {
        name: "mirror",
        url: "https://git.internal/acme/widgets",
        kind: "other",
        repositoryOwner: null,
        repositoryName: null,
        eligibility: "not_github",
        reason: "Not a GitHub repository.",
      },
    ],
    selection: "origin",
    origin: "attempt_only",
    persistedChoice: null,
  };
}

/** What `rename_draft` answers with, so a stale report is distinguishable. */
export const RENAMED = "Agent personas";

export const AGENT: Participant = {
  kind: "agent",
  agentId: "a1",
  handle: "arch",
  model: "m",
};

/** NAW-FR-07: a version of the prompt, as `"list draft history"` returns one. */
export function entry(
  seq: number,
  source: DraftHistoryEntry["source"],
  sha = `sha-${seq}`,
): DraftHistoryEntry {
  return {
    id: `e${seq}`,
    draftId: "d1",
    seq,
    path: PROMPT,
    createdAt: `2026-07-2${seq}T08:00:00Z`,
    byteLen: 10 * seq,
    sha256: sha,
    source,
  };
}

/**
 * No version at all — what a draft nobody has proposed a change to holds
 * (`../../specifications/core/DHS-draft-history.md` DHS-FR-07). Its live prompt
 * IS its `Original`, so `matchesLatest` is true: there is nothing it could have
 * moved on from.
 */
export function freshHistory(): DraftHistoryList {
  return {
    entries: [],
    live: { path: PROMPT, byteLen: 10, sha256: "sha-1", matchesLatest: true },
  };
}

/** The `Original` and one accepted version — what a first acceptance leaves. */
export function acceptedHistory(): DraftHistoryList {
  return {
    entries: [
      entry(1, { kind: "original" }),
      entry(2, { kind: "proposal_accepted", proposalId: "p1", agent: AGENT }),
    ],
    live: { path: PROMPT, byteLen: 20, sha256: "sha-2", matchesLatest: true },
  };
}

/** `Original` and two accepted versions, the newest matching the live prompt. */
export function threeVersions(): DraftHistoryList {
  const entries = [
    entry(1, { kind: "original" }),
    entry(2, { kind: "proposal_accepted", proposalId: "p1", agent: AGENT }),
    entry(3, { kind: "proposal_accepted", proposalId: "p2", agent: AGENT }),
  ];
  return {
    entries,
    live: { path: PROMPT, byteLen: 30, sha256: "sha-3", matchesLatest: true },
  };
}

export function draft(status: DraftStatus = "active") {
  return {
    id: "d1",
    name: "artifact-window",
    // NAW-FR-25: the prompt the draft's name is bound to.
    promptPath: PROMPT,
    status,
    createdAt: "2026-07-31T08:00:00Z",
    updatedAt: "2026-07-31T08:00:00Z",
  };
}

/**
 * DRS-FR-18 / NAW-FR-44: what `get_draft_graduation` answers — the run the
 * project's queue holds for this draft.
 */
export type DraftGraduationStub = {
  runId: string;
  state: string;
  locked: boolean;
  /**
   * / DRS-FR-KQTW: whether this run leaves the draft `graduated` —
   * it crossed the publication boundary and has not ended `discarded` or
   * `failed`. The backend always sends it.
   */
  graduated?: boolean;
  /** /: what a run waiting for Implement rests on. */
  implementationBlocker?: unknown;
};

export function renderWorkspace(drafts = new DraftSessionStore()) {
  const onDraftChanged = vi.fn();
  const onGraduationStarted = vi.fn();
  const onOpenRun = vi.fn();
  const onArchived = vi.fn();
  const onNameChanged = vi.fn();
  const view = render(
    <NewArtifactWorkspace
      draftId="d1"
      drafts={drafts}
      name="artifact-window"
      onDraftChanged={onDraftChanged}
      draftsRevision={0}
      onGraduationStarted={onGraduationStarted}
      onOpenRun={onOpenRun}
      onArchived={onArchived}
      onNameChanged={onNameChanged}
    />,
  );
  return {
    onDraftChanged,
    onGraduationStarted,
    onOpenRun,
    onArchived,
    onNameChanged,
    drafts,
    view,
  };
}

/**
 * The workspace under a host that holds the draft's name the way the shell does
 * (NAW-FR-04): the name is a prop, and a rename reported out of the tab comes
 * back in as the new one. Needed by anything asserting on what the chrome reads
 * *after* a rename, which is what NAW-FR-25 is about.
 */
export function renderNamed(initial: string) {
  const onNameChanged = vi.fn();
  const onDraftChanged = vi.fn();
  const onGraduationStarted = vi.fn();
  const onArchived = vi.fn();
  const drafts = new DraftSessionStore();
  /** Stands in for the shell: renames the draft from *outside* the tab. */
  let renameFromElsewhere: (name: string) => void = () => {};
  function Host() {
    const [name, setName] = useState(initial);
    // NAW-FR-25 / DRS-FR-22: the shell's count of everything that has moved a
    // draft, which a rename made in the Drafts panel bumps.
    const [revision, setRevision] = useState(0);
    renameFromElsewhere = (next: string) => {
      setName(next);
      setRevision((n) => n + 1);
    };
    return (
      <NewArtifactWorkspace
        draftId="d1"
        drafts={drafts}
        name={name}
        onDraftChanged={onDraftChanged}
        draftsRevision={revision}
        onGraduationStarted={onGraduationStarted}
        onOpenRun={vi.fn()}
        onArchived={onArchived}
        onNameChanged={(next) => {
          onNameChanged(next);
          setName(next);
        }}
      />
    );
  }
  const view = render(<Host />);
  return {
    onNameChanged,
    onDraftChanged,
    onGraduationStarted,
    onArchived,
    drafts,
    view,
    renameFromElsewhere: (name: string) => renameFromElsewhere(name),
  };
}

/**
 * NAW-FR-27 / NAW-FR-28: the floating action control, expanded onto its column
 * of four actions. Every lifecycle action a draft affords is behind it — there
 * is no lifecycle button anywhere in the tab's chrome to reach instead.
 */
export async function openActions() {
  await userEvent.click(
    await screen.findByRole("button", { name: "Draft actions" }),
  );
  return screen.getByRole("menu");
}

/**
 * DDS-FR-XQMF: hide the discussion column, from the action row's own control.
 *
 * The tab opens with the column shown, and that is the state in which the
 * control's **Discuss** is disabled (ACT-FR-QWNP) — so a case about what
 * Discuss DOES has to reach the other state first.
 */
export async function hideDiscussion() {
  const column = screen.queryByTestId("draft-discussion-column");
  // Named for the state it leaves behind rather than for the press it makes:
  // the control is a toggle, so calling this on a column that is already
  // hidden would show it and read as the opposite of what it says.
  if (column !== null && column.hasAttribute("hidden")) return;
  await userEvent.click(await screen.findByTestId("draft-discussion-toggle"));
  await waitFor(() =>
    expect(screen.getByTestId("draft-discussion-column")).toHaveAttribute(
      "hidden",
    ),
  );
}

/** NAW-FR-17 / NAW-FR-28: Graduate, reached the one way it can be. */
export async function graduate() {
  await openActions();
  // NAW-FR-17 / NAW-FR-17: Graduate carries no precondition on status or
  // content — the one thing that disables it is a run already holding this
  // draft, or one that has already graduated it.
  const item = await screen.findByRole("menuitem", { name: "Graduate" });
  await waitFor(() => expect(item).toBeEnabled());
  await userEvent.click(item);
  await screen.findByRole("dialog", { name: /Graduate .artifact-window./ });
}

/** The History rail, once its toggle has been activated (NAW-FR-08). */
export async function showRail() {
  await userEvent.click(
    await screen.findByRole("button", { name: "Show History" }),
  );
  return document.querySelector(".draft-rail") as HTMLElement;
}

/** NAW-FR-07: the rail's version rows, in the order it renders them. */
export function versionRows(): HTMLElement[] {
  return [
    ...document.querySelectorAll<HTMLElement>(
      ".draft-rail__versions .draft-version",
    ),
  ];
}

/** NAW-FR-37: the pinned live-prompt row. */
export function liveRow(): HTMLElement {
  return document.querySelector(".draft-version--live") as HTMLElement;
}

/**
 * Edit the selected file through the Editor's raw-Markdown surface — a real
 * textarea, unlike the contenteditable WYSIWYG default, and the same document
 * either way (EDT-FR-17).
 */
export async function editSource(value: string) {
  const toggle = screen.queryByRole("button", { name: "Edit as Markdown source" });
  if (toggle) fireEvent.click(toggle);
  const source = (await screen.findByLabelText(
    "Markdown source",
  )) as HTMLTextAreaElement;
  fireEvent.change(source, { target: { value } });
  return source;
}

/**
 * Put the application's real stylesheets in front of the tests that need them.
 *
 * jsdom does no layout, so nothing here can assert that a box ends up where it
 * belongs. It does resolve the cascade, though, which is enough to assert the
 * *declaration* a layout depends on — and the two defects these guard were both
 * a missing or mis-targeted declaration rather than an arithmetic mistake.
 *
 * Order matters: `kit.css` defines `.scrim` and `.modal`, `components.css`
 * refines them, exactly as the application loads them (`App.tsx`).
 */
export function withStyles(): () => void {
  const style = document.createElement("style");
  style.textContent = [
    readStylesheet("kit.css"),
    readStylesheet("components.css"),
  ].join("\n");
  document.head.appendChild(style);
  return () => style.remove();
}

/** An enrolled agent, as `list_project_agents` reports one. */
export function projectAgent(nickname: string) {
  return {
    agent: {
      id: `id-${nickname}`,
      nickname,
      provider: "openrouter",
      modelId: "m",
      instructions: "",
      createdAt: "2026-01-01T00:00:00Z",
      updatedAt: "2026-01-01T00:00:00Z",
      reasoning: null,
    },
    availability: "ready",
  };
}

export interface DiscussionComment {
  id: string;
  author: Participant;
  body: string;
}

/** A discussion as the backend's fold produces one (CMS-FR-53). */
export function discussionThread(
  id: string,
  comments: DiscussionComment[],
  over: Partial<Discussion> = {},
): Discussion {
  return {
    id,
    target: { kind: "draft", draftId: "d1" },
    fragmentTarget: null,
    comments: comments.map((c) => ({
      ...c,
      quotes: [],
      attachments: [],
      createdAt: "2026-02-01T00:00:00Z",
    })),
    locked: false,
    resolved: false,
    createdAt: "2026-02-01T00:00:00Z",
    updatedAt: "2026-02-01T00:00:00Z",
    ...over,
  };
}

/**
 * The column's opening composer, focused.
 *
 * ACT-FR-QWNP: the control's **Discuss** opens no composer of its own. Where
 * the column already stands the composer stands with it, and the action is
 * disabled because there is nothing left for it to do — so this reaches the
 * composer where it is. Where the column is hidden, Discuss is what brings the
 * column back and puts the caret in it (DDS-FR-JWNC), so that is the route
 * taken instead.
 */
export async function openComposer() {
  // `queryAllByTestId` rather than `queryByTestId`: a case that renders two
  // workspaces would make the singular form throw on the ambiguity.
  const column = screen.queryAllByTestId("draft-discussion-column")[0] ?? null;
  const hidden = column === null || column.hasAttribute("hidden");
  if (hidden) {
    await openActions();
    const entry = await screen.findByRole("menuitem", { name: "Discuss" });
    await waitFor(() => expect(entry).toBeEnabled());
    await userEvent.click(entry);
  }
  const composer = (await screen.findByRole("textbox", {
    name: "Discuss this draft",
  })) as HTMLTextAreaElement;
  await waitFor(() => expect(composer).toBeVisible());
  // Focused only where this helper had to reach past the column to find it. On
  // the hidden branch it was Discuss that put the caret there (DDS-FR-JWNC), and
  // focusing again here would satisfy a case that meant to check the feature.
  if (!hidden) composer.focus();
  return composer;
}

/**
 * Post the column's opening composer. Scoped to it, because a discussion card
 * carries a Post of its own and the two must not be confused (NAW-FR-32: every
 * later message is posted in the card rather than here).
 */
export async function postComposer() {
  const opener = screen.getByTestId("draft-open-discussion");
  await userEvent.click(within(opener).getByRole("button", { name: "Post" }));
}

export function pngFile(name: string, bytes = "hello") {
  return new File([bytes], name, { type: "image/png" });
}

/**
 * The two backend stubs, bound to the calling file's own `invoke` mock.
 */
export function makeStubs(invokeMock: Mock) {
  function stub(
    opts: {
      status?: DraftStatus;
      /**
       * DRS-FR-18 / NAW-FR-44: what `get_draft_graduation` answers — the run the
       * project's queue holds for this draft, or none.
       */
      graduation?: DraftGraduationStub | null | (() => DraftGraduationStub | null);
      history?: DraftHistoryList | (() => DraftHistoryList);
      proposals?: unknown[];
      /** DCR-FR-05: the changes the standing proposal holds. */
      hunks?: unknown;
      /**
       * GHP-FR-CWTG / NAW-FR-TSQE: what `get_draft_publication` answers — the
       * draft's current publication, its history, its standing attempt, and
       * whether the action is offered. A draft nobody published by default.
       */
      publication?: DraftPublicationView | (() => DraftPublicationView);
      /** GHP-FR-WKDE: what `list_publication_remotes` answers. */
      remotes?: PublicationRemoteResolution;
      /** GHP-FR-MDLD: what `load_publication_metadata` answers or throws. */
      metadata?: PublicationMetadata | (() => Promise<PublicationMetadata>);
    } = {},
  ) {
    let lastStatus: DraftStatus = opts.status ?? "active";
    /** The run the queue holds now — a value, or one the test moves on. */
    const currentGraduation = (): DraftGraduationStub | null =>
      (typeof opts.graduation === "function"
        ? opts.graduation()
        : opts.graduation) ?? null;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "set_draft_status") {
        lastStatus = (args as { status: DraftStatus }).status;
      }
      switch (cmd) {
        case "open_draft":
          return draft(lastStatus);
        case "get_draft_graduation":
          return currentGraduation() ?? null;
        // /: **Implement** reads the run for its captured
        // source worktree identity, which the trigger carries back so the
        // backend's check is bound to the checkout the run started from.
        case "get_graduation_run":
          return {
            id: currentGraduation()?.runId ?? "run-9",
            source: {
              kind: "git",
              sourceWorktreePath: "~/dev/acme",
              sourceBranch: "main",
              sourceRevision: "abc",
              graduationBranch: "synthesis/graduation/run-9",
              graduationWorktreePath: "/store/run-9/worktree",
            },
          };
        // NAW-FR-25: a rename carries the prompt with it, so the record that
        // comes back names the file under its new name.
        case "rename_draft":
          return {
            ...draft(lastStatus),
            name: RENAMED,
            promptPath: `${RENAMED}.md`,
          };
        // Echoes the requested status, so Restore is observable rather than
        // stubbed away (NAW-FR-16, NAW-FR-17, NAW-FR-28).
        case "set_draft_status":
          return draft(lastStatus);
        // NAW-FR-40 / DHS-FR-10: the rail's list, which reads no snapshot.
        case "list_draft_history":
          return typeof opts.history === "function"
            ? opts.history()
            : (opts.history ?? freshHistory());
        // NAW-FR-40 / DHS-FR-11: one version's text, fetched only on selection.
        case "load_draft_history_entry": {
          const id = (args as { entryId: string }).entryId;
          return { content: `the prompt as of ${id}`, sha256: `sha-${id}` };
        }
        // GHP-FR-CWTG / NAW-FR-TSQE: the tab's one publication read.
        case "get_draft_publication":
          return typeof opts.publication === "function"
            ? opts.publication()
            : (opts.publication ?? noPublication());
        case "list_publication_remotes":
          return opts.remotes ?? oneEligibleRemote();
        case "load_publication_metadata":
          return typeof opts.metadata === "function"
            ? opts.metadata()
            : (opts.metadata ?? publicationMetadata());
        case "load_draft_file_contents":
          return { body: "body text", checksum: "c1" };
        case "save_draft_file_contents":
          return { checksum: "c2" };
        case "graduate_draft":
          return { written: ["specifications/ui/NAW.md"] };
        // NAW-FR-35: the draft's proposals, so the pending marker has something
        // to read. Empty unless a test says otherwise.
        case "list_draft_change_proposals":
          return opts.proposals ?? [];
        // DCR-FR-05 / DCR-FR-08: the changes themselves, read only when a draft
        // holds a proposal.
        case "load_draft_change_proposal_hunks":
          return (
            opts.hunks ?? { hunks: [], resolutions: [], checksum: "h1", legacy: false }
          );
        default:
          return undefined;
      }
    });
  }

  /**
   * The base stub, plus everything the margin reads: the roster the mention
   * picker offers, the identity a comment is attributed to, the draft's existing
   * discussions, and what opening or continuing one answers with.
   */
  function stubDiscussions(
    opts: {
      agents?: string[];
      /** NAW-FR-33 / CMT-FR-24: null means no identity resolves. */
      identityError?: string | null;
      /** The participant the identity resolves to; defaults to a GitHub account. */
      identity?: Participant;
      existing?: ReturnType<typeof discussionThread>[];
      /** The typed error `open_discussion_thread` refuses with, when it does. */
      openError?: string;
      /** CTA-FR-QXIG: the turns `list_agent_turns` reports as still in flight. */
      turns?: AgentTurn[];
      /** CTA-FR-UUXA: the typed refusal `dispatch_agent_turn` answers with. */
      dispatchError?: string;
    } = {},
  ) {
    // A draft carrying versions, so a test here can read one: the discussions are
    // what these exercise, and a rail with nothing in it would exercise none of it.
    stub({ history: threeVersions() });
    const base = invokeMock.getMockImplementation()!;
    let opened = 0;
    const threads: Discussion[] = [...(opts.existing ?? [])];
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      switch (cmd) {
        case "list_project_agents":
          return (opts.agents ?? []).map(projectAgent);
        case "list_ai_api_catalogs":
          return [];
        case "get_active_ai_api_catalog":
          return { resolution: "none_configured", catalog: null };
        case "list_agent_turns":
          return opts.turns ?? [];
        case "resolve_comment_author_identity":
          if (opts.identityError) throw opts.identityError;
          return opts.identity ?? { kind: "human", login: "raver119" };
        case "list_discussions":
          return threads;
        case "open_discussion": {
          if (opts.openError) throw opts.openError;
          opened += 1;
          const { body } = args as { body: string };
          const created = discussionThread(`disc-${opened}`, [
            {
              id: `c-${opened}`,
              author: { kind: "human", login: "raver119" } as Participant,
              body,
            },
          ]);
          threads.push(created);
          return created;
        }
        case "add_comment": {
          const { discussionId, body } = args as {
            discussionId: string;
            body: string;
          };
          const found = threads.find((t) => t.id === discussionId)!;
          const updated: Discussion = {
            ...found,
            comments: [
              ...found.comments,
              {
                id: `c-reply-${found.comments.length}`,
                author: { kind: "human", login: "raver119" },
                body,
                quotes: [],
                attachments: [],
                createdAt: "2026-02-01T00:01:00Z",
              },
            ],
          };
          threads[threads.indexOf(found)] = updated;
          return updated;
        }
        case "set_discussion_resolution": {
          const { discussionId, resolved } = args as {
            discussionId: string;
            resolved: boolean;
          };
          const found = threads.find((t) => t.id === discussionId)!;
          const updated = { ...found, resolved };
          threads[threads.indexOf(found)] = updated;
          return updated;
        }
        case "dispatch_agent_turn":
          expectBackendOrigin((args as { origin: unknown }).origin);
          if (opts.dispatchError) throw opts.dispatchError;
          return {
            id: `turn-${(args as { nickname: string }).nickname}`,
            agentId: "a1",
            nickname: (args as { nickname: string }).nickname,
            origin: expectBackendOrigin((args as { origin: unknown }).origin),
            triggerCommentId: (args as { triggerCommentId: string }).triggerCommentId,
            state: "running",
            failure: null,
            retryPermitted: false,
            startedAt: "2026-02-01T00:00:00Z",
            endedAt: null,
          };
        default:
          return base(cmd, args);
      }
    });
  }

  return { stub, stubDiscussions };
}
