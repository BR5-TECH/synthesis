/**
 * The History rail of the New Artifact workspace, and the pure helpers it reads
 * (`../../specifications/ui/NAW-new-artifact.md` NAW-FR-07 … NAW-FR-43).
 */
import { describe, expect, it, vi, beforeEach, afterEach } from "vitest";
import {
  act,
  cleanup,
  fireEvent,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";

const invokeMock = vi.fn();
vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));
/**
 * The margin follows `"discussion changed"` and `"agent turn state changed"`
 * (CMT-FR-54, CTA-FR-QXIG), so the mock keeps its handlers rather than discarding
 * them — a test fires on a channel exactly as the backend would.
 */
const listeners = new Map<string, Set<(e: { payload: unknown }) => void>>();
function fireBus(name: string, payload: unknown) {
  [...(listeners.get(name) ?? [])].forEach((h) => h({ payload }));
}
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
  draft,
  entry,
  freshHistory,
  acceptedHistory,
  threeVersions,
  makeStubs,
  renderWorkspace,
  openActions,
  showRail,
  versionRows,
  liveRow,
  editSource,
  withStyles,
} from "../test/newArtifactFixtures";
import {
  AUTOSAVE_DELAY_MS,
  NewArtifactWorkspace,
  liveMarkerFor,
  versionCountOf,
  sourceLabelOf,
  standingOf,
} from "./NewArtifactWorkspace";
import { hunkCandidateBuffers } from "../state/candidateBuffers";
import { clearAllDiscussionSessions } from "../state/discussionSession";
import { resetDiscussionFocus } from "../state/discussionFocus";

const { stub } = makeStubs(invokeMock);

/** Every `invoke` of one operation, in the order they were made. */
const callsNamed = (name: string) =>
  invokeMock.mock.calls.filter((c) => c[0] === name);

beforeEach(() => {
  invokeMock.mockReset();
  listeners.clear();
  // DCR-FR-25: a candidate buffer outlives every surface by design, so it
  // carries from one test into the next unless a suite drops it.
  hunkCandidateBuffers.clear();
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  stub();
});

afterEach(cleanup);

// ---------------------------------------------------------------------------
// The History rail (NAW-FR-07, NAW-FR-38 … NAW-FR-09, NAW-FR-10, NAW-FR-12, NAW-FR-30, NAW-FR-37, NAW-FR-08, NAW-FR-40, NAW-FR-36, DHS-FR-07, NAW-FR-35, NAW-FR-42, DCR-FR-12, DCR-FR-13, DCR-FR-24 …
// NAW-FR-08, NAW-FR-38, NAW-FR-41, DRS-FR-15, DHS-FR-21)
//
// The rail records settled versions of the prompt and nothing else (NAW-FR-36):
// each change the author accepted from an agent, the first of them settling the
// prompt it superseded as `Original` beside it.
// ---------------------------------------------------------------------------

