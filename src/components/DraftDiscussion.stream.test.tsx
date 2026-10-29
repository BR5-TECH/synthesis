/**
 * The forms the draft discussion column draws its messages in
 * (`../../specifications/ui/DDS-draft-discussion.md` DDS-FR-ZMXQ through
 * DDS-FR-RJEV, and `../../specifications/ui/DQA-discussion-question-answering.md`
 * DQA-FR-KYWR, DQA-FR-ZPGM, DQA-FR-BHXT).
 *
 * `./DraftDiscussion.test.tsx` holds the column's *position* — the split, the
 * ratio, the auto-follow rule. This file holds what a message looks like once it
 * is in there: which forms are boxed, which carry the accent, and what the
 * column never draws twice.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { act, cleanup, fireEvent, render, screen, within } from "@testing-library/react";

const invokeMock = vi.fn();
import { resetLogBufferForTest } from "../logging";
import { resetProjectIdentity } from "../state/projectIdentity";
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
const listeners = new Map<string, Set<(e: { payload: unknown }) => void>>();
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    async (event: string, cb: (e: { payload: unknown }) => void) => {
      const set = listeners.get(event) ?? new Set();
      set.add(cb);
      listeners.set(event, set);
      return () => set.delete(cb);
    },
  ),
}));

import { postAcceleratorHint } from "./discussion";
import { resetDraftDiscussions } from "../state/draftDiscussion";
import {
  noteProposalChanged,
  resetDraftProposals,
} from "../state/draftProposals";
import {
  removeDiscussionTurn,
  setDiscussionTurns,
} from "../state/discussionSession";
import {
  DRAFT,
  columnElement,
  comment as message,
  discussion,
  type ColumnProps,
} from "../test/draftDiscussionFixtures";
import type {
  AgentTurn,
  Attachment,
  Comment,
  DraftChangeProposal,
  Participant,
} from "../types";
import {
  draftDiscussionOrigin,
} from "../test/origins";

const ME: Participant = { kind: "human", login: "raver119" };
const HELGA: Participant = {
  kind: "agent",
  agentId: "a1",
  handle: "helga",
  title: "UI/UX developer",
};

type Over = Partial<ColumnProps>;

function columnFor(comments: Comment[], over: Over = {}) {
  return columnElement({
    discussions: [discussion("disc-1", comments)],
    ...over,
  });
}

function renderColumn(comments: Comment[], over: Over = {}) {
  return render(columnFor(comments, over));
}

beforeEach(() => {
  invokeMock.mockReset();
  invokeMock.mockResolvedValue([]);
  resetDraftDiscussions();
  resetDraftProposals();
  // The project login and the log buffer are module-level, so a login or a
  // flush timer from one test must not reach the next.
  resetProjectIdentity();
  resetLogBufferForTest();
});
afterEach(cleanup);

describe("plain talk and the author's own message (DDS-FR-VTKD, DDS-FR-QJFE)", () => {
  it("DDS-FR-VTKD: the author's own message is marked, and one written to them is not", async () => {
    renderColumn([
      message("c1", HELGA, "publication metadata is append-only"),
      message("c2", ME, "never store local assets in the repo"),
    ]);
    const blocks = await screen.findAllByTestId("dds-message");
    expect(blocks).toHaveLength(2);
    // The rail is the whole of the mark: the stylesheet draws it from
    // `data-own`, and nothing else about the block changes.
    expect(blocks[0]).not.toHaveAttribute("data-own");
    expect(blocks[1]).toHaveAttribute("data-own", "true");
  });

  it("DDS-FR-LWPC: the author line reads name, then role, then time, on one row", async () => {
    renderColumn([message("c1", HELGA, "two questions first")]);
    const head = (await screen.findByTestId("dds-message")).firstElementChild!;
    expect(head.textContent).toMatch(/helga.*UI\/UX developer/);
    // The time is in the same row rather than floated into a corner.
    expect(head.querySelector(".dds-stream__time")).not.toBeNull();
  });

  it("DDS-FR-QJFE: a run by one author carries one author line and both bodies", async () => {
    renderColumn([
      message("c1", HELGA, "first paragraph"),
      message("c2", HELGA, "second paragraph"),
    ]);
    const blocks = await screen.findAllByTestId("dds-message");
    expect(blocks).toHaveLength(1);
    expect(blocks[0]).toHaveTextContent("first paragraph");
    expect(blocks[0]).toHaveTextContent("second paragraph");
    expect(within(blocks[0]).getAllByText("helga")).toHaveLength(1);
  });

  it("DDS-FR-NDSA: a day boundary is drawn as a divider between the two days", async () => {
    renderColumn([
      message("c1", HELGA, "yesterday's", { createdAt: "2026-01-01T09:00:00Z" }),
      message("c2", HELGA, "today's", { createdAt: "2026-01-02T09:00:00Z" }),
    ]);
    const dividers = await screen.findAllByTestId("dds-day-divider");
    expect(dividers).toHaveLength(2);
    // Lowercase, in the column's own register.
    expect(dividers[0].textContent).toBe(dividers[0].textContent!.toLowerCase());
  });
});

describe("the local participant's label (CVP-FR-TIBK, CMT-FR-ZCAE)", () => {
  const LOCAL: Participant = { kind: "human", login: "", displayName: "Me" };
  const names = () =>
    Array.from(document.querySelectorAll("[data-testid='dds-message']")).map(
      (block) => block.firstElementChild?.textContent ?? "",
    );

  it("CVP-FR-TIBK: reads Me in the draft discussion column while no project identity resolves", async () => {
    renderColumn([
      message("c1", LOCAL, "written without a token"),
      message("c2", HELGA, "an agent answers"),
    ]);
    const blocks = await screen.findAllByTestId("dds-message");
    expect(blocks[0]).toHaveTextContent("Me");
    expect(blocks[1]).toHaveTextContent("helga");
  });

  it("CVP-FR-TIBK, CMT-FR-ZCAE: reads the project login once a token resolves, and Me again when it stops", async () => {
    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === "resolve_comment_author_identity" ? LOCAL : [],
    );
    renderColumn([message("c1", LOCAL, "written without a token")]);
    await screen.findByTestId("dds-message");
    expect(names()[0]).toMatch(/^Me/);

    invokeMock.mockImplementation(async (cmd: string) =>
      cmd === "resolve_comment_author_identity"
        ? { kind: "human", login: "octocat" }
        : [],
    );
    await act(async () => {
      for (const cb of listeners.get("github-tokens-changed") ?? []) cb({ payload: null });
    });
    await vi.waitFor(() => expect(names()[0]).toMatch(/^octocat/));

    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "resolve_comment_author_identity") throw "github_token_selection_required";
      return [];
    });
    await act(async () => {
      for (const cb of listeners.get("github-tokens-changed") ?? []) cb({ payload: null });
    });
    await vi.waitFor(() => expect(names()[0]).toMatch(/^Me/));
  });
});

describe("a submitted exchange is one card (DQA-FR-KYWR, DQA-FR-ZPGM, DQA-FR-BHXT)", () => {
  const question = message(
    "c1",
    HELGA,
    "What should happen to the uploaded files?\n\n" +
      "1. Delete uploaded assets after failure\n" +
      "2. Keep and reuse marker-scoped uploads on retry",
  );

  it("DQA-FR-KYWR: marks the chosen option in place and states the answer nowhere else", async () => {
    renderColumn([
      question,
      message(
        "c2",
        ME,
        "**Selected option:** Keep and reuse marker-scoped uploads on retry\n\n" +
          "**Note:** Scope the marker per draft, not per repo.",
      ),
    ]);
    const card = await screen.findByTestId("dds-question-card");

    const chosen = card.querySelectorAll("[data-chosen]");
    expect(chosen).toHaveLength(1);
    expect(chosen[0]).toHaveTextContent(
      "Keep and reuse marker-scoped uploads on retry",
    );
    expect(card).toHaveTextContent("Scope the marker per draft, not per repo.");
    expect(card).toHaveTextContent("your note");

    // The answer is stated once. No message repeats it, and the fixed line the
    // submission composed never reaches the reader.
    expect(screen.queryByTestId("dds-message")).toBeNull();
    expect(screen.queryByText(/Selected option/)).toBeNull();
  });

  it("DQA-FR-ZPGM: an answer in the author's own words marks no option", async () => {
    renderColumn([
      question,
      message("c2", ME, "**Own answer:** delete them and say so in the log"),
    ]);
    const card = await screen.findByTestId("dds-question-card");
    expect(card.querySelectorAll("[data-chosen]")).toHaveLength(0);
    expect(card).toHaveTextContent("your answer");
    expect(card).toHaveTextContent("delete them and say so in the log");
    expect(screen.queryByText(/Own answer/)).toBeNull();
  });

  it("DQA-FR-BHXT: an answered card offers no control that changes the answer", async () => {
    renderColumn([
      question,
      message("c2", ME, "**Selected option:** Delete uploaded assets after failure"),
    ]);
    const card = await screen.findByTestId("dds-question-card");
    // Read-only about the **answer**: no field to change it with, no
    // re-selection, no reopen. The one control the card carries is the one the
    // conversation already offers on a message — Quote (CMT-FR-12). The
    // thread's own menu stands in the column header (DDS-FR-MZQJ).
    expect(card.querySelectorAll("input")).toHaveLength(0);
    const named = [...card.querySelectorAll("button")].map(
      (c) => c.getAttribute("title") ?? c.getAttribute("aria-label"),
    );
    expect(named).toEqual(["Quote"]);
  });

  it("DDS-FR-MZQJ, CMT-FR-15: a discussion that opens with a question carries its menu in the column header", async () => {
    // Whatever form the opening comment took, the menu is in one place.
    renderColumn([
      question,
      message("c2", ME, "**Selected option:** Delete uploaded assets after failure"),
    ]);
    const card = await screen.findByTestId("dds-question-card");
    expect(
      within(card).queryByRole("button", { name: "Thread actions for disc-1" }),
    ).toBeNull();
    const header = screen.getByTestId("discussion-header-actions");
    expect(
      await within(header).findByRole("button", { name: "Thread actions for disc-1" }),
    ).toBeInTheDocument();
  });

  it("DDS-FR-LWPC: the card names the role the agent asked under", async () => {
    renderColumn([
      question,
      message("c2", ME, "**Selected option:** Delete uploaded assets after failure"),
    ]);
    const card = await screen.findByTestId("dds-question-card");
    expect(card).toHaveTextContent("UI/UX developer");
  });

  it("CMT-FR-12: the question stays quotable, named by its position in the thread", async () => {
    renderColumn([
      question,
      message("c2", ME, "**Selected option:** Delete uploaded assets after failure"),
    ]);
    const card = await screen.findByTestId("dds-question-card");
    expect(
      within(card).getByRole("button", { name: "Quote comment 1 by helga" }),
    ).toBeInTheDocument();
  });

  it("DDS-FR-MCUP: the card names its kind with a QST chip", async () => {
    renderColumn([
      question,
      message("c2", ME, "**Selected option:** Delete uploaded assets after failure"),
    ]);
    const card = await screen.findByTestId("dds-question-card");
    expect(within(card).getByText("QST")).toBeInTheDocument();
  });

  it("DQA-FR-TSJD: a pair this reader cannot read falls back to two ordinary messages", async () => {
    // The grouping is lost and nothing else is: both comments are still there,
    // and the conversation reads as the two messages it is.
    renderColumn([
      message("c1", HELGA, "no option list here"),
      message("c2", ME, "**Selected option:** something"),
    ]);
    await screen.findAllByTestId("dds-message");
    expect(screen.queryByTestId("dds-question-card")).toBeNull();
    expect(screen.getByText("no option list here")).toBeInTheDocument();
  });

  /** The question half, carrying its recorded option descriptions. */
  const described: Comment = {
    ...question,
    optionDescriptions: [
      "Removes the files at once; a retry uploads them again.",
      "Saves the upload on retry; the marker must be cleaned up later.",
    ],
  };
  const chosenAnswer = message(
    "c2",
    ME,
    "**Selected option:** Keep and reuse marker-scoped uploads on retry",
  );

  it("DQA-FR-MDMU, DQA-FR-XRLK: a described card carries a visible control, collapsed by default", async () => {
    renderColumn([described, chosenAnswer]);
    const card = await screen.findByTestId("dds-question-card");
    const toggle = within(card).getByTestId("option-descriptions-toggle");
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(toggle).toHaveAccessibleName("Show option descriptions");
    // In the head, not among the actions that show on hover.
    expect(toggle.closest(".dds-message__actions")).toBeNull();
    expect(toggle.closest(".dds-question__head")).not.toBeNull();
    expect(within(card).queryAllByTestId("dds-question-description")).toHaveLength(0);
  });

  it("DQA-FR-XRLK: expanded, each description stands beneath its own option, and the chosen mark stays", async () => {
    renderColumn([described, chosenAnswer]);
    const card = await screen.findByTestId("dds-question-card");
    fireEvent.click(within(card).getByTestId("option-descriptions-toggle"));

    const options = card.querySelectorAll(".dds-question__option");
    expect(options[0]).toHaveTextContent(
      "Removes the files at once; a retry uploads them again.",
    );
    expect(options[1]).toHaveTextContent(
      "Saves the upload on retry; the marker must be cleaned up later.",
    );
    expect(card.querySelectorAll("[data-chosen]")).toHaveLength(1);
    expect(options[1]).toHaveAttribute("data-chosen");

    fireEvent.click(within(card).getByTestId("option-descriptions-toggle"));
    expect(within(card).queryAllByTestId("dds-question-description")).toHaveLength(0);
  });

  it("DQA-FR-MDMU, DQA-FR-XRLK: an unpaired described question drawn as a message keeps its control", async () => {
    renderColumn([described, message("c2", ME, "something else entirely")]);
    await screen.findAllByTestId("dds-message");
    expect(screen.queryByTestId("dds-question-card")).toBeNull();
    const toggle = screen.getByTestId("option-descriptions-toggle");
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    // Visible at all times, not among the actions that show on hover.
    expect(toggle.closest(".dds-message__actions")).toBeNull();
    fireEvent.click(toggle);
    const items = within(screen.getByTestId("option-descriptions")).getAllByRole("listitem");
    expect(items[0]).toHaveTextContent("Delete uploaded assets after failure");
    expect(items[0]).toHaveTextContent("Removes the files at once; a retry uploads them again.");
  });

  it("DQA-FR-XRLK: a card whose descriptions do not match its options lists them below rather than pairing them", async () => {
    renderColumn([
      { ...question, optionDescriptions: ["Only one description."] },
      chosenAnswer,
    ]);
    const card = await screen.findByTestId("dds-question-card");
    fireEvent.click(within(card).getByTestId("option-descriptions-toggle"));
    for (const option of card.querySelectorAll(".dds-question__option")) {
      expect(option).not.toHaveTextContent("Only one description.");
    }
    expect(within(card).getByTestId("option-descriptions")).toHaveTextContent(
      "Only one description.",
    );
  });

  it("DQA-FR-JADB: a card's descriptions show with provider citation markers hidden, paired or listed", async () => {
    renderColumn([
      {
        ...question,
        optionDescriptions: ["Removes them \uE200cite\uE201 at once.", "Saves the upload."],
      },
      chosenAnswer,
      { ...question, id: "c3", optionDescriptions: ["Only one \uE200cite\uE201 here."] },
      message("c4", ME, "**Selected option:** Delete uploaded assets after failure"),
    ]);
    const cards = await screen.findAllByTestId("dds-question-card");
    for (const card of cards) fireEvent.click(within(card).getByTestId("option-descriptions-toggle"));
    expect(within(cards[0]).getAllByTestId("dds-question-description")[0].textContent).toBe(
      "Removes them at once.",
    );
    expect(within(cards[1]).getByTestId("option-descriptions").textContent).toBe("Only one here.");
  });

  it("DQA-FR-UHXW: expanding one card leaves another collapsed", async () => {
    renderColumn([
      described,
      chosenAnswer,
      { ...described, id: "c3" },
      message("c4", ME, "**Selected option:** Delete uploaded assets after failure"),
    ]);
    const cards = await screen.findAllByTestId("dds-question-card");
    fireEvent.click(within(cards[0]).getByTestId("option-descriptions-toggle"));
    expect(within(cards[0]).getAllByTestId("dds-question-description")).toHaveLength(2);
    expect(within(cards[1]).queryAllByTestId("dds-question-description")).toHaveLength(0);
  });

  it("DQA-FR-MDMU: a card whose question carries no descriptions has no control", async () => {
    renderColumn([question, chosenAnswer]);
    const card = await screen.findByTestId("dds-question-card");
    expect(within(card).queryByTestId("option-descriptions-toggle")).toBeNull();
  });
});

