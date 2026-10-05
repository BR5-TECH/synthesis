/**
 * New Artifact discussions: the surfaces they open on, the event path that
 * refreshes them, and what is still coming
 * (`../../specifications/ui/NAW-new-artifact.md` NAW-FR-30 … NAW-FR-34).
 */
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
/**
 * The margin follows `"discussion changed"` and `"agent turn state changed"`
 * (CMT-FR-54, CTA-FR-QXIG), so the mock keeps its handlers rather than discarding
 * them — a test fires on a channel exactly as the backend would.
 */
const listeners = new Map<string, Set<(e: { payload: unknown }) => void>>();
function fireBus(name: string, payload: unknown) {
  [...(listeners.get(name) ?? [])].forEach((h) => h({ payload }));
}
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

import {
  makeStubs,
  discussionThread,
  renderWorkspace,
  openActions,
  openComposer,
  postComposer,
  pngFile,
} from "../test/newArtifactFixtures";
import type { Discussion, Participant } from "../types";
import { DraftSessionStore } from "../state/draftSessions";
import { hunkCandidateBuffers } from "../state/candidateBuffers";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { resetDiscussionFocus } from "../state/discussionFocus";
import {
  draftDiscussionOrigin,
} from "../test/origins";

const { stub, stubDiscussions } = makeStubs(invokeMock);

/** Every argument set one operation was invoked with. */
const callsTo = (cmd: string) =>
  invokeMock.mock.calls.filter((c) => c[0] === cmd).map((c) => c[1]);

beforeEach(() => {
  invokeMock.mockReset();
  listeners.clear();
  // DCR-FR-25: a candidate buffer outlives every surface by design, so it
  // carries from one test into the next unless a suite drops it.
  hunkCandidateBuffers.clear();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  stub();
});

afterEach(cleanup);

