/**
 * Shared builders for the change workspace tests
 * (`../components/OpenSpecChange/*.test.tsx`): a backend that answers the
 * change read, the discussion commands of a change owner, the commands of
 * `CEP-change-edit-proposals.md`, and those of `CVH-change-version-history.md`,
 * `CST-change-statistics.md`, and `NCP-new-change-proposals.md`. An edit of a
 * hunk answers the checksum `ck-2`.
 */
import { vi, type Mock } from "vitest";
import { render } from "@testing-library/react";

import { OpenSpecChangeWorkspace } from "../components/OpenSpecChange";
import { demoChange } from "../components/OpenSpecChange/fixtures";
import { resetLogBufferForTest } from "../logging";
import { resetAgentRegistry } from "../state/agentRegistry";
import { changeHunkCandidateBuffers } from "../state/candidateBuffers";
import { resetDiscussionColumns } from "../state/discussionColumn";
import { resetDiscussionFocus } from "../state/discussionFocus";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { resetDraftDiscussions } from "../state/draftDiscussion";
import { resetChangeProposals } from "../state/openspec/changeProposals";
import { resetChangeDiscussionRequests } from "../state/openspec/changeRoutes";
import { resetChangeViewsForTest } from "../state/openspec/changeView";
import { resetNewChangeProposals } from "../state/openspec/newChangeProposals";
import { resetProposalHunks } from "../state/proposalHunks";
import { resetToasts } from "../state/toasts";
import type {
  ChangeDecisionOutcome,
  ChangeDetail,
  ChangeSplit,
  ChangeStatistics,
  ChangeVersionFile,
  ChangeVersionList,
  NewChangeDecisionOutcome,
  NewChangeProposal,
  ChangeEditHunks,
  ChangeEditProposal,
  Discussion,
  GraduationRun,
  HunkState,
  OpenSpecChangeTabTarget,
} from "../types";

export const STREAM_ID = "s-1";
export const CHANGE = "add-logout-3f2a";
export const TARGET = { kind: "change", streamId: STREAM_ID, change: CHANGE } as const;
export const PROPOSAL_TEXT = "# Why\n\nUsers cannot log out.\n\nMore text.\n";
export const BEFORE = "Users cannot log out.";
export const AFTER = "Users cannot sign out.";
export const NEW_PATH = "specs/notify/spec.md";
export const NEW_TEXT = "## ADDED Requirements\n";
export const AGENT = { kind: "agent", agentId: "a1", handle: "arch", model: "m" } as const;
export const ME = { kind: "human", login: "raver119" } as const;

/** The demo change, with a proposal text that a hunk edits. */
export function changeDetail(): ChangeDetail {
  const detail = demoChange();
  detail.artifacts[0].files[0].text = PROPOSAL_TEXT;
  return detail;
}

/** A pending proposal with a hunk on `proposal.md` and a new-file hunk. */
export function changeProposal(
  states: [HunkState, HunkState] = ["pending", "pending"],
  over: Partial<ChangeEditProposal> = {},
): ChangeEditProposal {
  const count = (s: HunkState) => states.filter((x) => x === s).length;
  return {
    id: "p1",
    streamId: STREAM_ID,
    change: CHANGE,
    rationale: "Two edits.",
    threadId: "t1",
    commentId: "t1-c2",
    state: "pending",
    candidateEdited: false,
    commentOwed: false,
    commitOwed: false,
    hunkCount: 2,
    createdAt: "2026-01-01T00:00:00Z",
    counts: {
      pending: count("pending"),
      accepted: count("accepted"),
      rejected: count("rejected"),
      discussing: count("discussing"),
      withdrawn: 0,
    },
    writtenPaths: [],
    hunks: [
      { id: "h1", path: "proposal.md", kind: "replace", newFile: false, state: states[0], edited: false, revision: 0 },
      { id: "h2", path: NEW_PATH, kind: "add", newFile: true, state: states[1], edited: false, revision: 0 },
    ],
    agent: AGENT,
    ...over,
  };
}

