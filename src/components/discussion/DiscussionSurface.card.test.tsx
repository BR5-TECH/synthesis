/**
 * What the retired comment rail held for the shared discussion surface, held
 * now against the surface itself (`CVP-conversation-presentation.md`
 * CVP-FR-SDMQ). The card variant is the one the floating panel, the detached
 * overlay, and the conversation tab render, so its message list, its composer,
 * its pending contributions, and its question block are tested here directly.
 * The last describe holds the opening composer of the discussion column
 * (`DDS-draft-discussion.md`), which took the place of the rail's draft card.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";

// Strict: the log sink is answered, and any other command is refused.
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn(async (cmd: string) => {
    if (cmd === "append_log_records") return undefined;
    throw new Error(`unexpected command ${cmd}`);
  }),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
  emit: vi.fn(async () => {}),
}));

import { DiscussionSurface, type DiscussionSurfaceProps } from "./DiscussionSurface";
import { resetDiscussionFocus } from "../../state/discussionFocus";
import { clearAllDiscussionSessions } from "../../state/discussionSession";
import { clearQuestionSets, publishSet } from "../../state/questionSets";
import { forgetEveryDraft } from "../../state/questionAnswerDrafts";
import { columnElement, discussionSaying } from "../../test/draftDiscussionFixtures";
import { artifactCommentOrigin } from "../../test/origins";
import { invoke } from "@tauri-apps/api/core";
import { resetLogBufferForTest } from "../../logging";
import type {
  AgentTurn,
  Comment,
  Discussion,
  Participant,
  PendingQuestionSet,
  ProjectAgent,
} from "../../types";

beforeEach(() => {
  clearAllDiscussionSessions();
  clearQuestionSets();
  forgetEveryDraft();
  resetDiscussionFocus();
  // The surface logs on mount, and that flush must not reach the next test.
  resetLogBufferForTest();
});
afterEach(cleanup);

const author: Participant = { kind: "human", login: "raver119" };

function comment(id: string, body: string, by: Participant = author): Comment {
  return {
    id,
    author: by,
    body,
    quotes: [],
    attachments: [],
    createdAt: "2026-02-01T00:00:00Z",
  };
}

function agent(handle: string, title?: string): Participant {
  return {
    kind: "agent",
    agentId: `id-${handle}`,
    handle,
    model: "m",
    ...(title === undefined ? {} : { title }),
  } as Participant;
}

function discussion(over: Partial<Discussion> = {}): Discussion {
  return {
    id: "t1",
    target: { kind: "artifact", artifactId: "a.md" },
    fragmentTarget: {
      owner: { kind: "artifact", artifactId: "a.md" },
      path: "a.md",
      start: 0,
      end: 3,
      quote: "abc",
    },
    comments: [comment("t1-c1", "a passage")],
    resolved: false,
    createdAt: "2026-02-01T00:00:00Z",
    updatedAt: "2026-02-01T00:00:00Z",
    ...over,
  };
}

function surface(
  d: Discussion,
  over: Partial<DiscussionSurfaceProps> = {},
): React.ReactElement {
  return (
    <DiscussionSurface
      discussion={d}
      owner="panel"
      agents={[]}
      liveTurns={false}
      onReply={vi.fn(async () => undefined)}
      onSetResolved={vi.fn(async () => undefined)}
      {...over}
    />
  );
}

describe("CMT-FR-08, CMT-FR-15: a card leads with a message, not with a header", () => {
  it("CMT-FR-15: carries the discussion's menu on its first message and renders no row above it", () => {
    render(surface(discussion({ fragmentTarget: null })));

    const card = screen.getByTestId("comment-thread-t1");
    const messages = card.querySelectorAll(".comment");
    expect(messages).toHaveLength(1);
    // The menu is inside the message, so no headed row is left to read as an
    // empty first message.
    const menu = screen.getByRole("button", { name: "Thread actions for t1" });
    expect(menu.closest(".comment")).toBe(messages[0]);
    expect(card.querySelector(".comment-card__head")).toBeNull();
  });

  it("CMT-FR-15: carries exactly one menu however many messages the discussion holds", () => {
    render(
      surface(
        discussion({
          comments: [comment("t1-c1", "a passage"), comment("t1-c2", "a reply", agent("arch"))],
        }),
      ),
    );
    expect(screen.getAllByRole("button", { name: "Thread actions for t1" })).toHaveLength(1);
  });
});

describe("CMS-FR-53: a discussion that carries no fragment field at all", () => {
  it("CMS-FR-53, CMT-FR-55: renders as a whole-target discussion, not as an orphan", () => {
    // A Rust field that is skipped arrives as `undefined`, not `null`. A guard
    // that reads `.quote` off it unmounts the whole tree in React 19.
    const bare = discussion();
    delete (bare as { fragmentTarget?: unknown }).fragmentTarget;

    render(surface(bare));

    const card = screen.getByTestId("comment-thread-t1");
    expect(card).toHaveAttribute("data-target", "whole");
    expect(within(card).queryByTestId("discussion-fragment-quote")).toBeNull();
    expect(screen.queryByText("Orphaned")).toBeNull();
    expect(card).toHaveTextContent("a passage");
  });
});

describe("CMT-FR-28: a card raises its focus on the click, not on the press", () => {
  // Focusing a discussion can scroll its fragment into view. Raising the focus
  // on the press would move the card between the press and the release, so the
  // control the pointer is on would move out from under it.
  it("CMT-FR-28: does not focus on the press alone, and focuses once the click completes", () => {
    const onFocus = vi.fn();
    render(surface(discussion(), { onFocus }));

    fireEvent.mouseDown(screen.getByTestId("comment-thread-t1"));
    expect(onFocus).not.toHaveBeenCalled();

    fireEvent.click(screen.getByTestId("comment-thread-t1"));
    expect(onFocus).toHaveBeenCalledTimes(1);
  });
});

describe("CMT-FR-45, CMT-FR-11: the composer's controls are inline", () => {
  it("CMT-FR-45, CVP-FR-55: puts Attach before the message and Post after it, on one line", () => {
    render(surface(discussion()));

    const row = document.querySelector(".comment-composer__row") as HTMLElement;
    expect(row).not.toBeNull();
    const order = [...row.children].map(
      (el) =>
        el.getAttribute("aria-label") ??
        el.querySelector("[aria-label]")?.getAttribute("aria-label") ??
        el.tagName.toLowerCase(),
    );
    expect(order[0]).toBe("Attach");
    expect(order[order.length - 1]).toBe("Post");
    expect(within(row).getByLabelText("Reply to thread t1")).toBeInTheDocument();
  });

  it("CMT-FR-45: the Attach surface opens across the composer, from its leading edge", () => {
    // Attach leads the field, so a menu anchored to the control's trailing edge
    // runs out of the surface that holds it. Vitest applies no CSS, so the
    // attribute the stylesheet keys on is what a test can hold.
    const { container } = render(surface(discussion()));
    const anchors = [...container.querySelectorAll(".comment-attach")];
    expect(anchors).toHaveLength(1);
    expect(anchors[0]).toHaveAttribute("data-align", "leading");
  });
});

describe("CTA-FR-YOGW: an agent comment's title line", () => {
  it("CTA-FR-YOGW, CTA-FR-CHUJ, CMS-FR-65: renders the snapshot each comment carries, not the agent's current title", () => {
    render(
      surface(
        discussion({
          comments: [
            comment("t1-c1", "a passage"),
            comment("c2", "an answer", agent("arch", "Developer")),
            comment("c3", "an answer", agent("arch", "Architect")),
          ],
        }),
      ),
    );
    expect(
      screen.getAllByTestId("comment-agent-title").map((e) => e.textContent),
    ).toEqual(["Developer", "Architect"]);
  });

  it("CTA-FR-YOGW, CTA-FR-KYPK: renders no line and no placeholder for an absent or an empty snapshot, and none for a human", () => {
    render(
      surface(
        discussion({
          comments: [
            comment("t1-c1", "a passage"),
            comment("c2", "an answer", agent("scribe")),
            comment("c3", "an answer", agent("sec", "")),
          ],
        }),
      ),
    );
    const card = screen.getByTestId("comment-thread-t1");
    expect(within(card).queryAllByTestId("comment-agent-title")).toHaveLength(0);
    expect(card.textContent ?? "").not.toContain("Not defined");
    expect(card).toHaveTextContent("scribe");
    expect(card).toHaveTextContent("sec");
  });

  it("CTA-FR-YOGW, CTA-FR-KYPK: a pending contribution takes no title from the roster", () => {
    render(
      surface(
        discussion({
          comments: [
            comment("t1-c1", "a passage"),
            comment("c2", "an answer", agent("arch", "Developer")),
          ],
        }),
        { pendingTurns: [running([])] },
      ),
    );
    expect(
      screen.getAllByTestId("comment-agent-title").map((e) => e.textContent),
    ).toEqual(["Developer"]);
    expect(screen.getByTestId("comment-thread-t1")).toHaveTextContent("Thinking…");
  });

  it("CTA-FR-YOGW, CTA-FR-KYPK: a failed contribution takes no title from the roster", () => {
    render(
      surface(
        discussion({
          comments: [
            comment("t1-c1", "a passage"),
            comment("c2", "an answer", agent("arch", "Developer")),
          ],
        }),
        { turnFailure: "unreachable" },
      ),
    );
    expect(screen.getByTestId("comment-turn-error")).toBeInTheDocument();
    expect(
      screen.getAllByTestId("comment-agent-title").map((e) => e.textContent),
    ).toEqual(["Developer"]);
  });
});

function running(activeToolCalls: AgentTurn["activeToolCalls"]): AgentTurn {
  return {
    id: "turn-1",
    agentId: "id-arch",
    nickname: "arch",
    origin: artifactCommentOrigin("t1"),
    triggerCommentId: "t1-c1",
    state: "running",
    failure: null,
    retryPermitted: false,
    imagesOmitted: false,
    startedAt: "2026-02-01T00:02:00Z",
    endedAt: null,
    activeToolCalls,
  };
}

describe("CTA-FR-FBJR, CTA-FR-IVNG: the pending contribution's activity status", () => {
  const line = () => screen.getByTestId("comment-pending-status").textContent ?? "";

  it.each([
    ["Thinking…", []],
    ["Searching related specifications…", [{ id: "call-1", tool: "search_specifications", activationSeq: 1 }]],
    ["Working…", [{ id: "call-1", tool: "a_tool_this_surface_was_never_taught", activationSeq: 1 }]],
    [
      "Searching available skills…",
      [
        { id: "call-2", tool: "search_skills", activationSeq: 2 },
        { id: "call-1", tool: "read_file", activationSeq: 1 },
      ],
    ],
  ] as [string, AgentTurn["activeToolCalls"]][])(
    "CTA-FR-FBJR, CTA-FR-IHOB, CTA-FR-IWOJ: reads %s",
    (expected, calls) => {
      render(surface(discussion(), { pendingTurns: [running(calls)] }));
      expect(line()).toBe(expected);
    },
  );

  it("CTA-FR-IVNG, CTA-FR-JQUU: hands the line to the latest call still active, blank at no moment, and cancels as before", () => {
    const onCancelTurn = vi.fn();
    const view = render(
      surface(discussion(), {
        onCancelTurn,
        pendingTurns: [
          running([
            { id: "call-1", tool: "read_file", activationSeq: 1 },
            { id: "call-2", tool: "openrouter:web_search", activationSeq: 2 },
          ]),
        ],
      }),
    );
    expect(line()).toBe("Searching the web…");
    // CTA-FR-PCNW: nothing of the call itself — no tool name.
    expect(screen.getByTestId("comment-pending").textContent ?? "").not.toContain("openrouter");

    view.rerender(
      surface(discussion(), {
        onCancelTurn,
        pendingTurns: [running([{ id: "call-1", tool: "read_file", activationSeq: 1 }])],
      }),
    );
    expect(line()).toBe("Reading a project file…");

    view.rerender(surface(discussion(), { onCancelTurn, pendingTurns: [running([])] }));
    expect(line()).toBe("Thinking…");

    fireEvent.click(screen.getByTestId("comment-pending-cancel"));
    expect(onCancelTurn).toHaveBeenCalledWith("turn-1");
  });
});

describe("a discussion holding a question set (DQA-discussion-question-answering.md)", () => {
  const arch: Participant = { kind: "agent", agentId: "a1", handle: "arch", title: "Architect" };

  function questionSet(): PendingQuestionSet {
    return {
      setId: "s1",
      discussionId: "t1",
      askedBy: arch,
      askedAt: "2026-02-01T00:00:00Z",
      questions: [
        {
          position: 1,
          text: "One spec or two?",
          options: [
            { position: 1, value: "one" },
            { position: 2, value: "two" },
          ],
        },
      ],
    };
  }

  const opening = () => comment("c0", "@arch what would you change?");

  function draw(comments: Comment[]) {
    render(surface(discussion({ fragmentTarget: null, comments })));
  }

  it("DQA-FR-NRZB, DQA-FR-TVMH: renders the block on the card, outside the discussion's history", () => {
    publishSet("t1", questionSet());
    draw([opening()]);

    expect(screen.getByTestId("discussion-questions")).toBeInTheDocument();
    expect(screen.getByText("One spec or two?")).toBeInTheDocument();
    const messages = screen.getByTestId("discussion-messages");
    expect(within(messages).queryByTestId("discussion-questions")).toBeNull();
    expect(within(messages).getAllByText(/what would you change/)).toHaveLength(1);
  });

  it("DQA-FR-PXNC, DQA-FR-VJHT: the composer is replaced by a note that says why, and names nothing of the agent or the turn", () => {
    publishSet("t1", questionSet());
    draw([opening()]);

    const note = screen.getByTestId("discussion-questions-composer-note");
    expect(note).toHaveTextContent("Answer the questions above to continue the discussion.");
    expect(note.textContent ?? "").not.toMatch(/arch|turn|waiting|ago/i);
    expect(document.querySelector(".comment-composer")).toBeNull();
    // Reading and the menu stand.
    expect(screen.getByText(/what would you change/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Thread actions for t1" })).toBeInTheDocument();
  });

  it("DQA-FR-NRZB: a discussion holding no set renders the ordinary composer and no block", () => {
    draw([opening()]);

    expect(screen.queryByTestId("discussion-questions")).toBeNull();
    expect(screen.queryByTestId("discussion-questions-composer-note")).toBeNull();
    expect(document.querySelector(".comment-composer")).not.toBeNull();
  });

  it.each([
    ["a selected option", "**Selected option:** two"],
    ["an own answer (ADQ-FR-RECR)", "**Own answer:** three, one per layer"],
  ])("DQA-FR-FBWO: a submitted pair with %s renders as one joined entry", (_label, answer) => {
    draw([
      opening(),
      comment("s1:01-1q", "One spec or two?\n\n1. one\n2. two", arch),
      comment("s1:01-2a", answer),
    ]);

    const question = document.querySelector('[data-pair="question"]');
    const reply = document.querySelector('[data-pair="answer"]');
    expect(question).not.toBeNull();
    expect(reply).not.toBeNull();
    expect(question?.nextElementSibling).toBe(reply);
    expect(document.querySelectorAll("[data-pair]")).toHaveLength(2);
  });

  it("DQA-FR-GRUV: a comment between the halves breaks the pairing", () => {
    draw([
      opening(),
      comment("s1:01-1q", "One spec or two?", arch),
      comment("interjection", "hold on"),
      comment("s1:01-2a", "**Selected option:** two"),
    ]);

    expect(document.querySelectorAll("[data-pair]")).toHaveLength(0);
  });

  it("DQA-FR-NKAX: the appended comments stay quotable ordinary comments", () => {
    draw([
      opening(),
      comment("s1:01-1q", "One spec or two?", arch),
      comment("s1:01-2a", "**Selected option:** two"),
    ]);

    // No set stands, so every comment offers Quote — the pair included.
    expect(screen.getAllByLabelText(/^Quote comment/)).toHaveLength(3);
  });

  /** A submitted question comment carrying its option descriptions. */
  function describedQuestion(id = "s1:01-1q"): Comment {
    return {
      ...comment(id, "One spec or two?\n\n1. One spec\n2. Paired specs", arch),
      optionDescriptions: ["Easy to find; it grows large.", "Each stays small."],
    };
  }

  it("DQA-FR-MDMU, DQA-FR-XRLK, CMT-FR-KUPP, CMT-FR-35, DQA-FR-EWLB: a described question carries a collapsed, named descriptions control", () => {
    draw([opening(), describedQuestion(), comment("s1:01-2a", "**Selected option:** Paired specs")]);

    const toggle = screen.getByTestId("option-descriptions-toggle");
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(toggle).toHaveAccessibleName("Show option descriptions");
    expect(toggle.closest('[data-pair="question"]')).not.toBeNull();
    expect(screen.queryByText("Easy to find; it grows large.")).toBeNull();
    expect(screen.queryByTestId("option-descriptions")).toBeNull();
  });

  it("DQA-FR-XRLK: expanded, each description stands beneath its label, and the control collapses them again", () => {
    draw([opening(), describedQuestion(), comment("s1:01-2a", "**Selected option:** Paired specs")]);
    const toggle = screen.getByTestId("option-descriptions-toggle");

    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(toggle).toHaveAccessibleName("Hide option descriptions");
    const items = within(screen.getByTestId("option-descriptions")).getAllByRole("listitem");
    expect(items.map((item) => item.textContent)).toEqual([
      "One specEasy to find; it grows large.",
      "Paired specsEach stays small.",
    ]);
    // The question text still reads once, and the labels are not listed twice.
    expect(screen.getAllByText("One spec or two?")).toHaveLength(1);
    expect(screen.getAllByText("One spec")).toHaveLength(1);

    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByTestId("option-descriptions")).toBeNull();
    expect(screen.getByText("One spec")).toBeInTheDocument();
  });

  it("DQA-FR-UHXW: each question comment keeps its own expanded state", () => {
    draw([
      opening(),
      describedQuestion("s1:01-1q"),
      comment("s1:01-2a", "**Selected option:** One spec"),
      describedQuestion("s1:02-1q"),
      comment("s1:02-2a", "**Selected option:** Paired specs"),
    ]);
    const [first, second] = screen.getAllByTestId("option-descriptions-toggle");
    fireEvent.click(first);
    expect(first).toHaveAttribute("aria-expanded", "true");
    expect(second).toHaveAttribute("aria-expanded", "false");
    expect(screen.getAllByTestId("option-descriptions")).toHaveLength(1);
  });

  it("DQA-FR-MDMU: an unpaired described question carries the control too", () => {
    draw([opening(), describedQuestion(), comment("c9", "something else entirely")]);
    const toggle = screen.getByTestId("option-descriptions-toggle");
    expect(toggle.closest("[data-pair]")).toBeNull();
    fireEvent.click(toggle);
    expect(screen.getByText("Each stays small.")).toBeInTheDocument();
  });

  it("DQA-FR-MDMU, DQA-FR-EWUG: a question comment with no descriptions, or an empty list, carries no control", () => {
    draw([
      opening(),
      comment("s1:01-1q", "One spec or two?\n\n1. one\n2. two", arch),
      comment("s1:01-2a", "**Selected option:** two"),
      { ...comment("s2:01-1q", "Where?\n\n1. ui\n2. core", arch), optionDescriptions: [] },
      comment("s2:01-2a", "**Selected option:** ui"),
    ]);
    expect(screen.queryByTestId("option-descriptions-toggle")).toBeNull();
  });

  it("CMT-FR-KUPP: a resolved discussion keeps the descriptions control", () => {
    render(
      surface(
        discussion({
          fragmentTarget: null,
          resolved: true,
          comments: [opening(), describedQuestion(), comment("s1:01-2a", "**Selected option:** One spec")],
        }),
      ),
    );
    fireEvent.click(screen.getByTestId("option-descriptions-toggle"));
    expect(screen.getByText("Easy to find; it grows large.")).toBeInTheDocument();
  });

  it("DQA-FR-XRLK: a body the reader cannot split keeps its own text and lists the descriptions below it", () => {
    draw([
      opening(),
      { ...comment("q1", "A question with no list", arch), optionDescriptions: ["Only context."] },
    ]);
    fireEvent.click(screen.getByTestId("option-descriptions-toggle"));
    expect(screen.getByText("A question with no list")).toBeInTheDocument();
    const items = within(screen.getByTestId("option-descriptions")).getAllByRole("listitem");
    expect(items.map((item) => item.textContent)).toEqual(["Only context."]);
  });

  it("DQA-FR-XRLK, ADQ-FR-NUEB: expanded labels read unescaped, as the author saw them", () => {
    draw([
      opening(),
      {
        ...comment("s1:01-1q", "Which?\n\n1. a\\*b\n2. \\`code\\`", arch),
        optionDescriptions: ["First.", "Second."],
      },
    ]);
    fireEvent.click(screen.getByTestId("option-descriptions-toggle"));
    const labels = [...document.querySelectorAll(".comment__option-label")].map(
      (label) => label.textContent,
    );
    expect(labels).toEqual(["a*b", "`code`"]);
  });

  it("DQA-FR-XRLK: descriptions that do not match the options are listed below the whole body", () => {
    draw([
      opening(),
      { ...describedQuestion(), optionDescriptions: ["One.", "Two.", "Three."] },
    ]);
    fireEvent.click(screen.getByTestId("option-descriptions-toggle"));
    expect(document.querySelectorAll(".comment__option-label")).toHaveLength(0);
    const items = within(screen.getByTestId("option-descriptions")).getAllByRole("listitem");
    expect(items.map((item) => item.textContent)).toEqual(["One.", "Two.", "Three."]);
    expect(screen.getByText("One spec")).toBeInTheDocument();
  });

  it("DQA-FR-UHXW: a fresh rendering starts collapsed, the state having been held in memory alone", () => {
    const setItem = vi.spyOn(Storage.prototype, "setItem");
    try {
      const comments = [opening(), describedQuestion(), comment("s1:01-2a", "**Selected option:** One spec")];
      const view = render(surface(discussion({ fragmentTarget: null, comments })));
      // What the surface reads on mount is its own; the toggle adds nothing.
      vi.mocked(invoke).mockClear();
      fireEvent.click(screen.getByTestId("option-descriptions-toggle"));
      const commands = vi.mocked(invoke).mock.calls.map(([command]) => command);
      expect(commands.filter((command) => command !== "append_log_records")).toEqual([]);
      expect(screen.getByTestId("option-descriptions")).toBeInTheDocument();
      view.unmount();

      draw(comments);
      expect(screen.getByTestId("option-descriptions-toggle")).toHaveAttribute("aria-expanded", "false");
      expect(setItem).not.toHaveBeenCalled();
    } finally {
      setItem.mockRestore();
    }
  });

  it("DQA-FR-EWLB: the expanded state is carried by the glyph as well as the colour", () => {
    draw([opening(), describedQuestion()]);
    const toggle = screen.getByTestId("option-descriptions-toggle");
    const collapsed = toggle.innerHTML;
    fireEvent.click(toggle);
    expect(toggle.innerHTML).not.toBe(collapsed);
  });

  it("CMT-FR-35, DQA-FR-MDMU: the control is a glyph on the header line", () => {
    draw([opening(), describedQuestion()]);
    const toggle = screen.getByTestId("option-descriptions-toggle");
    expect(toggle.textContent).toBe("");
    expect(toggle.querySelector("svg")).not.toBeNull();
    expect(toggle).toHaveAttribute("title", "Show option descriptions");
    expect(toggle.closest(".comment__head")).not.toBeNull();
  });

  it("DQA-FR-JADB: an expanded description and label show with provider citation markers hidden", () => {
    draw([
      opening(),
      {
        ...comment("s1:01-1q", "One spec or two?\n\n1. One spec \uE200cite\uE201\n2. Paired specs", arch),
        optionDescriptions: ["Easy to find \uE200cite\uE201 here.", "Each stays small."],
      },
    ]);
    fireEvent.click(screen.getByTestId("option-descriptions-toggle"));
    expect(screen.getByText("Easy to find here.")).toBeInTheDocument();
    expect(document.querySelector(".comment__option-label")?.textContent).toBe("One spec");
  });
});

