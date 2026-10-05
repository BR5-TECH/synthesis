/**
 * EDT-FR-67, EDT-FR-68, EDT-FR-85 / EDT-FR-70, EDT-FR-86 — the surface's half of image support
 * (`../../specifications/ui/EDT-editor.md` EDT-FR-85, EDT-FR-86), together with
 * the removal action and the placeholder a New Artifact tab depends on
 * (`../../specifications/ui/NAW-new-artifact.md` NAW-FR-52 … NAW-FR-54).
 *
 * What is pinned here is the **surface's** behaviour given a host: that a
 * destination the host resolves is drawn, that one it does not resolves to a
 * labelled placeholder whose Markdown is preserved byte-for-byte, that image
 * data is offered to the host first and an Editor tab with no host takes none,
 * and that an insertion the host answers with lands as one edit with a
 * predictable selection.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, createEvent, fireEvent, render, screen, waitFor } from "@testing-library/react";

import { Editor } from "./Editor";
import { EditSessionStore } from "../state/editSessions";
import {
  IMAGE_LABEL_CLASS,
  IMAGE_UNRESOLVED_CLASS,
  imageFromTransfer,
  selectAdjacentImage,
  selectedImageAt,
  transferHasText,
  unresolvedImageLabel,
} from "./hostImages";
import type { Node as PMNode } from "@tiptap/pm/model";
import { EditorState, TextSelection } from "@tiptap/pm/state";
import type { ArtifactContents } from "../types";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));

function wireLoad(load: ArtifactContents) {
  invokeMock.mockImplementation(async (cmd: string) => {
    if (cmd === "load_artifact_contents_by_id") return load;
    if (cmd === "save_artifact_contents") return { checksum: "ck" };
    return undefined;
  });
}

const DRAWN = "data:image/png;base64,AAAA";

interface HostSpies {
  resolve: ((destination: string) => Promise<string | null>) & {
    mock: { calls: unknown[][] };
  };
  accept: ((image: {
    mediaType: string;
    filename: string | null;
    data: string;
  }) => Promise<string | null>) & { mock: { calls: unknown[][] } };
}

function hostThatResolves(only: string[], markdown: string | null = null): HostSpies {
  return {
    resolve: vi.fn(async (destination: string) =>
      only.includes(destination) ? DRAWN : null,
    ) as unknown as HostSpies["resolve"],
    accept: vi.fn(async () => markdown) as unknown as HostSpies["accept"],
  };
}

function mountEditor(host?: HostSpies) {
  return render(
    <Editor
      artifactId="a.md"
      artifactName="a.md"
      sessions={new EditSessionStore()}
      showComments={false}
      showActions={false}
      imageHost={host}
    />,
  );
}

/** Every `<img>` the rich surface has rendered. */
function images(): HTMLImageElement[] {
  return Array.from(
    document.querySelectorAll<HTMLImageElement>(".editor__prose img"),
  ).filter((img) => !img.classList.contains("ProseMirror-separator"));
}

/**
 * A `DataTransfer` stand-in carrying one image file. jsdom has no real
 * clipboard, and what the surface reads is `files`, `items`, and `types` — so
 * this supplies exactly those.
 */
function imageTransfer(withText = false): DataTransfer {
  const file = new File([new Uint8Array([1, 2, 3])], "shot.png", {
    type: "image/png",
  });
  return {
    files: [file],
    items: [{ kind: "file", type: "image/png", getAsFile: () => file }],
    types: withText ? ["Files", "text/plain"] : ["Files"],
    getData: () => "",
  } as unknown as DataTransfer;
}

/**
 * The ProseMirror document the mounted surface currently holds, read off the
 * DOM node the view is attached to. The view exposes itself there, which is how
 * a test reaches the document without the component exporting its editor.
 */
/** A caret at `pos`, for driving the selection helpers directly. */
function TextSelectionAt(state: EditorState, pos: number): TextSelection {
  return TextSelection.create(state.doc, pos);
}

/** The ProseMirror state the mounted surface currently holds. */
function capturedState(): EditorState {
  const prose = document.querySelector(".editor__prose") as unknown as {
    pmViewDesc?: { node: PMNode };
  };
  const doc = prose?.pmViewDesc?.node;
  if (!doc) throw new Error("the rich surface has no document yet");
  return EditorState.create({ doc, schema: doc.type.schema });
}

