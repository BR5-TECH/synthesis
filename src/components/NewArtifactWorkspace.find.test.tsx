/**
 * Find and Find & Replace over the draft's live prompt
 * (`../../specifications/ui/NAW-new-artifact.md` NAW-FR-57 … NAW-FR-59).
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
  threeVersions,
  makeStubs,
  renderWorkspace,
  openActions,
  showRail,
  versionRows,
} from "../test/newArtifactFixtures";
import { AUTOSAVE_DELAY_MS } from "./NewArtifactWorkspace";
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
  clearAllDiscussionSessions();
  resetDiscussionFocus();
  stub();
});

afterEach(cleanup);

// ---------------------------------------------------------------------------
// Implement, and the run-state tag across all eight phases (
// NAW-FR-BJQX)
// ---------------------------------------------------------------------------




/**
 * NAW-FR-57 – NAW-FR-59: Find and Find & Replace over the draft's live prompt.
 *
 * The accelerators and the two Edit-menu items are delivered by the native menu
 * and routed by the shell (SNV-FR-43), which toggles the form on the draft's own
 * edit session — the very store this tab hands the Editor (NAW-FR-12). These
 * tests perform that same mutation rather than synthesising a chord the webview
 * never receives while a native menu owns it; the menu wiring itself is covered
 * in `App.menu.test.tsx`.
 */
