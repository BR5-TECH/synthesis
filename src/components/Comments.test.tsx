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
import { clearAllDiscussionSessions } from "../state/discussionSession";
import {
  publishKnownDrafts,
  rememberNoteLabel,
  resetOwnerAvailability,
} from "../state/ownerAvailability";
import type {
  Comment,
  DiscussionListItem,
  Participant,
} from "../types";
import {
  Harness,
  SwitchableHarness,
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
  resetOwnerAvailability();
});
afterEach(cleanup);

// ---------------------------------------------------------------------------

describe("CMP-FR-01, SNV-FR-44 / CMP-FR-02: the panel lists the whole project", () => {
  it("invokes the project-wide read on mount and renders what it returns", async () => {
    listReturns([thread({ id: "t1" })]);
    render(<Harness />);
    await screen.findByText("Which session?");
    expect(listCalls()).toHaveLength(1);
    // CMP-FR-02: no artifact argument — the panel's scope is the project.
    expect(listCalls()[0][1]).toBeUndefined();
  });
});

describe("CMP-FR-02: the active tab does not steer the panel", () => {
  it("leaves the list untouched and issues nothing when the active tab changes", async () => {
    listReturns([
      thread({ id: "t1", artifactId: "specs/a.md", updatedAt: "2024-02-01T00:00:00Z" }),
      thread({
        id: "t2",
        artifactId: "specs/b.md",
        updatedAt: "2024-01-01T00:00:00Z",
        comments: [comment("c2", "the other one")],
      }),
    ]);
    // The panel has no tab input at all — which is the point. Re-rendering it
    // under a changing sibling stands in for the shell switching tabs: nothing
    // reaches the panel, so nothing about it may change.
    const { rerender } = render(
      <>
        <div data-testid="active-tab">specs/a.md</div>
        <Harness />
      </>,
    );
    await waitFor(() => expect(rows()).toHaveLength(2));
    const before = rows().map((r) => r.textContent);

    rerender(
      <>
        <div data-testid="active-tab">specs/b.md</div>
        <Harness />
      </>,
    );

    expect(rows().map((r) => r.textContent)).toEqual(before);
    expect(listCalls()).toHaveLength(1);
    // CMP-FR-02: no per-artifact read is ever issued by this panel.
    expect(
      invokeMock.mock.calls.some((c) => c[0] === "list_discussions"),
    ).toBe(false);
  });
});

describe("CMP-FR-03, CMP-FR-04, CMP-FR-05: the three groups and their counts", () => {
  it("renders Active, Locked and Resolved in that order, each thread once", async () => {
    listReturns([
      thread({ id: "t1" }),
      thread({ id: "t2", locked: true }),
      thread({ id: "t3", resolved: true }),
      thread({ id: "t4", locked: true, resolved: true }),
    ]);
    render(<Harness />);
    await waitFor(() => expect(rows()).toHaveLength(4));

    // CMP-FR-04: the both-locked-and-resolved thread is counted once, under
    // Resolved — so the counts sum to four rather than five.
    expect(groupHeaders()).toEqual(["Active 1", "Locked 1", "Resolved 2"]);
  });

  it("renders no header for a group the project has nothing for", async () => {
    listReturns([thread({ id: "t1", resolved: true })]);
    render(<Harness />);
    await waitFor(() => expect(rows()).toHaveLength(1));
    expect(groupHeaders()).toEqual(["Resolved 1"]);
  });
});