describe("a change reference is one row (DDS-FR-XHRB, DDS-FR-MCUP, DDS-FR-YSNB)", () => {
  const reference: Attachment = {
    kind: "proposal",
    proposalId: "p1",
    draftId: DRAFT,
    path: "Post draft to Github.md",
  };

  function seedProposal(state: DraftChangeProposal["state"]) {
    noteProposalChanged({
      draftId: DRAFT,
      proposal: {
        id: "p1",
        draftId: DRAFT,
        path: "Post draft to Github.md",
        agent: HELGA,
        rationale: "two changes",
        threadId: "disc-1",
        commentId: "c1",
        state,
        candidateEdited: false,
        legacy: false,
        hunkCount: 2,
        counts: { total: 2, accepted: 2, rejected: 0, pending: 0, discussing: 0 },
        ledger: [],
        createdAt: "2026-01-01T00:00:00Z",
      },
    } as unknown as Parameters<typeof noteProposalChanged>[0]);
  }

  it("DDS-FR-XHRB, DDS-FR-YSNB: names the file, the count, and the state in one lowercase word", async () => {
    seedProposal("accepted");
    renderColumn([
      message("c1", HELGA, "", { attachments: [reference] }),
    ]);
    const row = await screen.findByTestId("dds-change-row");
    expect(row).toHaveTextContent("Post draft to Github.md");
    expect(row).toHaveTextContent("2 changes");
    expect(row).toHaveTextContent("accepted");
    // DDS-FR-MCUP: the chip names the kind of the row.
    expect(within(row).getByText("CHG")).toBeInTheDocument();
  });

  it("DDS-FR-XHRB: a body and a reference together read as a message and a row", async () => {
    seedProposal("accepted");
    renderColumn([
      message("c1", HELGA, "I changed two things.", { attachments: [reference] }),
    ]);
    await screen.findByTestId("dds-change-row");
    expect(screen.getByTestId("dds-message")).toHaveTextContent(
      "I changed two things.",
    );
  });

  it("CTA-FR-UKIG: a reference the reading has not answered says nothing about it", async () => {
    // No proposal seeded, so nothing on the row may be read as a claim about
    // what the change has become — and it opens nothing.
    renderColumn([message("c1", HELGA, "", { attachments: [reference] })]);
    const row = await screen.findByTestId("dds-change-row");
    expect(row).toBeDisabled();
    expect(row).not.toHaveTextContent("accepted");
    expect(row).not.toHaveTextContent("pending");
  });
});