describe("find and replace over the live prompt (NAW-FR-57 … NAW-FR-59)", () => {
  /** What the shell does when ⌘F / ⌘R or an Edit-menu item fires. */
  function setFind(
    drafts: DraftSessionStore,
    patch: Parameters<DraftSessionStore["docs"]["setFind"]>[1],
  ) {
    act(() => {
      drafts.docs.setFind(drafts.key("d1", PROMPT), patch);
    });
  }

  const findInput = () => screen.getByLabelText("Find") as HTMLInputElement;
  const replaceInput = () =>
    screen.getByLabelText("Replace with") as HTMLInputElement;
  const activeMode = () =>
    within(screen.getByTestId("find-mode-toggles"))
      .getAllByRole("radio")
      .find((t) => t.getAttribute("aria-checked") === "true");

  it("NAW-FR-58, EFR-FR-ESDZ, EFR-FR-EXHA, EFR-FR-FYNZ: searches unsaved text, rewrites every occurrence in one write, and undoes in one step", async () => {
    vi.useFakeTimers();
    try {
      const { drafts } = renderWorkspace();
      await vi.waitFor(() =>
        expect(
          screen.queryByRole("button", { name: "Edit as Markdown source" }),
        ).not.toBeNull(),
      );
      fireEvent.click(
        screen.getByRole("button", { name: "Edit as Markdown source" }),
      );
      const source = (await vi.waitFor(() =>
        screen.getByLabelText("Markdown source"),
      )) as HTMLTextAreaElement;

      // NAW-FR-57: an unsaved edit is searchable the moment it is made — the
      // panels read the in-memory prompt rather than the bytes on disk.
      const before = "one widget\ntwo widget\nthree widget\n";
      fireEvent.change(source, { target: { value: before } });

      setFind(drafts, { form: "replace" });
      fireEvent.change(findInput(), { target: { value: "widget" } });
      expect(screen.getByTestId("find-count").textContent).toBe("1/3");
      fireEvent.change(replaceInput(), { target: { value: "control" } });
      fireEvent.click(screen.getByRole("button", { name: "Replace All" }));

      const after = "one control\ntwo control\nthree control\n";
      expect(source.value).toBe(after);
      // NAW-FR-58: the sweep marks the draft dirty and nothing is on disk yet.
      expect(screen.getByText("Saving…")).toBeInTheDocument();
      expect(savedBodies()).toHaveLength(0);

      // NAW-FR-58 / NAW-FR-13: it reaches disk through the draft's own rest
      // after the last edit — **one** write, not one per occurrence.
      await vi.advanceTimersByTimeAsync(AUTOSAVE_DELAY_MS + 200);
      expect(savedBodies()).toEqual([{ path: PROMPT, body: after }]);

      // EFR-FR-GBIV: one undo restores the prompt exactly as it stood before the
      // sweep, and leaves the panel open with its query, its replacement text
      // and its mode intact (EFR-FR-EVHJ).
      fireEvent.keyDown(source, { key: "z", metaKey: true });
      expect(
        (screen.getByLabelText("Markdown source") as HTMLTextAreaElement).value,
      ).toBe(before);
      expect(screen.getByTestId("find-panel")).toBeInTheDocument();
      expect(findInput().value).toBe("widget");
      expect(replaceInput().value).toBe("control");
      expect(screen.getByTestId("find-count").textContent).toBe("1/3");
    } finally {
      vi.useRealTimers();
    }
  });

  it("NAW-FR-58 / EFR-FR-EXHA: Replace rewrites the current occurrence alone and advances to the next", async () => {
    vi.useFakeTimers();
    try {
      const { drafts } = renderWorkspace();
      await vi.waitFor(() =>
        expect(
          screen.queryByRole("button", { name: "Edit as Markdown source" }),
        ).not.toBeNull(),
      );
      fireEvent.click(
        screen.getByRole("button", { name: "Edit as Markdown source" }),
      );
      const source = (await vi.waitFor(() =>
        screen.getByLabelText("Markdown source"),
      )) as HTMLTextAreaElement;

      // NAW-FR-57: unsaved text is searchable the moment it is typed.
      const before = "one widget\ntwo widget\nthree widget\n";
      fireEvent.change(source, { target: { value: before } });

      setFind(drafts, { form: "replace" });
      fireEvent.change(findInput(), { target: { value: "widget" } });
      fireEvent.change(replaceInput(), { target: { value: "control" } });
      expect(screen.getByTestId("find-count").textContent).toBe("1/3");

      fireEvent.click(screen.getByRole("button", { name: "Replace" }));

      // Only the current match was rewritten, in the in-memory prompt.
      expect(source.value).toBe("one control\ntwo widget\nthree widget\n");
      // EFR-FR-EXHA: the current match advanced past what was just written, so
      // the two occurrences below it are what remains to walk.
      expect(screen.getByTestId("find-count").textContent).toBe("1/2");
      // NAW-FR-58: the draft is dirty and nothing has reached disk yet.
      expect(screen.getByText("Saving…")).toBeInTheDocument();
      expect(savedBodies()).toHaveLength(0);

      // EFR-FR-FYNZ: a single Replace is one step, so one undo puts that one
      // occurrence back and leaves the panel as it was (EFR-FR-EVHJ).
      fireEvent.keyDown(source, { key: "z", metaKey: true });
      expect(
        (screen.getByLabelText("Markdown source") as HTMLTextAreaElement).value,
      ).toBe(before);
      expect(screen.getByTestId("find-count").textContent).toMatch(/^\d+\/3$/);
      expect(findInput().value).toBe("widget");
      expect(replaceInput().value).toBe("control");
    } finally {
      vi.useRealTimers();
    }
  });

  it("NAW-FR-12, NAW-FR-57, EFR-FR-GIPZ: reopening the draft in the same session brings the panel back as it was", async () => {
    const { drafts, view } = renderWorkspace();
    await screen.findByRole("button", { name: "Edit as Markdown source" });

    setFind(drafts, {
      form: "replace",
      query: "wid.et",
      replacement: "control",
      mode: "regex",
    });
    await screen.findByTestId("find-panel");

    // NAW-FR-23: closing the tab ends the tab and not the draft.
    view.unmount();

    // The same draft, opened again in the same session (NAW-FR-12).
    renderWorkspace(drafts);
    const panel = await screen.findByTestId("find-panel");
    expect(panel).toHaveAttribute("data-form", "replace");
    expect(findInput().value).toBe("wid.et");
    expect(replaceInput().value).toBe("control");
    // EFR-FR-CWUR: the mode is the draft's own value, restored with the rest.
    expect(activeMode()).toHaveTextContent(/^\.\*/);
  });

  it("NAW-FR-59: a version being read offers the shell no search surface", async () => {
    stub({ history: threeVersions });
    const { drafts } = renderWorkspace();
    await screen.findByRole("button", { name: "Edit as Markdown source" });
    await waitFor(() =>
      expect(drafts.searchTarget("d1")).toBe(drafts.key("d1", PROMPT)),
    );

    await showRail();
    await userEvent.click(versionRows()[0]);
    await screen.findByTestId("draft-version-reading");

    // NAW-FR-09: the reading is read-only and gains no editing search surface.
    await waitFor(() => expect(drafts.searchTarget("d1")).toBeNull());

    // NAW-FR-10: the return puts it back within reach.
    await userEvent.click(
      screen.getByRole("button", { name: /Back to live prompt/ }),
    );
    await waitFor(() =>
      expect(drafts.searchTarget("d1")).toBe(drafts.key("d1", PROMPT)),
    );
  });

  it("NAW-FR-59: an archived draft searches exactly as an active one does", async () => {
    // NAW-FR-16: archiving retires a draft from the panel's default view and
    // takes nothing else away — it opens and edits as an active one does, so
    // its panels are live too.
    stub({ status: "archived" });
    const { drafts } = renderWorkspace();
    await screen.findByText("Archived");
    await waitFor(() =>
      expect(drafts.searchTarget("d1")).toBe(drafts.key("d1", PROMPT)),
    );

    setFind(drafts, { form: "find", query: "body" });
    expect(await screen.findByTestId("find-panel")).toBeInTheDocument();
    await waitFor(() =>
      expect(screen.getByTestId("find-count").textContent).toBe("1/1"),
    );
  });



  it("NAW-FR-44: a graduated draft with no run in the queue reads its record", async () => {
    // The fallback that keeps a genuinely graduated draft read-only through the
    // window between the tab mounting and the queue answering. A draft whose
    // project holds no queue at all reads the same way.
    stub({ status: "graduated", graduation: null });
    renderWorkspace();

    expect(
      await screen.findByText(/kept as the record of what the specification/),
    ).toBeInTheDocument();
    await openActions();
    expect(screen.getByRole("menuitem", { name: "Graduate" })).toBeDisabled();
  });
});