export function changeHunks(): ChangeEditHunks {
  const at = PROPOSAL_TEXT.indexOf(BEFORE);
  return {
    hunks: [
      {
        id: "h1",
        kind: "replace",
        revision: 0,
        before: BEFORE,
        after: AFTER,
        anchor: { lead: "# Why\n\n", trail: "\n\nMore", hint_start: at, hint_end: at + BEFORE.length },
        path: "proposal.md",
        newFile: false,
        state: "pending",
      },
      {
        id: "h2",
        kind: "add",
        revision: 0,
        before: null,
        after: NEW_TEXT,
        anchor: { lead: "", trail: "", hint_start: 0, hint_end: 0 },
        path: NEW_PATH,
        newFile: true,
        state: "pending",
      },
    ],
    resolutions: [
      { kind: "resolved", start: at, end: at + BEFORE.length },
      { kind: "resolved", start: 0, end: 0 },
    ],
    checksum: "ck-1",
  };
}

/** A whole-change discussion, and a fragment discussion of `proposal.md`. */
export function changeDiscussions(): Discussion[] {
  const base = {
    resolved: false,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
  };
  return [
    {
      ...base,
      id: "t1",
      target: TARGET,
      fragmentTarget: null,
      comments: [
        { id: "t1-c1", author: ME, body: "Please fix the wording.", quotes: [], attachments: [], createdAt: base.createdAt },
        {
          id: "t1-c2",
          author: AGENT,
          body: "Two edits.",
          quotes: [],
          attachments: [{ kind: "changeProposal", proposalId: "p1", streamId: STREAM_ID, change: CHANGE }],
          createdAt: base.createdAt,
        },
      ],
    },
    {
      ...base,
      id: "t2",
      target: TARGET,
      fragmentTarget: { owner: TARGET, path: "proposal.md", start: 0, end: 5, quote: "# Why" },
      comments: [
        { id: "t2-c1", author: ME, body: "A heading question.", quotes: [], attachments: [], createdAt: base.createdAt },
      ],
    },
  ];
}

/** CVH: three versions of the change, newest first. */
export function changeVersions(): ChangeVersionList {
  const at = (n: number) => `2026-01-0${n}T10:00:00Z`;
  return {
    versions: [
      { commitId: "c3".padEnd(40, "3"), summary: "Apply the edits of p0", authorName: "Ann", committedAt: at(3), added: [], modified: ["proposal.md"], deleted: [] },
      { commitId: "c2".padEnd(40, "2"), summary: "Add the design", authorName: "Bob", committedAt: at(2), added: ["design.md", "diagram.png"], modified: [], deleted: [] },
      { commitId: "c1".padEnd(40, "1"), summary: "Propose add-logout-3f2a", authorName: "Ann", committedAt: at(1), added: ["proposal.md", "tasks.md"], modified: [], deleted: [] },
    ],
    truncated: false,
  };
}

/** CVH-FR-VDZZ: the files of the oldest version: an older proposal and a binary file. */
export function versionFiles(): ChangeVersionFile[] {
  return [
    { path: "diagram.png", bytes: 120, binary: true },
    { path: "proposal.md", text: "# Why\n\nUsers cannot leave.\n", bytes: 26, binary: false },
    { path: "tasks.md", text: "- [ ] 1.1 Add the entry\n", bytes: 24, binary: false },
  ];
}

/** CST: every statistic available. */
export function changeStatistics(over: Partial<ChangeStatistics> = {}): ChangeStatistics {
  const m = (value: number) => ({ value, availability: "available" as const });
  return {
    streamId: STREAM_ID,
    change: CHANGE,
    boundaryAt: "2026-01-01T00:00:00Z",
    workingTimeMs: m(90 * 60_000),
    aiInteractions: m(12),
    versions: m(3),
    acceptedProposals: m(2),
    rejectedProposals: m(1),
    conversationTokens: { input: m(12_345), output: m(678) },
    unreadableLines: 0,
    ...over,
  };
}