describe("the transient row (DDS-FR-TGWY, DDS-FR-YSNB)", () => {
  const turn = {
    id: "turn-1",
    agentId: "a1",
    nickname: "helga",
    origin: draftDiscussionOrigin("disc-1", DRAFT),
    triggerCommentId: "c1",
    state: "running",
    failure: null,
    retryPermitted: false,
    imagesOmitted: false,
    activeToolCalls: [],
    startedAt: "2026-01-01T00:00:00Z",
    endedAt: null,
  } as unknown as AgentTurn;

  it("DDS-FR-TGWY: a dot and the status in lowercase, with no skeleton and no spinner", async () => {
    setDiscussionTurns("disc-1", [turn]);
    renderColumn([message("c1", ME, "@helga thoughts?")]);
    const row = await screen.findByTestId("dds-transient-status");
    // Exactly the word, not merely containing it: the requirement is that the
    // conversation's `Thinking…` is set in lowercase with its ellipsis dropped,
    // and a substring match would pass for the original either way.
    expect(row.textContent).toBe("thinking");
    expect(row.querySelector(".dds-transient__dot")).not.toBeNull();
    // No skeleton, no spinner: the dot and the word are the whole of the row.
    expect(row.children).toHaveLength(2);
  });

  it("DDS-FR-TGWY: the message that arrives replaces the row rather than landing below it", async () => {
    setDiscussionTurns("disc-1", [turn]);
    const { rerender } = renderColumn([message("c1", ME, "@helga thoughts?")]);
    await screen.findByTestId("dds-transient");

    // The turn ends and the comment it delivered arrives together.
    act(() => removeDiscussionTurn("disc-1", "turn-1"));
    rerender(
      columnFor([
        message("c1", ME, "@helga thoughts?"),
        message("c2", HELGA, "the prompt is enough"),
      ]),
    );
    await screen.findByText("the prompt is enough");
    expect(screen.queryByTestId("dds-transient")).toBeNull();
  });
});