describe("the History rail (NAW-FR-07 … NAW-FR-43)", () => {
  it("NAW-FR-07, NAW-FR-38: lists versions newest first with their source, standing, and time", async () => {
    stub({ history: threeVersions() });
    renderWorkspace();
    const rail = await showRail();

    await waitFor(() => expect(versionRows()).toHaveLength(3));
    const rows = versionRows();
    // NAW-FR-07: newest first, so the version the prompt most recently settled
    // on sits directly under the live prompt it is compared against, and the
    // `Original` closes the column.
    expect(rows[0]).toHaveTextContent("@arch");
    expect(rows[2]).toHaveTextContent("Original");
    // NAW-FR-38: the standing of each, and no row marked as the current draft.
    expect(rows[0]).toHaveTextContent("Latest accepted");
    expect(rows[1]).toHaveTextContent("Superseded");
    for (const row of rows) expect(row).not.toHaveTextContent("Current");
    // The rendered order is the reverse of what the backend returned, rather
    // than an order of its own: the newest row carries the newest timestamp.
    const stamps = rows.map(
      (r) => (r.querySelector(".draft-version__at") as HTMLElement).title,
    );
    expect(stamps).toEqual([...stamps].sort().reverse());

    // NAW-FR-07: the absolute instant on hover, and a *shorter* rendering in the
    // row — a title equal to the label would disclose nothing.
    const at = rows[0].querySelector(".draft-version__at") as HTMLElement;
    const title = at.getAttribute("title")!;
    expect(new Date(title).toISOString()).toBe("2026-07-23T08:00:00.000Z");
    expect(at.textContent).not.toBe(title);
    expect(at.textContent).toMatch(/\d/);

    // NAW-FR-07: no control that restores, deletes, renames, or edits a version.
    for (const name of [/restore/i, /delete/i, /rename/i, /revert/i]) {
      expect(within(rail).queryByRole("button", { name })).toBeNull();
    }
  });

  it("NAW-FR-37, NAW-FR-38: the live-prompt row names the baseline the prompt has moved on from", async () => {
    // Byte-identical to the newest version: rendered plainly.
    stub({ history: threeVersions() });
    const { view } = renderWorkspace();
    await showRail();
    await waitFor(() => expect(versionRows()).toHaveLength(3));
    expect(liveRow()).not.toHaveTextContent("Modified");
    view.unmount();

    // Modified since an acceptance.
    const moved = threeVersions();
    moved.live.matchesLatest = false;
    stub({ history: moved });
    const second = renderWorkspace();
    await showRail();
    await waitFor(() =>
      expect(liveRow()).toHaveTextContent("Modified since latest accepted version"),
    );
    // NAW-FR-38: the newest version keeps its label rather than having it
    // withdrawn — the author reads two facts instead of one that is false.
    expect(versionRows()[0]).toHaveTextContent("Latest accepted");
    // NAW-FR-37: announced when it changes, in words rather than by colour.
    expect(
      liveRow().querySelector(".draft-version__standing"),
    ).toHaveAttribute("role", "status");
    second.view.unmount();

    // NAW-FR-37 / DHS-FR-07: a draft nobody has proposed a change to has no
    // version to have moved on FROM — its live prompt is the `Original` — so
    // however much the author types, the row is never marked modified.
    stub({ history: freshHistory() });
    renderWorkspace();
    await showRail();
    await waitFor(() => expect(liveRow()).toHaveTextContent("Original"));
    expect(liveRow()).not.toHaveTextContent("Modified");
    expect(versionRows()).toHaveLength(0);
  });

  it("NAW-FR-01, NAW-FR-03, NAW-FR-04, NAW-FR-05, NAW-FR-06, NAW-FR-07, NAW-FR-08, NAW-FR-37, NTA-FR-14 / NAW-FR-38: a draft nobody has changed holds one version — the live prompt", async () => {
    // DHS-FR-07: the `Original` IS the live prompt until an acceptance
    // supersedes it, so the rail has one thing to show and the toggle counts it.
    // A rail that said "0 versions" would report a draft written in all
    // afternoon as having no version of its prompt at all.
    stub({ history: freshHistory() });
    renderWorkspace();

    await waitFor(() => {
      const toggle = screen.getByRole("button", { name: "Show History" });
      expect(within(toggle).getByText("1")).toBeInTheDocument();
      expect(toggle).toHaveAttribute("title", "1 version of this draft");
    });

    const rail = await showRail();
    // The live-prompt row says which version it is…
    await waitFor(() => expect(liveRow()).toHaveTextContent("Original"));
    // …and there is no column of past versions under it, because there is no
    // past: an empty list under a heading reads as a history that failed.
    expect(versionRows()).toHaveLength(0);
    expect(rail.querySelector(".draft-rail__divider")).toBeNull();
    // NAW-FR-40: still exactly one list, and no snapshot read for it.
    expect(callsNamed("load_draft_history_entry")).toHaveLength(0);

    // And once a change is accepted, the prompt it superseded takes the column
    // and the live row is the accepted version rather than the Original.
    stub({ history: acceptedHistory() });
    act(() => {
      fireBus("draft-history-changed", {
        draftId: "d1",
        entry: acceptedHistory().entries[1],
      });
    });
    await waitFor(() => expect(versionRows()).toHaveLength(2));
    expect(liveRow()).not.toHaveTextContent("Original");
    // Newest first: the version just accepted takes the row under the live
    // prompt, and the `Original` it superseded closes the column.
    expect(versionRows()[0]).toHaveTextContent("Latest accepted");
    expect(versionRows()[1]).toHaveTextContent("Original");
  });

  it("NAW-FR-39: the live-prompt row is pinned and the versions scroll beneath it", async () => {
    // jsdom does no layout, so what is asserted is the *declaration* the pinning
    // rests on — and that is the whole of the defect it guards. With the rail
    // itself as the scroller, a draft that has accumulated a dozen versions
    // scrolls the live row off the top: the row that says what the prompt now
    // says, and the one activation back to it, are gone exactly when the history
    // is long enough to be worth reading.
    const drop = withStyles();
    try {
      stub({ history: threeVersions() });
      renderWorkspace();
      const rail = await showRail();
      await waitFor(() => expect(versionRows()).toHaveLength(3));

      // The rail does not scroll…
      expect(getComputedStyle(rail).overflowY).not.toBe("auto");
      expect(getComputedStyle(rail).minHeight).toBe("0px");
      // …the column of versions does, and it is the only thing that does.
      const versions = rail.querySelector(".draft-rail__versions") as HTMLElement;
      expect(getComputedStyle(versions).overflowY).toBe("auto");
      expect(getComputedStyle(versions).minHeight).toBe("0px");
      // NAW-FR-39: and the live row is outside that column, so it cannot be
      // scrolled away from.
      expect(versions.contains(liveRow())).toBe(false);
      expect(rail.contains(liveRow())).toBe(true);
    } finally {
      drop();
    }
  });

  it("NAW-FR-09, NAW-FR-10, NAW-FR-39: selecting a version reads it read-only beside the live-prompt row", async () => {
    stub({ history: threeVersions() });
    renderWorkspace();
    await showRail();
    await waitFor(() => expect(versionRows()).toHaveLength(3));

    // The `Original` closes the column now that the newest is at its head, so
    // this addresses `e1` by where it stands rather than by a fixed index.
    await userEvent.click(versionRows()[2]);

    const reading = await screen.findByTestId("draft-version-reading");
    expect(reading).toHaveTextContent("the prompt as of e1");
    // NAW-FR-09: announced read-only in words and in accessible semantics
    // rather than by colour or a fill alone. `aria-readonly` rides on a role
    // assistive technology will actually expose it from — on a generic element
    // it is ignored, so the attribute alone would announce nothing.
    expect(reading).toHaveAttribute("aria-readonly", "true");
    expect(reading).toHaveAttribute("role", "document");
    expect(reading.getAttribute("aria-label")).toContain("Read-only past version");
    expect(screen.getByText(/Reading a past version · read-only/)).toBeInTheDocument();
    // NAW-FR-09: on the same page in the same measure — the WYSIWYG surface the
    // Editor sets the live prompt on, so the shape of the document does not
    // change between reading a version and editing the prompt.
    expect(reading).toHaveClass("doc", "editor__prose");
    expect(reading.closest(".editor")).not.toBeNull();
    // Nothing in it accepts a keystroke.
    expect(reading).toHaveAttribute("contenteditable", "false");
    // NAW-FR-39: the row is marked in the rail and the live-prompt row stays
    // rendered with its own marker throughout.
    expect(versionRows()[2]).toHaveAttribute("aria-current", "true");
    expect(liveRow()).toBeInTheDocument();
    // The live prompt's own editing surface is gone while a version is showing.
    expect(
      screen.queryByRole("button", { name: "Edit as Markdown source" }),
    ).toBeNull();
  });

  it("NAW-FR-09, NAW-FR-10, NAW-FR-39: a version is read in both of the prompt's own modes", async () => {
    // NAW-FR-09 / EDT-FR-17: a version is read the way the prompt is read —
    // rich by default with the raw Markdown behind the same toggle. A version
    // rendered only as a slab of monospace is a different document from the one
    // it is a version of, and comparing the two would mean reading past the
    // difference in presentation before reaching the difference in text.
    stub({ history: threeVersions() });
    const { drafts } = renderWorkspace();
    await showRail();
    await waitFor(() => expect(versionRows()).toHaveLength(3));
    await userEvent.click(versionRows()[2]);

    // Rich, as the live prompt is.
    const rich = await screen.findByTestId("draft-version-reading");
    expect(rich).toHaveClass("editor__prose");

    // The toggle is the Editor's own, in the strip above the page.
    const toggle = screen.getByRole("button", { name: "View as Markdown source" });
    await userEvent.click(toggle);

    const source = await screen.findByTestId("draft-version-reading");
    expect(source).toHaveClass("editor__source");
    expect(source.closest(".editor__source-wrap")).not.toBeNull();
    expect(source).toHaveTextContent("the prompt as of e1");
    // Still read-only, on both surfaces.
    expect(source).toHaveAttribute("aria-readonly", "true");
    expect(source).toHaveAttribute("role", "document");
    // And back.
    await userEvent.click(
      screen.getByRole("button", { name: "View as rich text" }),
    );
    expect(await screen.findByTestId("draft-version-reading")).toHaveClass(
      "editor__prose",
    );

    // NAW-FR-10: the live prompt's own mode was not touched by any of it — the
    // toggle over a reading is not an edit to the state a return restores.
    expect(drafts.docs.get(drafts.key("d1", PROMPT))?.mode).toBe("wysiwyg");
  });

  it("NAW-FR-09, NAW-FR-10, NAW-FR-39 / NAW-FR-12, NAW-FR-30: a version opens in the mode the prompt is being read in", async () => {
    // NAW-FR-09: the reading follows the live prompt's mode rather than
    // resetting to rich, so an author working in raw Markdown is not thrown
    // between renderings every time they look back.
    stub({ history: threeVersions() });
    const { drafts } = renderWorkspace();
    await screen.findByRole("button", { name: "Edit as Markdown source" });
    await userEvent.click(
      screen.getByRole("button", { name: "Edit as Markdown source" }),
    );
    await screen.findByLabelText("Markdown source");

    await showRail();
    await waitFor(() => expect(versionRows()).toHaveLength(3));
    await userEvent.click(versionRows()[0]);

    expect(await screen.findByTestId("draft-version-reading")).toHaveClass(
      "editor__source",
    );
    // …and the live prompt is still in the mode the author put it in.
    expect(drafts.docs.get(drafts.key("d1", PROMPT))?.mode).toBe("text");
  });

  it("NAW-FR-10: the return to the live prompt leads the reading's strip", async () => {
    // The one control the author reaches for at the end of every reading, ahead
    // of the words that describe the state rather than at the far edge of the
    // tab.
    stub({ history: threeVersions() });
    renderWorkspace();
    await showRail();
    await waitFor(() => expect(versionRows()).toHaveLength(3));
    await userEvent.click(versionRows()[0]);
    await screen.findByTestId("draft-version-reading");

    const strip = document.querySelector(".draft-reading") as HTMLElement;
    const back = within(strip).getByRole("button", {
      name: "← Back to live prompt",
    });
    const state = within(strip).getByText(/Reading a past version/);
    expect(
      back.compareDocumentPosition(state) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
    // NAW-FR-09: and the rendering toggle keeps the trailing edge, where the
    // action row's own mode toggle sits.
    const toggle = within(strip).getByRole("button", {
      name: "View as Markdown source",
    });
    expect(
      state.compareDocumentPosition(toggle) & Node.DOCUMENT_POSITION_FOLLOWING,
    ).toBeTruthy();
  });

  it("NAW-FR-41: a version that could not be read offers no rendering to choose", async () => {
    // The strip's toggle governs a reading; over a failure there is none, and a
    // control that changes nothing is a control the author will try.
    stub({ history: threeVersions() });
    invokeMock.mockImplementation(
      (function (base) {
        return async (cmd: string, args?: unknown) => {
          if (cmd === "load_draft_history_entry") throw "snapshot_corrupt";
          return base(cmd, args);
        };
      })(invokeMock.getMockImplementation()!),
    );
    renderWorkspace();
    await showRail();
    await waitFor(() => expect(versionRows()).toHaveLength(3));
    await userEvent.click(versionRows()[0]);

    await screen.findByText("⚠ This version could not be read.");
    expect(
      screen.queryByRole("button", { name: /^View as/ }),
    ).toBeNull();
    // The strip itself stays: the reading is still the state the tab is in.
    expect(screen.getByText(/Reading a past version/)).toBeInTheDocument();
  });

  it("NAW-FR-08, NAW-FR-40: switching the reading's rendering re-reads no snapshot", async () => {
    // NAW-FR-40: a version's text is fetched when that version is selected and
    // at no other moment. The two renderings are two readings of one payload,
    // not two fetches of it.
    stub({ history: threeVersions() });
    renderWorkspace();
    await showRail();
    await waitFor(() => expect(versionRows()).toHaveLength(3));
    await userEvent.click(versionRows()[0]);
    await screen.findByTestId("draft-version-reading");
    expect(callsNamed("load_draft_history_entry")).toHaveLength(1);

    await userEvent.click(
      screen.getByRole("button", { name: "View as Markdown source" }),
    );
    await screen.findByTestId("draft-version-reading");
    await userEvent.click(
      screen.getByRole("button", { name: "View as rich text" }),
    );
    await screen.findByTestId("draft-version-reading");

    expect(callsNamed("load_draft_history_entry")).toHaveLength(1);
  });

  it("NAW-FR-09, NAW-FR-10, NAW-FR-12, NAW-FR-30: the live prompt's editor state survives a reading and every route back", async () => {
    stub({ history: threeVersions() });
    const { drafts } = renderWorkspace();
    await screen.findByRole("button", { name: "Edit as Markdown source" });
    await editSource("an unsaved edit");
    await showRail();
    await waitFor(() => expect(versionRows()).toHaveLength(3));

    const back = async () => {
      await waitFor(() =>
        expect(
          screen.queryByTestId("draft-version-reading"),
        ).not.toBeInTheDocument(),
      );
      // NAW-FR-12: the buffer was never replaced and nothing was written.
      expect(drafts.docs.get(drafts.key("d1", PROMPT))?.buffer).toBe(
        "an unsaved edit",
      );
      expect(drafts.isDirty("d1")).toBe(true);
      // NAW-FR-12: the editing mode came back too — the surface is still the
      // raw-Markdown one the author left it in.
      await screen.findByLabelText("Markdown source");
    };

    // NAW-FR-10: the rail's live-prompt row.
    await userEvent.click(versionRows()[0]);
    await screen.findByTestId("draft-version-reading");
    expect(drafts.docs.get(drafts.key("d1", PROMPT))?.buffer).toBe("an unsaved edit");
    await userEvent.click(liveRow());
    await back();

    // The strip's own control.
    await userEvent.click(versionRows()[0]);
    await screen.findByTestId("draft-version-reading");
    await userEvent.click(
      screen.getByRole("button", { name: "← Back to live prompt" }),
    );
    await back();

    // Escape, from anywhere in the reading.
    await userEvent.click(versionRows()[0]);
    await screen.findByTestId("draft-version-reading");
    fireEvent.keyDown(window, { key: "Escape" });
    await back();

    // And hiding the rail, so the author cannot be left reading a version with
    // no rail to leave it by.
    await userEvent.click(versionRows()[0]);
    await screen.findByTestId("draft-version-reading");
    await userEvent.click(screen.getByRole("button", { name: "Hide History" }));
    await back();
  });

  it("NAW-FR-13, NAW-FR-36, NAW-FR-37 / NAW-FR-38: a landed write moves the marker and adds no version", async () => {
    // NAW-FR-37: the comparison is made on the bytes the save path persisted
    // rather than on the buffer, so the marker moves when the write lands and
    // not before — which is also why the rail is re-read after a write. Without
    // that re-read the row goes on reading plainly for the rest of the session,
    // and the author is told their typing has changed nothing.
    let live = { path: PROMPT, byteLen: 20, sha256: "sha-2", matchesLatest: true };
    stub({
      history: () => ({ entries: acceptedHistory().entries, live }),
    });
    renderWorkspace();
    await showRail();
    await waitFor(() => expect(versionRows()).toHaveLength(2));
    expect(liveRow()).not.toHaveTextContent("Modified");

    await screen.findByRole("button", { name: "Edit as Markdown source" });
    await editSource("words the author typed");
    // Mid-burst: nothing has landed, so nothing has moved.
    expect(liveRow()).not.toHaveTextContent("Modified");

    // The write lands, and the backend's answer moves with it.
    live = { ...live, sha256: "sha-typed", matchesLatest: false };
    await waitFor(
      () =>
        expect(liveRow()).toHaveTextContent(
          "Modified since latest accepted version",
        ),
      { timeout: AUTOSAVE_DELAY_MS + 2000 },
    );
    // NAW-FR-36: and the author's own typing put no row in the rail.
    expect(versionRows()).toHaveLength(2);
  });

  it("NAW-FR-08, NAW-FR-40: the list reads no snapshot, and a version's text is fetched only on selection", async () => {
    stub({ history: threeVersions() });
    renderWorkspace();
    // NAW-FR-40: invoked once for the toggle's count even before the rail is
    // shown, and no snapshot is read for it.
    await waitFor(() =>
      expect(callsNamed("list_draft_history")).toHaveLength(1),
    );
    expect(callsNamed("load_draft_history_entry")).toHaveLength(0);

    await showRail();
    await waitFor(() => expect(versionRows()).toHaveLength(3));
    expect(callsNamed("list_draft_history")).toHaveLength(1);
    expect(callsNamed("load_draft_history_entry")).toHaveLength(0);

    await userEvent.click(versionRows()[1]);
    await screen.findByTestId("draft-version-reading");
    expect(callsNamed("load_draft_history_entry")).toHaveLength(1);
    expect(
      (callsNamed("load_draft_history_entry")[0][1] as { entryId: string })
        .entryId,
    ).toBe("e2");
    // Returning and selecting the same version again re-lists nothing.
    await userEvent.click(liveRow());
    await userEvent.click(versionRows()[1]);
    await screen.findByTestId("draft-version-reading");
    expect(callsNamed("list_draft_history")).toHaveLength(1);
  });

  it("NAW-FR-41, NAW-FR-10: a version whose snapshot cannot be read states so, and the rest still load", async () => {
    invokeMock.mockImplementation(async (cmd: string, args?: unknown) => {
      switch (cmd) {
        case "open_draft":
          return draft();
        case "list_draft_history":
          return threeVersions();
        case "load_draft_file_contents":
          return { body: "the live text", checksum: "c1" };
        case "load_draft_history_entry":
          if ((args as { entryId: string }).entryId === "e2")
            throw "snapshot_corrupt";
          return { content: "an older version", sha256: "s" };
        default:
          return undefined;
      }
    });
    const { drafts } = renderWorkspace();
    await screen.findByRole("button", { name: "Edit as Markdown source" });
    await editSource("an unsaved edit");
    await showRail();
    await waitFor(() => expect(versionRows()).toHaveLength(3));

    await userEvent.click(versionRows()[1]);

    await screen.findByText("⚠ This version could not be read.");
    expect(
      screen.getByText("Its stored snapshot did not match its checksum."),
    ).toBeInTheDocument();
    // NAW-FR-41: the live prompt's buffer and dirty state are untouched.
    expect(drafts.docs.get(drafts.key("d1", PROMPT))?.buffer).toBe("an unsaved edit");
    expect(drafts.isDirty("d1")).toBe(true);
    // …and the rail's other rows are still listed and still selectable.
    expect(versionRows()).toHaveLength(3);
    await userEvent.click(versionRows()[2]);
    expect(await screen.findByTestId("draft-version-reading")).toHaveTextContent(
      "an older version",
    );

    // The retry re-reads and reports the same thing; the return goes back to
    // the live prompt with its editor state intact.
    await userEvent.click(versionRows()[1]);
    await screen.findByText("⚠ This version could not be read.");
    await userEvent.click(screen.getByRole("button", { name: "Try again" }));
    await screen.findByText("⚠ This version could not be read.");
    await userEvent.click(
      within(screen.getByRole("alert")).getByRole("button", {
        name: "Live prompt",
      }),
    );
    await screen.findByLabelText("Markdown source");
    expect(drafts.docs.get(drafts.key("d1", PROMPT))?.buffer).toBe("an unsaved edit");
  });

  it("NAW-FR-08, NAW-FR-38, NAW-FR-41, DRS-FR-15, DHS-FR-21: an inconsistent draft renders its own state and no editing surface", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "open_draft") throw "draft_not_single_file";
      return undefined;
    });
    renderWorkspace();

    await screen.findByText("This draft cannot be opened.");
    // NAW-FR-41: no editing surface, no rail, and no file of it presented as
    // the prompt.
    expect(
      screen.queryByRole("button", { name: "Edit as Markdown source" }),
    ).toBeNull();
    expect(document.querySelector(".draft-rail")).toBeNull();
    expect(screen.queryByRole("button", { name: /Show History/ })).toBeNull();
    expect(screen.getByText(/Delete it from the Drafts panel/)).toBeInTheDocument();
    // Nothing under the draft was read or written for it.
    expect(callsNamed("load_draft_file_contents")).toHaveLength(0);
    expect(callsNamed("list_draft_history")).toHaveLength(0);
  });

  it("NAW-FR-08, NAW-FR-38, NAW-FR-41, DRS-FR-15, DHS-FR-21: a history that cannot be reconciled renders a retry rather than a verdict", async () => {
    let failing = true;
    invokeMock.mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case "open_draft":
          return draft();
        case "load_draft_file_contents":
          return { body: "the live text", checksum: "c1" };
        case "list_draft_history":
          if (failing) throw "history_recovery_failed";
          return threeVersions();
        default:
          return undefined;
      }
    });
    renderWorkspace();
    const rail = await showRail();

    await within(rail).findByText(/could not be recovered/);
    // NAW-FR-41: the prompt is presented as neither current nor stale until it
    // resolves, so no marker is rendered either way.
    expect(liveRow()).not.toHaveTextContent("Modified");
    expect(versionRows()).toHaveLength(0);
    // …and the live row is not marked `Original` either. With no answer from
    // the history, "this is the Original" is the same unconfirmed verdict as
    // "this has moved on from it" — NAW-FR-41 forbids both.
    expect(liveRow()).not.toHaveTextContent("Original");
    // Nor does the toggle claim a count. A draft holding three versions whose
    // history refused would otherwise report "1 version of this draft", which
    // is a statement about the draft rather than about the failure.
    const toggle = screen.getByRole("button", { name: "Hide History" });
    expect(toggle).toHaveAttribute("title", "Version history");
    expect(toggle.querySelector(".draft-rail__count")).toBeNull();

    failing = false;
    await userEvent.click(within(rail).getByRole("button", { name: "Try again" }));
    await waitFor(() => expect(versionRows()).toHaveLength(3));
    expect(within(rail).queryByText(/could not be recovered/)).toBeNull();
    // …and both come back the moment it resolves.
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Hide History" }),
      ).toHaveAttribute("title", "3 versions of this draft"),
    );
  });

  it("NAW-FR-10 / NAW-FR-30: Escape backs out of the surface above the reading, not both", async () => {
    // The reading is the outermost of this tab's states. Every surface over it
    // binds its own dismissal on the window, so an unguarded handler here made
    // one Escape back out of the modal AND cost the author the reading
    // underneath it — two dismissals for one keypress.
    stub({ history: threeVersions() });
    renderWorkspace();
    await showRail();
    await waitFor(() => expect(versionRows()).toHaveLength(3));
    await userEvent.click(versionRows()[0]);
    await screen.findByTestId("draft-version-reading");

    // The action control's own column, over the reading.
    await openActions();
    await userEvent.keyboard("{Escape}");
    await waitFor(() => expect(screen.queryByRole("menu")).toBeNull());
    expect(screen.getByTestId("draft-version-reading")).toBeInTheDocument();

    // …and the next Escape is the reading's.
    await userEvent.keyboard("{Escape}");
    await waitFor(() =>
      expect(screen.queryByTestId("draft-version-reading")).toBeNull(),
    );
  });


  it("NAW-FR-10: returning from inside the reading lands on the live-prompt row", async () => {
    // Focus dropped to the document body means the next Tab restarts at the top
    // of the tab, and a screen-reader user loses their place at exactly the
    // moment they asked to go back to the prompt.
    stub({ history: threeVersions() });
    renderWorkspace();
    await showRail();
    await waitFor(() => expect(versionRows()).toHaveLength(3));
    await userEvent.click(versionRows()[0]);
    await screen.findByTestId("draft-version-reading");

    // From the strip's own control…
    await userEvent.click(
      screen.getByRole("button", { name: "← Back to live prompt" }),
    );
    await waitFor(() => expect(document.activeElement).toBe(liveRow()));

    // …and from Escape, with focus inside the reading rather than on the rail.
    await userEvent.click(versionRows()[1]);
    (await screen.findByTestId("draft-version-reading")).focus();
    await userEvent.keyboard("{Escape}");
    await waitFor(() =>
      expect(screen.queryByTestId("draft-version-reading")).toBeNull(),
    );
    await waitFor(() => expect(document.activeElement).toBe(liveRow()));
  });

  it("NAW-FR-09: a reading opens in the live prompt's mode again once the author has left it", async () => {
    // The author's choice of rendering holds for as long as they stay in the
    // reading — including across versions selected one after another — and ends
    // with it. Without that, a raw reading left open a week ago decides how the
    // next one opens, whatever the prompt is being read as now.
    stub({ history: threeVersions() });
    renderWorkspace();
    await showRail();
    await waitFor(() => expect(versionRows()).toHaveLength(3));

    await userEvent.click(versionRows()[0]);
    await userEvent.click(
      await screen.findByRole("button", { name: "View as Markdown source" }),
    );
    expect(await screen.findByTestId("draft-version-reading")).toHaveClass(
      "editor__source",
    );

    // Another version, still inside the same reading: the author's choice holds.
    await userEvent.click(versionRows()[1]);
    expect(await screen.findByTestId("draft-version-reading")).toHaveClass(
      "editor__source",
    );

    // Back to the live prompt, which is rich — and the next reading opens rich.
    await userEvent.click(liveRow());
    await screen.findByRole("button", { name: "Edit as Markdown source" });
    await userEvent.click(versionRows()[2]);
    expect(await screen.findByTestId("draft-version-reading")).toHaveClass(
      "editor__prose",
    );
  });

  it("NAW-FR-36, NAW-FR-38, DHS-FR-07, NAW-FR-35, NAW-FR-42, DCR-FR-12, DCR-FR-13, DCR-FR-24 / NAW-FR-41, NAW-FR-10: an acceptance adds a version and returns to the live prompt", async () => {
    // NAW-FR-40 / NAW-FR-42: the rail follows `"draft history changed"`, so an
    // acceptance that lands while the tab is open puts its version at the foot
    // without the rail re-listing of its own accord — and a version that was
    // being read gives way to the accepted text.
    let history = acceptedHistory();
    invokeMock.mockImplementation(async (cmd: string) => {
      switch (cmd) {
        case "open_draft":
          return draft();
        case "load_draft_file_contents":
          return { body: "the live text", checksum: "c1" };
        case "list_draft_history":
          return history;
        case "load_draft_history_entry":
          return { content: "the original prompt", sha256: "s" };
        default:
          return undefined;
      }
    });
    renderWorkspace();
    await showRail();
    await waitFor(() => expect(versionRows()).toHaveLength(2));
    await userEvent.click(versionRows()[0]);
    await screen.findByTestId("draft-version-reading");

    // The acceptance lands: the backend's answer moves on, and the event says so.
    history = threeVersions();
    act(() => {
      fireBus("draft-history-changed", {
        draftId: "d1",
        entry: history.entries[2],
      });
    });

    await waitFor(() => expect(versionRows()).toHaveLength(3));
    // NAW-FR-42: the version it produced takes the head of the column, directly
    // under the live prompt, and the one before it is marked superseded.
    expect(versionRows()[0]).toHaveTextContent("Latest accepted");
    expect(versionRows()[1]).toHaveTextContent("Superseded");
    // NAW-FR-42: the tab returned to the live prompt rather than leaving the
    // author on the reading they had open, and the modified marker cleared.
    await waitFor(() =>
      expect(
        screen.queryByTestId("draft-version-reading"),
      ).not.toBeInTheDocument(),
    );
    expect(liveRow()).not.toHaveTextContent("Modified");
  });

  it("NAW-FR-43, HVW-FR-01, HVW-FR-02: the rail is the draft's own and reaches no application-wide History", async () => {
    // NAW-FR-43: it lists versions of one unpublished prompt held inside that
    // draft's own folder, opens no History detail tab, and reaches no Git
    // revision.
    //
    // Asserted as a whitelist rather than as an absence of named operations: a
    // list of history commands this tab must not call would have to be kept in
    // step with `HVW-history-viewer.md`'s own surface, and a renamed command
    // there would silently turn this into an assertion about nothing.
    stub({ history: threeVersions() });
    renderWorkspace();
    await showRail();
    await waitFor(() => expect(versionRows()).toHaveLength(3));
    await userEvent.click(versionRows()[1]);
    await screen.findByTestId("draft-version-reading");
    await userEvent.click(liveRow());

    const invoked = new Set(invokeMock.mock.calls.map((c) => String(c[0])));
    expect([...invoked].sort()).toEqual([
      // NAW-FR-44 / DRS-FR-18: the draft's lock is the run's state, read from
      // the queue rather than remembered — so the tab asks for it on mount.
      "get_draft_graduation",
      // NAW-FR-TSQE: the publication read, on mount and on its own event. It
      // reaches no history operation and renders no version.
      "get_draft_publication",
      // CTA-FR-XSGX: the unsupported-image notice, read alongside the offer to
      // retry and on exactly the same terms.
      "list_agent_turn_image_notices",
      "list_agent_turns",
      "list_discussions",
      "list_draft_history",
      "list_project_agents",
      // CTA-FR-STWA: the offer to ask again, read alongside the outstanding turns
      // this tab already reads when it mounts.
      "list_recoverable_agent_turn_failures",
      "load_draft_file_contents",
      "load_draft_history_entry",
      "open_draft",
      "resolve_comment_author_identity",
      // NAW-FR-55: housekeeping, triggered when the draft opens. A trigger and
      // not a query — the answer is not read, not waited on, and nothing is
      // rendered from it.
      "sweep_draft_assets",
    ]);
  });
});

