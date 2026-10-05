/**
 * The rail's cards as the shared discussion surface
 * (`CMT-comments.md` CMT-FR-HQNV, CMT-FR-70, CMT-FR-45, CMT-FR-28;
 * `CVP-conversation-presentation.md` CVP-FR-SDMQ, CVP-FR-40, CVP-FR-47).
 *
 * Driven directly rather than through a tab, for the reason the rail's own
 * suite is: the claim is about the rail's cards, and the surfaces that mount it
 * each serve only some of its regions.
 */
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(async () => undefined) }));
vi.mock("@tauri-apps/api/event", () => ({
  listen: (..._args: unknown[]) => Promise.resolve(() => {}),
}));

import { CommentRail } from "./CommentRail";
import { setComposerBody } from "../state/discussionSession";
import type { AnchoredThread } from "../state/commentAnchors";
import type { FragmentRange, Discussion, Participant } from "../types";

afterEach(cleanup);

const author: Participant = { kind: "human", login: "raver119" };

function comments(id: string, body: string) {
  return [
    {
      id: `${id}-c1`,
      author,
      body,
      quotes: [],
      attachments: [],
      createdAt: "2026-02-01T00:00:00Z",
    },
  ];
}

function anchored(
  id: string,
  anchor: FragmentRange,
  over: Partial<Discussion> = {},
): AnchoredThread {
  const thread: Discussion = {
    id,
    target: { kind: "artifact", artifactId: "specs/a.md" },
    fragmentTarget: {
      owner: { kind: "artifact", artifactId: "specs/a.md" },
      path: "specs/a.md",
      ...anchor,
    },
    comments: comments(id, "a passage"),
    locked: false,
    resolved: false,
    createdAt: "2026-02-01T00:00:00Z",
    updatedAt: "2026-02-01T00:00:00Z",
    ...over,
  };
  return { thread, anchor };
}

const refuse = async (): Promise<never> => {
  throw new Error("this test opens no discussion");
};

function mount(threads: AnchoredThread[], over: Record<string, unknown> = {}) {
  const noop = async () => {};
  const quote = over.draftQuote as string | undefined;
  return render(
    <CommentRail
      threads={threads}
      discussions={[]}
      anchorTops={{}}
      scrollTop={0}
      identity={author}
      identityBlock={null}
      blocked={false}
      focusedThreadId={null}
      onFocusThread={() => {}}
      draft={
        quote === undefined
          ? null
          : {
              target: { kind: "artifact", artifactId: "specs/a.md" },
              fragmentTarget: {
                owner: { kind: "artifact", artifactId: "specs/a.md" },
                path: "specs/a.md",
                start: 0,
                end: quote.length,
                quote,
              },
            }
      }
      onOpenDraft={refuse}
      onCancelDraft={() => {}}
      onReply={noop}
      onSetLock={noop}
      onSetResolved={noop}
      errors={{}}
      showResolved
      onToggleResolved={() => {}}
      agents={[]}
      pendingTurns={[]}
      turnFailures={{}}
      onCancelTurn={() => {}}
      ownerLabel="a.md"
      {...over}
    />,
  );
}

describe("CMT-FR-45 / CMT-FR-11: the composer's controls are inline", () => {
  it("puts Attach before the message and Post after it, on one line", () => {
    mount([anchored("t1", { start: 0, end: 3, quote: "abc" })]);

    const field = document.querySelector(".comment-composer__row") as HTMLElement;
    expect(field).not.toBeNull();
    // The three are one field rather than a text box above a row of buttons, so
    // writing a reply and sending it are one gesture in one place.
    const order = [...field.children].map((el) =>
      el.getAttribute("aria-label") ??
      el.querySelector("[aria-label]")?.getAttribute("aria-label") ??
      el.tagName.toLowerCase(),
    );
    expect(order[0]).toBe("Attach");
    expect(order[order.length - 1]).toBe("Post");
    expect(within(field).getByLabelText("Reply to thread t1")).toBeInTheDocument();
  });

  it("offers Cancel only while the reply composer has something to discard", async () => {
    mount([anchored("t1", { start: 0, end: 3, quote: "abc" })]);
    const field = document.querySelector(".comment-composer__row") as HTMLElement;
    expect(within(field).queryByLabelText("Cancel")).toBeNull();

    await userEvent.click(screen.getByLabelText("Reply to thread t1"));
    await userEvent.paste("second thoughts");
    expect(within(field).getByLabelText("Cancel")).toBeInTheDocument();
  });

  it("keeps the draft card's Cancel unconditional", () => {
    // CMT-FR-07: it discards the draft CARD rather than what was typed into it,
    // so an empty draft still has to be dismissible.
    mount([], { draftQuote: "a passage" });
    const draft = screen.getByTestId("comment-draft");
    expect(within(draft).getByLabelText("Cancel")).toBeInTheDocument();
    expect((within(draft).getByLabelText("New comment") as HTMLTextAreaElement).value).toBe("");
  });
});