describe("the reply field (DDS-FR-HQTX)", () => {
  it("DDS-FR-HQTX: carries the hint for the post accelerator", async () => {
    renderColumn([message("c1", HELGA, "anything")]);
    await screen.findByTestId("draft-discussion-column");
    // The hint is the platform's own words, so it is read from the same rule.
    expect(screen.getByText(postAcceleratorHint())).toBeInTheDocument();
  });
});

describe("the message actions (DDS-FR-FKZL)", () => {
  it("DDS-FR-FKZL: Quote stands in the author line and is reachable from the keyboard", async () => {
    renderColumn([message("c1", HELGA, "something worth quoting")]);
    const block = await screen.findByTestId("dds-message");
    const quote = within(block).getByRole("button", { name: /Quote comment/ });
    // In the author line rather than below the body, and in the document at all
    // times rather than mounted on hover — the stylesheet reveals it.
    expect(block.firstElementChild!.contains(quote)).toBe(true);
  });
});

describe("provider citation markers (DQA-FR-JADB, CMT-FR-AWIE)", () => {
  const span = "\uE200cite\uE202turn0search1\uE201";

  it("DQA-FR-JADB: a question card hides markers and still marks the chosen option", async () => {
    renderColumn([
      message("c1", HELGA, `Which store? ${span}\n\n1. Postgres ${span}\n2. Sqlite`),
      message("c2", ME, `**Selected option:** Postgres ${span}`),
    ]);
    const card = await screen.findByTestId("dds-question-card");

    expect(card.textContent ?? "").not.toMatch(/[\uE200-\uE2FF]/);
    expect(card).not.toHaveTextContent(/turn0search1/);
    const chosen = card.querySelectorAll("[data-chosen]");
    expect(chosen).toHaveLength(1);
    expect(chosen[0]).toHaveTextContent("Postgres");
  });

  it("CMT-FR-AWIE: an ordinary agent message hides markers", async () => {
    renderColumn([message("c1", HELGA, `The answer ${span}.`)]);
    const body = await screen.findByTestId("dds-message");
    expect(body).toHaveTextContent("The answer.");
    expect(body.textContent ?? "").not.toMatch(/[\uE200-\uE2FF]/);
  });
});