function capturedDoc(): PMNode {
  const prose = document.querySelector(".editor__prose") as unknown as {
    pmViewDesc?: { node: PMNode };
  };
  const doc = prose?.pmViewDesc?.node;
  if (!doc) throw new Error("the rich surface has no document yet");
  return doc;
}

beforeEach(() => {
  invokeMock.mockReset();
  // A default that answers with a promise. The logging facility batches and
  // flushes on a timer (`src/logging.ts`), so a flush can land after the test
  // that armed it has finished — against a bare `mockReset` that returns
  // `undefined`, which the flush then calls `.catch` on. Reported by Vitest as
  // an unhandled error, and unrelated to anything under test.
  invokeMock.mockImplementation(async () => undefined);
});
afterEach(cleanup);

// ---------------------------------------------------------------------------
// EDT-FR-67, EDT-FR-68, EDT-FR-85 — a destination the host resolves, and one it does not
// ---------------------------------------------------------------------------

describe("EDT-FR-85: an image's destination is resolved by the host", () => {
  it("EDT-FR-67, EDT-FR-68, EDT-FR-85: draws what the host resolves and asks it once per destination", async () => {
    wireLoad({
      body: "Text with ![a diagram](../assets/a.png) in it.\n",
      checksum: "c1",
    });
    const host = hostThatResolves(["../assets/a.png"]);
    mountEditor(host);

    await waitFor(() => expect(images()).toHaveLength(1));
    expect(host.resolve).toHaveBeenCalledWith("../assets/a.png");
    await waitFor(() => expect(images()[0].getAttribute("src")).toBe(DRAWN));
    // Asked once, not once per render: a destination already answered costs
    // nothing to keep showing.
    expect(host.resolve).toHaveBeenCalledTimes(1);
  });

  it("NAW-FR-52: asks only about the images that have come into view", async () => {
    // A picture is fetched when it is actually **shown** rather than when the
    // prompt loads, so a prompt carrying many of them costs nothing for the
    // ones the author has not scrolled to. Driven through a stub observer,
    // because jsdom lays nothing out and every element would otherwise be
    // either always visible or never.
    const observed: Element[] = [];
    let notify: ((entries: { target: Element; isIntersecting: boolean }[]) => void) | null =
      null;
    class StubObserver {
      constructor(callback: (entries: unknown[]) => void) {
        notify = callback as never;
      }
      observe(element: Element) {
        observed.push(element);
      }
      unobserve() {}
      disconnect() {}
    }
    const original = (globalThis as Record<string, unknown>).IntersectionObserver;
    (globalThis as Record<string, unknown>).IntersectionObserver = StubObserver;
    try {
      wireLoad({
        body: "![a](../assets/a.png)\n\n![b](../assets/b.png)\n",
        checksum: "c1",
      });
      const host = hostThatResolves(["../assets/a.png", "../assets/b.png"]);
      mountEditor(host);
      await waitFor(() => expect(images()).toHaveLength(2));
      await waitFor(() => expect(observed.length).toBe(2));
      // Nothing has been shown yet, so nothing has been asked for.
      expect(host.resolve).not.toHaveBeenCalled();

      // The first scrolls into view; only it is fetched.
      await act(async () => {
        notify?.([{ target: observed[0], isIntersecting: true }]);
      });
      await waitFor(() => expect(host.resolve).toHaveBeenCalledTimes(1));
      expect(host.resolve.mock.calls[0][0]).toBe("../assets/a.png");

      await act(async () => {
        notify?.([{ target: observed[1], isIntersecting: true }]);
      });
      await waitFor(() => expect(host.resolve).toHaveBeenCalledTimes(2));
    } finally {
      (globalThis as Record<string, unknown>).IntersectionObserver = original;
    }
  });

  it("EDT-FR-67, EDT-FR-68, EDT-FR-85: renders a labelled placeholder for a destination it does not resolve, and preserves the Markdown", async () => {
    const source = "Before ![a diagram](../assets/missing.png) after.\n";
    wireLoad({ body: source, checksum: "c1" });
    const host = hostThatResolves([]);
    mountEditor(host);

    await waitFor(() => expect(images()).toHaveLength(1));
    const placeholder = await waitFor(() => {
      const img = images()[0];
      expect(img.className).toContain(IMAGE_UNRESOLVED_CLASS);
      return img;
    });
    // NAW-FR-52 / EDT-FR-85: an accessible name that states the image could not
    // be loaded and names the destination — carried whether or not it is
    // focused, and never an empty frame.
    const label = placeholder.getAttribute("aria-label") ?? "";
    expect(label).toContain("could not be loaded");
    expect(label).toContain("../assets/missing.png");
    expect(label).toBe(unresolvedImageLabel("../assets/missing.png", "a diagram"));
    // EDT-FR-85: and a **sighted** reader sees which reference is broken — the
    // destination is drawn beside the frame as its own element rather than as
    // generated content on the image, which no browser paints. The label is
    // hidden from assistive technology, because the image itself already
    // carries the same words as its accessible name.
    const drawn = document.querySelector(`.${IMAGE_LABEL_CLASS}`);
    expect(drawn).not.toBeNull();
    expect(drawn?.textContent).toBe(
      unresolvedImageLabel("../assets/missing.png", "a diagram"),
    );
    expect(drawn?.getAttribute("aria-hidden")).toBe("true");
    // It is a sibling of the image rather than a child of it, which is the
    // whole reason it renders at all.
    expect(drawn?.tagName.toLowerCase()).toBe("span");
    expect(placeholder.contains(drawn)).toBe(false);
    // The surface is still editable, and the author can still type anywhere in
    // it — a placeholder is never an error dialog.
    expect(screen.queryByRole("alertdialog")).toBeNull();
    const prose = document.querySelector(".editor__prose") as HTMLElement;
    expect(prose.getAttribute("contenteditable")).toBe("true");

    // NAW-FR-52 / EDT-FR-85: and the Markdown behind it is preserved untouched,
    // so the author can repair it on either surface.
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /markdown|raw/i }));
    });
    const raw = document.querySelector(".editor__source") as HTMLTextAreaElement;
    expect(raw.value).toContain("![a diagram](../assets/missing.png)");
  });

  it("EDT-FR-85: a surface with no host resolves nothing and draws the destination as it stands", async () => {
    wireLoad({ body: "![a](../assets/a.png)\n", checksum: "c1" });
    mountEditor();
    await waitFor(() => expect(images()).toHaveLength(1));
    // Nothing overrode the `src`, because nothing was asked.
    expect(images()[0].getAttribute("src")).toBe("../assets/a.png");
  });
});

