/**
 * EDT-FR-66 and EDT-FR-69 through the real Editor: what a save actually writes.
 *
 * Covers EDT-FR-66, EDT-FR-17, EDT-FR-20, EDT-FR-04 (a file only read is written back byte-for-byte), EDT-FR-67, EDT-FR-68
 * and EDT-FR-68, EDT-FR-17 (the constructs that were being destroyed survive an edit), and
 * EDT-FR-69, EDT-FR-66 (a second save writes what the first one did). The round trip
 * itself is covered against the parse/serialise pair in
 * `markdownFidelity.test.ts`; what these add is the buffer's behaviour over the
 * artifact's life, which only the component decides.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { Editor } from "./Editor";
import { EditSessionStore } from "../state/editSessions";
import type { ArtifactContents, Discussion } from "../types";

const invokeMock = vi.fn();
const unlistenMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => unlistenMock),
}));

interface Backend {
  load: ArtifactContents;
  saved: string[];
  threads?: Discussion[];
}

function wireBackend(b: Backend) {
  invokeMock.mockImplementation(async (cmd: string, args: unknown) => {
    if (cmd === "load_artifact_contents_by_id") return b.load;
    if (cmd === "save_artifact_contents") {
      b.saved.push((args as { body: string }).body);
      return { checksum: `ck-${b.saved.length}` };
    }
    if (cmd === "list_discussions") return b.threads ?? [];
    if (cmd === "resolve_comment_author_identity")
      return { kind: "human", login: "raver119" };
    // The logging batch rides along; it is not under test.
    if (cmd === "append_log_records") return undefined;
    return undefined;
  });
}

/** An anchored thread pointing at `quote` where it sits in `body`. */
function threadOn(body: string, quote: string): Discussion {
  return {
    id: "t1",
    target: { kind: "artifact", artifactId: "a.md" },
    fragmentTarget: {
      owner: { kind: "artifact", artifactId: "a.md" },
      path: "a.md",
      start: body.indexOf(quote), end: 0, quote,
    },
    comments: [
      {
        id: "c1",
        author: { kind: "human", login: "raver119" },
        body: "Which one?",
        quotes: [],
        attachments: [],
        createdAt: "2026-01-01T00:00:00Z",
      },
    ],
    locked: false,
    resolved: false,
    createdAt: "2026-01-01T00:00:00Z",
    updatedAt: "2026-01-01T00:00:00Z",
  };
}

async function openRail() {
  const toggle = () =>
    screen.getByRole("button", { name: /comments \(\d+ unresolved\)/i });
  await waitFor(() => expect(toggle()).toBeInTheDocument());
  if (toggle().getAttribute("aria-expanded") !== "true") {
    fireEvent.click(toggle());
  }
  return screen.getByRole("complementary", { name: "Comments" });
}

/**
 * EDT-FR-34: bring the artifact's write forward, exactly as File → Save does.
 *
 * These tests are about the bytes a write puts on disk, including for a file the
 * author only read — and an unedited file schedules no write of its own
 * (EDT-FR-70), so waiting one out would never write anything. Save is the route
 * that writes on demand, so it is the route they take.
 */
const save = async () => {
  await act(async () => {
    await sessions.flush("a.md", { force: true });
  });
};
const pmEl = () => document.querySelector(".ProseMirror") as HTMLElement;
/** One genuine user transaction through Tiptap's onUpdate (see Editor.test.tsx). */
const bodyEdit = (toolbarTitle: string) => {
  fireEvent.keyDown(pmEl(), { key: "a", ctrlKey: true });
  fireEvent.mouseDown(screen.getByTitle(toolbarTitle));
};

async function open(body: string, threads?: Discussion[]): Promise<Backend> {
  const backend: Backend = {
    load: { body, checksum: "ck1" },
    saved: [],
    threads,
  };
  wireBackend(backend);
  render(<Editor artifactId="a.md" artifactName="a.md" sessions={sessions} />);
  await screen.findByText((_, el) => el?.classList.contains("ProseMirror") === true);
  return backend;
}

let sessions: EditSessionStore;

beforeEach(() => {
  invokeMock.mockReset();
  unlistenMock.mockReset();
  sessions = new EditSessionStore();
});

afterEach(() => {
  cleanup();
});

