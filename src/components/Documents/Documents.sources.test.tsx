// Adding and removing sources in the Documents panel (`DPN-documents-panel.md`
// DPN-FR-ZMBQ, DPN-FR-FDVO, DPN-FR-BADJ, DPN-FR-FZFL, DPN-FR-RTLG, DPN-FR-ZHEJ,
// DPN-FR-NQPS, DPN-FR-KEBH). The backend is mocked at `invoke`.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetDocumentsPanelStateForTest } from "../../state/documentsPanelState";
import type { DocumentsSnapshot, PickDocumentSourcesResult } from "../../types";
import { DocumentsPanel } from ".";
import {
  calls,
  doc,
  invokeMock,
  logErrorMock,
  logInfoMock,
  logWarnMock,
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

const alpha = doc("/refs/alpha/a.md");
const beta = doc("/refs/beta/b.pdf");
const shared = doc("/refs/alpha/shared.md");
const loose = doc("/loose/file.txt");

const base = (): DocumentsSnapshot =>
  snap(
    [src("folder", "/refs/alpha"), src("folder", "/refs/beta"), src("file", "/loose/file.txt")],
    [alpha, beta, shared, loose],
  );

function render_(extra: Partial<Parameters<typeof DocumentsPanel>[0]> = {}) {
  const props = {
    onOpenDocument: vi.fn(),
    onDocumentsRemoved: vi.fn(),
    onOverlayOpening: vi.fn(),
    ...extra,
  };
  render(<DocumentsPanel {...props} />);
  return props;
}

const menu = () => screen.getByRole("menu", { name: "Add documents" });
const addButton = () => screen.getByRole("button", { name: "Add documents" });

describe("DPN-FR-ZMBQ: the Add documents choice", () => {
  it("DPN-FR-ZMBQ: opens a floating choice with the two entries Files and Folder", async () => {
    const user = userEvent.setup();
    serve({ list: base });
    render_();
    await screen.findByRole("tree");
    await user.click(addButton());
    const items = within(menu()).getAllByRole("menuitem");
    expect(items.map((i) => i.textContent)).toEqual(["Files", "Folder"]);
    expect(addButton()).toHaveAttribute("aria-haspopup", "menu");
    expect(addButton()).toHaveAttribute("aria-expanded", "true");
    // A floating overlay, placed against the viewport so the panel never clips it.
    expect(menu().style.position).toBe("fixed");
  });

  it("DPN-FR-ZMBQ: asks every other overlay to close before it opens (SNV-FR-56)", async () => {
    const user = userEvent.setup();
    serve({ list: base });
    const props = render_();
    await screen.findByRole("tree");
    expect(props.onOverlayOpening).not.toHaveBeenCalled();
    await user.click(addButton());
    expect(props.onOverlayOpening).toHaveBeenCalledTimes(1);
  });

  it("DPN-FR-ZMBQ: the entries are reached with the Up and Down arrow keys and Enter", async () => {
    const user = userEvent.setup();
    serve({
      list: base,
      pick: async () => ({ cancelled: true, ignored_count: 0, snapshot: base() }),
    });
    render_();
    await screen.findByRole("tree");
    await user.click(addButton());
    const [files, folder] = within(menu()).getAllByRole("menuitem");
    expect(files).toHaveFocus();
    await user.keyboard("{ArrowDown}");
    expect(folder).toHaveFocus();
    await user.keyboard("{ArrowDown}");
    expect(files).toHaveFocus();
    await user.keyboard("{ArrowUp}");
    expect(folder).toHaveFocus();
    await user.keyboard("{Enter}");
    expect(calls("pick_document_sources")[0][1]).toEqual({ mode: "folder" });
  });

  it("DPN-FR-ZMBQ, DPN-FR-KEBH: the first entry takes the focus only once the choice is visible", async () => {
    // A browser ignores focus() on an element under `visibility: hidden`, and
    // jsdom does not. So the test records the visibility at each call: a call
    // made before the choice is placed leaves the focus where it was in the
    // real window, where the arrow keys then do nothing.
    const seen: string[] = [];
    const realFocus = HTMLElement.prototype.focus;
    const focus = vi
      .spyOn(HTMLElement.prototype, "focus")
      .mockImplementation(function (this: HTMLElement, options?: FocusOptions) {
        if (this.getAttribute("role") === "menuitem") {
          const frame = this.closest<HTMLElement>('[role="menu"]');
          seen.push(frame?.style.visibility ?? "");
        }
        realFocus.call(this, options);
      });
    try {
      const user = userEvent.setup();
      serve({ list: base });
      render_();
      await screen.findByRole("tree");
      await user.click(addButton());
      expect(seen).toEqual(["visible"]);
      expect(within(menu()).getAllByRole("menuitem")[0]).toHaveFocus();
    } finally {
      focus.mockRestore();
    }
  });

  it("DPN-FR-ZMBQ: Escape closes the choice with no change and returns the focus to Add documents", async () => {
    const user = userEvent.setup();
    serve({ list: base });
    render_();
    await screen.findByRole("tree");
    await user.click(addButton());
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("menu")).toBeNull();
    expect(addButton()).toHaveFocus();
    expect(calls("pick_document_sources")).toHaveLength(0);
    expect(invokeMock.mock.calls.map((c) => c[0])).toEqual(["list_documents"]);
  });

  it("DPN-FR-ZMBQ: a press outside the choice, and a second press on the button, close it", async () => {
    const user = userEvent.setup();
    serve({ list: base });
    render_();
    await screen.findByRole("tree");
    await user.click(addButton());
    await user.click(screen.getByRole("tree"));
    expect(screen.queryByRole("menu")).toBeNull();
    await user.click(addButton());
    expect(screen.getByRole("menu")).toBeInTheDocument();
    await user.click(addButton());
    expect(screen.queryByRole("menu")).toBeNull();
  });

  it("DPN-FR-ZMBQ: the empty state's action opens the same choice beside that button", async () => {
    const user = userEvent.setup();
    serve({ list: () => snap([], []) });
    render_();
    await screen.findByText("No documents");
    await user.click(addButton());
    expect(within(menu()).getAllByRole("menuitem")).toHaveLength(2);
  });
});