describe("CMP-FR-06 … CMP-FR-09: what a row shows", () => {
  it("shows the artifact, the anchor quote, the opening comment and the reply count", async () => {
    listReturns([
      thread({
        id: "t1",
        artifactId: "specs/onboarding.md",
        anchor: { start: 0, end: 17, quote: "the first session" },
        comments: [
          comment("c1", "Say **which** one"),
          comment("c2", "reply one"),
          comment("c3", "reply two"),
        ],
      }),
    ]);
    render(<Harness />);
    const row = (await screen.findAllByRole("button"))
      .map((b) => b)
      .find((b) => b.className.includes("comment-row"))!;

    // CMP-FR-06: the project-relative path, so two artifacts sharing a
    // basename are told apart without opening either.
    expect(within(row).getByText("specs/onboarding.md")).toBeInTheDocument();
    expect(within(row).getByText("the first session")).toBeInTheDocument();
    expect(within(row).getByText("raver119")).toBeInTheDocument();
    // CMP-FR-06: the body renders as rich text, not as Markdown source.
    expect(within(row).getByText("which").tagName).toBe("STRONG");
    expect(within(row).getByText("2 replies")).toBeInTheDocument();
    expect(
      within(row).getByText("specs/onboarding.md").getAttribute("title"),
    ).toBe("specs/onboarding.md");
  });

  it("shows no reply count on a thread of one comment", async () => {
    listReturns([thread({ id: "t1" })]);
    render(<Harness />);
    await screen.findByText("Which session?");
    expect(screen.queryByText(/repl(y|ies)/)).not.toBeInTheDocument();
  });

  it("renders one reply in the singular", async () => {
    listReturns([
      thread({ id: "t1", comments: [comment("c1", "a"), comment("c2", "b")] }),
    ]);
    render(<Harness />);
    expect(await screen.findByText("1 reply")).toBeInTheDocument();
  });

  it("renders the opening comment's title snapshot, and nothing without one (CMP-FR-30)", async () => {
    // CMP-FR-30, CMP-FR-07, CMP-FR-04, CMP-FR-09, CTA-FR-KFUF. The same three participants a card renders (CTA-FR-YOGW), read
    // the same way here: the snapshot, never the agent's current title, and no
    // placeholder where there is none.
    const titled: Participant = { ...agent, title: "Developer" };
    const empty: Participant = { ...agent, title: "" };
    // CMP-FR-30: the panel "resolves no agent record and holds no roster to
    // resolve one from" — but `useProjectAgents()` IS in scope in this
    // component, so the wrong implementation is one line away. Publishing a
    // roster whose current title *disagrees* with the snapshot is the only
    // thing that tells the two implementations apart.
    publishProjectAgents([
      {
        agent: {
          id: "claude_code",
          nickname: "claude",
          title: "Architect",
          modelId: "anthropic/claude-opus-5",
          instructions: "",
          reasoning: null,
          createdAt: "2026-01-01T00:00:00Z",
          updatedAt: "2026-01-01T00:00:00Z",
        },
        availability: "ready" as const,
      },
    ]);
    listReturns([
      thread({
        id: "t1",
        updatedAt: "2024-03-01T00:00:00Z",
        comments: [comment("c1", "from a titled agent", titled)],
      }),
      thread({
        id: "t2",
        updatedAt: "2024-02-01T00:00:00Z",
        comments: [comment("c2", "from an untitled agent", empty)],
      }),
      thread({
        id: "t3",
        updatedAt: "2024-01-01T00:00:00Z",
        // The older shape: an agent participant with no title key at all.
        comments: [comment("c3", "from an older log", agent)],
      }),
    ]);
    render(<Harness />);
    await waitFor(() => expect(rows()).toHaveLength(3));

    const titles = screen.getAllByTestId("comment-row-agent-title");
    expect(titles.map((e) => e.textContent)).toEqual(["Developer"]);
    // The roster's `Architect` is what a resolving implementation would show,
    // for all three rows. It must appear nowhere.
    expect(document.body.textContent ?? "").not.toContain("Architect");
    // The two without one render no line and no placeholder — and the rows are
    // otherwise intact, which is what "remains readable" means.
    expect(document.body.textContent ?? "").not.toContain("Not defined");
    expect(screen.getByText("from an untitled agent")).toBeInTheDocument();
    expect(screen.getByText("from an older log")).toBeInTheDocument();
  });

  it("changes no row's grouping, ordering, or counts by adding the line (CMP-FR-30, CMP-FR-07, CMP-FR-04, CMP-FR-09, CTA-FR-KFUF)", async () => {
    // CMP-FR-30 adds a line and nothing else. Asserted by rendering the same
    // three threads twice — once with title snapshots, once without — and
    // comparing everything the panel decides about a row.
    const shape = async (titled: boolean) => {
      const author = (title: string): Participant =>
        titled ? { ...agent, title } : agent;
      listReturns([
        thread({
          id: "t1",
          updatedAt: "2024-03-01T00:00:00Z",
          resolved: true,
          comments: [comment("c1", "settled", author("Developer"))],
        }),
        thread({
          id: "t2",
          updatedAt: "2024-02-01T00:00:00Z",
          locked: true,
          comments: [comment("c2", "locked", author("Architect"))],
        }),
        thread({
          id: "t3",
          updatedAt: "2024-01-01T00:00:00Z",
          comments: [
            comment("c3", "active", author("Reviewer")),
            comment("c4", "a reply"),
          ],
        }),
      ]);
      render(<Harness />);
      await waitFor(() => expect(rows()).toHaveLength(3));
      const out = {
        // The group headers, in order, with their counts — CMP-FR-03/FR-04/FR-05.
        groups: Array.from(
          document.querySelectorAll(".note-group__header"),
        ).map((h) => h.textContent),
        // Row order across groups — CMP-FR-09.
        order: rows().map((r) => r.querySelector(".comment-row__body")?.textContent),
        // Reply counts and attachment counts — CMP-FR-06.
        replies: Array.from(
          document.querySelectorAll(".comment-row__replies"),
        ).map((e) => e.textContent),
      };
      // A comparison of two empty structures proves nothing, so pin that the
      // selectors actually matched before trusting the equality below.
      expect(out.groups).toHaveLength(3);
      expect(out.order).toHaveLength(3);
      expect(out.replies).toHaveLength(1);
      cleanup();
      return out;
    };

    expect(await shape(true)).toEqual(await shape(false));
  });

  it("distinguishes an agent author from a human one (CMP-FR-07)", async () => {
    // Both participants carry the SAME display name, so the only thing that can
    // tell them apart is the marking itself — a test with two different names
    // would pass against a panel that distinguishes nothing.
    const sameName: Participant = { kind: "human", login: "claude" };
    listReturns([
      thread({
        id: "t1",
        updatedAt: "2024-02-01T00:00:00Z",
        comments: [comment("c1", "from a bot", agent)],
      }),
      thread({
        id: "t2",
        updatedAt: "2024-01-01T00:00:00Z",
        comments: [comment("c2", "from a person", sameName)],
      }),
    ]);
    render(<Harness />);
    await waitFor(() => expect(rows()).toHaveLength(2));

    const authors = Array.from(
      document.querySelectorAll(".comment-row__author"),
    );
    expect(authors).toHaveLength(2);
    // The handle, not the login — the panel renders the participant the backend
    // stamped rather than deriving an author of its own.
    expect(authors.map((a) => a.textContent)).toEqual(["✦ claude", "claude"]);
    expect(authors.map((a) => a.getAttribute("data-agent"))).toEqual([
      "true",
      "false",
    ]);
  });

  it("renders the compact relative form of the timestamp (CMP-FR-08)", async () => {
    const twoHoursAgo = new Date(Date.now() - 2 * 60 * 60 * 1000).toISOString();
    listReturns([thread({ id: "t1", updatedAt: twoHoursAgo })]);
    render(<Harness />);
    await screen.findByText("Which session?");
    // The rendered text is the relative form, not the ISO instant it carries in
    // `dateTime` and `title`.
    expect(document.querySelector("time")!.textContent).toBe("2h ago");
  });

  it("marks a locked row on its artifact line", async () => {
    listReturns([
      thread({ id: "t1", locked: true }),
      thread({ id: "t2", locked: true, resolved: true }),
      thread({ id: "t3" }),
    ]);
    render(<Harness />);
    await waitFor(() => expect(rows()).toHaveLength(3));

    // Both locked threads carry the marker — including the one that sits under
    // Resolved rather than under Locked — and the plain thread does not.
    const marked = rows().map(
      (r) => r.querySelector(".comment-row__artifact svg") !== null,
    );
    expect(marked.filter(Boolean)).toHaveLength(2);
    const plain = rows().find(
      (r) => r.querySelector(".comment-row__artifact svg") === null,
    );
    expect(plain).toBeDefined();
  });

  it("discloses the absolute instant behind the relative timestamp (CMP-FR-08)", async () => {
    listReturns([thread({ id: "t1", updatedAt: "2024-01-01T00:00:00Z" })]);
    render(<Harness />);
    await screen.findByText("Which session?");
    const time = document.querySelector("time")!;
    expect(time.getAttribute("dateTime")).toBe("2024-01-01T00:00:00Z");
    expect(time.getAttribute("title")).toBe(
      new Date("2024-01-01T00:00:00Z").toLocaleString(),
    );
  });
});

