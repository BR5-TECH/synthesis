// The Comments vertical panel (`specifications/ui/CMP-comments-panel.md`),
// covering CMP-FR-01, CMP-FR-02, SNV-FR-44 … CMP-FR-18, CMP-FR-07. The backend is mocked at `invoke`, so every
// assertion here is about what the panel renders and which operation it invokes
// — the thread record, its `unresolved` marking and the project-wide fold belong
// to `CMS-comments-storage.md` and are covered by its own Rust tests.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import {
  publishProjectAgents,
  resetAgentRegistry,
} from "../state/agentRegistry";
import {
  clearAllDiscussionSessions,
  patchDiscussionSession,
  setDiscussionTurns,
} from "../state/discussionSession";
import type {
  AgentTurn,
  DiscussionListItem,
  ProjectAgent,
} from "../types";
import {
  Harness,
  agent,
  comment,
  groupHeaders,
  human,
  rows,
  thread,
} from "../test/commentsPanelFixtures";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

const listenMock = vi.fn((..._args: unknown[]) => Promise.resolve(() => {}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (...args: unknown[]) => listenMock(...args),
}));


function listReturns(items: DiscussionListItem[]) {
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "list_all_discussions") return Promise.resolve(items);
    throw new Error(`unexpected command ${cmd}`);
  });
}

const listCalls = () =>
  invokeMock.mock.calls.filter((c) => c[0] === "list_all_discussions");

beforeEach(() => {
  invokeMock.mockReset();
  listenMock.mockClear();
  // The published enrolment is module-level, so one test's roster must not be
  // served to the next.
  resetAgentRegistry();
  // CVP-FR-48: the session stores are memory, so one test's records must not
  // carry their indicators into the next.
  clearAllDiscussionSessions();
});
afterEach(cleanup);
describe("CMP-FR-14 / CMP-FR-26 / CMP-FR-25, CMP-FR-26: attachments on a row", () => {
  function withAttachment() {
    const t = thread({ id: "t1" });
    return {
      ...t,
      discussion: {
        ...t.discussion,
        comments: [
          {
            ...t.discussion.comments[0],
            attachments: [
              {
                kind: "blob" as const,
                digest: "a3f9",
                mediaType: "image/png",
                filename: "kickoff-flow.png",
                bytes: 12,
              },
            ],
          },
        ],
      },
    };
  }

  it("states the count on a row that has one, and no marker on a row that has none", async () => {
    listReturns([
      withAttachment(),
      thread({
        id: "t2",
        anchor: { start: 0, end: 4, quote: "beta" },
        comments: [comment("c2", "nothing attached here")],
      }),
    ]);
    render(<Harness />);
    await waitFor(() => expect(rows()).toHaveLength(2));

    const markers = screen.getAllByTestId("comment-row-attachments");
    expect(markers).toHaveLength(1);
    expect(markers[0]).toHaveTextContent("1");
  });

  it("filters on an attachment name, client-side", async () => {
    listReturns([
      withAttachment(),
      thread({
        id: "t2",
        anchor: { start: 0, end: 4, quote: "beta" },
        comments: [comment("c2", "nothing attached here")],
      }),
    ]);
    render(<Harness />);
    await waitFor(() => expect(rows()).toHaveLength(2));
    const before = listCalls().length;

    await userEvent.type(screen.getByLabelText("Filter comments"), "kickoff-flow");
    await waitFor(() => expect(rows()).toHaveLength(1));
    expect(screen.queryByText("nothing attached here")).not.toBeInTheDocument();
    // CMP-FR-14: matched over what is already loaded, with no backend call.
    expect(listCalls()).toHaveLength(before);
  });

  it("counts the opening comment's attachments and no others", async () => {
    // CMP-FR-25 / CMP-FR-06: the row is a recognisable excerpt of a
    // conversation, not a summary of everything in it. A thread whose *reply*
    // attaches something carries no marker, so summing over every comment would
    // fail here.
    const t = thread({ id: "t1" });
    listReturns([
      {
        ...t,
        discussion: {
          ...t.discussion,
          comments: [
            t.discussion.comments[0],
            {
              ...comment("c2", "the reply has the picture"),
              attachments: [
                {
                  kind: "blob" as const,
                  digest: "b7c1",
                  mediaType: "image/png",
                  filename: "late.png",
                  bytes: 4,
                },
              ],
            },
          ],
        },
      },
    ]);
    render(<Harness />);
    await screen.findByText("Which session?");
    expect(screen.getByText(/1 reply/)).toBeInTheDocument();
    expect(
      screen.queryByTestId("comment-row-attachments"),
    ).not.toBeInTheDocument();
  });

  it("loads no attachment content anywhere in the panel", async () => {
    // CMP-FR-26: the row reports that an attachment exists; the rail is where it
    // is looked at.
    listReturns([withAttachment()]);
    render(<Harness />);
    await screen.findByTestId("comment-row-attachments");

    expect(
      invokeMock.mock.calls.some((c) => c[0] === "read_comment_attachment"),
    ).toBe(false);
    expect(document.querySelector("img")).toBeNull();
  });
});

