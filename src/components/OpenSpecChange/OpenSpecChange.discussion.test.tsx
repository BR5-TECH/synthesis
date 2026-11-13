/**
 * The discussion column of a change workspace on a stream
 * (`../../../specifications/ui/OCW-openspec-change-workspace.md` OCW-FR-TRLG,
 * OCW-FR-ZZHS, OCW-FR-JBFZ; `DDS-draft-discussion.md` DDS-FR-POJC;
 * `CMT-comments.md` CMT-FR-AYSL; `CVP-conversation-presentation.md`
 * CVP-FR-TFRD; `VOM-voice-mode.md` VOM-FR-NTKT).
 */
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async () => () => {}),
}));
const { noteOwnerDeleted } = vi.hoisted(() => ({ noteOwnerDeleted: vi.fn() }));
vi.mock("../../voice/voiceController", async (original) => ({
  ...(await original<typeof import("../../voice/voiceController")>()),
  noteOwnerDeleted,
}));
vi.mock("../Editor/ReviewOverlay", async () => {
  const { createElement } = await import("react");
  type Review = import("../Editor/props").EditorReview;
  return {
    ReviewOverlay: ({ review }: { review: Review | undefined }) => {
      if (review === undefined || review.focused === null) return null;
      if (!review.hunks.some((h) => h.id === review.focused)) return null;
      const id = review.focused;
      return createElement(
        "button",
        { type: "button", onClick: () => review.onDiscuss(id) },
        "Discuss",
      );
    },
  };
});

import { discussionTargetKey } from "../../types";
import { requestChangeDiscussion } from "../../state/openspec/changeRoutes";
import {
  CHANGE,
  PROPOSAL_TEXT,
  STREAM_ID,
  TARGET,
  changeBackend,
  drawChange,
  resetChangeWorkspaceStores,
  wireChangeBackend,
  type ChangeBackend,
} from "../../test/changeWorkspaceFixtures";
import { sourceOffsets } from "./SourceView";
import { changeColumn, setColumnHidden } from "../../state/discussionColumn";

const calls = (cmd: string) => invokeMock.mock.calls.filter((c) => c[0] === cmd);

let backend: ChangeBackend;

beforeEach(() => {
  resetChangeWorkspaceStores();
  invokeMock.mockReset();
  noteOwnerDeleted.mockReset();
  backend = changeBackend();
  wireChangeBackend(invokeMock, backend);
});
afterEach(() => {
  cleanup();
  window.getSelection()?.removeAllRanges();
  expect(backend.unknown).toEqual([]);
});

/** Select `text` inside `el`, as a reader does with the pointer. */
function selectText(el: Element, text: string) {
  const walker = document.createTreeWalker(el, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    const at = node.textContent?.indexOf(text) ?? -1;
    if (at === -1) continue;
    const range = document.createRange();
    range.setStart(node, at);
    range.setEnd(node, at + text.length);
    const selection = window.getSelection()!;
    selection.removeAllRanges();
    selection.addRange(range);
    return;
  }
  throw new Error(`text not found: ${text}`);
}

