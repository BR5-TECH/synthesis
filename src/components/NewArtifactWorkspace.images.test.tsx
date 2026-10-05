/**
 * NAW-FR-13, NAW-FR-50, NAW-FR-51 … NAW-FR-06, NAW-FR-26, NAW-FR-56, DRS-FR-21 — images in a draft's prompt
 * (`../../specifications/ui/NAW-new-artifact.md` NAW-FR-50 … NAW-FR-56).
 *
 * The tab's own half of image support: what it invokes when a picture is pasted
 * or dropped, what it inserts, what it says when a store is refused, what it
 * does when the write that carries a reference fails, and when it triggers the
 * draft's housekeeping. The surface's half — the placeholder, the removal
 * action, and the paste offer itself — is `Editor.images.test.tsx`'s.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";

const invokeMock = vi.fn();
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

import { NewArtifactWorkspace, imageRefusal } from "./NewArtifactWorkspace";
import { DraftSessionStore } from "../state/draftSessions";

const PROMPT = "artifact-window.md";

interface Options {
  /** What `store_draft_image` does — an asset, or a typed refusal to throw. */
  store?: { ok: true } | { ok: false; error: string };
  /** Whether `save_draft_file_contents` fails, which NAW-FR-51 turns on. */
  saveFails?: boolean;
  /** Whether `sweep_draft_assets` fails, which NAW-FR-55 turns on. */
  sweepFails?: boolean;
  /**
   * Whether `discard_draft_image` is itself refused, which NAW-FR-51's last
   * sentence turns on: the prompt still carries no reference, the author still
   * reads the write's own failure, and the orphaned asset is left for the next
   * housekeeping pass.
   */
  discardFails?: boolean;
}

function stub(opts: Options = {}) {
  invokeMock.mockImplementation(async (cmd: string) => {
    switch (cmd) {
      case "open_draft":
        return {
          id: "d1",
          name: "artifact-window",
          promptPath: PROMPT,
          status: "active",
          createdAt: "2026-07-20T08:00:00Z",
          updatedAt: "2026-07-20T08:00:00Z",
        };
      case "get_draft_graduation":
        return null;
      case "list_draft_history":
        return {
          entries: [],
          live: { path: PROMPT, byteLen: 10, sha256: "s", matchesLatest: true },
        };
      case "load_draft_file_contents":
        return { body: "body text", checksum: "c1" };
      case "save_draft_file_contents":
        if (opts.saveFails) throw "write_failed";
        return { checksum: "c2" };
      case "store_draft_image":
        if (opts.store && !opts.store.ok) throw opts.store.error;
        return {
          path: "assets/abc123.png",
          reference: "../assets/abc123.png",
          mediaType: "image/png",
          filename: "shot.png",
          bytes: 12,
        };
      case "read_draft_image":
        return { mediaType: "image/png", data: "AAAA" };
      case "discard_draft_image":
        if (opts.discardFails) throw "asset_cleanup_failed:delete_refused";
        return { discarded: true, retained: false };
      case "sweep_draft_assets":
        if (opts.sweepFails) throw "sweep_failed";
        return { scheduled: true, coalesced: false };
      case "list_draft_change_proposals":
        return [];
      default:
        return undefined;
    }
  });
}

/**
 * The tab writes the prompt on the edit that carries an insertion (NAW-FR-51),
 * and a write happens only for a document the session holds as dirty — which is
 * what the real surface's own edit does. This is that edit: the buffer moves,
 * so the flush the tab performs has something to write.
 */
function markPromptEdited(drafts: DraftSessionStore, body: string) {
  drafts.docs.update(`d1/${PROMPT}`, { buffer: body, dirty: true });
}

function renderWorkspace(drafts = new DraftSessionStore()) {
  return render(
    <NewArtifactWorkspace
      draftId="d1"
      drafts={drafts}
      name="artifact-window"
      onDraftChanged={vi.fn()}
      draftsRevision={0}
      onGraduationStarted={vi.fn()}
      onOpenRun={vi.fn()}
      onArchived={vi.fn()}
      onNameChanged={vi.fn()}
    />,
  );
}

/** Every command name the tab has invoked so far. */
function invoked(): string[] {
  return invokeMock.mock.calls.map((c) => String(c[0]));
}

function callsTo(name: string): unknown[][] {
  return invokeMock.mock.calls.filter((c) => c[0] === name).map((c) => c.slice(1));
}

beforeEach(() => {
  invokeMock.mockReset();
  listeners.clear();
  // The host is captured from whichever tab is mounted, so it has to be
  // forgotten between tests — an object left over from an unmounted tree
  // invokes the right commands and updates a tree nothing is rendering.
  capturedHost = null;
  capturedEdit = null;
  capturedAnnounce = null;
});
afterEach(cleanup);