describe("CMT-FR-70, CTA-FR-VQFJ, CMT-FR-51: the composer accelerator in the rail", () => {
  it("posts on Ctrl/Cmd+Enter and does nothing on an empty body", async () => {
    const onReply = vi.fn(async () => {});
    mount([anchored("t1", { start: 0, end: 3, quote: "abc" })], { onReply });

    const composer = screen.getByLabelText("Reply to thread t1");
    await userEvent.click(composer);
    await userEvent.keyboard("{Control>}{Enter}{/Control}");
    expect(onReply).not.toHaveBeenCalled();

    await userEvent.paste("looks right to me");
    await userEvent.keyboard("{Control>}{Enter}{/Control}");
    expect(onReply).toHaveBeenCalledTimes(1);
    expect(onReply).toHaveBeenCalledWith("t1", "looks right to me", [], []);
    // CVP-FR-40: the newline the key would otherwise have inserted is prevented.
    expect((composer as HTMLTextAreaElement).value).not.toContain("\n");
  });

  it("does nothing while the composer is disabled for want of an identity", async () => {
    const onReply = vi.fn(async () => {});
    // A non-empty body reaching a *disabled* composer is the case CVP-FR-40
    // names, and it is only reachable through the session store — a disabled
    // textarea takes no typing, so a test that types into one passes for the
    // wrong reason and would still pass with the guard deleted.
    setComposerBody("t1", "looks right to me");

    mount([anchored("t1", { start: 0, end: 3, quote: "abc" })], {
      onReply,
      identityBlock: { message: "Commenting needs a GitHub account." },
    });

    const composer = screen.getByLabelText("Reply to thread t1") as HTMLTextAreaElement;
    expect(composer.value).toBe("looks right to me");
    expect(composer).toBeDisabled();

    fireEvent.keyDown(composer, { key: "Enter", metaKey: true });
    fireEvent.keyDown(composer, { key: "Enter", ctrlKey: true });
    expect(onReply).not.toHaveBeenCalled();
  });

  it("does nothing mid-IME composition, and posts once the composition is committed", async () => {
    // CVP-FR-40: the keypress that commits a composition carries `isComposing`.
    // Posting on it would send a half-typed word and eat the commit.
    const onReply = vi.fn(async () => {});
    mount([anchored("t1", { start: 0, end: 3, quote: "abc" })], { onReply });

    const composer = screen.getByLabelText("Reply to thread t1");
    await userEvent.click(composer);
    await userEvent.paste("これは");

    fireEvent.keyDown(composer, { key: "Enter", ctrlKey: true, isComposing: true });
    expect(onReply).not.toHaveBeenCalled();

    fireEvent.keyDown(composer, { key: "Enter", ctrlKey: true });
    expect(onReply).toHaveBeenCalledTimes(1);
    expect(onReply).toHaveBeenCalledWith("t1", "これは", [], []);
  });

  it("starts no second post while one is already in flight", async () => {
    // CVP-FR-40: "does nothing while a post is already in flight" — only
    // observable with a reply that has not settled.
    let settle: (() => void) | undefined;
    const onReply = vi.fn(() => new Promise<void>((resolve) => (settle = resolve)));
    mount([anchored("t1", { start: 0, end: 3, quote: "abc" })], { onReply });

    const composer = screen.getByLabelText("Reply to thread t1");
    await userEvent.click(composer);
    await userEvent.paste("once only");

    fireEvent.keyDown(composer, { key: "Enter", ctrlKey: true });
    fireEvent.keyDown(composer, { key: "Enter", ctrlKey: true });
    expect(onReply).toHaveBeenCalledTimes(1);

    settle?.();
  });
});

describe("CMT-FR-45: the Attach surface opens across the card, not out of it", () => {
  // The inline field (CVP-FR-55) puts Attach at the composer's leading edge,
  // roughly 24px inside the rail — which `.comment-rail` clips. A menu anchored
  // to the control's trailing edge spends 150 of its 176 pixels outside the
  // surface, and neither the stylesheet guard nor a render test can see it
  // without this attribute, since Vitest runs with `css: false`.
  it("anchors both the reply composer's menu and the draft card's", () => {
    const { container } = mount([anchored("t1", { start: 0, end: 3, quote: "abc" })], {
      draftQuote: "a passage",
    });

    const anchors = [...container.querySelectorAll(".comment-attach")];
    expect(anchors.length, "the rail offers no Attach control").toBeGreaterThan(1);
    for (const control of anchors) {
      expect(control.getAttribute("data-align")).toBe("leading");
    }
  });
});