describe("the discussion column of a stream location", () => {
  it("OCW-FR-TRLG, DDS-FR-POJC: the reading pane, the splitter, and the discussion column stand side by side", async () => {
    drawChange();
    const split = await screen.findByTestId("openspec-change-split");
    const parts = [...split.children];
    expect(parts[0]).toContainElement(await screen.findByTestId("openspec-reading"));
    expect(parts[1]).toHaveAttribute("role", "separator");
    expect(parts[2]).toBe(screen.getByTestId("openspec-change-discussion"));
    expect(split.style.gridTemplateColumns).toContain("var(--dds-splitter-w)");
  });

  it("OCW-FR-TRLG: the column reads the discussions of the change owner and lists the whole-change and the fragment ones", async () => {
    drawChange();
    const column = await screen.findByTestId("openspec-change-discussion");
    await waitFor(() => expect(within(column).getAllByRole("tab")).toHaveLength(2));
    expect(calls("list_discussions")[0][1]).toEqual({ target: TARGET });
    const tabs = within(column).getAllByRole("tab");
    expect(tabs.map((t) => t.getAttribute("data-target"))).toEqual(["whole", "fragment"]);
    expect(within(column).getByText("Please fix the wording.")).toBeInTheDocument();
  });

  it("OCW-FR-ZZHS: a worktree that is not a stream carries no discussion column, and says where a change is discussed", async () => {
    drawChange({ location: { kind: "active_worktree" } });
    await screen.findByTestId("openspec-reading");
    expect(screen.queryByTestId("openspec-change-discussion")).not.toBeInTheDocument();
    expect(screen.queryByTestId("openspec-change-split")).not.toBeInTheDocument();
    expect(screen.getByTestId("openspec-change-no-discussion")).toHaveTextContent(
      "A change is discussed on its work stream",
    );
    expect(calls("list_discussions")).toHaveLength(0);
    expect(calls("list_change_edit_proposals")).toHaveLength(0);
  });
});

