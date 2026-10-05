import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";

import { Editor } from "./Editor";
import { EditSessionStore } from "../state/editSessions";
import type { Attachment, AttachmentInput, Discussion } from "../types";
import {
  BODY,
  human,
  makeThread,
  openRail,
  wireBackend,
  fragment,
} from "../test/editorCommentsFixtures";
import type { Backend } from "../test/editorCommentsFixtures";

/**
 * CMT-comments.md, driven through the real Editor.
 *
 * The rail's arithmetic — re-anchoring, tracking an edit, card stacking — lives
 * in `../state/commentAnchors` and is covered by its own unit tests. What is
 * exercised here is the wiring: which command each affordance invokes, what the
 * rail renders in each of its states, and the claims the Editor spec makes about
 * hosting it (EDT-FR-61).
 */
const invokeMock = vi.fn();
const unlistenMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

/**
 * The rail follows `"discussion changed"` (CMT-FR-04), so the mock keeps the
 * handlers rather than discarding them — a test emits on a channel exactly as
 * the backend would.
 */
const listeners = new Map<string, Set<(e: { payload: unknown }) => void>>();
function emitEvent(name: string, payload: unknown) {
  [...(listeners.get(name) ?? [])].forEach((h) => h({ payload }));
}
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(
    async (event: string, cb: (e: { payload: unknown }) => void) => {
      const set = listeners.get(event) ?? new Set();
      set.add(cb);
      listeners.set(event, set);
      return () => {
        set.delete(cb);
        unlistenMock();
      };
    },
  ),
}));

beforeEach(() => {
  invokeMock.mockReset();
  unlistenMock.mockReset();
});

afterEach(() => {
  cleanup();
});

async function mount(over: Partial<Backend> = {}) {
  const sessions = new EditSessionStore();
  const backend: Backend = {
    load: { body: BODY, checksum: "ck1" },
    threads: [],
    calls: [],
    ...over,
  };
  // The anchor's `end` is derived so a hand-written fixture cannot drift.
  backend.threads = backend.threads.map((t) => ({
    ...t,
    fragmentTarget:
      t.fragmentTarget === null
        ? null
        : {
            ...t.fragmentTarget,
            end: t.fragmentTarget.start + t.fragmentTarget.quote.length,
          },
  }));
  wireBackend(invokeMock, backend);
  render(
    <Editor
      artifactId="a.md"
      artifactName="a.md"
      artifactType="skill"
      sessions={sessions}
    />,
  );
  await screen.findByLabelText("artifact body");
  return { sessions, backend };
}

/**
 * Selecting text in a jsdom contenteditable does not give ProseMirror a real
 * selection, so these drive the affordance through the same DOM selection the
 * component reads (`window.getSelection().toString()`), which is the part under
 * test — the mapping from a selection to a source anchor and the command it
 * produces.
 */
function selectInBody(text: string) {
  // Deliberately NOT inside `.editor__prose`: mutating ProseMirror's own DOM
  // makes its observer read a DOM change and call `coordsAtPos`, which jsdom
  // cannot serve. `captureSelection` reads `window.getSelection().toString()`
  // and takes its position hint from the editor's own selection state, so a
  // node parked elsewhere in the document exercises exactly the same path.
  const host = document.createElement("div");
  host.textContent = text;
  document.body.appendChild(host);
  selectionHosts.push(host);
  const range = document.createRange();
  range.selectNodeContents(host);
  const sel = window.getSelection()!;
  sel.removeAllRanges();
  sel.addRange(range);
  fireEvent.mouseUp(screen.getByLabelText("artifact body"));
}

const selectionHosts: HTMLElement[] = [];
afterEach(() => {
  window.getSelection()?.removeAllRanges();
  while (selectionHosts.length) selectionHosts.pop()!.remove();
});

// ---------------------------------------------------------------------------
// CMT-FR-15 … CMT-FR-34, CMS-FR-47: attachments (CMT-FR-45 … CMT-FR-51)
// ---------------------------------------------------------------------------