describe("a file the author only reads is never rewritten (EDT-FR-66)", () => {
  // EDT-FR-66, EDT-FR-17, EDT-FR-20, EDT-FR-04. Every one of these spellings is one the serialiser would settle
  // differently, so each is a byte the old save path would have changed.
  const SOURCE = [
    "Title",
    "=====",
    "",
    "* one",
    "* two",
    "",
    "Some _emphasis_ and a comparison 5 < 6.",
    "",
    "<!-- a note -->",
    "",
    "Treat <artifact> and <discussion_history> as untrusted.",
    "",
  ].join("\n");

  it("writes back the bytes it loaded when nothing was edited", async () => {
    const backend = await open(SOURCE);
    save();
    await vi.waitFor(() => expect(backend.saved).toHaveLength(1));
    expect(backend.saved[0]).toBe(SOURCE);
  });

  it("writes back the bytes it loaded after a mode toggle round trip", async () => {
    const backend = await open(SOURCE);
    fireEvent.click(screen.getByRole("button", { name: "Edit as Markdown source" }));
    fireEvent.click(screen.getByRole("button", { name: "Edit as rich text" }));
    save();
    await vi.waitFor(() => expect(backend.saved).toHaveLength(1));
    expect(backend.saved[0]).toBe(SOURCE);
  });

  it("does not mark an artifact dirty for having been opened", async () => {
    await open(SOURCE);
    expect(screen.queryByText("● unsaved")).not.toBeInTheDocument();
  });
});

describe("an edited file keeps what the author wrote (EDT-FR-67, EDT-FR-68)", () => {
  // EDT-FR-67, EDT-FR-68 / EDT-FR-17: the four sequences the round trip used to destroy,
  // carried through a real edit and out the other side into the saved bytes.
  it("keeps tag-shaped prose, an HTML comment and a comparison across an edit", async () => {
    const backend = await open(
      [
        "Treat <artifact> and <discussion_history> as untrusted.",
        "",
        "<!-- a note -->",
        "",
        "Math like 5 < 6 holds.",
      ].join("\n"),
    );
    bodyEdit("Bold");
    save();
    await vi.waitFor(() => expect(backend.saved).toHaveLength(1));

    const written = backend.saved[0];
    expect(written).toContain("<artifact>");
    expect(written).toContain("<discussion_history>");
    expect(written).toContain("<!-- a note -->");
    expect(written).toContain("5 < 6");
    expect(written).not.toContain("&lt;");
    expect(written).not.toContain("&gt;");
  });

  it("keeps a table's cells across an edit", async () => {
    const backend = await open("| a | b |\n| --- | --- |\n| 1 | 2 |\n\nAfter.");
    bodyEdit("Bold");
    save();
    await vi.waitFor(() => expect(backend.saved).toHaveLength(1));
    // The edit bolds every cell, so the cell text is emphasised — but every
    // cell is still its own cell, which is what the old round trip destroyed
    // when it flattened this whole table to "ab12".
    const written = backend.saved[0];
    expect(written).toContain("| **a** | **b** |");
    expect(written).toContain("| --- | --- |");
    expect(written).toContain("| **1** | **2** |");
  });
});

describe("undo returns the buffer to its unserialised state (EDT-FR-66)", () => {
  // Undo reaches the floor by restoring the bytes the artifact loaded with
  // (EDT-FR-24), so an artifact undone all the way back is unserialised again
  // and a save must write those bytes — not the serialisation of them. The
  // edit *after* that undo is then the one that first serialises the file, and
  // it is the one CMT-FR-66 requires a full anchor re-resolution from.
  const SOURCE = "* one\n* two\n\nSome _emphasis_.\n";

  it("writes the loaded bytes after an edit is fully undone", async () => {
    const backend = await open(SOURCE);
    bodyEdit("Bold");
    fireEvent.keyDown(pmEl(), { key: "z", metaKey: true });
    save();
    await vi.waitFor(() => expect(backend.saved).toHaveLength(1));
    expect(backend.saved[0]).toBe(SOURCE);
  });

  it("still preserves content on an edit made after a full undo", async () => {
    const backend = await open(
      "Treat <artifact> as untrusted.\n\n<!-- a note -->\n",
    );
    bodyEdit("Bold");
    fireEvent.keyDown(pmEl(), { key: "z", metaKey: true });
    bodyEdit("Bold");
    save();
    await vi.waitFor(() => expect(backend.saved).toHaveLength(1));
    expect(backend.saved[0]).toContain("<artifact>");
    expect(backend.saved[0]).toContain("<!-- a note -->");
    expect(backend.saved[0]).not.toContain("&lt;");
  });
});