describe("CMP-FR-09: ordering within a group", () => {
  it("orders by last activity, most recent first, across artifacts", async () => {
    listReturns([
      thread({
        id: "t1",
        artifactId: "specs/a.md",
        updatedAt: "2024-01-01T00:00:00Z",
        comments: [comment("c1", "oldest")],
      }),
      thread({
        id: "t2",
        artifactId: "specs/b.md",
        updatedAt: "2024-03-01T00:00:00Z",
        comments: [comment("c2", "newest")],
      }),
      thread({
        id: "t3",
        artifactId: "specs/a.md",
        updatedAt: "2024-02-01T00:00:00Z",
        comments: [comment("c3", "middle")],
      }),
    ]);
    render(<Harness />);
    await waitFor(() => expect(rows()).toHaveLength(3));
    expect(rows().map((r) => r.querySelector(".comment-row__body")?.textContent))
      .toEqual(["newest", "middle", "oldest"]);
  });
});

describe("CMP-FR-10, CMP-FR-11, CMT-FR-36 / CMT-FR-17: the click-through", () => {
  it("calls the one route with the discussion's identity, owner, and subject", async () => {
    const onReveal = vi.fn();
    listReturns([thread({ id: "t1", artifactId: "specs/onboarding.md" })]);
    render(<Harness onReveal={onReveal} />);
    await screen.findByText("Which session?");

    await userEvent.click(rows()[0] as HTMLElement);
    expect(onReveal).toHaveBeenCalledTimes(1);
    expect(onReveal).toHaveBeenCalledWith(
      expect.objectContaining({
        discussionId: "t1",
        target: { kind: "artifact", artifactId: "specs/onboarding.md" },
        resolved: false,
        ownerLabel: "specs/onboarding.md",
        // CVP-FR-57: the subject travels with the route, so the surface an
        // activation lands on names the conversation as well as its owner.
        subject: "the first session",
        ownerUnavailable: false,
      }),
    );
  });

  it("flags a resolved thread so the rail's disclosure is expanded for it", async () => {
    const onReveal = vi.fn();
    listReturns([thread({ id: "t9", resolved: true })]);
    render(<Harness onReveal={onReveal} />);
    await screen.findByText("Which session?");

    await userEvent.click(rows()[0] as HTMLElement);
    expect(onReveal).toHaveBeenCalledWith(
      expect.objectContaining({ discussionId: "t9", resolved: true }),
    );
  });

  it("names the discussion and its kind rather than reading out its whole body", async () => {
    listReturns([thread({ id: "t1" })]);
    render(<Harness />);
    expect(
      await screen.findByRole("button", {
        name: "Fragment discussion on specs/onboarding.md: the first session",
      }),
    ).toBeInTheDocument();
  });
});