// ---------------------------------------------------------------------------
// Pure helpers
// ---------------------------------------------------------------------------

describe("pure helpers", () => {
  it("NAW-FR-38: labels a version by where it stands, and never as the current draft", () => {
    const entries = threeVersions().entries;
    expect(standingOf(entries[0], entries)).toBe("Original");
    expect(standingOf(entries[1], entries)).toBe("Superseded");
    expect(standingOf(entries[2], entries)).toBe("Latest accepted");
    // NAW-FR-07: the source label is the proposing agent, or `Original`.
    expect(sourceLabelOf(entries[0])).toBe("Original");
    expect(sourceLabelOf(entries[2])).toBe("@arch");
  });

  it("NAW-FR-37: the live marker names the baseline the prompt has moved on from", () => {
    // Byte-identical to the newest version: rendered plainly.
    expect(liveMarkerFor(threeVersions())).toBeNull();
    expect(liveMarkerFor(null)).toBeNull();

    // Modified, with an acceptance behind it.
    const accepted = threeVersions();
    accepted.live.matchesLatest = false;
    expect(liveMarkerFor(accepted)).toBe("Modified since latest accepted version");

    // The marker names the baseline that actually exists rather than asserting
    // one that does not: where the newest version is the `Original`, that is
    // what it names.
    const original = {
      entries: [entry(1, { kind: "original" as const })],
      live: { path: PROMPT, byteLen: 1, sha256: "x", matchesLatest: false },
    };
    expect(liveMarkerFor(original)).toBe("Modified since Original");

    // And a draft holding NO version has moved on from nothing, whatever the
    // list says about its digest — its live prompt is the `Original` itself
    // (DHS-FR-07), so there is no baseline to name.
    const fresh = freshHistory();
    fresh.live.matchesLatest = false;
    expect(liveMarkerFor(fresh)).toBeNull();
    expect(versionCountOf(fresh)).toBe(1);
    expect(versionCountOf(threeVersions())).toBe(3);
    // NAW-FR-41: a history that has not answered — not yet, or not at all
    // because it could not be reconciled — is counted as nothing rather than as
    // one, a count being a claim about the draft.
    expect(versionCountOf(null)).toBeNull();
  });

  it("NAW-FR-19: nothing here decides where a specification goes", () => {
    // NAW-FR-19 / NAW-FR-19: there is no target-path chooser, no destination
    // field, and no validator for one — the agent selects the paths from the
    // captured prompt and the application's gate is what stands between that
    // choice and the project. The absence is the requirement, so it is what is
    // asserted.
    const source = NewArtifactWorkspace.toString();
    for (const gone of ["destinationRoot", "graduate_draft"]) {
      expect(source).not.toContain(gone);
    }
  });
});