describe("fragment discussions of a file", () => {
  it("OCW-FR-JBFZ, CMT-FR-AYSL: a selection in Source opens a fragment discussion with the change-relative path", async () => {
    backend.proposals = [];
    drawChange({ file: "proposal.md" });
    await userEvent.click(await screen.findByRole("button", { name: "Source" }));
    const text = screen.getByTestId("openspec-source-text");
    selectText(text, "cannot log out");
    act(() => {
      text.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
    });
    await userEvent.click(await screen.findByRole("button", { name: "Discuss selection" }));
    const composer = await screen.findByRole("textbox", { name: "Discuss this change" });
    await userEvent.type(composer, "Why this word?");
    await userEvent.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() => expect(calls("open_discussion")).toHaveLength(1));
    const start = PROPOSAL_TEXT.indexOf("cannot log out");
    expect(calls("open_discussion")[0][1]).toMatchObject({
      target: TARGET,
      fragmentTarget: {
        owner: TARGET,
        path: "proposal.md",
        start,
        end: start + "cannot log out".length,
        quote: "cannot log out",
      },
    });
  });

  it("CMT-FR-28, CMT-FR-AYSL: the Source marks each open fragment discussion of the file, and a mark selects its discussion", async () => {
    backend.proposals = [];
    drawChange({ file: "proposal.md" });
    await userEvent.click(await screen.findByRole("button", { name: "Source" }));
    const mark = await screen.findByTestId("openspec-fragment-mark");
    expect(mark).toHaveTextContent("# Why");
    expect(mark).toHaveAttribute("aria-label", "Fragment of discussion t2");
    expect(screen.getByTestId("openspec-source-text").textContent).toBe(PROPOSAL_TEXT);
    await userEvent.click(mark);
    const column = screen.getByTestId("openspec-change-discussion");
    await waitFor(() =>
      expect(within(column).getByRole("tab", { selected: true })).toHaveAttribute("data-target", "fragment"),
    );
    expect(screen.getByTestId("openspec-fragment-mark")).toHaveAttribute(
      "aria-label",
      "Fragment of discussion t2 (focused)",
    );
  });

  it("CMT-FR-28, OCW-FR-COLR: a file that holds hunks marks its fragment discussions, also inside the text a hunk strikes through", async () => {
    const [, t2] = backend.discussions;
    const start = PROPOSAL_TEXT.indexOf("cannot log out");
    backend.discussions.push({
      ...t2,
      id: "t3",
      fragmentTarget: { owner: TARGET, path: "proposal.md", start, end: start + 14, quote: "cannot log out" },
    });
    drawChange({ file: "proposal.md" });
    await userEvent.click(await screen.findByRole("button", { name: "Source" }));
    await waitFor(() => expect(screen.getAllByTestId("openspec-fragment-mark")).toHaveLength(2));
    const [heading, inside] = screen.getAllByTestId("openspec-fragment-mark");
    expect(heading).toHaveAttribute("data-comment-thread", "t2");
    expect(heading.closest(".hunk--del")).toBeNull();
    expect(inside).toHaveAttribute("data-comment-thread", "t3");
    expect(inside).toHaveTextContent("cannot log out");
    // The struck text of hunk h1 holds the mark, and the hunk is still drawn.
    expect(inside.closest(".hunk--del")).not.toBeNull();
  });

  it("CMT-FR-28: a mark activated while the column is hidden shows the column and focuses its discussion", async () => {
    backend.proposals = [];
    setColumnHidden(changeColumn(STREAM_ID, CHANGE), true);
    drawChange({ file: "proposal.md" });
    await userEvent.click(await screen.findByRole("button", { name: "Source" }));
    const split = await screen.findByTestId("openspec-change-split");
    expect(split).toHaveAttribute("data-discussion", "hidden");
    await userEvent.click(await screen.findByTestId("openspec-fragment-mark"));
    await waitFor(() => expect(split).toHaveAttribute("data-discussion", "shown"));
    const column = screen.getByTestId("openspec-change-discussion");
    expect(within(column).getByRole("tab", { selected: true })).toHaveAttribute("data-target", "fragment");
  });

  it("CMT-FR-AYSL, CMT-FR-28: a selection that starts inside a mark counts the file's own text", async () => {
    backend.proposals = [];
    drawChange({ file: "proposal.md" });
    await userEvent.click(await screen.findByRole("button", { name: "Source" }));
    const mark = await screen.findByTestId("openspec-fragment-mark");
    const text = screen.getByTestId("openspec-source-text");
    const after = mark.nextSibling!;
    const range = document.createRange();
    range.setStart(mark.firstChild!, 2);
    range.setEnd(after, "\n\nUsers".length);
    window.getSelection()!.removeAllRanges();
    window.getSelection()!.addRange(range);
    act(() => {
      text.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
    });
    await userEvent.click(await screen.findByRole("button", { name: "Discuss selection" }));
    await userEvent.type(await screen.findByRole("textbox", { name: "Discuss this change" }), "Why?");
    await userEvent.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() => expect(calls("open_discussion")).toHaveLength(1));
    expect(calls("open_discussion")[0][1]).toMatchObject({
      fragmentTarget: { path: "proposal.md", start: 2, end: "# Why\n\nUsers".length, quote: "Why\n\nUsers" },
    });
  });

  it("OCW-FR-JBFZ: a worktree location offers no fragment discussion", async () => {
    backend.proposals = [];
    drawChange({ location: { kind: "active_worktree" }, file: "proposal.md" });
    // The worktree location has rendered before the absence is checked.
    await screen.findByTestId("openspec-change-no-discussion");
    await userEvent.click(await screen.findByRole("button", { name: "Source" }));
    const text = await screen.findByTestId("openspec-source-text");
    expect(text.textContent).toBe(PROPOSAL_TEXT);
    selectText(text, "cannot log out");
    act(() => {
      text.dispatchEvent(new MouseEvent("mouseup", { bubbles: true }));
    });
    // The selection stands, so only the location can keep the offer away.
    expect(window.getSelection()?.toString()).toBe("cannot log out");
    expect(screen.queryByRole("button", { name: "Discuss selection" })).not.toBeInTheDocument();
  });

  it("CMT-FR-AYSL: a range in a decorated Source counts the file's own text alone", () => {
    const root = document.createElement("div");
    root.innerHTML =
      'one <span class="hunk--del">two</span><span class="hunk--add hunk--source-block">NEW</span> three';
    document.body.appendChild(root);
    const last = root.lastChild!;
    const range = document.createRange();
    range.setStart(last, 1);
    range.setEnd(last, 6);
    const found = sourceOffsets(root, range);
    expect(found).toEqual({ start: 8, end: 13 });
    // The file's own text is the text outside the proposed block.
    const own = [...root.childNodes]
      .filter((n) => !(n instanceof Element && n.matches(".hunk--source-block")))
      .map((n) => n.textContent)
      .join("");
    expect(own.slice(found!.start, found!.end)).toBe(range.toString());
    root.remove();
  });
});