/** A `File` the composer can encode, without needing a real image on disk. */
function pngFile(name: string, bytes = "hello") {
  return new File([bytes], name, { type: "image/png" });
}

describe("CMT-FR-15 / CMT-FR-45: how an attachment is added", () => {
  it("offers a file and a link, and attaches a dropped or pasted file without the control", async () => {
    await mount({ threads: [makeThread()] });
    await openRail();

    // The Attach control offers exactly the two entries.
    fireEvent.click(screen.getByTestId("comment-attach-button"));
    expect(
      screen.getByRole("menuitem", { name: "Attach a file…" }),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("menuitem", { name: "Attach a link…" }),
    ).toBeInTheDocument();

    // Choosing a file: the picker's change event carries it into the strip.
    const input = screen.getByTestId("comment-attach-file-input");
    await act(async () => {
      fireEvent.change(input, { target: { files: [pngFile("picked.png")] } });
    });
    expect(await screen.findByText("picked.png")).toBeInTheDocument();

    // Dropping a file attaches it without the control being opened at all.
    const composer = screen.getByLabelText("Reply to thread t1");
    await act(async () => {
      fireEvent.drop(composer.parentElement!, {
        dataTransfer: { files: [pngFile("dropped.png")] },
      });
    });
    expect(await screen.findByText("dropped.png")).toBeInTheDocument();

    // Pasting image data does the same.
    await act(async () => {
      fireEvent.paste(composer, {
        clipboardData: { files: [pngFile("pasted.png")] },
      });
    });
    expect(await screen.findByText("pasted.png")).toBeInTheDocument();
  });

  it("carries no Attach control on a locked thread", async () => {
    // CMT-FR-15: a locked card has no composer and therefore nowhere to attach.
    await mount({ threads: [makeThread({ locked: true })] });
    await openRail();
    expect(screen.queryByTestId("comment-attach-button")).not.toBeInTheDocument();
  });
});

