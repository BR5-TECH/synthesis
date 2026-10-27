// Sources that the Document type makes, in the Documents panel
// (`DPN-documents-panel.md` DPN-FR-PZMB, DPN-FR-TQWE, DPN-FR-FZFL,
// DPN-FR-RTLG, DPN-FR-CDFO). The backend is mocked at `invoke`.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { cleanup, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import { resetDocumentsPanelStateForTest } from "../../state/documentsPanelState";
import type { DocumentsSnapshot } from "../../types";
import { DocumentsPanel } from ".";
import {
  calls,
  doc,
  emitSnapshot,
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

const intro = doc("/work/proj/guides/intro.md", { project_path: "guides/intro.md" });
const ref = doc("/refs/ref.md");

const typed = (): DocumentsSnapshot =>
  snap(
    [
      src("file", "/refs/ref.md"),
      src("folder", "/work/proj/guides", { origin: "type" }),
    ],
    [intro, ref],
  );

function render_() {
  const props = {
    onOpenDocument: vi.fn(),
    onDocumentsRemoved: vi.fn(),
    onOverlayOpening: vi.fn(),
  };
  render(<DocumentsPanel {...props} />);
  return props;
}

const rowOf = (path: string) =>
  screen.getByRole("button", { name: `Remove ${path}` }).closest("li") as HTMLElement;

describe("DPN-FR-PZMB: a source that the Document type makes", () => {
  it("DPN-FR-PZMB, DPN-FR-FZFL, LIB-FR-08: a type source carries the DOC tag of the tree, and a manual source carries none", async () => {
    serve({ list: typed });
    render_();
    await screen.findByRole("tree");

    const tag = within(rowOf("/work/proj/guides")).getByText("DOC");
    expect(tag).toHaveClass("chip-type");
    expect(tag).toHaveAttribute("data-type", "doc");
    expect(tag.getAttribute("title")).toMatch(/Document type/);
    expect(within(rowOf("/refs/ref.md")).queryByText("DOC")).not.toBeInTheDocument();
  });

  it("DPN-FR-PZMB: a path that is a manual and a type source at once carries the tag too", async () => {
    serve({
      list: () => snap([src("folder", "/work/proj/guides", { origin: "both" })], [intro]),
    });
    render_();
    await screen.findByRole("tree");
    expect(within(rowOf("/work/proj/guides")).getByText("DOC")).toBeInTheDocument();
  });

  it("DPN-FR-FZFL, DCL-FR-NLCE: the rows follow the order of the snapshot", async () => {
    serve({ list: typed });
    render_();
    await screen.findByRole("tree");
    const rows = screen
      .getAllByRole("button", { name: /^Remove / })
      .map((b) => b.getAttribute("aria-label"));
    expect(rows).toEqual(["Remove /refs/ref.md", "Remove /work/proj/guides"]);
  });

  it("DPN-FR-PZMB, DPN-FR-RTLG, DCL-FR-ATNL: Remove on a type source calls the one removal command and renders what it returns", async () => {
    const user = userEvent.setup();
    serve({ list: typed, remove: () => snap([src("file", "/refs/ref.md")], [ref]) });
    const props = render_();
    await screen.findByRole("tree");

    await user.click(screen.getByRole("button", { name: "Remove /work/proj/guides" }));

    expect(calls("remove_document_source")).toEqual([
      ["remove_document_source", { path: "/work/proj/guides" }],
    ]);
    await waitFor(() => expect(props.onDocumentsRemoved).toHaveBeenCalledWith([intro.id]));
    expect(screen.queryByText("intro.md")).not.toBeInTheDocument();
    expect(screen.getByText("ref.md")).toBeInTheDocument();
  });
});

describe("DPN-FR-TQWE: a Document type given or cleared elsewhere", () => {
  it("DPN-FR-TQWE, DCL-FR-RMVT: shows in the tree and the sources with no manual refresh", async () => {
    serve({ list: () => snap([src("file", "/refs/ref.md")], [ref]) });
    render_();
    await screen.findByText("ref.md");
    expect(screen.queryByText("intro.md")).not.toBeInTheDocument();

    await emitSnapshot(typed());

    expect(await screen.findByText("intro.md")).toBeInTheDocument();
    expect(within(rowOf("/work/proj/guides")).getByText("DOC")).toBeInTheDocument();
    expect(calls("list_documents")).toHaveLength(1);

    await emitSnapshot(snap([src("file", "/refs/ref.md")], [ref]));
    expect(screen.queryByText("intro.md")).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Remove /work/proj/guides" })).toBeNull();
  });
});

describe("DPN-FR-CDFO: opening a document of the worktree", () => {
  it("DPN-FR-CDFO: activating the row hands the entry, with its project path, to the shell", async () => {
    const user = userEvent.setup();
    serve({ list: typed });
    const props = render_();
    await screen.findByRole("tree");
    await user.click(screen.getByText("intro.md"));
    expect(props.onOpenDocument).toHaveBeenCalledWith(
      expect.objectContaining({ id: intro.id, project_path: "guides/intro.md" }),
    );
  });
});