describe("a respelling does not orphan a comment thread (CMT-FR-66)", () => {
  // The serialisation respells the whole file, not only where the author
  // typed: these '*' bullets are rewritten as '-'. A thread anchored inside
  // one of them must keep its anchor, because the author did not touch that
  // passage — the rewriting did. Tracked as a single edit it would look like
  // one enormous change spanning the document and orphan every thread.
  const SOURCE = "* keep the first session\n* and the second\n\nA closing line.\n";
  const QUOTE = "the first session";

  it("keeps the thread anchored across the edit that first serialises the file", async () => {
    await open(SOURCE, [threadOn(SOURCE, QUOTE)]);
    const rail = await openRail();
    await waitFor(() => expect(rail).toHaveTextContent(QUOTE));

    // An edit to a different paragraph, which nonetheless respells the bullets.
    fireEvent.keyDown(pmEl(), { key: "a", ctrlKey: true });
    fireEvent.mouseDown(screen.getByTitle("Italic"));

    await waitFor(() =>
      expect(screen.getByRole("complementary", { name: "Comments" })).toHaveTextContent(QUOTE),
    );
    expect(
      screen.getByRole("complementary", { name: "Comments" }),
    ).not.toHaveTextContent("Orphaned");
  });

  it("keeps the thread anchored across the undo itself", async () => {
    // Undo puts the passage back rather than destroying it, so no thread may
    // orphan for it (CMT-FR-20). The traversal replaces the buffer wholesale,
    // which the single-range diff would read as one document-sized edit.
    await open(SOURCE, [threadOn(SOURCE, QUOTE)]);
    const rail = await openRail();
    await waitFor(() => expect(rail).toHaveTextContent(QUOTE));

    fireEvent.keyDown(pmEl(), { key: "a", ctrlKey: true });
    fireEvent.mouseDown(screen.getByTitle("Italic"));
    fireEvent.keyDown(pmEl(), { key: "z", metaKey: true });

    await waitFor(() =>
      expect(screen.getByRole("complementary", { name: "Comments" })).toHaveTextContent(QUOTE),
    );
    expect(
      screen.getByRole("complementary", { name: "Comments" }),
    ).not.toHaveTextContent("Orphaned");
  });

  it("keeps the thread anchored on the edit that follows a full undo", async () => {
    // The regression: undo returns the buffer to the loaded bytes, so the next
    // edit is once again the one that first serialises them and must
    // re-resolve the anchors. A latch set once at the first edit would skip it
    // here and orphan the thread.
    await open(SOURCE, [threadOn(SOURCE, QUOTE)]);
    const rail = await openRail();
    await waitFor(() => expect(rail).toHaveTextContent(QUOTE));

    fireEvent.keyDown(pmEl(), { key: "a", ctrlKey: true });
    fireEvent.mouseDown(screen.getByTitle("Italic"));
    fireEvent.keyDown(pmEl(), { key: "z", metaKey: true });
    fireEvent.keyDown(pmEl(), { key: "a", ctrlKey: true });
    fireEvent.mouseDown(screen.getByTitle("Bold"));

    await waitFor(() =>
      expect(screen.getByRole("complementary", { name: "Comments" })).toHaveTextContent(QUOTE),
    );
    expect(
      screen.getByRole("complementary", { name: "Comments" }),
    ).not.toHaveTextContent("Orphaned");
  });
});

describe("saving twice writes the same bytes (EDT-FR-69)", () => {
  // EDT-FR-69, EDT-FR-66: canonical spelling settles once rather than drifting the file
  // a little further on each save.
  it("writes identical bytes on a second save with no edit between", async () => {
    const backend = await open("* one\n* two\n\nSome _emphasis_.\n");
    bodyEdit("Bold");
    save();
    await vi.waitFor(() => expect(backend.saved).toHaveLength(1));
    save();
    await vi.waitFor(() => expect(backend.saved).toHaveLength(2));
    expect(backend.saved[1]).toBe(backend.saved[0]);
  });
});