describe("CMP-FR-04, NTS-FR-23 / CMP-FR-12: a thread whose artifact has gone", () => {
  it("shows the last-known path, is never hidden, and still activates the route (CMP-FR-29)", async () => {
    const onReveal = vi.fn();
    listReturns([
      thread({ id: "t1", artifactId: "specs/deleted.md" }, true),
    ]);
    render(<Harness onReveal={onReveal} />);
    await screen.findByText("Which session?");

    // Present, and showing where the artifact was.
    const row = rows()[0];
    expect(
      within(row as HTMLElement).getByText("specs/deleted.md"),
    ).toBeInTheDocument();
    // An activation target like any other: the route opens the fallback tab.
    expect(row.tagName).toBe("BUTTON");
    await userEvent.click(row as HTMLElement);
    expect(onReveal).toHaveBeenCalledWith(
      expect.objectContaining({ discussionId: "t1", ownerUnavailable: true }),
    );
  });

  // CMP-FR-12, CMP-FR-04, NTS-FR-23: the marker names the ARTIFACT, not the conversation, so it is
  // independent of which group the thread's own state puts it in.
  it("marks the row Unresolved in whichever group its own state puts it", async () => {
    listReturns([
      thread({ id: "t1", artifactId: "specs/gone.md" }, true),
      thread({ id: "t2", artifactId: "specs/gone.md", resolved: true }, true),
    ]);
    render(<Harness />);
    await screen.findAllByText("Which session?");

    // One under Active, one under Resolved — the marker did not move either.
    expect(groupHeaders()).toEqual(["Active 1", "Resolved 1"]);
    const markers = document.querySelectorAll(".unresolved-marker");
    expect(markers).toHaveLength(2);
    // Spelled exactly as the Notes panel spells it (NTS-FR-23).
    markers.forEach((m) => expect(m.textContent).toBe("Unresolved"));
  });

  // The panel's rows run at the UI type scale; the Editor's rail keeps the
  // document scale it sits beside.
  it("renders the opening comment at the panel's UI type scale", async () => {
    listReturns([thread({ id: "t1" })]);
    render(<Harness />);
    await screen.findByText("Which session?");

    const body = document.querySelector(".comment-row__body .comment__body")!;
    expect(body).toHaveClass("doc");
    expect(body).toHaveClass("doc--ui");
  });
});