describe("DPN-FR-FDVO: picking sources", () => {
  const picked = (extra: Partial<PickDocumentSourcesResult> = {}): PickDocumentSourcesResult => ({
    cancelled: false,
    ignored_count: 0,
    snapshot: snap(
      [...base().sources, src("file", "/new/more.md")],
      [...base().documents, doc("/new/more.md")],
    ),
    ...extra,
  });

  it("DPN-FR-FDVO: Files calls the pick command with mode files and passes no path", async () => {
    const user = userEvent.setup();
    serve({ list: base, pick: async () => picked() });
    render_();
    await screen.findByRole("tree");
    await user.click(addButton());
    await user.click(screen.getByRole("menuitem", { name: "Files" }));
    expect(calls("pick_document_sources")).toEqual([["pick_document_sources", { mode: "files" }]]);
  });

  it("DPN-FR-FDVO: Folder calls the pick command with mode folder and passes no path", async () => {
    const user = userEvent.setup();
    serve({ list: base, pick: async () => picked() });
    render_();
    await screen.findByRole("tree");
    await user.click(addButton());
    await user.click(screen.getByRole("menuitem", { name: "Folder" }));
    expect(calls("pick_document_sources")).toEqual([["pick_document_sources", { mode: "folder" }]]);
  });

  it("DPN-FR-FDVO: renders the snapshot the pick returns, and reads no path from a picker itself", async () => {
    const user = userEvent.setup();
    serve({ list: base, pick: async () => picked() });
    render_();
    await screen.findByRole("tree");
    await user.click(addButton());
    await user.click(screen.getByRole("menuitem", { name: "Files" }));
    expect(await screen.findByText("more.md")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Remove /new/more.md" })).toBeInTheDocument();
    // The frontend never calls the dialog plugin: only the backend picks.
    expect(invokeMock.mock.calls.map((c) => c[0])).toEqual(["list_documents", "pick_document_sources"]);
  });

  it("DPN-FR-FDVO: the empty state's action picks on the same terms", async () => {
    const user = userEvent.setup();
    serve({
      list: () => snap([], []),
      pick: async () => picked({ snapshot: snap([src("file", "/x/y.md")], [doc("/x/y.md")]) }),
    });
    render_();
    await screen.findByText("No documents");
    await user.click(addButton());
    await user.click(screen.getByRole("menuitem", { name: "Files" }));
    expect(await screen.findByText("y.md")).toBeInTheDocument();
    expect(screen.queryByText("No documents")).toBeNull();
  });
});

describe("DPN-FR-BADJ: the pending picker", () => {
  it("DPN-FR-BADJ: Add documents is disabled and exposes its busy state while the picker call is pending, then returns", async () => {
    const user = userEvent.setup();
    let finish: (value: PickDocumentSourcesResult) => void = () => {};
    serve({
      list: base,
      pick: () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    });
    render_();
    await screen.findByRole("tree");
    await user.click(addButton());
    await user.click(screen.getByRole("menuitem", { name: "Files" }));
    expect(addButton()).toBeDisabled();
    expect(addButton()).toHaveAttribute("aria-busy", "true");
    await act(async () => finish({ cancelled: true, ignored_count: 0, snapshot: base() }));
    expect(addButton()).toBeEnabled();
    expect(addButton()).toHaveAttribute("aria-busy", "false");
    await waitFor(() => expect(addButton()).toHaveFocus());
  });

  it("DPN-FR-BADJ: a cancelled pick leaves the tree and the sources as they were, with no status line and no log", async () => {
    const user = userEvent.setup();
    serve({
      list: base,
      pick: async () => ({ cancelled: true, ignored_count: 5, snapshot: snap([], []) }),
    });
    render_();
    await screen.findByRole("tree");
    await user.click(addButton());
    await user.click(screen.getByRole("menuitem", { name: "Files" }));
    await waitFor(() => expect(addButton()).toBeEnabled());
    expect(screen.getByRole("tree")).toBeInTheDocument();
    expect(screen.getAllByRole("button", { name: /^Remove / })).toHaveLength(3);
    expect(document.querySelector(".documents-notice")!.textContent).toBe("");
    expect(logInfoMock).not.toHaveBeenCalled();
    expect(logWarnMock).not.toHaveBeenCalled();
    expect(logErrorMock).not.toHaveBeenCalled();
  });

  it("DPN-FR-BADJ: a pick that ignored files shows one polite status line with the count", async () => {
    const user = userEvent.setup();
    serve({
      list: base,
      pick: async () => ({ cancelled: false, ignored_count: 3, snapshot: base() }),
    });
    render_();
    await screen.findByRole("tree");
    await user.click(addButton());
    await user.click(screen.getByRole("menuitem", { name: "Files" }));
    const line = await screen.findByText(
      "3 selected files were ignored because their type is not supported.",
    );
    expect(line).toHaveAttribute("role", "status");
    expect(line).toHaveAttribute("aria-live", "polite");
    expect(document.querySelectorAll(".documents-notice")).toHaveLength(1);
  });

  it("DPN-FR-BADJ: one ignored file is said in the singular, and a later pick clears the line", async () => {
    const user = userEvent.setup();
    let ignored = 1;
    serve({
      list: base,
      pick: async () => ({ cancelled: false, ignored_count: ignored, snapshot: base() }),
    });
    render_();
    await screen.findByRole("tree");
    await user.click(addButton());
    await user.click(screen.getByRole("menuitem", { name: "Files" }));
    expect(
      await screen.findByText("1 selected file was ignored because its type is not supported."),
    ).toBeInTheDocument();
    ignored = 0;
    await user.click(addButton());
    await user.click(screen.getByRole("menuitem", { name: "Files" }));
    await waitFor(() => expect(document.querySelector(".documents-notice")!.textContent).toBe(""));
  });

  it("DPN-FR-BADJ: the line shows in the empty state too, when every selected file was ignored", async () => {
    const user = userEvent.setup();
    serve({
      list: () => snap([], []),
      pick: async () => ({ cancelled: false, ignored_count: 2, snapshot: snap([], []) }),
    });
    render_();
    await screen.findByText("No documents");
    await user.click(addButton());
    await user.click(screen.getByRole("menuitem", { name: "Files" }));
    expect(
      await screen.findByText("2 selected files were ignored because their type is not supported."),
    ).toBeInTheDocument();
    expect(screen.getByText("No documents")).toBeInTheDocument();
  });
});

describe("DPN-FR-FZFL: Selected sources", () => {
  it("DPN-FR-FZFL: lists every source in stored order with a File or Folder label and the path as selected", async () => {
    serve({ list: base });
    const { container } = render(
      <DocumentsPanel onOpenDocument={vi.fn()} onDocumentsRemoved={vi.fn()} />,
    );
    await screen.findByRole("tree");
    const rows = Array.from(container.querySelectorAll(".documents-source"));
    expect(rows.map((r) => r.querySelector(".documents-source__kind")!.textContent)).toEqual([
      "Folder",
      "Folder",
      "File",
    ]);
    expect(
      rows.map((r) => r.querySelector(".sr-only")!.textContent),
    ).toEqual(["/refs/alpha", "/refs/beta", "/loose/file.txt"]);
    // The whole path is in the tooltip of a path that truncates in the middle.
    expect(rows[2].querySelector(".documents-source__path")).toHaveAttribute("title", "/loose/file.txt");
  });

  it("DPN-FR-FZFL: an unavailable source shows Unavailable followed by the reason, in words", async () => {
    serve({
      list: () =>
        snap(
          [
            src("file", "/a/missing.md", { status: "unavailable", reason: "missing" }),
            src("folder", "/a/locked", { status: "unavailable", reason: "unreadable" }),
            src("file", "/a/link.md", { status: "unavailable", reason: "link" }),
            src("folder", "/a/fine"),
          ],
          [doc("/a/fine/x.md")],
        ),
    });
    const { container } = render(
      <DocumentsPanel onOpenDocument={vi.fn()} onDocumentsRemoved={vi.fn()} />,
    );
    await screen.findByRole("tree");
    const states = Array.from(container.querySelectorAll(".documents-source__state")).map(
      (s) => s.textContent,
    );
    expect(states).toEqual([
      "Unavailable — not found",
      "Unavailable — cannot be read",
      "Unavailable — symbolic link",
    ]);
    expect(container.querySelectorAll(".documents-source")).toHaveLength(4);
    expect(container.querySelectorAll("[data-unavailable]")).toHaveLength(3);
  });
});

describe("DPN-FR-RTLG: removing a source", () => {
  const afterRemoval = (path: string): DocumentsSnapshot => {
    const current = base();
    return snap(
      current.sources.filter((s) => s.path !== path),
      current.documents.filter((d) => !d.path.startsWith(`${path}/`) && d.path !== path),
    );
  };

  it("DPN-FR-RTLG: each source row has a Remove button named for its path, and no confirmation is asked", async () => {
    const user = userEvent.setup();
    const confirm = vi.spyOn(window, "confirm");
    serve({ list: base, remove: (path) => afterRemoval(path) });
    render_();
    await screen.findByRole("tree");
    await user.click(screen.getByRole("button", { name: "Remove /refs/beta" }));
    expect(calls("remove_document_source")).toEqual([
      ["remove_document_source", { path: "/refs/beta" }],
    ]);
    expect(confirm).not.toHaveBeenCalled();
    expect(screen.queryByRole("dialog")).toBeNull();
    expect(screen.queryByRole("alertdialog")).toBeNull();
    confirm.mockRestore();
  });

  it("DPN-FR-RTLG: renders the returned snapshot", async () => {
    const user = userEvent.setup();
    serve({ list: base, remove: (path) => afterRemoval(path) });
    render_();
    await screen.findByRole("tree");
    expect(screen.getByText("b.pdf")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Remove /refs/beta" }));
    await waitFor(() => expect(screen.queryByText("b.pdf")).toBeNull());
    expect(screen.queryByRole("button", { name: "Remove /refs/beta" })).toBeNull();
    expect(screen.getByText("a.md")).toBeInTheDocument();
  });

  it("DPN-FR-RTLG: moves the focus to the next source row", async () => {
    const user = userEvent.setup();
    serve({ list: base, remove: (path) => afterRemoval(path) });
    render_();
    await screen.findByRole("tree");
    await user.click(screen.getByRole("button", { name: "Remove /refs/alpha" }));
    await waitFor(() =>
      expect(screen.getByRole("button", { name: "Remove /refs/beta" })).toHaveFocus(),
    );
  });

  it("DPN-FR-RTLG: removing the last source row moves the focus to Add documents, and the empty state's action when nothing is left", async () => {
    const user = userEvent.setup();
    serve({
      list: () => snap([src("file", "/a/one.md"), src("file", "/a/two.md")], [doc("/a/one.md"), doc("/a/two.md")]),
      remove: (path) =>
        path === "/a/two.md"
          ? snap([src("file", "/a/one.md")], [doc("/a/one.md")])
          : snap([], []),
    });
    render_();
    await screen.findByRole("tree");
    await user.click(screen.getByRole("button", { name: "Remove /a/two.md" }));
    await waitFor(() => expect(addButton()).toHaveFocus());
    await user.click(screen.getByRole("button", { name: "Remove /a/one.md" }));
    expect(await screen.findByText("No documents")).toBeInTheDocument();
    await waitFor(() => expect(addButton()).toHaveFocus());
  });

  it("DPN-FR-RTLG: every Remove button is disabled while a removal is under way", async () => {
    const user = userEvent.setup();
    let finish: (value: DocumentsSnapshot) => void = () => {};
    serve({
      list: base,
      remove: () =>
        new Promise<DocumentsSnapshot>((resolve) => {
          finish = resolve;
        }) as unknown as DocumentsSnapshot,
    });
    render_();
    await screen.findByRole("tree");
    await user.click(screen.getByRole("button", { name: "Remove /refs/alpha" }));
    for (const button of screen.getAllByRole("button", { name: /^Remove / })) {
      expect(button).toBeDisabled();
    }
    await act(async () => finish(afterRemoval("/refs/alpha")));
    await waitFor(() => expect(screen.getAllByRole("button", { name: /^Remove / })).toHaveLength(2));
    for (const button of screen.getAllByRole("button", { name: /^Remove / })) {
      expect(button).toBeEnabled();
    }
  });

  it("DPN-FR-RTLG: a failed removal says so, keeps the list, and logs an ERROR without the path", async () => {
    const user = userEvent.setup();
    serve({
      list: base,
      remove: () => {
        throw new Error("io_error: /refs/alpha");
      },
    });
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "list_documents" ? Promise.resolve(base()) : Promise.reject("invalid_path: /refs/alpha"),
    );
    render_();
    await screen.findByRole("tree");
    await user.click(screen.getByRole("button", { name: "Remove /refs/alpha" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("The source could not be removed.");
    expect(screen.getAllByRole("button", { name: /^Remove / })).toHaveLength(3);
    expect(logErrorMock).toHaveBeenCalledTimes(1);
    expect(loggedText()).not.toContain("/refs/alpha");
    expect(loggedText()).toContain("invalid_path");
  });
});

describe("DPN-FR-ZHEJ: documents that leave the collection", () => {
  it("DPN-FR-ZHEJ: reports the documents the returned snapshot no longer holds, and keeps one that another source includes", async () => {
    const user = userEvent.setup();
    const both = doc("/refs/alpha/both.md");
    const only = doc("/refs/alpha/only.md");
    serve({
      list: () =>
        snap(
          [src("folder", "/refs/alpha"), src("file", "/refs/alpha/both.md")],
          [both, only],
        ),
      remove: () => snap([src("file", "/refs/alpha/both.md")], [both]),
    });
    const props = render_();
    await screen.findByRole("tree");
    await user.click(screen.getByRole("button", { name: "Remove /refs/alpha" }));
    await waitFor(() => expect(props.onDocumentsRemoved).toHaveBeenCalledTimes(1));
    expect(props.onDocumentsRemoved).toHaveBeenCalledWith([only.id]);
    // The shared document stays in the tree.
    expect(screen.getByText("both.md")).toBeInTheDocument();
    expect(screen.queryByText("only.md")).toBeNull();
  });

  it("DPN-FR-ZHEJ: reports nothing when the removal takes no document out of the collection", async () => {
    const user = userEvent.setup();
    serve({
      list: () => snap([src("folder", "/empty")], []),
      remove: () => snap([], []),
    });
    const props = render_();
    await screen.findByText("No supported documents in the selected sources.");
    await user.click(screen.getByRole("button", { name: "Remove /empty" }));
    await screen.findByText("No documents");
    expect(props.onDocumentsRemoved).not.toHaveBeenCalled();
  });

  it("DPN-FR-ZHEJ: adding a source and a refresh event report no removal, because only a removal closes a viewer tab", async () => {
    const user = userEvent.setup();
    serve({
      list: base,
      pick: async () => ({ cancelled: false, ignored_count: 0, snapshot: snap([], []) }),
    });
    const props = render_();
    await screen.findByRole("tree");
    await user.click(addButton());
    await user.click(screen.getByRole("menuitem", { name: "Files" }));
    await screen.findByText("No documents");
    expect(props.onDocumentsRemoved).not.toHaveBeenCalled();
  });
});

describe("DPN-FR-NQPS: logging", () => {
  it("DPN-FR-NQPS: an INFO record carries the source kind and counts when a source is added, and none carries a path or a name", async () => {
    const user = userEvent.setup();
    serve({
      list: base,
      pick: async () => ({
        cancelled: false,
        ignored_count: 0,
        snapshot: snap([...base().sources, src("folder", "/secret/folder")], [...base().documents, doc("/secret/folder/top-secret.md")]),
      }),
    });
    render_();
    await screen.findByRole("tree");
    await user.click(addButton());
    await user.click(screen.getByRole("menuitem", { name: "Folder" }));
    await screen.findByText("top-secret.md");
    expect(logInfoMock).toHaveBeenCalledTimes(1);
    expect(logInfoMock).toHaveBeenCalledWith(["frontend"], expect.any(String), {
      kind: "folder",
      added: 1,
      sources: 4,
      documents: 5,
    });
    expect(loggedText()).not.toContain("secret");
    expect(loggedText()).not.toContain("top-secret");
  });

  it("DPN-FR-NQPS: an INFO record carries the source kind and counts when a source is removed", async () => {
    const user = userEvent.setup();
    serve({
      list: base,
      remove: () => snap([src("folder", "/refs/beta")], [beta]),
    });
    render_();
    await screen.findByRole("tree");
    await user.click(screen.getByRole("button", { name: "Remove /refs/alpha" }));
    await waitFor(() => expect(logInfoMock).toHaveBeenCalledTimes(1));
    expect(logInfoMock).toHaveBeenCalledWith(["frontend"], expect.any(String), {
      kind: "folder",
      sources: 1,
      documents: 1,
    });
    expect(loggedText()).not.toContain("/refs");
  });

  it("DPN-FR-NQPS: a pick that ignores files writes a WARN record with the count only", async () => {
    const user = userEvent.setup();
    serve({
      list: base,
      pick: async () => ({ cancelled: false, ignored_count: 4, snapshot: base() }),
    });
    render_();
    await screen.findByRole("tree");
    await user.click(addButton());
    await user.click(screen.getByRole("menuitem", { name: "Files" }));
    await waitFor(() => expect(logWarnMock).toHaveBeenCalledTimes(1));
    expect(logWarnMock).toHaveBeenCalledWith(["frontend"], expect.any(String), { ignored: 4 });
  });

  it("DPN-FR-NQPS: a failed pick writes an ERROR record with the typed code, shows a line, and never carries the path or the filter text", async () => {
    const user = userEvent.setup();
    invokeMock.mockImplementation((cmd: string) =>
      cmd === "list_documents"
        ? Promise.resolve(base())
        : Promise.reject("invalid_path: /secret/dir/file.pdf"),
    );
    render_();
    await user.type(await screen.findByRole("textbox", { name: "Filter documents" }), "needle");
    await user.click(addButton());
    await user.click(screen.getByRole("menuitem", { name: "Files" }));
    expect(await screen.findByRole("alert")).toHaveTextContent("Documents could not be added.");
    expect(logErrorMock).toHaveBeenCalledWith(["frontend"], expect.any(String), {
      mode: "files",
      reason: "invalid_path",
    });
    expect(loggedText()).not.toContain("secret");
    expect(loggedText()).not.toContain("needle");
    await waitFor(() => expect(addButton()).toBeEnabled());
  });

  it("DPN-FR-NQPS: no record of the panel carries the filter text or a file name", async () => {
    const user = userEvent.setup();
    serve({ list: base });
    render_();
    await user.type(await screen.findByRole("textbox", { name: "Filter documents" }), "alpha");
    await user.click(screen.getByRole("treeitem", { name: /a\.md/ }));
    expect(loggedText()).not.toContain("alpha");
    expect(loggedText()).not.toContain("a.md");
  });
});

describe("DPN-FR-KEBH: the sources and the choice by keyboard alone", () => {
  it("DPN-FR-KEBH: Add documents, its choice, and a Remove button operate with Enter and Space", async () => {
    const user = userEvent.setup();
    serve({
      list: base,
      pick: async () => ({ cancelled: true, ignored_count: 0, snapshot: base() }),
      remove: () => snap([src("folder", "/refs/beta")], [beta]),
    });
    render_();
    await screen.findByRole("tree");
    addButton().focus();
    await user.keyboard("{Enter}");
    expect(screen.getByRole("menu")).toBeInTheDocument();
    await user.keyboard("{Escape}");
    await user.keyboard(" ");
    expect(screen.getByRole("menu")).toBeInTheDocument();
    await user.keyboard(" ");
    await waitFor(() => expect(calls("pick_document_sources")).toHaveLength(1));
    const remove = screen.getByRole("button", { name: "Remove /refs/alpha" });
    remove.focus();
    await user.keyboard("{Enter}");
    await waitFor(() => expect(calls("remove_document_source")).toHaveLength(1));
  });
});
