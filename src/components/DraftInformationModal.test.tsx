import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

type Handler = (payload: { draftId: string }) => void;
const listeners: Handler[] = [];
vi.mock("@tauri-apps/api/event", () => ({
  listen: (_name: string, handler: (event: { payload: { draftId: string } }) => void) => {
    const wrapped: Handler = (payload) => handler({ payload });
    listeners.push(wrapped);
    return Promise.resolve(() => {
      const at = listeners.indexOf(wrapped);
      if (at >= 0) listeners.splice(at, 1);
    });
  },
  emit: vi.fn(),
}));

import {
  DraftInformationModal,
  formatBoundary,
  formatCount,
  formatDuration,
  NOT_CAPTURED,
  PARTIAL_RECORD,
} from "./DraftInformationModal";
import type {
  DraftPublicationView,
  DraftStatistics,
  Measure,
  PublicationRecord,
  TokenPair,
} from "../types";

const available = (value: number): Measure => ({ value, availability: "available" });
const incomplete = (value: number): Measure => ({ value, availability: "incomplete" });
const unavailable = (): Measure => ({ value: null, availability: "unavailable" });
const pair = (input: Measure, output: Measure): TokenPair => ({ input, output });

function statistics(overrides: Partial<DraftStatistics> = {}): DraftStatistics {
  const times = {
    refinement: available(3_840_000),
    authoring: available(9_060_000),
    validationHandoffPublication: available(1_080_000),
    implementation: available(11_220_000),
    reviews: available(2_460_000),
    reconciliation: available(600_000),
    total: available(28_260_000),
  };
  const tokens = {
    refinement: pair(available(91_204), available(12_880)),
    authoring: pair(available(302_771), available(44_190)),
    validationHandoffPublication: pair(available(28_004), available(3_112)),
    implementation: pair(available(511_663), available(77_421)),
    reviews: pair(available(63_900), available(9_004)),
    reconciliation: pair(available(1_000), available(100)),
    total: pair(available(998_542), available(146_707)),
  };
  return {
    draftId: "d-1",
    boundaryAt: "2026-03-14T09:12:00.000Z",
    editingTimeMs: available(15_120_000),
    aiInteractions: available(37),
    draftEdits: available(6),
    acceptedProposals: available(5),
    rejectedProposals: available(2),
    conversationTokens: pair(available(412_908), available(12_880)),
    agentTimeMs: times,
    agentTokens: tokens,
    unreadableLines: 0,
    ...overrides,
  };
}

/** DFI-FR-TRXG: a draft nobody published — the group is absent. */
const NO_PUBLICATION: DraftPublicationView = {
  current: null,
  history: [],
  attempt: null,
  eligibility: { publishable: true, reasonCode: null, reason: null, localAssets: [] },
};

function renderModal(
  payload: DraftStatistics,
  onClose = vi.fn(),
  publication: DraftPublicationView = NO_PUBLICATION,
) {
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "read_draft_statistics") return payload;
    if (cmd === "get_draft_publication") return publication;
    return undefined;
  });
  render(
    <DraftInformationModal
      draftId={payload.draftId}
      draftName="artifact-window"
      onClose={onClose}
    />,
  );
  return onClose;
}

const calls = (name: string) => invokeMock.mock.calls.filter(([cmd]) => cmd === name);

/** One publication record, as the backend returns one. */
const record = (issueNumber: number, marker: string, publishedAt: string): PublicationRecord => ({
  provider: "github",
  repositoryOwner: "acme",
  repositoryName: "widgets",
  issueNumber,
  issueUrl: `https://github.com/acme/widgets/issues/${issueNumber}`,
  publishedAt,
  marker,
});

/** The body's labelled rows, in the order they render. */
const rowLabels = () =>
  Array.from(document.querySelectorAll(".draft-info__label")).map((n) => n.textContent);