describe("CMP-FR-13: orphaning is not this panel's concern", () => {
  it("has no Orphaned group, and groups every thread by its stored status", async () => {
    listReturns([
      thread({ id: "t1" }),
      thread({ id: "t2", locked: true }),
      thread({ id: "t3", resolved: true }),
    ]);
    render(<Harness />);
    await waitFor(() => expect(rows()).toHaveLength(3));
    expect(groupHeaders()).toEqual(["Active 1", "Locked 1", "Resolved 1"]);
    expect(screen.queryByText(/orphan/i)).not.toBeInTheDocument();
  });
});

describe("CMP-FR-14 / CMP-FR-15: the filter", () => {
  it("narrows the list client-side, hides an emptied group, and issues no call", async () => {
    listReturns([
      thread({
        id: "t1",
        anchor: { start: 0, end: 5, quote: "alpha" },
        comments: [comment("c1", "active one")],
      }),
      thread({
        id: "t2",
        resolved: true,
        anchor: { start: 0, end: 4, quote: "beta" },
        comments: [comment("c2", "resolved one")],
      }),
    ]);
    render(<Harness />);
    await waitFor(() => expect(rows()).toHaveLength(2));
    const before = listCalls().length;

    await userEvent.type(screen.getByLabelText("Filter comments"), "alpha");
    await waitFor(() => expect(rows()).toHaveLength(1));
    expect(groupHeaders()).toEqual(["Active 1"]);
    // CMP-FR-14: matching runs over the loaded set — no backend call.
    expect(listCalls()).toHaveLength(before);
  });

  it("matches the artifact path too", async () => {
    listReturns([
      thread({ id: "t1", artifactId: "specs/onboarding.md" }),
      thread({
        id: "t2",
        artifactId: "src/main.rs",
        comments: [comment("c2", "why mutable")],
      }),
    ]);
    render(<Harness />);
    await waitFor(() => expect(rows()).toHaveLength(2));

    await userEvent.type(screen.getByLabelText("Filter comments"), "main.rs");
    await waitFor(() => expect(rows()).toHaveLength(1));
    expect(screen.getByText("why mutable")).toBeInTheDocument();
  });

  it("keeps the text across a panel switch (CMP-FR-15)", async () => {
    listReturns([thread({ id: "t1" })]);
    render(<SwitchableHarness />);
    await screen.findByText("Which session?");

    await userEvent.type(screen.getByLabelText("Filter comments"), "session");
    await userEvent.click(screen.getByText("toggle surface"));
    expect(screen.queryByLabelText("Filter comments")).not.toBeInTheDocument();

    await userEvent.click(screen.getByText("toggle surface"));
    expect(await screen.findByLabelText("Filter comments")).toHaveValue(
      "session",
    );
  });
});