// ---------------------------------------------------------------------------
// EDT-FR-70, EDT-FR-86 — image data is offered to the host first
// ---------------------------------------------------------------------------

describe("EDT-FR-86: image data pasted or dropped is offered to the host first", () => {
  it("EDT-FR-70, EDT-FR-86: an Editor tab on a project file takes none of it", async () => {
    wireLoad({ body: "Some prose.\n", checksum: "c1" });
    mountEditor();
    await waitFor(() => expect(document.querySelector(".editor__prose")).toBeTruthy());
    const prose = document.querySelector(".editor__prose") as HTMLElement;

    const paste = createEvent.paste(prose, {
      clipboardData: imageTransfer(),
    });
    await act(async () => {
      fireEvent(prose, paste);
    });
    // The host took none of it, so the paste behaved as it does today and
    // nothing was written into the file.
    expect(paste.defaultPrevented).toBe(false);
    expect(images()).toHaveLength(0);
  });

  it("EDT-FR-70, EDT-FR-86: a host that answers with Markdown has it inserted at the cursor as one edit", async () => {
    wireLoad({ body: "Some prose.\n", checksum: "c1" });
    const host = hostThatResolves(
      ["../assets/new.png"],
      "![](../assets/new.png)",
    );
    mountEditor(host);
    await waitFor(() => expect(document.querySelector(".editor__prose")).toBeTruthy());
    const prose = document.querySelector(".editor__prose") as HTMLElement;

    await act(async () => {
      fireEvent(
        prose,
        createEvent.paste(prose, { clipboardData: imageTransfer() }),
      );
    });

    await waitFor(() => expect(host.accept).toHaveBeenCalledTimes(1));
    // The host was handed the image's own media type, its filename, and its
    // bytes base64-encoded.
    const offered = host.accept.mock.calls[0][0] as {
      mediaType: string;
      filename: string | null;
      data: string;
    };
    expect(offered.mediaType).toBe("image/png");
    expect(offered.filename).toBe("shot.png");
    expect(typeof offered.data).toBe("string");

    // And what it answered with is in the document: one image, drawn from what
    // the host resolved its destination to.
    await waitFor(() => expect(images()).toHaveLength(1));
    await waitFor(() => expect(images()[0].getAttribute("src")).toBe(DRAWN));
    expect(host.resolve).toHaveBeenCalledWith("../assets/new.png");

    // NAW-FR-50: the Markdown the host answered with is what the file now
    // carries — one reference, exactly as typed text would have been inserted.
    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: /markdown|raw/i }));
    });
    const raw = document.querySelector(".editor__source") as HTMLTextAreaElement;
    expect(raw.value).toContain("![](../assets/new.png)");
  });

  it("EDT-FR-86: a transfer carrying a text flavour is a copy of rendered content, and the text is what the fallback pastes", async () => {
    wireLoad({ body: "Some prose.\n", checksum: "c1" });
    const host = hostThatResolves([], "![](../assets/new.png)");
    mountEditor(host);
    await waitFor(() => expect(document.querySelector(".editor__prose")).toBeTruthy());
    const prose = document.querySelector(".editor__prose") as HTMLElement;

    await act(async () => {
      fireEvent(
        prose,
        createEvent.paste(prose, { clipboardData: imageTransfer(true) }),
      );
    });
    expect(host.accept).not.toHaveBeenCalled();
  });
});