/** NCP: a proposal of the split of the change. */
export function newChangeProposal(over: Partial<NewChangeProposal> = {}): NewChangeProposal {
  return {
    id: "n1",
    setId: "set-1",
    streamId: STREAM_ID,
    change: CHANGE,
    name: "add-logout-ui",
    paths: ["proposal.md", "specs/ui/spec.md"],
    position: 1,
    total: 2,
    agent: AGENT,
    rationale: "The UI part stands alone.",
    threadId: "t3",
    commentId: "t3-c2",
    state: "pending",
    revision: 0,
    commitOwed: false,
    createdAt: "2026-01-02T00:00:00Z",
    ...over,
  };
}

/** NCP / CMS-FR-QYGP: a discussion whose agent comment carries one new-change proposal. */
export function splitDiscussion(): Discussion {
  const at = "2026-01-02T00:00:00Z";
  return {
    id: "t3",
    target: TARGET,
    fragmentTarget: null,
    resolved: false,
    createdAt: at,
    updatedAt: at,
    comments: [
      { id: "t3-c1", author: ME, body: "Please split this change.", quotes: [], attachments: [], createdAt: at },
      {
        id: "t3-c2",
        author: AGENT,
        body: "The UI part stands alone.",
        quotes: [],
        attachments: [{ kind: "newChangeProposal", proposalId: "n1", streamId: STREAM_ID, change: CHANGE }],
        createdAt: at,
      },
    ],
  };
}

export interface ChangeBackend {
  detail: ChangeDetail;
  proposals: ChangeEditProposal[];
  hunks: ChangeEditHunks;
  discussions: Discussion[];
  runs: GraduationRun[];
  /** What a decision answers, or a code it rejects with. */
  decide: (cmd: string, args: Record<string, unknown>) => ChangeDecisionOutcome | { reject: string };
  complete?: () => ChangeDecisionOutcome;
  readReject?: string;
  /** CVH: the versions, or a code the list rejects with. */
  versions: ChangeVersionList | { reject: string };
  /** CVH: the files every version read answers, or a code it rejects with. */
  versionFiles: ChangeVersionFile[] | { reject: string };
  /** CST: the statistics each read answers, or a code it rejects with. */
  statistics: ChangeStatistics | { reject: string };
  /** NCP: the proposals of the split, and the split. */
  newChanges: NewChangeProposal[];
  split: ChangeSplit | null;
  /** NCP: what a new-change decision answers, or a code it rejects with. */
  decideNew: (cmd: string, args: Record<string, unknown>) => NewChangeDecisionOutcome | { reject: string };
  /** NCP: what a confirmation answers, or a code it rejects with. */
  confirm: () => ChangeSplit | { reject: string };
  /** OSM-FR-BLEY: the images `read_change_image` answers, by change-relative path. */
  images: Record<string, { data: string; mediaType: string }>;
  /** The paths `read_change_image` was asked for, in order. */
  imageReads: string[];
  unknown: string[];
}

export function changeBackend(over: Partial<ChangeBackend> = {}): ChangeBackend {
  return {
    images: {},
    imageReads: [],
    detail: changeDetail(),
    proposals: [changeProposal()],
    hunks: changeHunks(),
    discussions: changeDiscussions(),
    runs: [],
    decide: () => ({ reject: "write_failed" }),
    versions: changeVersions(),
    versionFiles: versionFiles(),
    statistics: changeStatistics(),
    newChanges: [],
    split: null,
    decideNew: () => ({ reject: "write_failed" }),
    confirm: () => ({ reject: "nothing_accepted" }),
    unknown: [],
    ...over,
  };
}

