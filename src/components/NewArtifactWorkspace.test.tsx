/**
 * The New Artifact workspace tab: its layout, its lifecycle actions, and the
 * chrome around the prompt (`../../specifications/ui/NAW-new-artifact.md`).
 *
 * The History rail, the discussions, the proposed-change review, and find and
 * replace each live in a `NewArtifactWorkspace.<topic>.test.tsx` sibling.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

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

import {
  PROMPT,
  RENAMED,
  draft,
  freshHistory,
  threeVersions,
  makeStubs,
  renderWorkspace,
  renderNamed,
  openActions,
  hideDiscussion,
  graduate,
  showRail,
  versionRows,
  editSource,
  withStyles,
  pngFile,
} from "../test/newArtifactFixtures";
import {
  AUTOSAVE_DELAY_MS,
  NewArtifactWorkspace,
} from "./NewArtifactWorkspace";
import { flushLogs } from "../logging";
import { resetDraftDiscussions } from "../state/draftDiscussion";
import { DraftSessionStore } from "../state/draftSessions";
import { hunkCandidateBuffers } from "../state/candidateBuffers";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { resetDiscussionFocus } from "../state/discussionFocus";

const { stub } = makeStubs(invokeMock);

const savedBodies = () =>
  invokeMock.mock.calls
    .filter((c) => c[0] === "save_draft_file_contents")
    .map((c) => {
      const { path, body } = c[1] as { path: string; body: string };
      return { path, body };
    });

beforeEach(() => {
  invokeMock.mockReset();
  listeners.clear();
  // DCR-FR-25: a candidate buffer outlives every surface by design, so it
  // carries from one test into the next unless a suite drops it.
  hunkCandidateBuffers.clear();
  // DDS-FR-XQMF / SNV-FR-08: whether a draft's discussion column is hidden is
  // kept per draft and restored across sessions, so it outlives a surface the
  // same way and carries from one case into the next unless a suite drops it.
  resetDraftDiscussions();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  stub();
});

afterEach(cleanup);

describe("New Artifact workspace (NAW-new-artifact.md)", () => {
  // --- layout (NAW-FR-05 / NAW-FR-08) -------------------------------------

  it("NAW-FR-01, NAW-FR-03, NAW-FR-04, NAW-FR-05, NAW-FR-06, NAW-FR-07, NAW-FR-08, NAW-FR-37, NTA-FR-14: opens on the draft's first file with the rail hidden", async () => {
    renderWorkspace();

    // NAW-FR-11: the Editor's own surface, not a lookalike — its mode toggle
    // is what says so.
    await screen.findByRole("button", { name: "Edit as Markdown source" });
    // NAW-FR-08: the whole rail is gone, not merely its tree — and nothing
    // stands in its place either. A gutter left behind to hold the toggle is
    // width the editing surface was promised, so the editor is the body's only
    // child rather than its last one.
    expect(document.querySelector(".draft-rail")).toBeNull();
    const body = document.querySelector(".draft-workspace__body")!;
    // ACT-FR-09: the split, and the one control whose corner is the tab's own
    // rather than either column's. Nothing else — a gutter left behind to hold
    // a toggle is width the editing surface was promised.
    expect(body.children).toHaveLength(2);
    expect(body.lastElementChild!.querySelector(".draft-actions__toggle"))
      .not.toBeNull();
    // ACT-FR-09 / NAW-FR-27: and it keeps that place when the History rail
    // comes back beside the columns, the corner being the tab's own.
    expect(document.querySelector(".draft-editor .draft-actions__toggle"))
      .toBeNull();
    // DDS-FR-KTVW: what the body holds is the two-column split, and the split
    // holds the document column, the splitter, and the discussion column — both
    // columns visible whenever the tab is.
    const split = body.firstElementChild!;
    expect(split).toHaveClass("dds-split");
    expect(split.children).toHaveLength(3);
    expect(split.firstElementChild).toHaveClass("draft-editor");
    expect(screen.getByTestId("draft-discussion-column")).toBeInTheDocument();
    // NAW-FR-06: the draft's one prompt is what loaded — there is nothing to
    // choose, and no file tree was read to find it.
    const load = invokeMock.mock.calls.find(
      (c) => c[0] === "load_draft_file_contents",
    );
    expect((load![1] as { path: string }).path).toBe(PROMPT);
    expect(invokeMock.mock.calls.some((c) => c[0] === "list_draft_files")).toBe(
      false,
    );
  });

  /**
   * DRS-FR-39, SET-FR-19 / NAW-FR-06: a draft created from a configured template opens on
   * the template's text, and the tab offers no control that reapplies,
   * refreshes, or reveals it — the copy is starting content and nothing more,
   * taken once at creation by the creation operation itself (DRS-FR-39).
   */
  it("NAW-FR-06, DRS-FR-39, SET-FR-19: the tab opens on the prompt's text and offers no template control", async () => {
    const template = "# Context\n\n## Decision\n";
    // The prompt was created holding the project's template; the tab reads it
    // like any other prompt and knows nothing about where the bytes came from.
    const base = invokeMock.getMockImplementation()!;
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) =>
      cmd === "load_draft_file_contents"
        ? { body: template, checksum: "c1" }
        : base(cmd, args),
    );
    renderWorkspace();

    fireEvent.click(
      await screen.findByRole("button", { name: "Edit as Markdown source" }),
    );
    const source = (await screen.findByLabelText(
      "Markdown source",
    )) as HTMLTextAreaElement;
    expect(source.value).toBe(template);

    // Nothing anywhere in the tab is about the template.
    expect(screen.queryByText(/template/i)).toBeNull();
    expect(screen.queryByRole("button", { name: /template/i })).toBeNull();
    // And the tab never reads the project's template for itself.
    expect(
      invokeMock.mock.calls.some((c) => c[0] === "load_project_config"),
    ).toBe(false);
  });

  it("NAW-FR-05, NAW-FR-07, NAW-FR-08, NAW-FR-27: the rail's toggle shows and hides it, and the state is per draft", async () => {
    const drafts = new DraftSessionStore();
    const { view } = renderWorkspace(drafts);

    const rail = await showRail();
    // "History" is the rail's own static heading; the rows under it come from
    // `"list draft history"`, which arrives after the rail can be opened.
    expect(within(rail).getByText("History")).toBeInTheDocument();
    expect(await within(rail).findByText("Live prompt")).toBeInTheDocument();

    await userEvent.click(screen.getByRole("button", { name: "Hide History" }));
    expect(document.querySelector(".draft-rail")).toBeNull();

    // Shown again, then unmounted and remounted: the state belongs to the
    // draft, not to this component (NAW-FR-08).
    await showRail();
    view.unmount();
    render(
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
    await waitFor(() =>
      expect(document.querySelector(".draft-rail")).not.toBeNull(),
    );
  });

  it("NAW-FR-08, NAW-FR-38: the toggle carries the number of versions the draft holds", async () => {
    stub({ history: threeVersions() });
    renderWorkspace();
    await screen.findByRole("button", { name: "Show History" });

    // The toggle renders with the workspace, but its count comes from the
    // history list, which arrives separately — so the button existing is a
    // weaker condition than the one asserted here, and waiting on it alone can
    // read the toggle while it still carries no count at all (NAW-FR-41).
    await waitFor(() => {
      const toggle = screen.getByRole("button", { name: "Show History" });
      expect(within(toggle).getByText("3")).toBeInTheDocument();
      expect(toggle).toHaveAttribute("title", "3 versions of this draft");
    });
  });

  it("NAW-FR-05, NAW-FR-07, NAW-FR-08, NAW-FR-27: the toggle is at the head of the action row, not beside the surface", async () => {
    // NAW-FR-08: it belongs to the row the draft's own actions are on. Rendered
    // beside the editing surface instead, it reads as a control of the text, and
    // it leaves a gutter behind that a hidden rail is supposed to have given up.
    renderWorkspace();
    const toggle = await screen.findByRole("button", { name: "Show History" });

    const chrome = document.querySelector(".draft-workspace__chrome");
    expect(chrome).toContainElement(toggle);
    // At the head of it: nothing of the draft's own chrome precedes it.
    expect(chrome!.firstElementChild).toBe(toggle);
  });

  it("NAW-FR-15, NAW-FR-36, ACT-FR-13, ACT-FR-14: the editing surface may shrink, so the dock keeps its space", async () => {
    // The mechanism behind NAW-FR-27's dock, which is the part jsdom can see.
    //
    // The Editor's root carries `height: 100%` and states no minimum, so as a
    // flex item in this column its automatic minimum is the whole document's
    // height: it refuses to shrink and the dock beneath it is laid out past the
    // foot of the column, off the bottom of the window, the moment a draft
    // grows beyond one screen. `min-height: 0` on the surface is what buys the
    // dock its space back. There is no layout here to measure — `auto` instead
    // of `0px` IS the defect, and it is the whole of it.
    const drop = withStyles();
    try {
      renderWorkspace();
      await screen.findByRole("button", { name: "Edit as Markdown source" });

      const surface = document.querySelector(".draft-editor__surface");
      expect(surface).not.toBeNull();
      expect(getComputedStyle(surface!).minHeight).toBe("0px");
      // NAW-FR-27: the strip of field held open for the collapsed control is a
      // CONSTANT — opening the control's actions or its composer must not
      // resize the page under the author. Those two surfaces overlay instead,
      // and `data-overlay` gives the page more to SCROLL while one is open
      // rather than less to occupy. The heights are pinned against the
      // stylesheet in `style-invariants.test.ts`; the hook they hang off is
      // what is asserted here, without which those rules select nothing.
      const dock = document.querySelector(".draft-editor") as HTMLElement;
      expect(dock).toHaveAttribute("data-overlay", "off");
      await openActions();
      expect(dock).toHaveAttribute("data-overlay", "on");
      // ACT-FR-QWNP: Discuss opens no surface over the page here — it collapses
      // the control and moves focus to the column — so the page has nothing left
      // lying over its trailing edge and needs nothing extra to scroll. It is
      // the hidden column that makes the action available at all, so the menu is
      // closed, the column hidden, and the menu opened again.
      fireEvent.keyDown(window, { key: "Escape" });
      await waitFor(() => expect(dock).toHaveAttribute("data-overlay", "off"));
      await hideDiscussion();
      await openActions();
      await userEvent.click(screen.getByRole("menuitem", { name: "Discuss" }));
      await waitFor(() => expect(dock).toHaveAttribute("data-overlay", "off"));
      await openActions();
      expect(dock).toHaveAttribute("data-overlay", "on");
      fireEvent.keyDown(window, { key: "Escape" });
      await waitFor(() => expect(dock).toHaveAttribute("data-overlay", "off"));
      // ACT-FR-09: the control is outside the document column altogether, in
      // the tab's own body. Nested in the surface it would scroll away with the
      // text; nested in the column it would move every time the discussion
      // column was shown, hidden, or dragged.
      expect(surface!.querySelector(".draft-actions__toggle")).toBeNull();
      expect(dock.querySelector(".draft-actions__toggle")).toBeNull();
      const body = document.querySelector(".draft-workspace__body")!;
      expect(body.querySelector(".draft-actions__toggle")).not.toBeNull();
    } finally {
      drop();
    }
  });

  it("ACT-FR-QWNP, ACT-FR-23, NAW-FR-34: Discuss is disabled while the column already stands, and says why", async () => {
    // The conversation is a column of this tab, so with that column shown the
    // action has nothing left to do. Disabled rather than absent (ACT-FR-06),
    // so what a draft affords stays legible, and the reason is on the control
    // rather than left for the author to work out from nothing happening.
    renderWorkspace();
    await screen.findByRole("button", { name: "Edit as Markdown source" });
    await openActions();
    const discuss = screen.getByRole("menuitem", { name: "Discuss" });
    expect(discuss).toBeDisabled();
    expect(discuss).toHaveAttribute(
      "title",
      "The discussion is already beside the draft",
    );
    // ACT-FR-05: disabled rather than absent, and the entries beside it are
    // untouched — only Discuss has nothing left to do. An entry that vanished
    // would say the draft no longer affords it at all.
    expect(
      screen.getAllByRole("menuitem").map((e) => e.textContent?.trim()),
    ).toEqual(["Discuss", "Graduate", "Publish to GitHub", "Archive"]);
    expect(screen.getByRole("menuitem", { name: "Graduate" })).toBeEnabled();
    expect(screen.getByRole("menuitem", { name: "Archive" })).toBeEnabled();
  });

  it("ACT-FR-06, ACT-FR-23: every state that disables Discuss gives its own reason", async () => {
    // A disabled entry carrying the reason of an enabled one tells the author
    // nothing. Graduate opens a modal over the tab, which makes every operation
    // of the control inert (CMT-FR-33) — and that is a different reason from
    // the conversation already standing beside the draft.
    renderWorkspace();
    await screen.findByRole("button", { name: "Edit as Markdown source" });
    await hideDiscussion();
    await openActions();
    expect(screen.getByRole("menuitem", { name: "Discuss" })).toHaveAttribute(
      "title",
      "Discuss this with the agents you address",
    );

    await userEvent.click(screen.getByRole("menuitem", { name: "Graduate" }));
    await screen.findByRole("dialog");
    await openActions();
    const discuss = screen.getByRole("menuitem", { name: "Discuss" });
    expect(discuss).toBeDisabled();
    expect(discuss).toHaveAttribute(
      "title",
      "Finish what is open over this tab first",
    );
  });

  it("ACT-FR-QWNP, DDS-FR-JWNC: with the column hidden, Discuss brings it back and puts the caret in it", async () => {
    renderWorkspace();
    await screen.findByRole("button", { name: "Edit as Markdown source" });
    await hideDiscussion();
    // DDS-FR-XQMF: hidden is out of the layout rather than merely narrow — the
    // document column takes the tab's whole width.
    expect(screen.getByTestId("draft-discussion-column")).not.toBeVisible();
    expect(screen.getByTestId("draft-discussion-split")).toHaveAttribute(
      "data-discussion",
      "hidden",
    );

    await openActions();
    const discuss = screen.getByRole("menuitem", { name: "Discuss" });
    expect(discuss).toBeEnabled();
    await userEvent.click(discuss);

    // DDS-FR-JWNC: the column is back, and the caret is in its composer.
    await waitFor(() =>
      expect(screen.getByTestId("draft-discussion-column")).toBeVisible(),
    );
    await waitFor(() =>
      expect(
        screen.getByRole("textbox", { name: "Discuss this draft" }),
      ).toHaveFocus(),
    );
    // …and the action row's own control agrees, the two reaching one state
    // rather than each holding its own (DDS-FR-XQMF).
    expect(screen.getByTestId("draft-discussion-toggle")).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    // Having brought the column back, the action has nothing left to do.
    await openActions();
    expect(screen.getByRole("menuitem", { name: "Discuss" })).toBeDisabled();
  });

  it("NAW-FR-15, ACT-FR-QWNP: Discuss moves focus to the column's composer, which invokes nothing as it is typed in", async () => {
    renderWorkspace();
    await screen.findByRole("button", { name: "Edit as Markdown source" });

    // ACT-FR-QWNP: the action is reachable while the column is hidden, which is
    // the state it exists to leave.
    await hideDiscussion();
    await openActions();
    await userEvent.click(screen.getByRole("menuitem", { name: "Discuss" }));

    // NAW-FR-15 / DDS-FR-KTVW: the composer stands in the discussion column
    // rather than opening over the draft. On a draft with nothing said about it
    // yet, it is the composer that opens the first discussion.
    const composer = (await screen.findByRole("textbox", {
      name: "Discuss this draft",
    })) as HTMLTextAreaElement;

    // ACT-FR-QWNP: no second composer opened over the draft, and the control
    // collapsed (NAW-FR-30). The caret is in the column, where the conversation
    // this message joins is already on screen.
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();
    expect(document.querySelector(".draft-composer")).toBeNull();
    expect(
      screen.getAllByRole("textbox", { name: "Discuss this draft" }),
    ).toHaveLength(1);
    await waitFor(() => expect(composer).toHaveFocus());

    // Cleared once the mount has settled, so what follows is the composer's
    // doing alone. The log batch is drained first: showing the column is a
    // decision worth a record, and a record still in the batch would land in
    // the middle of the typing below and read as the composer's doing.
    flushLogs();
    invokeMock.mockClear();
    await userEvent.type(composer, "rewrite the intent");
    fireEvent.keyDown(composer, { key: "Enter" });

    expect(composer.value).toBe("rewrite the intent");
    // Nothing was sent, nothing was written, no integration was contacted —
    // typing here reaches no backend operation at all.
    expect(invokeMock.mock.calls.map((c) => c[0])).toEqual([]);
  });

  it("NAW-FR-15, DDS-FR-KTVW, ACT-FR-QWNP: the column's composer keeps what is written in it, and no dismissal takes it away", async () => {
    // The composer is part of the column rather than a transient surface over
    // the draft, so the things that used to dismiss it — Escape, and a press
    // elsewhere in the tab — leave it and everything in it exactly as it is.
    // That is the point of moving it: a half-written message is not something
    // an author should be able to lose by clicking on their own document.
    renderWorkspace();
    const composer = await screen.findByRole("textbox", {
      name: "Discuss this draft",
    });
    await userEvent.type(composer, "rework the graduation part");
    await act(async () => {
      fireEvent.change(screen.getByTestId("comment-attach-file-input"), {
        target: { files: [pngFile("picked.png")] },
      });
    });
    expect(await screen.findByText("picked.png")).toBeInTheDocument();

    fireEvent.mouseDown(document.querySelector(".draft-editor__surface")!);
    fireEvent.keyDown(window, { key: "Escape" });

    expect(
      screen.getByRole("textbox", { name: "Discuss this draft" }),
    ).toHaveValue("rework the graduation part");
    expect(screen.getByText("picked.png")).toBeInTheDocument();
  });

  it("NAW-FR-30, NAW-FR-18, ACT-FR-11, ACT-FR-12, ACT-FR-QWNP, SNV-FR-56: the control opens no composer of its own, and its own surfaces stay mutually exclusive", async () => {
    // ACT-FR-QWNP: on this tab the conversation is already rendered, so Discuss
    // opens nothing over the draft — it collapses the control and puts the caret
    // in the column. NAW-FR-30's mutual exclusion still governs the control's
    // own surfaces; there is simply one fewer of them here.
    renderWorkspace();
    await hideDiscussion();
    await openActions();
    await userEvent.click(screen.getByRole("menuitem", { name: "Discuss" }));
    await waitFor(() =>
      expect(screen.queryByRole("menu")).not.toBeInTheDocument(),
    );
    expect(document.querySelector(".draft-composer")).toBeNull();
    await waitFor(() =>
      expect(
        screen.getByRole("textbox", { name: "Discuss this draft" }),
      ).toHaveFocus(),
    );

    // Reopening the control changes nothing about the column's composer.
    await openActions();
    expect(screen.getByRole("menu")).toBeInTheDocument();
    expect(
      screen.getByRole("textbox", { name: "Discuss this draft" }),
    ).toBeInTheDocument();

    expect(
      invokeMock.mock.calls.some((c) => c[0] === "set_draft_status"),
    ).toBe(false);
  });

  it("NAW-FR-15: the composer's text is not part of any draft file", async () => {
    renderWorkspace();
    const composer = await screen.findByRole("textbox", {
      name: "Discuss this draft",
    });
    await userEvent.type(composer, "not a document");

    await editSource("real content");
    await waitFor(() => expect(savedBodies().length).toBeGreaterThan(0));
    expect(savedBodies().every((s) => !s.body.includes("not a document"))).toBe(
      true,
    );
  });

  // --- the editing surface (NAW-FR-11 / NAW-FR-12) -------------------------

  it("NAW-FR-11, EDT-FR-17, EDT-FR-18, EFR-FR-AYNZ: the surface is the Editor's two-mode Markdown surface", async () => {
    renderWorkspace();

    // EDT-FR-17: WYSIWYG by default, with the raw-Markdown mode behind the same
    // toggle in the same place.
    const toModeSource = await screen.findByRole("button", {
      name: "Edit as Markdown source",
    });
    expect(screen.getByLabelText("artifact body")).toBeInTheDocument();
    // EFR-FR-ABVQ: the formatting toolbar occupies the band above the surface.
    expect(screen.getByTitle("Bold")).toBeInTheDocument();

    fireEvent.click(toModeSource);
    const source = await screen.findByLabelText("Markdown source");
    expect((source as HTMLTextAreaElement).value).toBe("body text");
    // And back, without losing the document.
    fireEvent.click(screen.getByRole("button", { name: "Edit as rich text" }));
    expect(await screen.findByLabelText("artifact body")).toBeInTheDocument();
  });

  it("NAW-FR-13, NAW-FR-36, NAW-FR-37: the surface presents no Save control", async () => {
    renderWorkspace();
    await screen.findByRole("button", { name: "Edit as Markdown source" });

    expect(screen.queryByRole("button", { name: "Save" })).not.toBeInTheDocument();
  });

  // NOT conformance with NAW-FR-14, which requires the margin to carry the
  // draft file's review threads. This pins a deliberate, temporary scope
  // reduction: the *anchored* threads of a draft file are served by the DRAFT
  // scope of the thread operations (`CMS-comments-storage.md` CMS-FR-36) and
  // this surface does not distinguish it yet, so the Editor's own rail stays off
  // rather than reading and writing ARTIFACT-scoped threads against a draft's
  // key. NAW-FR-14, NAW-FR-09 and CMT-FR-37, CMS-FR-36, NAW-FR-20, CMS-FR-39 are unimplemented until it does.
  //
  // The draft's DISCUSSIONS are served (CMT-FR-53), which is a different
  // operation over a different log — hence the two assertions below rather than
  // "no comment operation at all".
  it("NAW-FR-13, NAW-FR-36, NAW-FR-37: the draft writes itself once the typing stops, not mid-word", async () => {
    vi.useFakeTimers();
    try {
      renderWorkspace();
      await vi.waitFor(() =>
        expect(
          screen.queryByRole("button", { name: "Edit as Markdown source" }),
        ).not.toBeNull(),
      );
      fireEvent.click(
        screen.getByRole("button", { name: "Edit as Markdown source" }),
      );
      const source = await vi.waitFor(() =>
        screen.getByLabelText("Markdown source"),
      );

      fireEvent.change(source, { target: { value: "hel" } });
      // Mid-word: nothing has been written, and the surface reports it pending.
      await vi.advanceTimersByTimeAsync(AUTOSAVE_DELAY_MS - 100);
      expect(savedBodies()).toHaveLength(0);
      expect(screen.getByText("Saving…")).toBeInTheDocument();

      // The next keystroke restarts the rest, so the burst is still one write.
      fireEvent.change(source, { target: { value: "hello" } });
      await vi.advanceTimersByTimeAsync(AUTOSAVE_DELAY_MS - 100);
      expect(savedBodies()).toHaveLength(0);

      await vi.advanceTimersByTimeAsync(200);
      expect(savedBodies()).toEqual([{ path: PROMPT, body: "hello" }]);
      await vi.waitFor(() =>
        expect(screen.getByText("Saved")).toBeInTheDocument(),
      );
    } finally {
      vi.useRealTimers();
    }
  });

  it("NAW-FR-13: the write report is laid out in the action row, not floated over it", async () => {
    // Placed over the row, the report lands in its trailing corner — which is
    // exactly where the rendering toggle sits, in the Editor's action row and in
    // the reading's strip alike. jsdom does no layout, so what is asserted is
    // the two things the collision rested on: that the report is positioned at
    // all, and that it is a sibling of the control it covered rather than a
    // child of the row.
    const drop = withStyles();
    try {
      stub({ history: threeVersions() });
      renderWorkspace();
      await screen.findByRole("button", { name: "Edit as Markdown source" });
      await editSource("words the author typed");

      const report = await screen.findByText("Saving…");
      expect(getComputedStyle(report).position).not.toBe("absolute");
      // In the row's own trailing cluster, ahead of the mode toggle.
      const cluster = report.closest(".editor__actions");
      expect(cluster).not.toBeNull();
      // `editSource` put the surface in raw mode, so the toggle now offers the
      // way back — it is the same control either way.
      const toggle = within(cluster as HTMLElement).getByRole("button", {
        name: /^Edit as/,
      });
      expect(
        report.compareDocumentPosition(toggle) &
          Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();
      // …and the Editor's own unsaved badge is not doubled beside it: the two
      // say the same thing about the same buffer.
      expect(screen.queryByText(/unsaved/)).toBeNull();

      // The same report, in the reading's strip, while a version is showing —
      // an autosave can land while the author is reading one.
      await showRail();
      await waitFor(() => expect(versionRows()).toHaveLength(3));
      await userEvent.click(versionRows()[0]);
      await screen.findByTestId("draft-version-reading");

      const inStrip = await screen.findByText(/^Sav/);
      const strip = inStrip.closest(".draft-reading");
      expect(strip).not.toBeNull();
      expect(getComputedStyle(inStrip).position).not.toBe("absolute");
      const readingToggle = within(strip as HTMLElement).getByRole("button", {
        name: /^View as/,
      });
      expect(
        inStrip.compareDocumentPosition(readingToggle) &
          Node.DOCUMENT_POSITION_FOLLOWING,
      ).toBeTruthy();
    } finally {
      drop();
    }
  });

  it("NAW-FR-13: a failed write keeps the content in front of the author", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "open_draft") return draft();
      if (cmd === "list_draft_history") return freshHistory();
      if (cmd === "load_draft_file_contents")
        return { body: "body text", checksum: "c1" };
      if (cmd === "save_draft_file_contents") throw "disk full";
      return undefined;
    });
    const { drafts } = renderWorkspace();
    await screen.findByRole("button", { name: "Edit as Markdown source" });
    await editSource("still here");

    await screen.findByText(/Could not write this draft's prompt/);
    expect(drafts.isDirty("d1")).toBe(true);
    expect(drafts.docs.get(drafts.key("d1", PROMPT))?.buffer).toBe("still here");
  });

  it("NAW-FR-12 / NAW-FR-23: an unmount mid-edit leaves the buffer for the shell", async () => {
    const { drafts, view } = renderWorkspace();
    await screen.findByRole("button", { name: "Edit as Markdown source" });
    await editSource("half a sentence");

    view.unmount();

    // The shell can still write it, because the state never lived in the tab.
    expect(drafts.isDirty("d1")).toBe(true);
    await drafts.flush("d1");
    const writes = savedBodies();
    expect(writes[writes.length - 1]).toEqual({
      path: PROMPT,
      body: "half a sentence",
    });
  });

  // --- the empty draft (NAW-FR-10) -----------------------------------------

  it("NAW-FR-05, NAW-FR-17, NAW-FR-18, NAW-FR-27, NAW-FR-28, NAW-FR-34: the chrome carries no lifecycle control, and the three actions are behind one", async () => {
    renderWorkspace();
    await screen.findByRole("button", { name: /artifact-window/ });

    // NAW-FR-05: no status control, no Graduate button, no readiness
    // declaration anywhere in the tab's chrome.
    const chrome = document.querySelector(
      ".draft-workspace__chrome",
    ) as HTMLElement;
    // Matched loosely: a button reading " Graduate " or carrying an icon is
    // the same lifecycle control this requirement takes out of the chrome.
    expect(
      within(chrome).queryByRole("button", { name: /graduate|archive|ready/i }),
    ).toBeNull();
    expect(screen.queryByText(/Mark ready/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/ready for implementation/i)).not.toBeInTheDocument();

    // NAW-FR-28: exactly four named entries, in this order.
    const menu = await openActions();
    expect(
      within(menu)
        .getAllByRole("menuitem")
        .map((b) => b.textContent?.trim()),
    ).toEqual(["Discuss", "Graduate", "Publish to GitHub", "Archive"]);
    // NAW-FR-17: enabled, with nothing declared beforehand.
    expect(screen.getByRole("menuitem", { name: "Graduate" })).toBeEnabled();

    await userEvent.click(screen.getByRole("menuitem", { name: "Graduate" }));
    expect(
      await screen.findByRole("dialog", { name: /Graduate .artifact-window./ }),
    ).toBeInTheDocument();
  });

  it("NAW-FR-27, NAW-FR-30, ACT-FR-03: the control expands, collapses, and dismisses on Escape and an outside press", async () => {
    renderWorkspace();
    const toggle = await screen.findByRole("button", { name: "Draft actions" });
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();

    // Expanded, the control itself becomes the dismissal.
    await userEvent.click(toggle);
    expect(screen.getByRole("menu")).toBeInTheDocument();
    await userEvent.click(
      screen.getByRole("button", { name: "Close draft actions" }),
    );
    expect(screen.queryByRole("menu")).not.toBeInTheDocument();

    await openActions();
    fireEvent.keyDown(window, { key: "Escape" });
    await waitFor(() =>
      expect(screen.queryByRole("menu")).not.toBeInTheDocument(),
    );

    // NAW-FR-27: a pointer-down anywhere else in the tab collapses it, and the
    // press still reaches what it landed on.
    await openActions();
    fireEvent.mouseDown(document.querySelector(".draft-editor__surface")!);
    await waitFor(() =>
      expect(screen.queryByRole("menu")).not.toBeInTheDocument(),
    );

    // Nothing any of that did reached the backend.
    expect(
      invokeMock.mock.calls.some((c) => c[0] === "set_draft_status"),
    ).toBe(false);
  });

  it("NAW-FR-16, NAW-FR-28, NAW-FR-36: Archive writes the pending file, sets `archived`, and closes the tab", async () => {
    stub();
    const { onArchived, onDraftChanged } = renderWorkspace();
    // The action control renders before the file has loaded, so waiting on it
    // would leave the edit below racing the Editor's own mount.
    await screen.findByRole("button", { name: "Edit as Markdown source" });
    await editSource("half a paragraph");

    await openActions();
    // Cleared immediately before the action, so the ordering below is
    // Archive's own doing. Without this the autosave timer (AUTOSAVE_DELAY_MS)
    // can fire first on a slow machine, leaving the buffer clean by the time
    // `setStatus` runs — and the assertion then passes without Archive's
    // pre-save having executed at all.
    invokeMock.mockClear();
    await userEvent.click(screen.getByRole("menuitem", { name: "Archive" }));

    await waitFor(() => expect(onArchived).toHaveBeenCalled());
    // The write comes first: the tab closes behind this, and a skipped write is
    // the one way the action can lose the author's last paragraph. The rail's
    // own re-read sits between them — a landed write is what moves the modified
    // marker (NAW-FR-37) — so the ordering asserted is of the two that matter.
    const order = invokeMock.mock.calls
      .map((c) => c[0])
      .filter((c) => c === "save_draft_file_contents" || c === "set_draft_status");
    expect(order.slice(0, 2)).toEqual([
      "save_draft_file_contents",
      "set_draft_status",
    ]);
    expect(savedBodies()[0].body).toBe("half a paragraph");
    expect(
      invokeMock.mock.calls.find((c) => c[0] === "set_draft_status")?.[1],
    ).toEqual({ id: "d1", status: "archived" });
    // NAW-FR-16 / NAW-FR-23: archiving removes nothing.
    expect(order).not.toContain("delete_draft");
    expect(order).not.toContain("delete_draft_path");
    expect(onDraftChanged).toHaveBeenCalled();
  });

  it("NAW-FR-16, NAW-FR-17, NAW-FR-28: an archived draft opens marked, restores in place, and still graduates", async () => {
    stub({ status: "archived" });
    const { onArchived } = renderWorkspace();

    // NAW-FR-16: marked beside the label, and the surface behaves as it does
    // for an active draft.
    expect(await screen.findByText("Archived")).toBeInTheDocument();
    expect(await screen.findByLabelText("artifact body")).toBeInTheDocument();

    // NAW-FR-17: graduation carries no precondition on the status.
    await graduate();
    const dialog = await screen.findByRole("dialog", {
      name: /Graduate .artifact-window./,
    });
    await userEvent.click(within(dialog).getByRole("button", { name: "Cancel" }));

    // NAW-FR-28: the fourth entry reads Restore, and the tab stays open.
    const menu = await openActions();
    expect(within(menu).queryByRole("menuitem", { name: "Archive" })).toBeNull();
    await userEvent.click(within(menu).getByRole("menuitem", { name: "Restore" }));

    await waitFor(() =>
      expect(screen.queryByText("Archived")).not.toBeInTheDocument(),
    );
    expect(
      invokeMock.mock.calls.find((c) => c[0] === "set_draft_status")?.[1],
    ).toEqual({ id: "d1", status: "active" });
    expect(onArchived).not.toHaveBeenCalled();
  });







  it("NAW-FR-44, NAW-FR-13, NAW-FR-28: a graduated draft is read-only while that status stands", async () => {
    // NAW-FR-44 / DRS-FR-20: not archiving and not deletion — the prompt is
    // still here and still readable, and it stays read-only for as long as a
    // run stands for the specification it published (DRS-FR-KQTW).
    stub({
      status: "graduated",
      graduation: {
        runId: "run-9",
        state: "implemented",
        locked: false,
        graduated: true,
      },
    });
    renderWorkspace();

    expect(
      await screen.findByText(/kept as the record of what the specification/),
    ).toBeInTheDocument();
    await openActions();
    expect(screen.getByRole("menuitem", { name: "Graduate" })).toBeDisabled();
    expect(screen.getByRole("menuitem", { name: "Archive" })).toBeDisabled();
  });



  it("NAW-FR-BJQX: a draft with no run at all carries no run-state tag", async () => {
    stub({ graduation: null });
    renderWorkspace();

    await screen.findByRole("button", { name: /artifact-window/ });
    // The tag is the one route this tab offers into a run, so a draft no run
    // holds offers none. There is no second affordance beside it: one run does
    // the whole of the work, and the tag is how it is reached.
    expect(screen.queryByTestId("draft-handoff-open")).toBeNull();
    expect(screen.queryByTestId("draft-run-state")).toBeNull();
  });


  it("NAW-FR-04, NAW-FR-25, NAW-FR-26, DRP-FR-11: renaming the draft reports the new name to the strip", async () => {
    const { onNameChanged } = renderWorkspace();

    await userEvent.click(
      await screen.findByRole("button", { name: /artifact-window/ }),
    );
    const field = screen.getByLabelText("Draft name");
    await userEvent.clear(field);
    // Deliberately not what the stub answers with: the typed text and the
    // stored name differ here, so reporting `name` or the field's own value
    // instead of `updated.name` fails rather than passing by coincidence.
    await userEvent.type(field, "agent personas{Enter}");

    await waitFor(() => expect(onNameChanged).toHaveBeenCalledWith(RENAMED));
    const call = invokeMock.mock.calls.find((c) => c[0] === "rename_draft");
    expect((call![1] as { name: string }).name).toBe("agent personas");
  });

  it("NAW-FR-04: committing an empty or unchanged name invokes nothing", async () => {
    // DRP-FR-11's other half, on this surface: an empty commit leaves the
    // stored name untouched rather than being sent to be refused.
    const { onNameChanged } = renderWorkspace();

    await userEvent.click(
      await screen.findByRole("button", { name: /artifact-window/ }),
    );
    const field = screen.getByLabelText("Draft name");
    await userEvent.clear(field);
    await userEvent.type(field, "   {Enter}");

    expect(invokeMock.mock.calls.some((c) => c[0] === "rename_draft")).toBe(false);
    expect(onNameChanged).not.toHaveBeenCalled();
    // The field closes and the chrome reads the name it had.
    expect(screen.queryByLabelText("Draft name")).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: /artifact-window/ })).toBeInTheDocument();

    // And a commit of the unchanged name is likewise not a rename.
    await userEvent.click(screen.getByRole("button", { name: /artifact-window/ }));
    await userEvent.type(screen.getByLabelText("Draft name"), "{Enter}");
    expect(invokeMock.mock.calls.some((c) => c[0] === "rename_draft")).toBe(false);
  });

  it("NAW-FR-25: a rename made elsewhere moves this tab's prompt and its writes", async () => {
    // The Drafts panel renames the draft — and with it the prompt. A tab told
    // only about the name would go on writing the author's typing to the old
    // path, which recreates the file the rename moved away and leaves the draft
    // holding two (DRS-FR-11's own refusal).
    let promptPath = "Untitled.md";
    let name = "Untitled";
    invokeMock.mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case "open_draft":
          return { ...draft(), name, promptPath };
        case "list_draft_history":
          return freshHistory();
        case "load_draft_file_contents":
          return { body: "body text", checksum: "c1" };
        case "save_draft_file_contents":
          return { checksum: "c2" };
        default:
          return undefined;
      }
    });
    const { drafts, renameFromElsewhere } = renderNamed("Untitled");

    await waitFor(() => expect(drafts.get("d1")?.selected).toBe("Untitled.md"));
    await editSource("words typed before the rename");

    // The panel's rename has landed on disk; the shell reports the new name and
    // bumps the revision, exactly as `drafts-changed` does.
    promptPath = "overview.md";
    name = "overview";
    renameFromElsewhere("overview");

    // The selection and the unsaved buffer followed the file rather than being
    // stranded on a path that no longer exists.
    await waitFor(() => expect(drafts.get("d1")?.selected).toBe("overview.md"));
    expect(drafts.docs.get(drafts.key("d1", "overview.md"))?.buffer).toBe(
      "words typed before the rename",
    );

    // And the pending write goes to the new path — writing the old one would
    // recreate the file the rename moved away.
    invokeMock.mockClear();
    await new Promise((resolve) => setTimeout(resolve, AUTOSAVE_DELAY_MS + 50));
    await waitFor(() =>
      expect(
        invokeMock.mock.calls.filter((c) => c[0] === "save_draft_file_contents"),
      ).not.toHaveLength(0),
    );
    expect(savedBodies().map((s) => s.path)).not.toContain("Untitled.md");
    expect(savedBodies().map((s) => s.path)).toContain("overview.md");
  });

});