// ---------------------------------------------------------------------------
// NAW-FR-55 — housekeeping is triggered on open and on close (NAW-FR-55)
// ---------------------------------------------------------------------------

describe("NAW-FR-55: the draft's asset housekeeping", () => {
  it("NAW-FR-55: is triggered when the draft opens and when it closes, and nothing waits on either", async () => {
    stub();
    const view = renderWorkspace();
    await screen.findByRole("textbox", { name: /markdown|prompt/i }).catch(() => null);
    await waitFor(() =>
      expect(callsTo("sweep_draft_assets")).toHaveLength(1),
    );
    expect(callsTo("sweep_draft_assets")[0][0]).toEqual({ draftId: "d1" });

    view.unmount();
    // The close-triggered call is made before the tab releases the draft.
    expect(callsTo("sweep_draft_assets")).toHaveLength(2);
  });

  it("NAW-FR-55: a pass that fails changes nothing about the open or the close", async () => {
    stub({ sweepFails: true });
    const view = renderWorkspace();
    await waitFor(() => expect(callsTo("sweep_draft_assets")).toHaveLength(1));
    // The tab still opened: its chrome is rendered and no dialog stands over it.
    expect(await screen.findByText("artifact-window")).toBeInTheDocument();
    expect(screen.queryByRole("alertdialog")).toBeNull();
    expect(() => view.unmount()).not.toThrow();
    expect(callsTo("sweep_draft_assets")).toHaveLength(2);
  });

  it("NAW-FR-55: the answer is a trigger and not a query — nothing is rendered from it", async () => {
    stub();
    renderWorkspace();
    await waitFor(() => expect(callsTo("sweep_draft_assets")).toHaveLength(1));
    // Nothing anywhere in the tab reports what a pass found: no count, no
    // tally, and no second call asking for an outcome.
    expect(document.body.textContent).not.toMatch(/scanned|removed|retained/i);
    expect(invoked().filter((c) => c === "sweep_draft_assets")).toHaveLength(1);
  });
});

// ---------------------------------------------------------------------------
// NAW-FR-13, NAW-FR-50, NAW-FR-51 — what an insertion invokes and what a refusal says
// ---------------------------------------------------------------------------