describe("New Artifact discussions: the surfaces, the event path, and what is still coming", () => {
  it("NAW-FR-30, NAW-FR-18, ACT-FR-11, ACT-FR-12, ACT-FR-QWNP, SNV-FR-56: expanding the control dismisses a card's overflow menu, and neither invokes anything", async () => {
    // NAW-FR-30 / CMT-FR-32: the action control's expansion and a card's
    // overflow menu are one set — at most one of them is open at any moment.
    // The composer is no longer one of them: it stands permanently in the
    // discussion column (ACT-FR-QWNP), so there is nothing about it to dismiss.
    stubDiscussions({
      existing: [
        discussionThread("disc-1", [
          { id: "c1", author: { kind: "human", login: "raver119" } as Participant, body: "one" },
        ]),
      ],
    });
    renderWorkspace();
    const card = await screen.findByTestId("comment-thread-disc-1");
    const menuButton = within(card).getByRole("button", {
      name: "Thread actions for disc-1",
    });

    invokeMock.mockClear();
    await userEvent.click(menuButton);
    expect(
      screen.getByRole("menuitem", { name: "Mark resolved" }),
    ).toBeInTheDocument();

    // Expanding the control dismisses the menu.
    await openActions();
    await waitFor(() =>
      expect(
        screen.queryByRole("menuitem", { name: "Mark resolved" }),
      ).not.toBeInTheDocument(),
    );

    // None of that invoked anything. The session log's own batched flush is not
    // part of the feature and lands on its own timer (per
    // `../../specifications/core/LGC-logging.md`).
    expect(
      invokeMock.mock.calls
        .map((c) => c[0])
        .filter((cmd) => cmd !== "append_log_records"),
    ).toEqual([]);
  });

  it("CMT-FR-52, CMT-FR-53, CMT-FR-54, CMT-FR-55, CMT-FR-62: a discussion redraws from the `discussion changed` payload, without re-reading", async () => {
    // CMT-FR-54 / CTA-FR-QTNB: the only route by which an agent's delivered answer,
    // or a lock set in another window, reaches a card.
    stubDiscussions({
      existing: [
        discussionThread("disc-1", [
          { id: "c1", author: { kind: "human", login: "raver119" } as Participant, body: "opening" },
        ]),
      ],
    });
    renderWorkspace();
    await screen.findByTestId("comment-thread-disc-1");
    expect(callsTo("list_discussions")).toHaveLength(1);

    // An agent's answer, arriving as an append this window did not make.
    await act(async () => {
      fireBus(
        "discussion-changed",
        discussionThread("disc-1", [
          { id: "c1", author: { kind: "human", login: "raver119" } as Participant, body: "opening" },
          {
            id: "c2",
            author: { kind: "agent", agentId: "a1", handle: "arch" } as Participant,
            body: "Graduate it into ui/.",
          },
        ]),
      );
    });
    expect(await screen.findByText("Graduate it into ui/.")).toBeInTheDocument();
    // Redrawn from the payload rather than from a re-read.
    expect(callsTo("list_discussions")).toHaveLength(1);

    // A discussion this margin has not seen is admitted and joins the tail.
    await act(async () => {
      fireBus(
        "discussion-changed",
        discussionThread("disc-2", [
          { id: "c3", author: { kind: "human", login: "raver119" } as Participant, body: "second" },
        ]),
      );
    });
    // DDS-FR-KTVW: both discussions are the column's, in the order they were
    // opened. The column reads one at a time and offers a chooser between them,
    // so what the arrival added is a second choice rather than a second card.
    const column = await screen.findByTestId("draft-discussion-column");
    await waitFor(() =>
      expect(within(column).getAllByRole("tab")).toHaveLength(2),
    );
    await within(column).findByTestId("comment-thread-disc-1");
  });

  it("CMT-FR-52, CMT-FR-53, CMT-FR-54, CMT-FR-55, CMT-FR-62: a payload for another draft, or for an anchored thread, is ignored", async () => {
    stubDiscussions({
      existing: [
        discussionThread("disc-1", [
          { id: "c1", author: { kind: "human", login: "raver119" } as Participant, body: "opening" },
        ]),
      ],
    });
    renderWorkspace();
    await screen.findByTestId("comment-thread-disc-1");

    await act(async () => {
      // Another draft's discussion: nothing to do with this margin.
      fireBus(
        "discussion-changed",
        discussionThread(
          "other-draft-disc",
          [{ id: "cx", author: { kind: "human", login: "raver119" } as Participant, body: "elsewhere" }],
          { target: { kind: "draft", draftId: "d2" } },
        ),
      );
      // An anchored thread, which this margin does not render at all.
      fireBus("discussion-changed", {
        ...discussionThread("anchored-1", [
          { id: "cy", author: { kind: "human", login: "raver119" } as Participant, body: "a passage" },
        ]),
        target: { kind: "artifact", artifactId: "ui/NAW.md" },
        fragmentTarget: {
          owner: { kind: "artifact", artifactId: "ui/NAW.md" },
          path: "ui/NAW.md",
          start: 0,
          end: 3,
          quote: "abc",
        },
      } as Discussion);
    });

    expect(screen.queryByText("elsewhere")).not.toBeInTheDocument();
    expect(screen.queryByText("a passage")).not.toBeInTheDocument();
    // The column still holds exactly the one discussion it started with, so
    // neither payload reached it.
    const column = screen.getByTestId("draft-discussion-column");
    expect(within(column).getAllByRole("tab")).toHaveLength(1);
    expect(within(column).getByTestId("comment-thread-disc-1")).toBeInTheDocument();
  });

  it("CMT-FR-55, CMT-FR-19, CMT-FR-21: a discussion card renders what is still coming, and never appears among the orphans", async () => {
    // `CTA-comment-agent-turns.md` CTA-FR-ZOLW, CTA-FR-QTNB / CMT-FR-45 through
    // CMT-FR-55, and CMT-FR-55's "never orphaned".
    stubDiscussions({
      agents: ["arch"],
      existing: [
        discussionThread("disc-1", [
          {
            id: "c1",
            author: { kind: "human", login: "raver119" } as Participant,
            body: "@arch thoughts?",
          },
        ]),
      ],
      turns: [
        {
          id: "turn-1",
          agentId: "a1",
          nickname: "arch",
          origin: draftDiscussionOrigin("disc-1", "d1"),
          triggerCommentId: "c1",
          state: "running",
          failure: null,
          retryPermitted: false,
          imagesOmitted: false,
          activeToolCalls: [],
          startedAt: "2026-02-01T00:00:00Z",
          endedAt: null,
        },
      ],
    });
    renderWorkspace();

    // The pending contribution, in the position the agent's answer will take.
    // DDS-FR-TGWY: a dot and one lowercase word, and no skeleton or spinner.
    const pending = await screen.findByTestId("dds-transient");
    expect(pending).toHaveTextContent("thinking");
    expect(pending).toHaveTextContent("arch");
    // Never orphaned, however the draft's files are edited (CMT-FR-55).
    expect(screen.queryByText("Orphaned")).not.toBeInTheDocument();

    // Cancelling it invokes the turn's own operation and leaves the comments.
    await userEvent.click(screen.getByTestId("dds-transient-cancel"));
    await waitFor(() => expect(callsTo("cancel_agent_turn")).toHaveLength(1));
    expect(callsTo("cancel_agent_turn")[0]).toEqual({ turnId: "turn-1" });
    expect(screen.queryByTestId("dds-transient")).not.toBeInTheDocument();
    expect(screen.getByTestId("comment-thread-disc-1")).toHaveTextContent(
      "thoughts?",
    );
  });

  it("CMT-FR-55, CMT-FR-19, CMT-FR-21: a turn that fails renders its typed failure at the foot of the discussion card", async () => {
    stubDiscussions({
      agents: ["arch"],
      existing: [
        discussionThread("disc-1", [
          {
            id: "c1",
            author: { kind: "human", login: "raver119" } as Participant,
            body: "@arch thoughts?",
          },
        ]),
      ],
      turns: [
        {
          id: "turn-1",
          agentId: "a1",
          nickname: "arch",
          origin: draftDiscussionOrigin("disc-1", "d1"),
          triggerCommentId: "c1",
          state: "running",
          failure: null,
          retryPermitted: false,
          imagesOmitted: false,
          activeToolCalls: [],
          startedAt: "2026-02-01T00:00:00Z",
          endedAt: null,
        },
      ],
    });
    renderWorkspace();
    await screen.findByTestId("dds-transient");

    await act(async () => {
      fireBus("agent-turn-state-changed", {
        id: "turn-1",
        agentId: "a1",
        nickname: "arch",
        origin: draftDiscussionOrigin("disc-1", "d1"),
        triggerCommentId: "c1",
        state: "failed",
        failure: "unreachable",
        startedAt: "2026-02-01T00:00:00Z",
        endedAt: "2026-02-01T00:00:10Z",
      });
    });

    // The placeholder is gone, the failure is stated, and the conversation
    // gained no line — it gains one only when someone actually said something.
    await waitFor(() =>
      expect(screen.queryByTestId("dds-transient")).not.toBeInTheDocument(),
    );
    expect(await screen.findByTestId("comment-turn-error")).toHaveTextContent(
      /could not be reached/i,
    );
    expect(screen.getByTestId("comment-thread-disc-1")).toHaveTextContent(
      "thoughts?",
    );
  });

  it("NAW-FR-32, CMT-FR-55, CTA-FR-QUXJ: a discussion carries a quote and an attachment on its reply, and none on a locked one", async () => {
    // CMT-FR-55: an ordinary card in every respect that does not depend on an
    // anchor — including what a post actually carries (CMT-FR-13, CMT-FR-47).
    stubDiscussions({
      existing: [
        discussionThread("disc-1", [
          { id: "c1", author: { kind: "human", login: "raver119" } as Participant, body: "opening" },
        ]),
      ],
    });
    renderWorkspace();
    const card = await screen.findByTestId("comment-thread-disc-1");

    await userEvent.click(
      within(card).getByRole("button", { name: /^Quote comment 1/ }),
    );
    const reply = within(card).getByLabelText("Reply to thread disc-1");
    await act(async () => {
      fireEvent.drop(reply.parentElement!, {
        dataTransfer: { files: [pngFile("evidence.png")] },
      });
    });
    await screen.findByText("evidence.png");
    fireEvent.change(reply, { target: { value: "as quoted" } });
    await userEvent.click(within(card).getByRole("button", { name: "Post" }));

    await waitFor(() => expect(callsTo("add_comment")).toHaveLength(1));
    const [posted] = callsTo("add_comment") as [
      {
        quotes: { commentId: string; excerpt: string }[];
        attachments: { filename?: string }[];
      },
    ];
    expect(posted.quotes).toEqual([{ commentId: "c1", excerpt: "opening" }]);
    expect(posted.attachments).toHaveLength(1);
    expect(posted.attachments[0].filename).toBe("evidence.png");

    // CMT-FR-15: a locked discussion carries no composer, and therefore nowhere
    // to address anyone and nothing to attach.
    cleanup();
    invokeMock.mockReset();
    stubDiscussions({
      existing: [
        discussionThread(
          "disc-1",
          [{ id: "c1", author: { kind: "human", login: "raver119" } as Participant, body: "settled" }],
          { locked: true },
        ),
      ],
    });
    renderWorkspace();
    const locked = await screen.findByTestId("comment-thread-disc-1");
    expect(
      within(locked).queryByLabelText("Reply to thread disc-1"),
    ).not.toBeInTheDocument();
    expect(
      within(locked).queryByTestId("comment-attach-button"),
    ).not.toBeInTheDocument();
    expect(locked).toHaveTextContent("Locked");
  });

  it("NAW-FR-33, CMT-FR-24, CMT-FR-26, CMT-FR-51: the refused attachment is removed and the message posted without being retyped", async () => {
    // The clause NAW-FR-33, CMT-FR-24, CMT-FR-26, CMT-FR-51 closes on, and the one the whole "leaves the body and
    // the strip intact" rule exists for (CMT-FR-51).
    let refuse = true;
    stubDiscussions({});
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      if (cmd === "open_discussion" && refuse) throw "unsupported_media_type";
      return base(cmd, args);
    });

    renderWorkspace();
    const composer = await openComposer();
    fireEvent.change(composer, { target: { value: "look at this" } });
    await act(async () => {
      fireEvent.drop(composer.parentElement!, {
        dataTransfer: { files: [pngFile("bad.tiff"), pngFile("good.png")] },
      });
    });
    await screen.findByText("bad.tiff");
    await postComposer();
    await screen.findByRole("alert");

    // Remove the offending one — which invokes nothing — and post again. The
    // session log's own batched flush is not part of the feature and lands on
    // its own timer (per `../../specifications/core/LGC-logging.md`).
    invokeMock.mockClear();
    await userEvent.click(
      screen.getByRole("button", { name: "Remove attachment bad.tiff" }),
    );
    expect(
      invokeMock.mock.calls.filter((c) => c[0] !== "append_log_records"),
    ).toEqual([]);
    refuse = false;
    await postComposer();

    await waitFor(() => expect(callsTo("open_discussion")).toHaveLength(1));
    const [posted] = callsTo("open_discussion") as [
      { body: string; attachments: { filename?: string }[] },
    ];
    expect(posted.body).toBe("look at this");
    expect(posted.attachments.map((a) => a.filename)).toEqual(["good.png"]);
  });

  it("NAW-FR-31, NAW-FR-32, ACT-FR-16, ACT-FR-19, CVP-FR-64, CMT-FR-53: an attachment is added by the control, by a paste, and as a link", async () => {
    // NAW-FR-31 adopts CMT-FR-45 wholesale, so all four routes have to work here
    // and not only the dropped one.
    stubDiscussions({});
    renderWorkspace();
    const composer = await openComposer();

    const input = screen.getByTestId("comment-attach-file-input");
    await act(async () => {
      fireEvent.change(input, { target: { files: [pngFile("picked.png")] } });
    });
    expect(await screen.findByText("picked.png")).toBeInTheDocument();

    await act(async () => {
      fireEvent.paste(composer, {
        clipboardData: { files: [pngFile("pasted.png")] },
      });
    });
    expect(await screen.findByText("pasted.png")).toBeInTheDocument();

    fireEvent.click(screen.getByTestId("comment-attach-button"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Attach a link…" }));
    fireEvent.change(screen.getByLabelText("Attachment address"), {
      target: { value: "https://example.test/spec.png" },
    });
    fireEvent.change(screen.getByLabelText("Attachment label"), {
      target: { value: "spec v2" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Attach link" }));
    expect(await screen.findByText("spec v2")).toBeInTheDocument();

    // Nothing has been sent anywhere until the message is posted (CMT-FR-46).
    expect(callsTo("open_discussion")).toHaveLength(0);

    fireEvent.change(composer, { target: { value: "three of them" } });
    await postComposer();
    await waitFor(() => expect(callsTo("open_discussion")).toHaveLength(1));
    const [posted] = callsTo("open_discussion") as [
      { attachments: ({ filename?: string; url?: string; label?: string })[] },
    ];
    expect(posted.attachments.map((a) => a.filename ?? a.label)).toEqual([
      "picked.png",
      "pasted.png",
      "spec v2",
    ]);
  });

  it("NAW-FR-15, DDS-FR-KTVW, DDS-FR-LPSC, CVP-FR-47: the column's composer keeps its text and its attachments across a dismissal and across the tab closing and opening again", async () => {
    // The composer is part of the column now, so nothing that used to dismiss
    // it takes what is in it. The text and the strip are held in the session
    // store, which outlives the tab (DDS-FR-LPSC), and nothing but a project or
    // worktree change discards them.
    stubDiscussions({});
    const drafts = new DraftSessionStore();
    const first = renderWorkspace(drafts);
    const composer = await openComposer();
    fireEvent.change(composer, { target: { value: "with a picture" } });
    await act(async () => {
      fireEvent.drop(composer.parentElement!, {
        dataTransfer: { files: [pngFile("chooser.png")] },
      });
    });
    await screen.findByText("chooser.png");

    fireEvent.keyDown(window, { key: "Escape" });
    expect(
      screen.getByRole("textbox", { name: "Discuss this draft" }),
    ).toHaveValue("with a picture");
    expect(screen.getByText("chooser.png")).toBeInTheDocument();

    first.view.unmount();
    renderWorkspace(drafts);
    const afterReopen = await openComposer();
    expect(afterReopen).toHaveValue("with a picture");
    expect(screen.getByText("chooser.png")).toBeInTheDocument();
  });

  it("DDS-FR-WCHN: an arriving message never scrolls the document column", async () => {
    // The two columns are two scroll regions and neither moves the other. This
    // is the half that is easy to lose: a card scrolling itself into view when
    // a message lands would take the document with it if the two shared a
    // scroller, and the author would be reading a different passage than the one
    // they were deciding.
    stubDiscussions({
      existing: [
        discussionThread("disc-1", [
          { id: "c1", author: { kind: "human", login: "raver119" } as Participant, body: "opening" },
        ]),
      ],
    });
    renderWorkspace();
    await screen.findByTestId("comment-thread-disc-1");

    const document_ = document.querySelector(".editor") as HTMLElement;
    expect(document_, "the document's own scroller").not.toBeNull();
    let top = 420;
    Object.defineProperty(document_, "scrollTop", {
      configurable: true,
      get: () => top,
      set: (next: number) => {
        top = next;
      },
    });

    await act(async () => {
      fireBus(
        "discussion-changed",
        discussionThread("disc-1", [
          { id: "c1", author: { kind: "human", login: "raver119" } as Participant, body: "opening" },
          { id: "c2", author: { kind: "human", login: "raver119" } as Participant, body: "and another thing" },
        ]),
      );
    });
    await screen.findByText("and another thing");

    // To the pixel: the document moves for hunk navigation and for the author's
    // own scrolling, and for nothing else.
    expect(top).toBe(420);
  });

});