describe("the discussion of one hunk", () => {
  it("OCW-FR-COLR: Discuss opens a discussion whose opening comment names the hunk of the change, and holds the hunk", async () => {
    drawChange({ file: "proposal.md" });
    await userEvent.click(await screen.findByRole("button", { name: "Discuss" }));
    const composer = await screen.findByRole("textbox", { name: "Discuss this change" });
    expect(composer).toHaveValue("@arch ");
    await userEvent.type(composer, "why?");
    await userEvent.click(screen.getByRole("button", { name: "Post" }));
    await waitFor(() => expect(calls("open_discussion")).toHaveLength(1));
    expect(calls("open_discussion")[0][1]).toMatchObject({
      target: TARGET,
      fragmentTarget: null,
      attachments: [
        {
          kind: "proposalHunk",
          proposalId: "p1",
          hunkId: "h1",
          change: { streamId: STREAM_ID, change: CHANGE },
          path: "proposal.md",
        },
      ],
    });
    await waitFor(() => expect(calls("set_change_edit_hunk_discussing")).toHaveLength(1));
    expect(calls("set_change_edit_hunk_discussing")[0][1]).toEqual({
      proposalId: "p1",
      hunkId: "h1",
      discussing: true,
    });
  });
});

describe("routes into the column", () => {
  it("CVP-FR-TFRD: a reveal of a change owner selects the discussion in the column", async () => {
    requestChangeDiscussion(discussionTargetKey(TARGET), "t2");
    drawChange();
    const column = await screen.findByTestId("openspec-change-discussion");
    await waitFor(() => expect(within(column).getAllByRole("tab")).toHaveLength(2));
    await waitFor(() =>
      expect(within(column).getAllByRole("tab")[1]).toHaveAttribute("aria-selected", "true"),
    );
    expect(within(column).getByText("A heading question.")).toBeInTheDocument();
  });

  it("CVP-FR-TFRD: a reveal while the tab is open selects the discussion at once", async () => {
    drawChange();
    const column = await screen.findByTestId("openspec-change-discussion");
    await waitFor(() => expect(within(column).getAllByRole("tab")).toHaveLength(2));
    act(() => requestChangeDiscussion(discussionTargetKey(TARGET), "t2"));
    await waitFor(() =>
      expect(within(column).getAllByRole("tab")[1]).toHaveAttribute("aria-selected", "true"),
    );
  });
});

describe("voice mode and the owner of a change discussion", () => {
  it("VOM-FR-NTKT: a read that finds the change folder gone deletes the owner of its discussion", async () => {
    backend.readReject = "change_not_found";
    drawChange();
    await screen.findByTestId("openspec-change-failure");
    await waitFor(() => expect(noteOwnerDeleted).toHaveBeenCalledWith(TARGET));
  });

  it("VOM-FR-NTKT: a read that finds the stream gone deletes the owner, and another failure does not", async () => {
    backend.readReject = "cli_failed";
    const first = drawChange();
    await screen.findByTestId("openspec-change-failure");
    expect(noteOwnerDeleted).not.toHaveBeenCalled();
    first.view.unmount();
    backend.readReject = "unknown_stream";
    drawChange({}, "tab-2");
    await waitFor(() => expect(noteOwnerDeleted).toHaveBeenCalledWith(TARGET));
    // The cli_failed read did not delete the owner later either.
    expect(noteOwnerDeleted).toHaveBeenCalledTimes(1);
  });
});