describe("DraftInformationModal", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    listeners.length = 0;
  });
  afterEach(cleanup);

  it("DFI-FR-ZGBU, DFI-FR-LTJA: opening reads the draft's statistics once and invokes nothing else", async () => {
    renderModal(statistics());
    await screen.findByText("Time spent editing");

    expect(calls("read_draft_statistics")).toHaveLength(1);
    expect(calls("read_draft_statistics")[0][1]).toEqual({ draftId: "d-1" });
    // DFI-FR-LTJA / DFI-FR-KAVX: no draft, conversation, proposal, history,
    // graduation, or publishing operation is invoked by this surface. The
    // publication read beside the statistics read is exactly that — a read.
    expect(invokeMock.mock.calls.map(([cmd]) => cmd).sort()).toEqual([
      "get_draft_publication",
      "read_draft_statistics",
    ]);
    expect(screen.getByRole("dialog")).toHaveAttribute(
      "aria-label",
      "Information for artifact-window",
    );
  });

  it("DFI-FR-LTJA, DFI-FR-RQVE: the body shows aggregates alone and no per-run row or route", async () => {
    renderModal(statistics());
    await screen.findByText("Time spent editing");

    // DFI-FR-LTJA: with no publication record the only interactive element is
    // the dismissing control.
    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getAllByRole("button").map((b) => b.getAttribute("aria-label")))
      .toEqual(["Close"]);
    expect(within(dialog).queryByText(/run/i)).toBeNull();
  });

  it("DFI-FR-RQVE, DFI-FR-MODK, DFI-FR-HSYB: the statistics render in order, with the six buckets first-to-last", async () => {
    renderModal(statistics());
    await screen.findByText("Time spent editing");

    const labels = rowLabels();
    // DFI-FR-RQVE: the five counters, then the conversation token pair.
    expect(labels.slice(0, 7)).toEqual([
      "Time spent editing",
      "AI interactions",
      "Draft edits",
      "Accepted proposals",
      "Rejected proposals",
      "Input",
      "Output",
    ]);
    // DFI-FR-MODK: the same six buckets and a total, twice, in the fixed order.
    const buckets = [
      "Refinement",
      "Authoring",
      "Validation / hand-off / publication",
      "Implementation",
      "Reviews",
      "Reconciliation",
      "Total",
    ];
    expect(labels.slice(7, 14)).toEqual(buckets);
    expect(labels.slice(14, 21)).toEqual(buckets);
    // DFI-FR-HSYB: the total is the value the read returned rather than one the
    // surface calculated.
    expect(screen.getAllByLabelText("Total, 7h 51m")).toHaveLength(1);
  });

  it("DFI-FR-RQVE, DFI-FR-MODK, DFI-FR-HSYB: an unavailable bucket renders as not captured beside a partial total", async () => {
    const base = statistics();
    renderModal(
      statistics({
        agentTimeMs: {
          ...base.agentTimeMs,
          reconciliation: unavailable(),
          total: incomplete(27_660_000),
        },
      }),
    );
    await screen.findByText("Time spent editing");

    expect(screen.getByLabelText(`Reconciliation, ${NOT_CAPTURED}`)).toBeInTheDocument();
    // The partial sum the read returned, with the incomplete marker — the modal
    // added nothing and read no unavailable bucket as a zero.
    expect(
      screen.getByLabelText(`Total, 7h 41m, ${PARTIAL_RECORD}`),
    ).toBeInTheDocument();
  });

  it("DFI-FR-HSYB, DFI-FR-CVAX: unavailable, incomplete, and one absent token direction all render", async () => {
    renderModal(
      statistics({
        editingTimeMs: unavailable(),
        aiInteractions: incomplete(12),
        conversationTokens: pair(available(412_908), unavailable()),
      }),
    );
    await screen.findByText("Time spent editing");

    expect(
      screen.getByLabelText(`Time spent editing, ${NOT_CAPTURED}`),
    ).toBeInTheDocument();
    expect(
      screen.getByLabelText(`AI interactions, 12, ${PARTIAL_RECORD}`),
    ).toBeInTheDocument();
    expect(screen.getByLabelText("Input, 412 908")).toBeInTheDocument();
    expect(screen.getByLabelText(`Output, ${NOT_CAPTURED}`)).toBeInTheDocument();
    // DFI-FR-HSYB: no unavailable figure renders as a zero.
    expect(screen.queryByText("0")).toBeNull();
  });

  it("DFI-FR-HSYB, DFI-FR-CVAX: every statistic unavailable still renders every row and no error state", async () => {
    const none = unavailable();
    const nonePair = pair(none, none);
    renderModal(
      statistics({
        boundaryAt: null,
        editingTimeMs: none,
        aiInteractions: none,
        draftEdits: none,
        acceptedProposals: none,
        rejectedProposals: none,
        conversationTokens: nonePair,
        agentTimeMs: {
          refinement: none,
          authoring: none,
          validationHandoffPublication: none,
          implementation: none,
          reviews: none,
          reconciliation: none,
          total: none,
        },
        agentTokens: {
          refinement: nonePair,
          authoring: nonePair,
          validationHandoffPublication: nonePair,
          implementation: nonePair,
          reviews: nonePair,
          reconciliation: nonePair,
          total: nonePair,
        },
      }),
    );
    await screen.findByText("Time spent editing");

    expect(rowLabels()).toHaveLength(21);
    expect(screen.queryByRole("alert")).toBeNull();
  });

  it("DFI-FR-NTPG: the footer states the boundary, or that nothing was captured", async () => {
    renderModal(statistics());
    expect(
      await screen.findByText(/Statistics cover activity captured since/),
    ).toBeInTheDocument();

    cleanup();
    invokeMock.mockReset();
    renderModal(statistics({ boundaryAt: null }));
    expect(
      await screen.findByText("Nothing has been captured for this draft yet."),
    ).toBeInTheDocument();
    // DFI-FR-NTPG: opening the modal records nothing of any kind — its two
    // calls are both reads.
    expect(invokeMock.mock.calls.map(([cmd]) => cmd).sort()).toEqual([
      "get_draft_publication",
      "read_draft_statistics",
    ]);
  });

  it("DFI-FR-EBIL: it dismisses by the close control, by Escape, and by the backdrop", async () => {
    const onClose = renderModal(statistics());
    await screen.findByText("Time spent editing");

    await userEvent.click(screen.getByLabelText("Close"));
    expect(onClose).toHaveBeenCalledTimes(1);

    await act(async () => {
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape" }));
    });
    expect(onClose).toHaveBeenCalledTimes(2);

    await userEvent.click(document.querySelector(".scrim") as HTMLElement);
    expect(onClose).toHaveBeenCalledTimes(3);
    // DFI-FR-EBIL: dismissing invokes nothing anywhere — the two reads the
    // modal opened with are all it ever made.
    expect(invokeMock.mock.calls.map(([cmd]) => cmd).sort()).toEqual([
      "get_draft_publication",
      "read_draft_statistics",
    ]);
  });

  it("DFI-FR-ZGBU: it re-reads on a change event for this draft and ignores another's", async () => {
    renderModal(statistics());
    await screen.findByText("Time spent editing");
    // Two subscriptions: the statistics one and the publication one.
    await waitFor(() => expect(listeners).toHaveLength(2));

    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === "read_draft_statistics"
        ? statistics({ aiInteractions: available(99) })
        : undefined,
    );
    // Both channels are fired: the modal follows its own and ignores the rest.
    await act(async () => {
      for (const handler of [...listeners]) handler({ draftId: "d-1" });
    });
    expect(await screen.findByLabelText("AI interactions, 99")).toBeInTheDocument();
    expect(calls("read_draft_statistics")).toHaveLength(2);

    await act(async () => {
      for (const handler of [...listeners]) handler({ draftId: "another-draft" });
    });
    expect(calls("read_draft_statistics")).toHaveLength(2);
  });

  it("DFI-FR-CVAX: a failed read is stated with a retry rather than shown as zeros", async () => {
    invokeMock.mockImplementation(async () => {
      throw new Error("read_failed");
    });
    render(
      <DraftInformationModal draftId="d-1" draftName="artifact-window" onClose={vi.fn()} />,
    );
    expect(await screen.findByRole("alert")).toHaveTextContent(
      "This draft's statistics could not be read.",
    );
    expect(screen.queryByText("0")).toBeNull();

    invokeMock.mockImplementation(async () => statistics());
    await userEvent.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByText("Time spent editing")).toBeInTheDocument();
  });

  it("DFI-FR-TRXG: a draft nobody published carries no publication group at all", async () => {
    renderModal(statistics());
    await screen.findByText("Time spent editing");

    // An absent group is not an unavailable statistic: it is simply not there,
    // and it carries none of the not-captured wording.
    expect(screen.queryByText("Published to GitHub")).toBeNull();
    expect(screen.queryByText("Current")).toBeNull();
    expect(rowLabels()).not.toContain("Published to GitHub");
  });

  it("DFI-FR-BZQN: the current record leads the group and the earlier ones follow newest first", async () => {
    renderModal(statistics(), vi.fn(), {
      current: record(418, "pub-c", "2026-09-12T10:04:00.000Z"),
      history: [
        record(418, "pub-c", "2026-09-12T10:04:00.000Z"),
        record(402, "pub-b", "2026-09-04T16:20:00.000Z"),
        record(377, "pub-a", "2026-08-28T09:11:00.000Z"),
      ],
      attempt: null,
      eligibility: NO_PUBLICATION.eligibility,
    });
    await screen.findByText("Published to GitHub");

    expect(screen.getByText("Current")).toBeInTheDocument();
    expect(screen.getByText("History")).toBeInTheDocument();
    // Every record names the repository, the issue number, the instant, and
    // the marker.
    const issues = screen
      .getAllByRole("button")
      .map((b) => b.textContent ?? "")
      .filter((text) => text.includes("acme/widgets"));
    expect(issues).toEqual([
      "acme/widgets \u00b7 #418",
      "acme/widgets \u00b7 #402",
      "acme/widgets \u00b7 #377",
    ]);
    expect(screen.getByText("marker pub-c")).toBeInTheDocument();
    expect(screen.getByText("marker pub-a")).toBeInTheDocument();
    expect(
      screen.getByText(formatBoundary("2026-09-12T10:04:00.000Z")),
    ).toBeInTheDocument();
  });

  it("DFI-FR-WQLE, DFI-FR-KAVX: an issue link opens outside the application and nothing here publishes", async () => {
    renderModal(statistics(), vi.fn(), {
      current: record(418, "pub-c", "2026-09-12T10:04:00.000Z"),
      history: [record(418, "pub-c", "2026-09-12T10:04:00.000Z")],
      attempt: null,
      eligibility: NO_PUBLICATION.eligibility,
    });
    await screen.findByText("Published to GitHub");

    await userEvent.click(screen.getByTitle("https://github.com/acme/widgets/issues/418"));
    expect(calls("open_publication_issue")[0][1]).toEqual({
      draftId: "d-1",
      url: "https://github.com/acme/widgets/issues/418",
    });
    // The modal stays open on the draft it was reading, and it offers no
    // control that starts, retries, recovers, or abandons a publication.
    expect(screen.getByRole("dialog")).toBeInTheDocument();
    const invoked = invokeMock.mock.calls.map(([cmd]) => cmd);
    for (const forbidden of [
      "publish_draft_to_github",
      "retry_draft_publication",
      "resolve_draft_publication_conflict",
      "cancel_draft_publication_attempt",
    ]) {
      expect(invoked).not.toContain(forbidden);
    }
  });

  it("DFI-FR-BZQN, NAW-FR-QEZG: each record states root or sub-issue, with the parent, Type, and milestone it holds", async () => {
    const sub = {
      ...record(418, "pub-c", "2026-09-12T10:04:00.000Z"),
      choice: {
        kind: "sub_issue" as const,
        parentRepositoryOwner: "acme",
        parentRepositoryName: "widgets",
        parentIssueNumber: 412,
        issueType: "Task",
        milestoneNumber: 7,
        milestoneTitle: "v1.2",
      },
    };
    renderModal(statistics(), vi.fn(), {
      current: sub,
      history: [sub, record(402, "pub-b", "2026-09-04T16:20:00.000Z")],
      attempt: null,
      eligibility: NO_PUBLICATION.eligibility,
    });
    await screen.findByText("Published to GitHub");

    const choices = screen
      .getAllByTestId("publication-record-choice")
      .map((node) => node.textContent);
    expect(choices).toEqual([
      "Sub-issue of #412 in acme/widgets \u00b7 Type Task \u00b7 Milestone v1.2",
      // A record written before the choice existed reads as a root issue.
      "Root issue",
    ]);
  });

  it("DFI-FR-BZQN: the group re-reads on a publication change naming this draft", async () => {
    renderModal(statistics());
    await screen.findByText("Time spent editing");
    const before = calls("get_draft_publication").length;

    await act(async () => {
      for (const handler of [...listeners]) handler({ draftId: "d-1" });
    });
    expect(calls("get_draft_publication").length).toBeGreaterThan(before);
  });

  it("formats a duration, a count, and a boundary the way the wireframe reads", () => {
    expect(formatDuration(15_120_000)).toBe("4h 12m");
    expect(formatDuration(1_080_000)).toBe("18m");
    expect(formatDuration(4_000)).toBe("4s");
    expect(formatCount(412908)).toBe("412 908");
    expect(formatBoundary("2026-03-14T09:12:00.000Z")).toBe("14 Mar 2026");
  });
});