describe("NAW-FR-50 / NAW-FR-51: storing a pasted image", () => {
  /**
   * The tab's `imageHost` is what the surface is handed, and it is the contract
   * this suite is about: what it invokes, what Markdown it answers with, and
   * what it does when the store or the write fails. Reached through the Editor
   * the tab mounts rather than through a prop of the tab's own, because the tab
   * does not export it — the surface asks for it, and this is the same object
   * the surface would be given.
   */
  async function host(): Promise<{
    accept: (image: {
      mediaType: string;
      filename: string | null;
      data: string;
    }) => Promise<string | null>;
    resolve: (destination: string) => Promise<string | null>;
  }> {
    const captured = await waitFor(() => {
      const value = capturedHost;
      expect(value).not.toBeNull();
      return value!;
    });
    return captured;
  }

  it("NAW-FR-13, NAW-FR-50, NAW-FR-51: invokes the store once and answers with the destination it returned", async () => {
    stub();
    renderWorkspace();
    const imageHost = await host();

    const markdown = await imageHost.accept({
      mediaType: "image/png",
      filename: "shot.png",
      data: "AAAA",
    });

    expect(callsTo("store_draft_image")).toHaveLength(1);
    expect(callsTo("store_draft_image")[0][0]).toEqual({
      draftId: "d1",
      mediaType: "image/png",
      filename: "shot.png",
      data: "AAAA",
    });
    // NAW-FR-50: the returned destination, inserted as ordinary Markdown. The
    // tab derives no path of its own — the reference is the store's answer.
    expect(markdown).toBe("![](../assets/abc123.png)");
  });

  it("NAW-FR-51: a refused store inserts nothing and renders its typed error inline", async () => {
    stub({ store: { ok: false, error: "unsupported_media_type" } });
    renderWorkspace();
    const imageHost = await host();

    let markdown: string | null = "unset";
    await act(async () => {
      markdown = await imageHost.accept({
        mediaType: "image/tiff",
        filename: "scan.tiff",
        data: "AAAA",
      });
    });

    // No reference is inserted, and the prompt is byte-for-byte what it was —
    // no write was even scheduled for it.
    expect(markdown).toBeNull();
    expect(callsTo("save_draft_file_contents")).toHaveLength(0);
    // The typed error renders where the author is looking.
    expect(await screen.findByRole("alert")).toHaveTextContent(/not an image/i);
  });

  it("NAW-FR-51: a store beyond the size bound says so in its own words", async () => {
    stub({ store: { ok: false, error: "image_too_large" } });
    renderWorkspace();
    const imageHost = await host();
    await act(async () => {
      expect(
        await imageHost.accept({
          mediaType: "image/png",
          filename: null,
          data: "A",
        }),
      ).toBeNull();
    });
    expect(await screen.findByRole("alert")).toHaveTextContent(/larger than/i);
  });

  it("NAW-FR-52: a destination the draft owns resolves to drawable content and one it does not resolves to nothing", async () => {
    stub();
    renderWorkspace();
    const imageHost = await host();

    expect(await imageHost.resolve("../assets/abc123.png")).toBe(
      "data:image/png;base64,AAAA",
    );
    expect(callsTo("read_draft_image")[0][0]).toEqual({
      draftId: "d1",
      path: "../assets/abc123.png",
    });

    invokeMock.mockImplementationOnce(async () => {
      throw "asset_not_found";
    });
    // A destination that resolves to no draft-owned asset answers `null`, and
    // the surface renders its non-blocking placeholder for it.
    expect(await imageHost.resolve("../assets/missing.png")).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// NAW-FR-51 clause 2 — a write that fails after a successful store
// ---------------------------------------------------------------------------

describe("NAW-FR-51: a write that fails after a successful store", () => {
  it("NAW-FR-51: discards the asset, so the insertion leaves neither a reference nor an image", async () => {
    stub({ saveFails: true });
    const drafts = new DraftSessionStore();
    renderWorkspace(drafts);
    const imageHost = await waitFor(() => {
      expect(capturedHost).not.toBeNull();
      return capturedHost!;
    });

    // The store succeeds and the tab answers with the destination to insert.
    let markdown: string | null = null;
    await act(async () => {
      markdown = await imageHost.accept({
        mediaType: "image/png",
        filename: "shot.png",
        data: "AAAA",
      });
    });
    expect(markdown).toBe("![](../assets/abc123.png)");

    // The surface applies it and reports the edit, exactly as the real one
    // does — which is what makes the tab write the prompt immediately rather
    // than after the ordinary rest (NAW-FR-51).
    await act(async () => {
      markPromptEdited(drafts, `body text\n${markdown}`);
      capturedEdit?.();
    });

    await waitFor(() => expect(callsTo("save_draft_file_contents").length).toBeGreaterThan(0));
    // The write failed, so the asset is taken back: a failed insertion leaves
    // neither a reference to a missing image nor an image nothing references.
    await waitFor(() => expect(callsTo("discard_draft_image")).toHaveLength(1));
    expect(callsTo("discard_draft_image")[0][0]).toEqual({
      draftId: "d1",
      path: "assets/abc123.png",
    });
  });

  it("NAW-FR-13, NAW-FR-50, NAW-FR-51: a write that lands discards nothing", async () => {
    stub();
    const drafts = new DraftSessionStore();
    renderWorkspace(drafts);
    const imageHost = await waitFor(() => {
      expect(capturedHost).not.toBeNull();
      return capturedHost!;
    });
    await act(async () => {
      await imageHost.accept({
        mediaType: "image/png",
        filename: "shot.png",
        data: "AAAA",
      });
    });
    await act(async () => {
      markPromptEdited(drafts, "body text\n![](../assets/abc123.png)");
      capturedEdit?.();
    });
    await waitFor(() =>
      expect(callsTo("save_draft_file_contents").length).toBeGreaterThan(0),
    );
    // NAW-FR-51: written immediately rather than after the ordinary rest — the
    // saved prompt is what protects the asset from housekeeping.
    expect(callsTo("discard_draft_image")).toHaveLength(0);
  });

  it("NAW-FR-51: a discard that is itself refused changes none of it", async () => {
    stub({ saveFails: true, discardFails: true });
    const drafts = new DraftSessionStore();
    renderWorkspace(drafts);
    const imageHost = await waitFor(() => {
      expect(capturedHost).not.toBeNull();
      return capturedHost!;
    });
    await act(async () => {
      await imageHost.accept({
        mediaType: "image/png",
        filename: "shot.png",
        data: "AAAA",
      });
    });
    await act(async () => {
      markPromptEdited(drafts, "body text\n![](../assets/abc123.png)");
      capturedEdit?.();
    });
    await waitFor(() => expect(callsTo("discard_draft_image")).toHaveLength(1));
    // The author reads the write's own failure and nothing about the cleanup;
    // the orphaned asset is left for the next housekeeping pass.
    await waitFor(() =>
      expect(screen.getByTestId("draft-image-status").textContent).toMatch(
        /could not be written/i,
      ),
    );
    expect(document.body.textContent).not.toMatch(/cleanup/i);
  });
});

// ---------------------------------------------------------------------------
// NAW-FR-54 — every outcome is announced (NAW-FR-54)
// ---------------------------------------------------------------------------

describe("NAW-FR-54: outcomes are announced through the tab's own feedback", () => {
  it("NAW-FR-54: announces an insertion, a refusal, and a removal in one live region", async () => {
    stub();
    renderWorkspace();
    const imageHost = await waitFor(() => {
      expect(capturedHost).not.toBeNull();
      return capturedHost!;
    });

    const region = await screen.findByTestId("draft-image-status");
    // A polite live region, so a screen-reader user learns of an outcome
    // without hunting for it, and it is not a second `status` competing with
    // the action row's own.
    expect(region).toHaveAttribute("aria-live", "polite");
    expect(region).not.toHaveAttribute("role", "status");

    await act(async () => {
      await imageHost.accept({
        mediaType: "image/png",
        filename: "shot.png",
        data: "AAAA",
      });
    });
    expect(region.textContent).toMatch(/inserted/i);

    // A refusal is announced too, and says which kind of thing went wrong.
    stub({ store: { ok: false, error: "unsupported_media_type" } });
    await act(async () => {
      await imageHost.accept({
        mediaType: "image/tiff",
        filename: "scan.tiff",
        data: "AAAA",
      });
    });
    expect(region.textContent).toMatch(/not an image/i);

    // And a removal, which the surface reports through the same channel.
    await act(async () => {
      capturedAnnounce?.("Image reference removed.");
    });
    expect(region.textContent).toMatch(/removed/i);
  });
});

// ---------------------------------------------------------------------------
// NAW-FR-51 — the wording of each refusal
// ---------------------------------------------------------------------------

describe("NAW-FR-51: the refusals divide in what they tell the author", () => {
  it("says the image was not acceptable, the draft takes no write, or the store did not happen", () => {
    // Three different things to say, because they call for three different
    // responses (per `../../specifications/core/DAS-draft-assets.md`
    // DAS-FR-03, DAS-FR-04).
    expect(imageRefusal("unsupported_media_type")).toMatch(/not an image/i);
    expect(imageRefusal("image_too_large")).toMatch(/larger/i);
    expect(imageRefusal("malformed_image")).toMatch(/could not be read/i);
    expect(imageRefusal("draft_locked_by_graduation")).toMatch(
      /graduation holds it/i,
    );
    expect(imageRefusal("asset_store_failed")).toMatch(/try again/i);
    // Each is distinct, so an author can tell which of the three happened.
    const said = [
      imageRefusal("unsupported_media_type"),
      imageRefusal("image_too_large"),
      imageRefusal("malformed_image"),
      imageRefusal("draft_locked_by_graduation"),
      imageRefusal("asset_store_failed"),
    ];
    expect(new Set(said).size).toBe(said.length);
    // None of them names a path, an address, or an operating-system message.
    for (const message of said) {
      expect(message).not.toMatch(/\//);
      expect(message).not.toMatch(/os error/i);
    }
  });

  it("falls back to a plain statement for a refusal it does not know", () => {
    expect(imageRefusal("something_unexpected")).toMatch(/could not be inserted/i);
  });

  it("reads a typed value that carries its safe diagnostic category", () => {
    // DAS-FR-27: a value may arrive as `<value>:<category>`, the category being
    // for a reader of the log. The surface matches on the value, so the two
    // spellings say the same thing to the author.
    expect(imageRefusal("asset_store_failed:write_refused")).toBe(
      imageRefusal("asset_store_failed"),
    );
    expect(imageRefusal("asset_store_failed:folder_unavailable")).toMatch(/try again/i);
    // And the category itself never reaches the author.
    expect(imageRefusal("asset_store_failed:write_refused")).not.toMatch(
      /write_refused/,
    );
  });
});

// ---------------------------------------------------------------------------
// Capturing the host the tab hands its surface
// ---------------------------------------------------------------------------

let capturedEdit: (() => void) | null = null;
let capturedAnnounce: ((message: string) => void) | null = null;
let capturedHost:
  | {
      accept: (image: {
        mediaType: string;
        filename: string | null;
        data: string;
      }) => Promise<string | null>;
      resolve: (destination: string) => Promise<string | null>;
    }
  | null = null;

vi.mock("./Editor", () => ({
  Editor: (props: {
    imageHost?: {
      accept: (image: {
        mediaType: string;
        filename: string | null;
        data: string;
      }) => Promise<string | null>;
      resolve: (destination: string) => Promise<string | null>;
    };
    announce?: (message: string) => void;
    onEdit?: () => void;
  }) => {
    capturedHost = props.imageHost ?? null;
    // The real surface applies the host's answer as an ordinary edit and
    // reports it (EDT-FR-86), which is what starts the tab's write schedule.
    // The stub lends the test that same signal so the tab's own sequencing —
    // store, insert, write immediately, discard on failure — is exercised
    // rather than mocked away.
    capturedEdit = props.onEdit ?? null;
    capturedAnnounce = props.announce ?? null;
    return <div data-testid="editor-stub" />;
  },
}));