/** Route `invoke` by command name. An unknown command is a rejected promise. */
export function wireChangeBackend(invokeMock: Mock, b: ChangeBackend): void {
  invokeMock.mockImplementation(async (cmd: string, args: Record<string, unknown> = {}) => {
    switch (cmd) {
      case "append_log_records":
        return undefined;
      case "read_openspec_change":
        if (b.readReject) throw b.readReject;
        return b.detail;
      case "list_discussions":
        return b.discussions;
      case "read_discussion": {
        const found = b.discussions.find((d) => d.id === args.discussionId);
        if (!found) throw "discussion_not_found";
        return found;
      }
      case "resolve_comment_author_identity":
        return ME;
      case "list_project_agents":
        return [];
      case "list_agent_turns":
      case "list_agent_turn_image_notices":
      case "list_recoverable_agent_turn_failures":
        return [];
      case "read_discussion_question_set":
        return null;
      case "estimate_discussion_context":
        return null;
      case "list_change_edit_proposals":
        return b.proposals;
      case "load_change_edit_proposal_hunks":
        return b.hunks;
      case "list_graduation_queue":
        return { projectKey: "k", runs: b.runs };
      case "accept_change_edit_hunk":
      case "reject_change_edit_hunk":
      case "decline_change_edit_proposal": {
        const answer = b.decide(cmd, args);
        if ("reject" in answer) throw answer.reject;
        return answer;
      }
      case "complete_change_edit_decision":
        if (!b.complete) throw "proposal_not_found";
        return b.complete();
      case "set_change_edit_hunk_discussing":
        return b.proposals[0];
      case "edit_change_edit_hunk":
        return { checksum: "ck-2" };
      case "list_change_versions":
        if ("reject" in b.versions) throw b.versions.reject;
        return b.versions;
      case "read_change_image": {
        const path = String(args?.path ?? "");
        b.imageReads.push(path);
        const image = b.images[path];
        if (!image) throw "file_not_found";
        return image;
      }
      case "read_change_version":
        if ("reject" in b.versionFiles) throw b.versionFiles.reject;
        return { files: b.versionFiles };
      case "read_change_statistics":
        if ("reject" in b.statistics) throw b.statistics.reject;
        return b.statistics;
      case "record_change_working_interval":
        return null;
      case "list_new_change_proposals":
        return { proposals: b.newChanges, split: b.split };
      case "load_new_change_proposal_files":
        return { files: [{ path: "proposal.md", text: "# Why\n\nThe UI part.\n" }] };
      case "accept_new_change_proposal":
      case "reject_new_change_proposal":
      case "complete_new_change_decision": {
        const answer = b.decideNew(cmd, args);
        if ("reject" in answer) throw answer.reject;
        return answer;
      }
      case "confirm_change_split": {
        const answer = b.confirm();
        if ("reject" in answer) throw answer.reject;
        return answer;
      }
      case "open_discussion": {
        const created: Discussion = {
          id: "t-new",
          target: TARGET,
          fragmentTarget: (args.fragmentTarget as Discussion["fragmentTarget"]) ?? null,
          comments: [
            {
              id: "t-new-c1",
              author: ME,
              body: String(args.body ?? ""),
              quotes: [],
              attachments: (args.attachments as never) ?? [],
              createdAt: "2026-01-02T00:00:00Z",
            },
          ],
          resolved: false,
          createdAt: "2026-01-02T00:00:00Z",
          updatedAt: "2026-01-02T00:00:00Z",
        };
        b.discussions = [...b.discussions, created];
        return created;
      }
      default:
        b.unknown.push(cmd);
        throw new Error(`unexpected invoke ${cmd}`);
    }
  });
}

/** Every module store the change workspace touches, reset for the next test. */
export function resetChangeWorkspaceStores(): void {
  resetLogBufferForTest();
  resetChangeViewsForTest();
  resetChangeProposals();
  resetNewChangeProposals();
  resetProposalHunks();
  resetDraftDiscussions();
  resetDiscussionColumns();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  resetToasts();
  resetChangeDiscussionRequests();
  resetAgentRegistry();
  changeHunkCandidateBuffers.clear();
}

export function changeTarget(over: Partial<OpenSpecChangeTabTarget> = {}): OpenSpecChangeTabTarget {
  return {
    location: { kind: "stream", streamId: STREAM_ID },
    name: CHANGE,
    streamName: "auth-work",
    file: null,
    openSeq: 1,
    ...over,
  };
}

export function drawChange(over: Partial<OpenSpecChangeTabTarget> = {}, tabId = "tab-1") {
  const onOpenArtifact = vi.fn();
  const view = render(
    <OpenSpecChangeWorkspace tabId={tabId} target={changeTarget(over)} onOpenArtifact={onOpenArtifact} />,
  );
  return { onOpenArtifact, view };
}