describe("CMP-FR-16: the panel is read-only", () => {
  it("offers nothing that opens, posts, locks, resolves or quotes", async () => {
    listReturns([
      thread({ id: "t1" }),
      thread({ id: "t2", locked: true }),
      thread({ id: "t3", resolved: true }),
    ]);
    render(<Harness />);
    await waitFor(() => expect(rows()).toHaveLength(3));

    // Every control in the panel is either a row or the filter field.
    const names = screen
      .getAllByRole("button")
      .map((b) => b.getAttribute("aria-label") ?? b.textContent);
    expect(names.every((n) => /^(Fragment|Whole) discussion on /.test(n ?? ""))).toBe(true);
    expect(screen.queryByRole("textbox", { name: /repl|comment body/i }))
      .not.toBeInTheDocument();
    for (const label of [/post/i, /lock/i, /resolve/i, /quote/i, /new comment/i]) {
      expect(screen.queryByRole("button", { name: label })).not.toBeInTheDocument();
    }

    // And nothing but the one read reached the backend.
    expect(invokeMock.mock.calls.map((c) => c[0])).toEqual([
      "list_all_discussions",
    ]);
  });
});

describe("CMP-FR-18, CMP-FR-07 / CMP-FR-22: the panel opens no floating overlay", () => {
  it("renders no dialog, menu or draggable node, so it joins no exclusivity group", async () => {
    listReturns([thread({ id: "t1" }), thread({ id: "t2", locked: true })]);
    render(<Harness />);
    await waitFor(() => expect(rows()).toHaveLength(2));

    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(screen.queryByRole("menuitem")).not.toBeInTheDocument();
    expect(document.querySelector("[draggable=\"true\"]")).toBeNull();

    // Nor is there anything to open one with.
    await userEvent.click(rows()[0] as HTMLElement);
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
  });
});

describe("CMP-FR-17: reading needs no identity", () => {
  it("lists the project's threads without resolving an author", async () => {
    listReturns([thread({ id: "t1" })]);
    render(<Harness />);
    await screen.findByText("Which session?");
    expect(
      invokeMock.mock.calls.some(
        (c) => c[0] === "resolve_comment_author_identity",
      ),
    ).toBe(false);
  });
});