describe("CMT-FR-11 / CMT-FR-46, CMT-FR-47: the pending strip and what a post carries", () => {
  it("holds attachments until the post, removes one without invoking anything, and discards them on cancel", async () => {
    const { backend } = await mount({ threads: [makeThread()] });
    await openRail();

    const input = screen.getByTestId("comment-attach-file-input");
    await act(async () => {
      fireEvent.change(input, { target: { files: [pngFile("keep.png")] } });
    });
    // A link, through the Attach control's second entry.
    fireEvent.click(screen.getByTestId("comment-attach-button"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Attach a link…" }));
    fireEvent.change(screen.getByLabelText("Attachment address"), {
      target: { value: "https://example.test/spec.png" },
    });
    fireEvent.change(screen.getByLabelText("Attachment label"), {
      target: { value: "spec v2" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Attach link" }));

    await screen.findByText("keep.png");
    expect(screen.getByText("spec v2")).toBeInTheDocument();
    // Nothing has been sent anywhere yet.
    expect(backend.calls.some((c) => c.cmd === "add_comment")).toBe(false);

    // Removing one invokes nothing and leaves the other.
    fireEvent.click(
      screen.getByRole("button", { name: "Remove attachment spec v2" }),
    );
    await waitFor(() =>
      expect(screen.queryByText("spec v2")).not.toBeInTheDocument(),
    );
    expect(screen.getByText("keep.png")).toBeInTheDocument();
    expect(backend.calls.some((c) => c.cmd === "add_comment")).toBe(false);

    // CMT-FR-47: the post carries what is left, encoded.
    fireEvent.change(screen.getByLabelText("Reply to thread t1"), {
      target: { value: "look at this" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() =>
      expect(backend.calls.some((c) => c.cmd === "add_comment")).toBe(true),
    );
    const call = backend.calls.find((c) => c.cmd === "add_comment")!;
    const sent = (call.args as { attachments: AttachmentInput[] }).attachments;
    expect(sent).toHaveLength(1);
    expect(sent[0]).toMatchObject({
      kind: "inline",
      mediaType: "image/png",
      filename: "keep.png",
    });
    expect((sent[0] as { data: string }).data).toBe(btoa("hello"));
  });

  it("discards the strip with the typed body when the composer is cancelled", async () => {
    const { backend } = await mount({ threads: [makeThread()] });
    await openRail();
    await act(async () => {
      fireEvent.change(screen.getByTestId("comment-attach-file-input"), {
        target: { files: [pngFile("gone.png")] },
      });
    });
    await screen.findByText("gone.png");

    fireEvent.click(within(screen.getByTestId("comment-thread-t1")).getByRole(
      "button",
      { name: "Cancel" },
    ));
    await waitFor(() =>
      expect(screen.queryByText("gone.png")).not.toBeInTheDocument(),
    );
    expect(backend.calls.some((c) => c.cmd === "add_comment")).toBe(false);
  });

  it("sends an empty list when nothing is attached", async () => {
    const { backend } = await mount({ threads: [makeThread()] });
    await openRail();
    fireEvent.change(screen.getByLabelText("Reply to thread t1"), {
      target: { value: "plain reply" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() =>
      expect(backend.calls.some((c) => c.cmd === "add_comment")).toBe(true),
    );
    const call = backend.calls.find((c) => c.cmd === "add_comment")!;
    expect((call.args as { attachments: unknown[] }).attachments).toEqual([]);
  });
});

describe("CMT-FR-48, CMT-FR-49: how a posted attachment renders", () => {
  function threadWithAttachments(): Discussion {
    return makeThread({
      comments: [
        {
          id: "c1",
          author: human("raver119"),
          body: "see these",
          quotes: [],
          attachments: [
            {
              kind: "blob",
              digest: "a3f9",
              mediaType: "image/png",
              filename: "diff.png",
              bytes: 5,
            },
            {
              kind: "blob",
              digest: "b7c1",
              mediaType: "application/pdf",
              filename: "notes.pdf",
              bytes: 9,
            },
          ],
          createdAt: "2026-01-01T00:00:00Z",
        },
      ],
    });
  }

  it("renders an image as a thumbnail and anything else as a named chip", async () => {
    const { backend } = await mount({ threads: [threadWithAttachments()] });
    await openRail();

    const image = await screen.findByTestId("comment-attachment-image");
    expect(image.querySelector("img")).toHaveAttribute(
      "src",
      "data:image/png;base64,aGVsbG8=",
    );

    // CMT-FR-48: the PDF is a *named* chip, not a broken one. "We will not
    // inline this" is not "this failed" — the distinction the mock above exists
    // to make observable, by reporting each digest's real media type.
    const chip = await screen.findByTestId("comment-attachment-chip");
    expect(chip).toHaveTextContent("notes.pdf");
    expect(chip).toHaveTextContent("application/pdf");
    expect(chip).not.toHaveTextContent("could not be loaded");
    expect(screen.queryByTestId("comment-attachment-broken")).not.toBeInTheDocument();
    // CMT-FR-49: a chip shows a filename and a media type, both of which the
    // log line already carries — so its bytes are not read to draw it. Only the
    // image's were.
    const reads = () =>
      backend.calls.filter((c) => c.cmd === "read_comment_attachment");
    const digestsRead = () =>
      new Set(reads().map((r) => (r.args as { digest: string }).digest));
    // Only the image's. The card may re-render more than once, so this asserts
    // *which* digests were fetched rather than how many calls were made.
    expect(digestsRead()).toEqual(new Set(["a3f9"]));

    // It is still activatable (CMT-FR-48): the read happens then, and the bytes
    // are opened as a download rather than as a page the media type chose.
    expect(chip).toHaveAttribute("role", "button");
    expect(chip).toHaveAttribute("tabindex", "0");
    const opened = vi.fn();
    vi.stubGlobal("open", opened);
    fireEvent.click(chip);
    await waitFor(() => expect(digestsRead()).toEqual(new Set(["a3f9", "b7c1"])));
    await waitFor(() =>
      expect(opened).toHaveBeenCalledWith(
        expect.stringMatching(/^data:application\/octet-stream;base64,/),
        "_blank",
        "noreferrer",
      ),
    );
    vi.unstubAllGlobals();
  });

  it("reads an attachment only for a card that is rendered", async () => {
    // CMT-FR-49: a resolved thread's card lives behind the collapsed disclosure
    // (CMT-FR-17), so it is not rendered and its attachment costs nothing until
    // the author asks to see it.
    const resolved = { ...threadWithAttachments(), resolved: true };
    const { backend } = await mount({ threads: [resolved] });
    await openRail();
    await waitFor(() =>
      expect(
        backend.calls.filter((c) => c.cmd === "list_discussions"),
      ).toHaveLength(1),
    );
    expect(
      backend.calls.some((c) => c.cmd === "read_comment_attachment"),
    ).toBe(false);

    fireEvent.click(screen.getByRole("button", { name: /resolved thread/i }));
    await waitFor(() =>
      expect(
        backend.calls.some((c) => c.cmd === "read_comment_attachment"),
      ).toBe(true),
    );
  });

  it("says an attachment could not be loaded rather than leaving an empty frame", async () => {
    await mount({
      threads: [threadWithAttachments()],
      readAttachmentError: "attachment_not_found",
    });
    await openRail();
    const broken = await screen.findAllByTestId("comment-attachment-broken");
    expect(broken[0]).toHaveTextContent("diff.png");
    expect(broken[0]).toHaveTextContent("could not be loaded");
  });

  it("renders a link from its own address, without reading it back", async () => {
    const { backend } = await mount({
      threads: [
        makeThread({
          comments: [
            {
              id: "c1",
              author: human("raver119"),
              body: "hosted already",
              quotes: [],
              attachments: [
                {
                  kind: "url",
                  url: "https://example.test/spec.png",
                  mediaType: "image/png",
                  label: "spec v2",
                },
              ],
              createdAt: "2026-01-01T00:00:00Z",
            },
          ],
        }),
      ],
    });
    await openRail();
    const image = await screen.findByTestId("comment-attachment-image");
    expect(image.querySelector("img")).toHaveAttribute(
      "src",
      "https://example.test/spec.png",
    );
    expect(
      backend.calls.some((c) => c.cmd === "read_comment_attachment"),
    ).toBe(false);
  });
});

describe("CMT-FR-14 / CMT-FR-50: an attachment is immutable with its comment", () => {
  it("offers nothing anywhere to remove, replace, or re-order one", async () => {
    await mount({
      threads: [
        makeThread({
          comments: [
            {
              id: "c1",
              author: human("raver119"),
              body: "posted",
              quotes: [],
              attachments: [
                {
                  kind: "blob",
                  digest: "a3f9",
                  mediaType: "image/png",
                  filename: "diff.png",
                  bytes: 5,
                },
              ],
              createdAt: "2026-01-01T00:00:00Z",
            },
          ],
        }),
      ],
    });
    await openRail();
    const posted = await screen.findByTestId("comment-attachments");
    // The strip's removal control is the only one in the rail, and a posted
    // comment's list is not a strip.
    expect(within(posted).queryByRole("button")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /Thread actions/ }));
    const menu = screen.getByRole("menu");
    expect(within(menu).getAllByRole("menuitem")).toHaveLength(2);
    expect(menu).not.toHaveTextContent(/attach/i);
  });
});

describe("CMT-FR-34, CMS-FR-47 / CMT-FR-51: an append refused for its attachments", () => {
  it("keeps the body and the whole strip, and posts once the offender is removed", async () => {
    const { backend } = await mount({
      threads: [makeThread()],
      addCommentError: "unsupported_media_type",
    });
    await openRail();

    await act(async () => {
      fireEvent.change(screen.getByTestId("comment-attach-file-input"), {
        target: { files: [pngFile("ok.png"), pngFile("bad.png")] },
      });
    });
    await screen.findByText("ok.png");
    fireEvent.change(screen.getByLabelText("Reply to thread t1"), {
      target: { value: "look at this" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Post" }));

    // The typed error renders on the card, and nothing was added to the thread.
    expect(await screen.findByRole("alert")).toBeInTheDocument();
    expect(screen.getByLabelText("Reply to thread t1")).toHaveValue(
      "look at this",
    );
    expect(screen.getByText("ok.png")).toBeInTheDocument();
    expect(screen.getByText("bad.png")).toBeInTheDocument();

    // Removing the offender and posting again succeeds, carrying the rest.
    fireEvent.click(
      screen.getByRole("button", { name: "Remove attachment bad.png" }),
    );
    backend.addCommentError = undefined;
    fireEvent.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() =>
      expect(backend.calls.filter((c) => c.cmd === "add_comment")).toHaveLength(2),
    );
    const posts = backend.calls.filter((c) => c.cmd === "add_comment");
    const last = posts[posts.length - 1];
    const sent = (last.args as { attachments: AttachmentInput[] }).attachments;
    expect(sent).toHaveLength(1);
    expect(sent[0]).toMatchObject({ filename: "ok.png" });
  });
});

describe("CMT-FR-45 … CMT-FR-47 on the draft card, which owns its own strip", () => {
  async function openDraftCard() {
    await openRail();
    selectInBody("the first session");
    fireEvent.click(
      await screen.findByRole("button", { name: "Comment on selection" }),
    );
    return screen.findByTestId("comment-draft");
  }

  it("carries an attachment into the thread it opens", async () => {
    const { backend } = await mount();
    const draft = await openDraftCard();

    await act(async () => {
      fireEvent.change(screen.getByTestId("comment-attach-file-input"), {
        target: { files: [pngFile("first.png")] },
      });
    });
    expect(await screen.findByText("first.png")).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("New comment"), {
      target: { value: "needs tightening" },
    });
    fireEvent.click(within(draft).getByRole("button", { name: "Post" }));

    await waitFor(() =>
      expect(backend.calls.some((c) => c.cmd === "open_discussion")).toBe(true),
    );
    const args = backend.calls.find((c) => c.cmd === "open_discussion")!
      .args as { attachments: AttachmentInput[] };
    expect(args.attachments).toHaveLength(1);
    expect(args.attachments[0]).toMatchObject({
      kind: "inline",
      filename: "first.png",
      mediaType: "image/png",
    });
  });

  it("attaches a dropped file and discards the strip when cancelled", async () => {
    const { backend } = await mount();
    const draft = await openDraftCard();

    await act(async () => {
      fireEvent.drop(screen.getByLabelText("New comment").parentElement!, {
        dataTransfer: { files: [pngFile("dropped.png")] },
      });
    });
    expect(await screen.findByText("dropped.png")).toBeInTheDocument();

    fireEvent.click(within(draft).getByRole("button", { name: "Cancel" }));
    await waitFor(() =>
      expect(screen.queryByTestId("comment-draft")).not.toBeInTheDocument(),
    );
    expect(screen.queryByText("dropped.png")).not.toBeInTheDocument();
    expect(backend.calls.some((c) => c.cmd === "open_discussion")).toBe(false);
  });
});

describe("CMT-FR-04: a new thread arriving twice is still one card", () => {
  it("does not double-insert when the event beats the command's reply", async () => {
    // `open_comment_thread` emits `"discussion changed"` from inside the
    // command, so the event can land before the `await` resolves and there is no
    // ordering between the two. A blind append would render the same thread as
    // two cards sharing one id — and every later update would only reach the
    // first copy.
    const { backend } = await mount();
    await openRail();

    let created: Discussion | undefined;
    backend.onOpenThread = () => {
      created = makeThread({ id: "t-new" });
      // The backend announces it before this handler's value is returned.
      emitEvent("discussion-changed", created);
      return created;
    };

    selectInBody("the first session");
    fireEvent.click(
      await screen.findByRole("button", { name: "Comment on selection" }),
    );
    fireEvent.change(screen.getByLabelText("New comment"), {
      target: { value: "needs tightening" },
    });
    fireEvent.click(
      within(await screen.findByTestId("comment-draft")).getByRole("button", {
        name: "Post",
      }),
    );

    await waitFor(() =>
      expect(screen.queryByTestId("comment-draft")).not.toBeInTheDocument(),
    );
    expect(screen.getAllByTestId("comment-thread-t-new")).toHaveLength(1);

    // And the single card still tracks later updates, which a duplicate would
    // have broken by leaving the stale copy first in the list.
    act(() =>
      emitEvent("discussion-changed", {
        ...created!,
        comments: [
          ...created!.comments,
          {
            id: "c-later",
            author: human("octocat"),
            body: "a later reply",
            quotes: [],
            attachments: [],
            createdAt: "2026-01-03T00:00:00Z",
          },
        ],
      }),
    );
    expect(await screen.findByText("a later reply")).toBeInTheDocument();
    expect(screen.getAllByTestId("comment-thread-t-new")).toHaveLength(1);
  });
});

describe("CMT-FR-32: the attach menu is one of the card's transient surfaces", () => {
  const attachOpen = () =>
    screen.queryByRole("menuitem", { name: "Attach a file…" }) !== null;

  it("dismisses on Escape and on an outside pointer-down, invoking nothing", async () => {
    const { backend } = await mount({ threads: [makeThread()] });
    await openRail();
    const before = backend.calls.length;

    fireEvent.click(screen.getByTestId("comment-attach-button"));
    expect(attachOpen()).toBe(true);
    fireEvent.keyDown(document, { key: "Escape" });
    await waitFor(() => expect(attachOpen()).toBe(false));
    // Escape puts focus back on the control rather than dropping it to <body>,
    // so a keyboard user's next Tab resumes where they were.
    expect(document.activeElement).toBe(
      screen.getByTestId("comment-attach-button"),
    );

    fireEvent.click(screen.getByTestId("comment-attach-button"));
    expect(attachOpen()).toBe(true);
    fireEvent.mouseDown(document.body);
    await waitFor(() => expect(attachOpen()).toBe(false));

    // The session log's own batched flush is not part of the feature and lands
    // on its own timer (per `../../specifications/core/LGC-logging.md`).
    expect(
      backend.calls
        .slice(before)
        .filter((c: { cmd: string }) => c.cmd !== "append_log_records"),
    ).toEqual([]);
  });

  it("is mutually exclusive with the card's overflow menu", async () => {
    await mount({ threads: [makeThread()] });
    await openRail();

    // Opening the overflow menu dismisses the attach menu…
    fireEvent.click(screen.getByTestId("comment-attach-button"));
    expect(attachOpen()).toBe(true);
    fireEvent.click(screen.getByRole("button", { name: /Thread actions/ }));
    await waitFor(() => expect(attachOpen()).toBe(false));
    expect(screen.getByRole("menu")).toBeInTheDocument();

    // …and opening the attach menu dismisses the overflow menu.
    fireEvent.click(screen.getByTestId("comment-attach-button"));
    await waitFor(() => expect(attachOpen()).toBe(true));
    expect(
      screen.queryByRole("menuitem", { name: "Lock thread" }),
    ).not.toBeInTheDocument();
  });

  it("returns focus to the control when the link form is confirmed", async () => {
    await mount({ threads: [makeThread()] });
    await openRail();

    fireEvent.click(screen.getByTestId("comment-attach-button"));
    fireEvent.click(screen.getByRole("menuitem", { name: "Attach a link…" }));
    fireEvent.change(screen.getByLabelText("Attachment address"), {
      target: { value: "https://example.test/a.png" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Attach link" }));

    await screen.findByText("https://example.test/a.png");
    expect(document.activeElement).toBe(
      screen.getByTestId("comment-attach-button"),
    );
  });
});