describe("CMT-FR-28: a card raises its focus on the click, not on the press", () => {
  // Found in a browser: focusing a card scrolls its fragment into view and the
  // rail re-arranges around it, moving the card by ~56px. Raising the focus on
  // `mousedown` did that between the press and the release, so the control
  // travelled out from under the pointer. jsdom has no layout and would never
  // see the movement — what it can hold is the ordering that caused it.
  it("does not focus on the press alone", () => {
    const onFocusThread = vi.fn();
    mount([anchored("t1", { start: 0, end: 3, quote: "abc" })], { onFocusThread });

    fireEvent.mouseDown(screen.getByTestId("comment-thread-t1"));
    expect(onFocusThread).not.toHaveBeenCalled();
  });

  it("focuses once the click completes", () => {
    const onFocusThread = vi.fn();
    mount([anchored("t1", { start: 0, end: 3, quote: "abc" })], { onFocusThread });

    fireEvent.click(screen.getByTestId("comment-thread-t1"));
    expect(onFocusThread).toHaveBeenCalledWith("t1");
  });
});

describe("CMT-FR-RPLC, CVP-FR-45: the unavailable and orphaned states keep the discussion", () => {
  it("CMT-FR-RPLC: names the owner as unavailable and keeps the comments and the composer", () => {
    mount([anchored("t1", { start: 0, end: 3, quote: "abc" })], {
      ownerUnavailable: true,
      ownerLabel: "specs/a.md",
    });

    const card = screen.getByTestId("comment-thread-t1");
    expect(card).toHaveAttribute("data-availability", "unavailable");
    expect(within(card).getByTestId("discussion-owner-unavailable")).toHaveTextContent(
      "specs/a.md",
    );
    expect(card).toHaveTextContent("a passage");
    expect(within(card).getByLabelText("Reply to thread t1")).toBeEnabled();
  });

  it("CMT-FR-19: an orphaned fragment is labelled and stays in the orphaned section", () => {
    const orphan = anchored("t1", { start: 0, end: 3, quote: "abc" });
    mount([{ ...orphan, anchor: null }]);

    const card = screen.getByTestId("comment-thread-t1");
    expect(card).toHaveAttribute("data-availability", "orphaned");
    expect(within(card).getByText("Orphaned")).toBeInTheDocument();
    expect(card).toHaveTextContent("abc");
    expect(card.closest(".comment-rail__unaligned")).not.toBeNull();
  });
});

describe("CVP-FR-02, CVP-FR-05: one surface for one discussion id", () => {
  it("renders the second surface of the same id as nothing while the first stands", () => {
    const entry = anchored("t1", { start: 0, end: 3, quote: "abc" });
    mount([entry]);
    mount([entry]);

    expect(screen.getAllByTestId("comment-thread-t1")).toHaveLength(1);
  });
});

describe("CVP-FR-47: the unsent text outlives the surface", () => {
  it("returns the typed reply to a surface mounted again", async () => {
    const entry = anchored("t1", { start: 0, end: 3, quote: "abc" });
    const first = mount([entry]);
    await userEvent.click(screen.getByLabelText("Reply to thread t1"));
    await userEvent.paste("unfinished thought");
    first.unmount();

    mount([entry]);
    expect(screen.getByLabelText("Reply to thread t1")).toHaveValue("unfinished thought");
  });
});

describe("CVP-FR-40, CMT-FR-07: the draft card posts from the keyboard", () => {
  it("opens the discussion on Ctrl+Enter with the draft's fragment target", async () => {
    const onOpenDraft = vi.fn(async () => {
      throw new Error("refused");
    });
    mount([], { draftQuote: "a passage", onOpenDraft });

    const field = screen.getByLabelText("New comment");
    await userEvent.click(field);
    await userEvent.paste("first comment");
    await userEvent.keyboard("{Control>}{Enter}{/Control}");

    expect(onOpenDraft).toHaveBeenCalledTimes(1);
    const request = (onOpenDraft.mock.calls[0] as unknown[])[0] as {
      fragmentTarget: { quote: string };
      body: string;
    };
    expect(request.body).toBe("first comment");
    expect(request.fragmentTarget.quote).toBe("a passage");
  });
});
