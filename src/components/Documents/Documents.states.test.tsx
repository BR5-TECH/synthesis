// The Documents panel's states and layout (`DPN-documents-panel.md` DPN-FR-VPRV,
// DPN-FR-FAOR, DPN-FR-UKXW, DPN-FR-EPCH, DPN-FR-AREM). The backend is mocked at
// `invoke`.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetDocumentsPanelStateForTest } from "../../state/documentsPanelState";
import { blocksFor, decl, sheet } from "../../test/cssRules";
import { DocumentsPanel } from ".";
import {
  calls,
  doc,
  emitSnapshot,
  invokeMock,
  listenerCount,
  logErrorMock,
  loggedText,
  resetPanelFixtures,
  serve,
  snap,
  src,
} from "./panelFixtures";

vi.mock("@tauri-apps/api/core", async () => ({
  invoke: (await import("./panelFixtures")).invokeMock,
}));
vi.mock("@tauri-apps/api/event", async () => ({
  listen: (await import("./panelFixtures")).listenMock,
}));
vi.mock("../../logging", async () => (await import("./panelFixtures")).loggingMock);

beforeEach(() => {
  resetPanelFixtures();
  resetDocumentsPanelStateForTest();
});
afterEach(cleanup);

const panel = (extra: Partial<Parameters<typeof DocumentsPanel>[0]> = {}) =>
  render(
    <DocumentsPanel
      onOpenDocument={vi.fn()}
      onDocumentsRemoved={vi.fn()}
      {...extra}
    />,
  );

const reference = [
  doc("/Users/me/reference/specs/api-guide.pdf"),
  doc("/Users/me/reference/notes.md"),
];