describe("CTA-FR-NMNE: a composer that OPENS a conversation names no recipient", () => {
  function projectAgent(nickname: string, id: string): ProjectAgent {
    return {
      scope: "project",
      availability: "ready",
      agent: {
        id,
        nickname,
        title: "",
        modelId: "anthropic/claude-opus-5",
        instructions: "",
        createdAt: "2026-01-01T00:00:00Z",
        updatedAt: "2026-01-01T00:00:00Z",
        reasoning: null,
      },
    } as ProjectAgent;
  }

  it("CTA-FR-NMNE, DDS-FR-QMBC: the column's opening composer keeps its own placeholder where the tab already has active agents", () => {
    // The discussion the column holds addresses two agents. The opening
    // composer for a passage has no conversation yet for an agent to be active
    // in, so it names what is to be discussed and lists nobody.
    render(
      columnElement({
        discussions: [discussionSaying("disc-1", ["@arch @sec please review"])],
        agents: [projectAgent("arch", "a1"), projectAgent("sec", "a2")],
        openingFragment: {
          owner: { kind: "draft", draftId: "d1" },
          path: "Spec.md",
          start: 0,
          end: 29,
          quote: "a passage worth arguing about",
        },
      }),
    );

    const opening = within(screen.getByTestId("draft-open-discussion")).getByRole("textbox", {
      name: "Discuss this draft",
    });
    expect(opening).toHaveAttribute("placeholder", "Ask an agent about this draft…");
    expect(screen.getByTestId("draft-open-discussion").textContent ?? "").not.toContain("@arch");
    // CMT-FR-45: its Attach surface also opens from the leading edge.
    const attach = screen.getByTestId("draft-open-discussion").querySelector(".comment-attach");
    expect(attach).toHaveAttribute("data-align", "leading");
  });
});