// ---------------------------------------------------------------------------
// DAS-FR-11 — the removal action (NAW-FR-53)
// ---------------------------------------------------------------------------

describe("NAW-FR-53: the removal action a selected image carries", () => {
  it("is offered only where the host takes images at all", async () => {
    wireLoad({ body: "![a](../assets/a.png)\n", checksum: "c1" });
    mountEditor();
    await waitFor(() => expect(images()).toHaveLength(1));
    // An Editor tab on a project file has no image to remove, so it offers no
    // control for it (EDT-FR-86).
    expect(screen.queryByRole("button", { name: /remove image/i })).toBeNull();
  });

  it("NAW-FR-54: an image is selected from the keyboard, so the action is reachable without a pointer", async () => {
    // NAW-FR-54: ProseMirror walks the caret straight across an inline leaf
    // without selecting it, so arrowing over a picture would never raise the
    // action and the only route to it would be a mouse. Extending the selection
    // onto an image is what selects it.
    wireLoad({ body: "Before ![a](../assets/a.png) after.\n", checksum: "c1" });
    const host = hostThatResolves(["../assets/a.png"]);
    mountEditor(host);
    await waitFor(() => expect(images()).toHaveLength(1));

    const doc = capturedDoc();
    let imagePos = -1;
    doc.descendants((node: { type: { name: string } }, pos: number) => {
      if (node.type.name === "image") imagePos = pos;
      return true;
    });
    expect(imagePos).toBeGreaterThanOrEqual(0);

    const state = capturedState();
    // The caret immediately before the image: a step forward selects it.
    const before = state.apply(
      state.tr.setSelection(TextSelectionAt(state, imagePos)),
    );
    const forward = selectAdjacentImage(before, 1);
    expect(forward).not.toBeNull();
    expect(forward!.selection.from).toBe(imagePos);
    expect(forward!.selection.to).toBe(imagePos + 1);

    // And the caret immediately after it: a step back selects the same image.
    const after = state.apply(
      state.tr.setSelection(TextSelectionAt(state, imagePos + 1)),
    );
    const backward = selectAdjacentImage(after, -1);
    expect(backward).not.toBeNull();
    expect(backward!.selection.from).toBe(imagePos);

    // A caret that is beside no image is the surface's own arrow handling, and
    // this declines it rather than swallowing the key.
    const elsewhere = state.apply(state.tr.setSelection(TextSelectionAt(state, 1)));
    expect(selectAdjacentImage(elsewhere, 1)).toBeNull();
    expect(selectAdjacentImage(elsewhere, -1)).toBeNull();
  });

  it("NAW-FR-54: the control appears whichever side the caret approached the image from", async () => {
    // The asymmetry this guards against is invisible from one direction: a
    // selection extended *forward* onto an image leaves `from` exactly where
    // the caret already was, so a surface that recomputes on `from` alone shows
    // the control when the author arrives from the right and never when they
    // arrive from the left — while looking like a working feature either way.
    wireLoad({ body: "Before ![a](../assets/a.png) after.\n", checksum: "c1" });
    const host = hostThatResolves(["../assets/a.png"]);
    mountEditor(host);
    await waitFor(() => expect(images()).toHaveLength(1));

    const doc = capturedDoc();
    let imagePos = -1;
    doc.descendants((node: { type: { name: string } }, pos: number) => {
      if (node.type.name === "image") imagePos = pos;
      return true;
    });

    // Both gestures select the same image, and both must be able to raise the
    // action — which is a fact about the selection rather than about how it was
    // reached, so it is asserted over both ranges.
    const state = capturedState();
    const forward = selectAdjacentImage(
      state.apply(state.tr.setSelection(TextSelectionAt(state, imagePos))),
      1,
    );
    const backward = selectAdjacentImage(
      state.apply(state.tr.setSelection(TextSelectionAt(state, imagePos + 1))),
      -1,
    );
    for (const tr of [forward, backward]) {
      expect(tr).not.toBeNull();
      const { from, to } = tr!.selection;
      expect(selectedImageAt(doc, from, to)).not.toBeNull();
    }
    // And the two produce the identical range, so nothing downstream can tell
    // them apart — the surface has no excuse to treat one differently.
    expect(forward!.selection.from).toBe(backward!.selection.from);
    expect(forward!.selection.to).toBe(backward!.selection.to);
  });

  it("NAW-FR-53, DAS-FR-11: is offered over exactly one selected image and over nothing else", async () => {
    // The derivation itself, over a document built from the same Markdown the
    // surface would parse. Asserted here rather than through a pointer gesture,
    // because NAW-FR-54 requires the action not to depend on one — and because
    // what decides whether it is offered is the selection's shape rather than
    // how the selection was made.
    wireLoad({
      body: "Before ![a](../assets/a.png) after.\n",
      checksum: "c1",
    });
    const host = hostThatResolves(["../assets/a.png"]);
    mountEditor(host);
    await waitFor(() => expect(images()).toHaveLength(1));

    const doc = capturedDoc();
    // Find the one image node's position.
    let imagePos = -1;
    doc.descendants((node: { type: { name: string } }, pos: number) => {
      if (node.type.name === "image") imagePos = pos;
      return true;
    });
    expect(imagePos).toBeGreaterThanOrEqual(0);

    const selected = selectedImageAt(doc, imagePos, imagePos + 1);
    expect(selected).not.toBeNull();
    expect(selected?.src).toBe("../assets/a.png");
    // A caret elsewhere selects no image, so the action is not offered.
    expect(selectedImageAt(doc, 1, 1)).toBeNull();
    // And a selection spanning the picture and the prose around it is not one
    // image: removing it would take text the author never meant to lose.
    expect(selectedImageAt(doc, imagePos, imagePos + 4)).toBeNull();
  });
});

// ---------------------------------------------------------------------------
// The transfer helpers themselves
// ---------------------------------------------------------------------------

describe("EDT-FR-86: reading the image a transfer carried", () => {
  it("reads a file's media type, its name, and its bytes", async () => {
    const image = await imageFromTransfer(imageTransfer());
    expect(image).not.toBeNull();
    expect(image?.mediaType).toBe("image/png");
    expect(image?.filename).toBe("shot.png");
    expect(atob(image!.data)).toHaveLength(3);
  });

  it("answers nothing for a transfer carrying no image", async () => {
    const text = {
      files: [],
      items: [],
      types: ["text/plain"],
    } as unknown as DataTransfer;
    expect(await imageFromTransfer(text)).toBeNull();
    expect(await imageFromTransfer(null)).toBeNull();
    expect(transferHasText(text)).toBe(true);
    expect(transferHasText(imageTransfer())).toBe(false);
  });

  it("names an unresolved destination with and without alt text", () => {
    expect(unresolvedImageLabel("../assets/a.png", "a diagram")).toBe(
      "a diagram: Image could not be loaded — ../assets/a.png",
    );
    expect(unresolvedImageLabel("../assets/a.png", "")).toBe(
      "Image could not be loaded — ../assets/a.png",
    );
  });
});