describe("DPN-FR-VPRV: the layout of the panel", () => {
  it("DPN-FR-VPRV: renders the header with the panel name and Add documents, then the filter, the tree, and Selected sources, top to bottom", async () => {
    serve({ list: () => snap([src("folder", "/Users/me/reference")], reference) });
    const { container } = panel();
    await screen.findByRole("tree", { name: "Documents" });
    const root = container.querySelector(".documents-panel")!;
    const header = root.querySelector(".panel-header")!;
    expect(header.textContent).toContain("Documents");
    expect(within(header as HTMLElement).getByRole("button", { name: "Add documents" })).toBeInTheDocument();
    const order = Array.from(
      root.querySelectorAll(".panel-header, .panel-controls, .documents-tree, .documents-sources"),
    ).map((el) => el.className.split(" ")[0]);
    expect(order).toEqual(["panel-header", "panel-controls", "documents-tree", "documents-sources"]);
    expect(screen.getByRole("textbox", { name: "Filter documents" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Selected sources" })).toBeInTheDocument();
  });

  it("DPN-FR-VPRV: the header, the filter, and the section heading stay visible while only the tree region scrolls", () => {
    const css = sheet("components.css");
    const root = blocksFor(css, ".documents-panel");
    expect(decl(root[0], "display")).toBe("flex");
    expect(decl(root[0], "flex-direction")).toBe("column");
    expect(decl(root[0], "height")).toBe("100%");
    // The scrolling region is the shared panel body; the sources section is
    // pinned beneath it and does not shrink with the tree.
    const body = blocksFor(sheet("kit.css"), ".vpanel__body");
    expect(decl(body[0], "overflow")).toBe("auto");
    expect(decl(body[0], "flex")).toBe("1");
    const sources = blocksFor(css, ".documents-sources");
    expect(decl(sources[0], "flex")).toBe("none");
    const heading = blocksFor(css, ".documents-sources__heading");
    expect(heading).toHaveLength(1);
    const list = blocksFor(css, ".documents-sources__list");
    expect(decl(list[0], "overflow")).toBe("auto");
  });
});

describe("DPN-FR-FAOR: loading and failure", () => {
  it("DPN-FR-FAOR: shows Loading documents… in place of the tree while the first list call is pending", async () => {
    invokeMock.mockImplementation(() => new Promise(() => {}));
    panel();
    expect(await screen.findByText("Loading documents…")).toBeInTheDocument();
    expect(screen.queryByRole("tree")).toBeNull();
    expect(screen.queryByRole("heading", { name: "Selected sources" })).toBeNull();
  });

  it("DPN-FR-FAOR: a failed list call shows Documents could not be loaded. with Retry, and logs the failure without detail", async () => {
    invokeMock.mockImplementation(() => Promise.reject("store_unavailable: /secret/store.json"));
    panel();
    expect(await screen.findByText("Documents could not be loaded.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retry" })).toBeInTheDocument();
    expect(screen.queryByRole("tree")).toBeNull();
    expect(logErrorMock).toHaveBeenCalledTimes(1);
    expect(logErrorMock.mock.calls[0][0]).toEqual(["frontend"]);
    expect(loggedText()).not.toContain("/secret");
    expect(loggedText()).toContain("store_unavailable");
  });

  it("DPN-FR-FAOR: Retry calls the list again and shows what it returns", async () => {
    const user = userEvent.setup();
    let fail = true;
    serve({
      list: () => {
        if (fail) throw new Error("boom");
        return snap([src("folder", "/Users/me/reference")], reference);
      },
    });
    invokeMock.mockImplementationOnce(() => Promise.reject("io_error"));
    panel();
    await screen.findByText("Documents could not be loaded.");
    fail = false;
    await user.click(screen.getByRole("button", { name: "Retry" }));
    expect(await screen.findByRole("tree", { name: "Documents" })).toBeInTheDocument();
    expect(calls("list_documents")).toHaveLength(2);
    expect(screen.queryByText("Documents could not be loaded.")).toBeNull();
  });

  it("DPN-FR-FAOR: after a failure the panel follows documents changed and replaces its list without calling again", async () => {
    invokeMock.mockImplementation(() => Promise.reject("io_error"));
    panel();
    await screen.findByText("Documents could not be loaded.");
    expect(listenerCount()).toBe(1);
    await emitSnapshot(snap([src("folder", "/Users/me/reference")], reference));
    expect(await screen.findByRole("tree", { name: "Documents" })).toBeInTheDocument();
    expect(screen.queryByText("Documents could not be loaded.")).toBeNull();
    expect(calls("list_documents")).toHaveLength(1);
  });

  it("DPN-FR-FAOR: a payload replaces the list whole, and a payload that lands while the first call is pending wins", async () => {
    let release: (value: unknown) => void = () => {};
    invokeMock.mockImplementation(
      () =>
        new Promise((resolve) => {
          release = resolve;
        }),
    );
    panel();
    await screen.findByText("Loading documents…");
    await emitSnapshot(snap([src("file", "/a/new.md")], [doc("/a/new.md")]));
    expect(await screen.findByText("new.md")).toBeInTheDocument();
    release(snap([src("file", "/b/old.md")], [doc("/b/old.md")]));
    await waitFor(() => expect(calls("list_documents")).toHaveLength(1));
    expect(screen.getByText("new.md")).toBeInTheDocument();
    expect(screen.queryByText("old.md")).toBeNull();
    await emitSnapshot(snap([src("file", "/c/later.md")], [doc("/c/later.md")]));
    expect(await screen.findByText("later.md")).toBeInTheDocument();
    expect(screen.queryByText("new.md")).toBeNull();
    expect(calls("list_documents")).toHaveLength(1);
  });

  it("DPN-FR-FAOR: stops following the event when it unmounts", async () => {
    serve({ list: () => snap([], []) });
    const view = panel();
    await screen.findByText("No documents");
    expect(listenerCount()).toBe(1);
    view.unmount();
    await waitFor(() => expect(listenerCount()).toBe(0));
  });
});

describe("DPN-FR-UKXW: the first-class empty state", () => {
  it("DPN-FR-UKXW: a collection with no source and no document shows the line, one sentence, and one Add documents action", async () => {
    serve({ list: () => snap([], []) });
    const { container } = panel();
    expect(await screen.findByText("No documents")).toBeInTheDocument();
    const block = container.querySelector(".panel-empty") as HTMLElement;
    expect(block).not.toBeNull();
    expect(block.querySelector(".panel-empty__body")?.textContent).toBe(
      "Documents are reference files. They stay where they are on disk.",
    );
    expect(screen.getAllByRole("button", { name: "Add documents" })).toHaveLength(1);
    expect(within(block).getByRole("button", { name: "Add documents" })).toBeInTheDocument();
  });

  it("DPN-FR-UKXW: the filter and the Selected sources section are not rendered beside it", async () => {
    serve({ list: () => snap([], []) });
    panel();
    await screen.findByText("No documents");
    expect(screen.queryByRole("textbox", { name: "Filter documents" })).toBeNull();
    expect(screen.queryByRole("heading", { name: "Selected sources" })).toBeNull();
    expect(screen.queryByRole("tree")).toBeNull();
  });

  it("DPN-FR-UKXW: the empty state is centred in the list region, as every vertical panel does (SNV-FR-60)", async () => {
    serve({ list: () => snap([], []) });
    const { container } = panel();
    await screen.findByText("No documents");
    const body = container.querySelector(".vpanel__body--empty");
    expect(body).not.toBeNull();
    expect(body!.querySelector(".panel-empty")).not.toBeNull();
  });
});

describe("DPN-FR-EPCH: sources without documents, and a filter with no match", () => {
  it("DPN-FR-EPCH: sources but no document shows the sentence in the tree region and keeps Selected sources", async () => {
    serve({ list: () => snap([src("folder", "/Users/me/empty")], []) });
    const { container } = panel();
    expect(
      await screen.findByText("No supported documents in the selected sources."),
    ).toBeInTheDocument();
    expect(container.querySelector(".panel-empty")).toBeNull();
    expect(screen.getByRole("heading", { name: "Selected sources" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Remove /Users/me/empty" })).toBeInTheDocument();
    // The header action stays, so a user can add more.
    expect(screen.getByRole("button", { name: "Add documents" })).toBeInTheDocument();
  });

  it("DPN-FR-EPCH: a filter that matches nothing shows No documents match and the typed text, with the filter still holding it", async () => {
    const user = userEvent.setup();
    serve({ list: () => snap([src("folder", "/Users/me/reference")], reference) });
    const { container } = panel();
    const filter = await screen.findByRole("textbox", { name: "Filter documents" });
    await user.type(filter, "zzz");
    const message = container.querySelector(".panel-filtered") as HTMLElement;
    expect(message.textContent).toBe("No documents match zzz");
    expect(container.querySelector(".vpanel__body--filtered")).not.toBeNull();
    expect(screen.getByRole("textbox", { name: "Filter documents" })).toHaveValue("zzz");
    expect(screen.queryByRole("tree")).toBeNull();
    expect(screen.getByRole("heading", { name: "Selected sources" })).toBeInTheDocument();
    expect(container.querySelector(".panel-empty")).toBeNull();
    await user.clear(filter);
    expect(await screen.findByRole("tree", { name: "Documents" })).toBeInTheDocument();
  });
});

describe("DPN-FR-AREM: session-only filter and expansion", () => {
  const collection = () =>
    snap(
      [src("folder", "/Users/me/reference")],
      [
        doc("/Users/me/reference/specs/api-guide.pdf"),
        doc("/Users/me/reference/specs/old.txt"),
        doc("/Users/me/reference/notes.md"),
      ],
    );

  it("DPN-FR-AREM: the filter text and the folder state survive a switch to another panel, which unmounts this one", async () => {
    const user = userEvent.setup();
    serve({ list: collection });
    const first = panel();
    const filter = await screen.findByRole("textbox", { name: "Filter documents" });
    await user.type(filter, "o");
    await user.click(await screen.findByRole("treeitem", { name: /Users\/me\/reference/ }));
    expect(screen.getByRole("treeitem", { name: /Users\/me\/reference/ })).toHaveAttribute(
      "aria-expanded",
      "false",
    );
    first.unmount();
    panel();
    expect(await screen.findByRole("textbox", { name: "Filter documents" })).toHaveValue("o");
    expect(screen.getByRole("treeitem", { name: /Users\/me\/reference/ })).toHaveAttribute(
      "aria-expanded",
      "false",
    );
  });

  it("DPN-FR-AREM: the state survives a remount of the whole shell subtree, as a change of the active worktree does", async () => {
    const user = userEvent.setup();
    serve({ list: collection });
    const first = panel();
    await user.type(await screen.findByRole("textbox", { name: "Filter documents" }), "notes");
    first.unmount();
    cleanup();
    panel();
    expect(await screen.findByRole("textbox", { name: "Filter documents" })).toHaveValue("notes");
    expect(await screen.findByText("notes.md")).toBeInTheDocument();
  });

  it("DPN-FR-AREM: nothing of the state is written to disk or anywhere outside the window", async () => {
    const user = userEvent.setup();
    serve({ list: collection });
    panel();
    await user.type(await screen.findByRole("textbox", { name: "Filter documents" }), "notes");
    await user.clear(screen.getByRole("textbox", { name: "Filter documents" }));
    await user.click(screen.getByRole("treeitem", { name: /Users\/me\/reference/ }));
    const commands = invokeMock.mock.calls.map((c) => c[0]);
    expect(commands.every((c) => c === "list_documents")).toBe(true);
    expect(localStorage.length).toBe(0);
    expect(sessionStorage.length).toBe(0);
  });

  it("DPN-FR-AREM: a relaunch starts with an empty filter and every folder expanded", async () => {
    const user = userEvent.setup();
    serve({ list: collection });
    const first = panel();
    await user.type(await screen.findByRole("textbox", { name: "Filter documents" }), "api");
    await user.clear(screen.getByRole("textbox", { name: "Filter documents" }));
    await user.click(screen.getByRole("treeitem", { name: /Users\/me\/reference/ }));
    first.unmount();
    resetDocumentsPanelStateForTest();
    panel();
    expect(await screen.findByRole("textbox", { name: "Filter documents" })).toHaveValue("");
    expect(screen.getByRole("treeitem", { name: /Users\/me\/reference/ })).toHaveAttribute(
      "aria-expanded",
      "true",
    );
  });
});