describe("CMP-FR-04 / CMP-FR-18 / CMP-FR-19: how the panel stays current", () => {
  /** The `"discussion changed"` handler the panel subscribed with. */
  function threadEmitter(): (event: { payload: unknown }) => void {
    const call = listenMock.mock.calls.find(
      (c) => c[0] === "discussion-changed",
    );
    expect(call).toBeDefined();
    return call?.[1] as (event: { payload: unknown }) => void;
  }

  it("updates from the event payload without re-reading, and polls for nothing", async () => {
    // Fake timers so "ten minutes pass" is a real claim rather than a sleep.
    // The panel's own timestamp-refresh interval must not be mistaken for a
    // poll, so the assertion below counts reads, not renders.
    vi.useFakeTimers({ shouldAdvanceTime: true });
    listReturns([thread({ id: "t1" })]);
    render(<Harness />);
    await screen.findByText("Which session?");
    expect(listCalls()).toHaveLength(1);

    // CMP-FR-18: a thread the panel has never seen joins the list from the
    // payload alone — no second read.
    const emit = threadEmitter();
    act(() =>
      emit({
        payload: thread({
          id: "t2",
          comments: [comment("c2", "brand new thread")],
        }).discussion,
      }),
    );
    expect(await screen.findByText("brand new thread")).toBeInTheDocument();
    expect(listCalls()).toHaveLength(1);

    // CMP-FR-19: nothing a *log* does makes it re-read, and there is no poll.
    // Asserted as the absence of the mechanism: the panel subscribes to exactly
    // one channel and issues no call as time passes.
    expect(listenMock.mock.calls.map((c) => c[0])).toEqual([
      "discussion-changed",
    ]);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(10 * 60_000);
    });
    expect(listCalls()).toHaveLength(1);
    vi.useRealTimers();
  });

  it("moves a row between groups when the write changed which group it belongs to", async () => {
    // CMP-FR-18 / CMP-FR-04: a resolution set in the rail regroups the row here.
    listReturns([thread({ id: "t1" })]);
    render(<Harness />);
    await screen.findByText("Which session?");
    expect(screen.getByText("Active")).toBeInTheDocument();

    const emit = threadEmitter();
    act(() =>
      emit({ payload: { ...thread({ id: "t1" }).discussion, resolved: true } }),
    );

    await waitFor(() =>
      expect(screen.queryByText("Active")).not.toBeInTheDocument(),
    );
    expect(screen.getByText("Resolved")).toBeInTheDocument();
    expect(listCalls()).toHaveLength(1);
  });

  it("shows an agent's answer without the artifact being open, and without a read", async () => {
    // CMP-FR-07 / CMP-FR-18. The panel is on screen whether or not the artifact
    // the agent was addressed in is open at all, so it follows the thread event
    // rather than waiting to be told by a rail that may not be mounted.
    listReturns([
      thread({ id: "t1" }),
      // CMP-FR-07 marks the *opening* comment's author, so an agent's answer is
      // only distinguishable on a row it opened — which is what this second
      // thread is for.
      thread({
        id: "t2",
        anchor: { start: 0, end: 4, quote: "beta" },
        comments: [comment("c3", "An agent opened this one.", agent)],
      }),
    ]);
    render(<Harness />);
    await waitFor(() => expect(rows()).toHaveLength(2));
    expect(listCalls()).toHaveLength(1);

    const emit = threadEmitter();
    act(() =>
      emit({
        payload: {
          ...thread({ id: "t1" }).discussion,
          comments: [
            comment("c1", "Which session?"),
            comment("c2", "The first.", agent),
          ],
        },
      }),
    );

    // The row's reply count moved without the user opening the artifact and
    // without the panel asking. The panel renders the *opening* comment
    // (CMP-FR-07), so the agent's reply shows up as the count.
    expect(await screen.findByText(/1 reply/)).toBeInTheDocument();
    expect(listCalls()).toHaveLength(1);
    expect(screen.getByText("An agent opened this one.")).toBeInTheDocument();
    // CMP-FR-07: an agent-authored comment renders distinguishably from a
    // human's.
    const authors = Array.from(
      document.querySelectorAll(".comment-row__author"),
    );
    const flags = authors.map((a) => a.getAttribute("data-agent"));
    expect(flags).toContain("true");
    expect(flags).toContain("false");
  });

  it("keeps the filter text across an event-driven update", async () => {
    listReturns([
      thread({ id: "t1", anchor: { start: 0, end: 5, quote: "alpha" } }),
      thread({
        id: "t2",
        anchor: { start: 0, end: 4, quote: "beta" },
        comments: [comment("c2", "the other one")],
      }),
    ]);
    render(<Harness />);
    await waitFor(() => expect(rows()).toHaveLength(2));

    await userEvent.type(screen.getByLabelText("Filter comments"), "alpha");
    await waitFor(() => expect(rows()).toHaveLength(1));

    const emit = threadEmitter();
    act(() =>
      emit({
        payload: {
          ...thread({ id: "t2", anchor: { start: 0, end: 4, quote: "beta" } })
            .discussion,
          comments: [comment("c2", "the other one, replied to")],
        },
      }),
    );

    // The update must not quietly widen the list back out under the author.
    expect(screen.getByLabelText("Filter comments")).toHaveValue("alpha");
    expect(rows()).toHaveLength(1);
  });

  it("ignores an event that arrives before the first read has landed", async () => {
    // The payload names a thread, not the project-wide list, so admitting it
    // before the list exists would render one row and hide every other.
    let release: (v: DiscussionListItem[]) => void = () => {};
    invokeMock.mockImplementation(
      () => new Promise((res) => (release = res)),
    );
    render(<Harness />);
    await waitFor(() => expect(listenMock.mock.calls.length).toBeGreaterThan(0));

    const emit = threadEmitter();
    act(() =>
      emit({
        payload: thread({ id: "t9", comments: [comment("c9", "early")] }).discussion,
      }),
    );
    expect(screen.queryByText("early")).not.toBeInTheDocument();

    await act(async () => {
      release([thread({ id: "t1" })]);
      await Promise.resolve();
    });
    expect(await screen.findByText("Which session?")).toBeInTheDocument();
  });
});