describe("SNV-FR-60, SNV-FR-61 / CMP-FR-21 / CMP-FR-24: the two empty states", () => {
  it("renders the centred first-class block, with no action and no filter, when the project holds no thread", async () => {
    listReturns([]);
    render(<Harness />);

    // CMP-FR-21 / SNV-FR-60: the shared block, centred by the body it sits in.
    const line = await screen.findByText("No comments yet.");
    const block = line.closest(".panel-empty");
    expect(block).not.toBeNull();
    expect(block?.parentElement).toHaveClass("vpanel__body--empty");
    // It names where a thread is opened. (The measure that block wraps on is
    // CSS, guarded in `../test/style-invariants.test.ts` — with `css: false`
    // nothing here can see it.)
    expect(
      screen.getByText(/opened from the comment rail beside an artifact/i),
    ).toBeInTheDocument();
    // SNV-FR-60: no action at all rather than a disabled one — the panel is
    // read-only (CMP-FR-16), so there is nothing here to offer.
    expect(
      block?.querySelectorAll("button, :disabled, [aria-disabled]"),
    ).toHaveLength(0);
    // SNV-FR-60: the filter is not rendered beside it — nothing to narrow.
    expect(screen.queryByLabelText("Filter comments")).not.toBeInTheDocument();
    expect(groupHeaders()).toEqual([]);
  });

  it("renders a filter that matched nothing in the list region, filter still present (CMP-FR-24)", async () => {
    listReturns([thread({ id: "t1" })]);
    render(<Harness />);
    await screen.findByText("Which session?");

    await userEvent.type(
      screen.getByLabelText("Filter comments"),
      "nothing-matches-this",
    );
    const message = await screen.findByText("No comments match this filter.");
    expect(message).toBeInTheDocument();
    // SNV-FR-61: not the first-class block, and the filter stays put holding
    // what was typed, because that is what the author changes next.
    expect(document.querySelector(".panel-empty")).toBeNull();
    expect(screen.getByLabelText("Filter comments")).toHaveValue(
      "nothing-matches-this",
    );
    expect(groupHeaders()).toEqual([]);
  });

  it("renders a failed read as a message rather than an empty project", async () => {
    invokeMock.mockRejectedValue("no_project_open");
    render(<Harness />);
    expect(await screen.findByText("no_project_open")).toBeInTheDocument();
    expect(screen.queryByText("No comments yet.")).not.toBeInTheDocument();
  });

  it("clears a read error once a later read succeeds", async () => {
    invokeMock.mockRejectedValueOnce("no_project_open");
    render(<Harness />);
    await screen.findByText("no_project_open");

    listReturns([thread({ id: "t1" })]);
    // A failed read left no list to update in place, so the next event is what
    // makes the panel try again rather than being folded into a list it has not
    // got (CMP-FR-18).
    const emit = listenMock.mock.calls.find(
      (c) => c[0] === "discussion-changed",
    )?.[1] as (event: { payload: unknown }) => void;
    act(() => emit({ payload: thread({ id: "t1" }).discussion }));
    await screen.findByText("Which session?");
    expect(screen.queryByText("no_project_open")).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// SNV-FR-58: a surface offering only one of the two controls renders
// it in the position it would occupy in the pair — lowest, above the list.
// ---------------------------------------------------------------------------

describe("filter-row arrangement (SNV-FR-58)", () => {
  it("pins the lone filter directly above the list it narrows", async () => {
    listReturns([thread({ id: "t1" })]);
    render(<Harness />);
    await screen.findByText("Which session?");

    const controls = document.querySelector(".panel-controls")!;
    const filter = screen.getByLabelText("Filter comments");
    const body = document.querySelector(".vpanel__body")!;

    expect(controls).toContainElement(filter);
    // The panel carries no scope selector (CMP-FR-02), so the filter is the
    // stack's only child and sits last by construction.
    expect(controls.children).toHaveLength(1);
    expect(
      controls.compareDocumentPosition(body) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });
});

describe("a discussion row (CMP-FR-27)", () => {
  it("states that it is a discussion in the anchor quote's place", async () => {
    listReturns([
      thread({ id: "t1" }),
      thread({
        id: "d1",
        // A whole-target discussion carries no fragment target, there being no
        // passage it is pinned to. Everything else a row carries, it carries.
        anchor: null,
        comments: [comment("c9", "Is this ready to implement?")],
      }),
    ]);
    render(<Harness />);

    // Both kinds reach the panel: the SCOPE is what decides, not the kind
    // (CMS-FR-40).
    await screen.findByText("Is this ready to implement?");
    expect(screen.getByText("Which session?")).toBeInTheDocument();

    // The discussion row says what it is rather than rendering a blank quote,
    // so it is never read as a thread whose anchor failed to load.
    const marker = screen.getByText("Discussion about the whole file");
    expect(marker).toBeInTheDocument();
    const row = marker.closest(".comment-row") as HTMLElement;
    expect(within(row).getByText("specs/onboarding.md")).toBeInTheDocument();
    expect(within(row).queryByText("the first session")).not.toBeInTheDocument();

    // And the anchored row beside it is untouched.
    const anchored = screen
      .getByText("Which session?")
      .closest(".comment-row") as HTMLElement;
    expect(within(anchored).getByText("the first session")).toBeInTheDocument();
    expect(
      within(anchored).queryByText("Discussion about the whole file"),
    ).not.toBeInTheDocument();
  });
});

// ---------------------------------------------------------------------------
// AGT-FR-29: a live tag is bold here too
// ---------------------------------------------------------------------------

describe("AGT-FR-28, AGT-FR-38 / AGT-FR-29: tags in a panel row", () => {
  /** The enrolment the chrome's agents control publishes (AGT-FR-02). */
  const enrol = (
    nicknames: string[],
    availability: ProjectAgent["availability"] = "ready",
  ) =>
    publishProjectAgents(
      nicknames.map((nickname, i) => ({
        agent: {
          id: `a${i}`,
          nickname,
          title: "",
          provider: "openrouter" as const,
          modelId: "anthropic/claude-opus-5",
          instructions: "",
          reasoning: null,
          createdAt: "2026-01-01T00:00:00Z",
          updatedAt: "2026-01-01T00:00:00Z",
        },
        availability,
      })),
    );

  const tags = () =>
    screen.queryAllByTestId("comment-tag").map((e) => e.textContent);

  it("bolds a live tag and leaves an address alone, issuing no call for the roster", async () => {
    // AGT-FR-29: a reader scanning the project's conversations can tell who each
    // was addressed to without opening it.
    enrol(["arch", "sec"]);
    listReturns([
      thread({
        id: "t1",
        comments: [
          comment("c1", "@all and @arch, plus me@example.com and the @media rule"),
        ],
      }),
    ]);
    render(<Harness />);
    await waitFor(() => expect(rows()).toHaveLength(1));

    expect(tags()).toEqual(["@all", "@arch"]);
    expect(document.body.textContent).toContain("me@example.com");
    expect(document.body.textContent).toContain("@media");

    // CMP-FR-18 and the panel's contract boundary: exactly one list call, and it
    // invokes nothing else. The roster came from what the chrome had already read.
    expect(invokeMock.mock.calls.map((c) => c[0])).toEqual([
      "list_all_discussions",
    ]);
  });

  it("leaves every tag as prose while nothing is enrolled", async () => {
    // AGT-FR-28 / AGT-FR-38: the same body in a project enrolling nobody marks
    // nothing at all.
    listReturns([
      thread({ id: "t1", comments: [comment("c1", "@all and @arch, have a look")] }),
    ]);
    render(<Harness />);
    await waitFor(() => expect(rows()).toHaveLength(1));
    expect(tags()).toEqual([]);
    expect(document.body.textContent).toContain("@all and @arch, have a look");
  });

  it("bolds the tags naming an agent enrolled while the panel was open", async () => {
    // The published roster is what keeps the panel current without it re-reading:
    // enrolling an agent bolds the tags naming it in place.
    listReturns([
      thread({ id: "t1", comments: [comment("c1", "@arch have a look")] }),
    ]);
    render(<Harness />);
    await waitFor(() => expect(rows()).toHaveLength(1));
    expect(tags()).toEqual([]);

    act(() => enrol(["arch"]));
    await waitFor(() => expect(tags()).toEqual(["@arch"]));
    // Still one call: the roster arrived without the panel asking.
    expect(invokeMock.mock.calls.map((c) => c[0])).toEqual([
      "list_all_discussions",
    ]);
  });

  it("leaves @all as prose when every enrolled agent is degraded", async () => {
    // AGT-FR-38, read through the published availability rather than assumed.
    enrol(["arch"], "provider_unverified");
    listReturns([
      thread({ id: "t1", comments: [comment("c1", "@all take a look")] }),
    ]);
    render(<Harness />);
    await waitFor(() => expect(rows()).toHaveLength(1));
    expect(tags()).toEqual([]);
  });
});

// ---------------------------------------------------------------------------
// CMP-FR-28, CMP-FR-18, CMP-FR-09, CVP-FR-37, CTA-FR-RHPP: the failed-response indicator
// ---------------------------------------------------------------------------

describe("CMP-FR-18, CMP-FR-09, CVP-FR-37, CTA-FR-RHPP / CMP-FR-28: a conversation an agent could not answer in", () => {
  /** The badges a row carries, in the order they render. */
  const badges = () =>
    Array.from(
      document.querySelectorAll(".comment-row__badge"),
    ).map((b) => b.getAttribute("data-kind"));

  const running = { id: "turn-1", state: "running" } as AgentTurn;
  const failed = (on: boolean) =>
    patchDiscussionSession("t1", { failedResponse: on });

  it("reads pending while a turn is outstanding and failed once it is not", async () => {
    listReturns([thread({ id: "t1" }, true)]);
    render(<Harness />);
    await screen.findByText("specs/onboarding.md");

    // A conversation retrying a model call automatically is pending: the turn is
    // still outstanding, so the row says the author is being answered.
    act(() => setDiscussionTurns("t1", [running]));
    await waitFor(() => expect(badges()).toEqual(["pending"]));

    // Once the attempts run out the row says an agent could not answer here and
    // the offer to ask it again is standing — which is otherwise
    // indistinguishable, from this panel, from a conversation nobody has replied
    // to yet.
    act(() => {
      setDiscussionTurns("t1", []);
      failed(true);
    });
    await waitFor(() => expect(badges()).toEqual(["failed"]));
    expect(
      document.querySelector('.comment-row__badge[data-kind="failed"]')?.textContent,
    ).toBe("No answer");

    // CMP-FR-28: it changes nothing else about the row.
    expect(rows()).toHaveLength(1);
    expect(groupHeaders()).toEqual(["Active 1"]);

    // Taking the Retry, or a later human comment retiring the offer, clears it.
    act(() => failed(false));
    await waitFor(() => expect(badges()).toEqual([]));
  });

  it("reads unread from the discussion's session record, as text", async () => {
    listReturns([thread({ id: "t1" })]);
    render(<Harness />);
    await screen.findByText("Which session?");

    act(() =>
      patchDiscussionSession("t1", { unreadCount: 2, firstUnreadId: "c1" }),
    );
    await waitFor(() => expect(badges()).toEqual(["unread"]));
    expect(
      document.querySelector('.comment-row__badge[data-kind="unread"]')?.textContent,
    ).toBe("Unread");
  });

  it("carries no indicator for a discussion with no session record", async () => {
    listReturns([thread({ id: "t1" }, true)]);
    render(<Harness />);
    await screen.findByText("specs/onboarding.md");
    // There is no reading of it for anything to be unread — or unanswered —
    // against.
    expect(badges()).toEqual([]);
  });

  it("offers no way to retry from here", async () => {
    listReturns([thread({ id: "t1" }, true)]);
    render(<Harness />);
    await screen.findByText("specs/onboarding.md");
    act(() => failed(true));
    await waitFor(() => expect(badges()).toEqual(["failed"]));

    // CMP-FR-16: the panel is read-only. Retrying is done where the conversation
    // is read, which activating the row is the route to.
    expect(screen.queryByRole("button", { name: /retry/i })).not.toBeInTheDocument();
    expect(
      invokeMock.mock.calls.filter((c) => c[0] === "retry_agent_turn"),
    ).toHaveLength(0);
  });
});