describe("CMP-FR-02, CMP-FR-06, CMP-FR-27: every target kind is indexed", () => {
  const whole = (over: Parameters<typeof thread>[0]) => thread({ anchor: null, ...over });

  it("lists artifact, draft, and note discussions, fragment and whole, each with its owner", async () => {
    publishKnownDrafts([{ id: "d1", name: "Onboarding rewrite" }]);
    rememberNoteLabel("n1", "Ask legal");
    listReturns([
      thread({ id: "frag-art", artifactId: "specs/a.md" }),
      whole({ id: "whole-art", artifactId: "specs/b.md" }),
      thread({
        id: "frag-draft",
        target: { kind: "draft", draftId: "d1" },
        fragmentTarget: {
          owner: { kind: "draft", draftId: "d1" },
          path: "prompt.md",
          start: 0,
          end: 3,
          quote: "one",
        },
      }),
      whole({ id: "whole-draft", target: { kind: "draft", draftId: "d1" } }),
      whole({ id: "note", target: { kind: "note", noteId: "n1" } }),
    ]);
    render(<Harness />);
    await waitFor(() => expect(rows()).toHaveLength(5));

    const byLabel = (re: RegExp) => screen.getByRole("button", { name: re });

    // The kind is text, not colour alone, and a whole row says it is whole in the
    // fragment quote's place.
    expect(within(byLabel(/^Fragment discussion on specs\/a\.md/)).getByText("Fragment")).toBeInTheDocument();
    const wholeArtifact = byLabel(/^Whole discussion on specs\/b\.md/);
    expect(within(wholeArtifact).getByText("Whole")).toBeInTheDocument();
    expect(within(wholeArtifact).getByText(/about the whole file/i)).toBeInTheDocument();
    expect(within(byLabel(/^Fragment discussion on Onboarding rewrite/)).getByText("one")).toBeInTheDocument();
    expect(within(byLabel(/^Whole discussion on Onboarding rewrite/)).getByText(/whole draft/i)).toBeInTheDocument();
    expect(within(byLabel(/^Whole discussion on Ask legal/)).getByText(/whole note/i)).toBeInTheDocument();
  });

  it("passes the owner kind of each row to the route", async () => {
    const onReveal = vi.fn();
    rememberNoteLabel("n1", "Ask legal");
    listReturns([
      whole({ id: "note", target: { kind: "note", noteId: "n1" } }),
      whole({ id: "draft", target: { kind: "draft", draftId: "d1" } }),
    ]);
    render(<Harness onReveal={onReveal} />);
    await waitFor(() => expect(rows()).toHaveLength(2));
    for (const r of rows()) await userEvent.click(r as HTMLElement);
    const kinds = onReveal.mock.calls.map((c) => c[0].target.kind).sort();
    expect(kinds).toEqual(["draft", "note"]);
    expect(onReveal).toHaveBeenCalledWith(
      expect.objectContaining({ discussionId: "note", ownerLabel: "Ask legal" }),
    );
  });
});
