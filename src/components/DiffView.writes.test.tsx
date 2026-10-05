import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
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

import { DERIVE_SETTLE_MS, DiffView } from "./DiffView";
import { AUTOSAVE_DELAY_MS, EditSessionStore } from "../state/editSessions";
import { sealBurst } from "../state/editHistory";
import { resetDiffModes } from "../state/diffModes";
import { resetAppPreferencesCache } from "../state/appPreferences";
import {
  againstMain,
  awaitDiff,
  diffLine,
  makeWireBackend,
  renderDiff,
  revs,
  setCaret,
  target,
  targetCell,
  targetCells,
  toolbarGroup,
  typeInto,
  uncommitted,
} from "../test/diffViewFixtures";

const invokeMock = vi.fn();
const unlistenMock = vi.fn();
let listeners: Record<string, (event: { payload: unknown }) => void> = {};

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn(async (event: string, cb: (event: { payload: unknown }) => void) => {
    listeners[event] = cb;
    return unlistenMock;
  }),
}));

const wireBackend = makeWireBackend(invokeMock);

const callsTo = (name: string) =>
  invokeMock.mock.calls.filter((c) => c[0] === name);

beforeEach(() => {
  invokeMock.mockReset();
  unlistenMock.mockReset();
  listeners = {};
  resetDiffModes();
  resetAppPreferencesCache();
  wireBackend();
});

afterEach(cleanup);

describe("the editable target (DFV-FR-41 .. DFV-FR-50)", () => {
  const FILE = "one\ntwo\nthree\n";

  it("DFV-FR-51, EDT-FR-70, EDT-FR-71, EDT-FR-34 writes once when the typing settles, and keeps the edits when the write fails", async () => {
    vi.useFakeTimers();
    try {
      let failing = true;
      invokeMock.mockImplementation(async (cmd: string) => {
        if (cmd === "get_file_revisions") return revs("one\nTWO\nthree\n", FILE);
        if (cmd === "load_artifact_contents_by_id")
          return { body: FILE, checksum: "sum" };
        if (cmd === "save_artifact_contents") {
          if (failing) throw "disk is full";
          return { checksum: "sum2" };
        }
        if (cmd === "load_app_preferences") return { theme: "system" };
        return undefined;
      });
      const { sessions } = renderDiff(target("notes.md"));
      await act(async () => {
        await vi.advanceTimersByTimeAsync(0);
      });

      // A burst of typing is one write at the end of it, not one per keystroke.
      typeInto(targetCell("two"), "t");
      typeInto(targetCell("t"), "tw");
      typeInto(targetCell("tw"), "two!");
      expect(callsTo("save_artifact_contents")).toHaveLength(0);

      await act(async () => {
        await vi.advanceTimersByTimeAsync(AUTOSAVE_DELAY_MS + 50);
      });
      expect(callsTo("save_artifact_contents")).toHaveLength(1);

      // DFV-FR-51 / EDT-FR-71: the failure is reported, every edit is still in
      // the buffer, and nothing is retried on a timer.
      expect(sessions.get("notes.md")?.buffer).toBe("one\ntwo!\nthree\n");
      expect(sessions.get("notes.md")?.error).toContain("disk is full");
      await act(async () => {
        await vi.advanceTimersByTimeAsync(AUTOSAVE_DELAY_MS * 4);
      });
      expect(callsTo("save_artifact_contents")).toHaveLength(1);

      // The next edit is what retries it.
      failing = false;
      typeInto(targetCell("two!"), "two fixed");
      await act(async () => {
        await vi.advanceTimersByTimeAsync(AUTOSAVE_DELAY_MS + 50);
      });
      expect(callsTo("save_artifact_contents")).toHaveLength(2);
      expect(sessions.get("notes.md")?.dirty).toBe(false);
    } finally {
      vi.useRealTimers();
    }
  });

  it("DFV-FR-42, TAB-FR-09, EDT-FR-22, EDT-FR-30 shares one buffer and one dirty state with every other surface on the file", async () => {
    wireBackend({ revisions: revs("one\nTWO\nthree\n", FILE) });
    // Two Diff tabs on one file under two comparisons, mounted the way the
    // strip mounts them: one at a time, because only the active tab renders.
    const sessions = new EditSessionStore();
    const { rerender } = render(
      <DiffView target={target("notes.md")} sessions={sessions} />,
    );
    await awaitDiff();
    typeInto(targetCell("two"), "two edited");
    expect(sessions.get("notes.md")?.dirty).toBe(true);

    rerender(
      <DiffView target={target("notes.md", againstMain)} sessions={sessions} />,
    );
    await awaitDiff();

    // DFV-FR-42: the second tab shows the SAME buffer with the same dirty state
    // and renders it against its own original — and reads no target at all, the
    // artifact's session already holding one (DFV-FR-25, DFV-FR-42).
    expect(sessions.get("notes.md")?.buffer).toBe("one\ntwo edited\nthree\n");
    expect(sessions.get("notes.md")?.dirty).toBe(true);
    expect(targetCell("two edited")).toBeTruthy();
    expect(callsTo("load_artifact_contents_by_id")).toHaveLength(1);
    expect(callsTo("get_file_revisions")).toHaveLength(2);
  });

  it("DFV-FR-53, EXC-FR-VTUH, EXC-FR-WDAV, EXC-FR-WDEJ, EDT-FR-70 raises the external-change modal and leaves the buffer alone", async () => {
    wireBackend({ revisions: revs("one\nTWO\nthree\n", FILE) });
    const { sessions } = renderDiff(target("notes.md"));
    await awaitDiff();
    typeInto(targetCell("two"), "two edited");

    // DFV-FR-53: answered by the artifact's own external-change resolution and
    // by nothing else.
    act(() => {
      sessions.update("notes.md", { pending: "other", conflict: true });
    });

    const modal = await screen.findByRole("dialog", {
      name: "File changed on disk",
    });
    expect(modal).toBeInTheDocument();
    // The buffer is neither reloaded nor overwritten, and the editing surface
    // is inert until the author resolves it.
    expect(sessions.get("notes.md")?.buffer).toBe("one\ntwo edited\nthree\n");
    for (const cell of targetCells()) {
      expect(cell.getAttribute("contenteditable")).toBe("false");
    }

    await userEvent.click(
      within(modal).getByRole("button", { name: "Keep my version" }),
    );
    expect(sessions.get("notes.md")?.conflict).toBe(false);
    expect(sessions.get("notes.md")?.buffer).toBe("one\ntwo edited\nthree\n");
  });

  it("DFV-FR-54, DFV-FR-15 restores a deleted target and then edits it", async () => {
    let deleted = true;
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_file_revisions")
        return deleted ? revs("gone\n", null) : revs("gone\n", "");
      if (cmd === "load_artifact_contents_by_id")
        return { body: "", checksum: "sum" };
      if (cmd === "create_file") {
        deleted = false;
        return { path: "old-notes.md" };
      }
      if (cmd === "load_app_preferences")
        return { theme: "system", diffVisualizationMode: "final" };
      return undefined;
    });
    const { sessions } = renderDiff(target("old-notes.md"));

    // DFV-FR-15 / DFV-FR-54: the deleted state states what it is and carries a
    // restore affordance stating what it will do. Nothing on screen is editable.
    expect(
      await screen.findByText(/does not exist in the new revision/),
    ).toBeInTheDocument();
    expect(targetCells()).toHaveLength(0);
    const restore = screen.getByRole("button", { name: /Restore/ });

    await userEvent.click(restore);

    // The file is created at the comparison's own path, and the tab then holds
    // an editing session on it whose target opens empty and editable.
    await waitFor(() => expect(callsTo("create_file")).toHaveLength(1));
    expect(callsTo("create_file")[0][1]).toEqual({
      location: null,
      name: "old-notes.md",
    });
    await waitFor(() => expect(targetCells().length).toBeGreaterThan(0));
    expect(
      screen.getByText(/The new revision of this file holds nothing/),
    ).toBeInTheDocument();
    typeInto(targetCells()[0], "written from nothing");
    expect(sessions.get("old-notes.md")?.buffer).toBe("written from nothing");
  });

  it("DFV-FR-54, DFV-FR-15 leaves the deleted state standing when the restore fails", async () => {
    invokeMock.mockImplementation(async (cmd: string) => {
      if (cmd === "get_file_revisions") return revs("gone\n", null);
      if (cmd === "create_file") throw "permission denied";
      if (cmd === "load_app_preferences")
        return { theme: "system", diffVisualizationMode: "final" };
      return undefined;
    });
    renderDiff(target("old-notes.md"));
    await userEvent.click(await screen.findByRole("button", { name: /Restore/ }));

    expect(await screen.findByRole("alert")).toHaveTextContent(
      /permission denied/,
    );
    expect(
      screen.getByText(/does not exist in the new revision/),
    ).toBeInTheDocument();
  });

  it("DFV-FR-48, DFV-FR-41 leaves a Flow's rich reading read-only and its Source editable", async () => {
    const flowBody = (name: string) =>
      `${JSON.stringify({ version: 1, name, nodes: [], edges: [] }, null, 2)}\n`;
    wireBackend({
      revisions: revs(flowBody("Review"), flowBody("Reviewed")),
      prefs: { diffRenderingMode: "rich" },
    });
    const { sessions } = renderDiff(target("review.flow", uncommitted, "flow"));
    await awaitDiff();

    // DFV-FR-48: the graph comparison is a reading of two graphs rather than a
    // document, so nothing in it accepts input and no Markdown editing
    // affordance is present anywhere.
    expect(document.querySelector(".diff-flow")).toBeTruthy();
    expect(targetCells()).toHaveLength(0);

    // And Source over the same file is editable on the ordinary target-only
    // terms.
    await userEvent.click(screen.getByRole("radio", { name: "Source" }));
    await waitFor(() => expect(targetCells().length).toBeGreaterThan(0));
    typeInto(targetCells()[0], "{ }");
    expect(sessions.get("review.flow")?.buffer).toContain("{ }");
  });

  it("DFV-FR-49, DFV-FR-18 edits a file rich rendering does not apply to", async () => {
    wireBackend({ revisions: revs("const a = 1;\n", "const a = 2;\n") });
    const { sessions } = renderDiff(target("src/Library.tsx"));
    await awaitDiff();

    for (const toggle of within(toolbarGroup("Diff rendering")).getAllByRole("radio")) {
      expect(toggle).toBeDisabled();
    }
    typeInto(targetCell("const a = 2;"), "const a = 3;");
    expect(sessions.get("src/Library.tsx")?.buffer).toBe("const a = 3;\n");
  });

  it("DFV-FR-41, DFV-FR-50 announces the original read-only and names the editable target", async () => {
    wireBackend({ revisions: revs("one\nTWO\nthree\n", FILE) });
    renderDiff(target("notes.md"));
    await awaitDiff();

    // Every original row reports itself read-only; no target row does.
    const removed = diffLine("TWO")!.querySelector(".diff-line__text")!;
    expect(removed).toHaveAttribute("aria-readonly", "true");
    for (const cell of targetCells()) {
      expect(cell).not.toHaveAttribute("aria-readonly");
      // DFV-FR-50: the accessible name says which file and which revision.
      expect(cell.getAttribute("aria-label")).toBe(
        "notes.md — target revision (editable)",
      );
      expect(cell).toHaveAttribute("role", "textbox");
    }
  });

  it("DFV-FR-56, DFV-FR-43 re-marks the comparison once the typing settles", async () => {
    vi.useFakeTimers();
    try {
      wireBackend({
        revisions: revs("one\nTWO\nthree\nFOUR\n", "one\ntwo\nthree\nfour\n"),
      });
      renderDiff(target("notes.md"));
      await act(async () => {
        await vi.advanceTimersByTimeAsync(0);
      });
      expect(diffLine("two")).toHaveAttribute("data-kind", "add");

      // Edited back to exactly the original's text: the recomputation says there
      // is no longer a difference there, so the marking goes — while the other
      // change is still marked as the change it is.
      typeInto(targetCell("two"), "TWO");
      await act(async () => {
        await vi.advanceTimersByTimeAsync(DERIVE_SETTLE_MS + 50);
      });
      expect(diffLine("TWO")).toHaveAttribute("data-kind", "context");
      expect(diffLine("four")).toHaveAttribute("data-kind", "add");

      // And an addition the author extends is still an addition.
      typeInto(targetCell("four"), "four and a bit");
      await act(async () => {
        await vi.advanceTimersByTimeAsync(DERIVE_SETTLE_MS + 50);
      });
      expect(diffLine("four and a bit")).toHaveAttribute("data-kind", "add");
    } finally {
      vi.useRealTimers();
    }
  });

  it("DFV-FR-41, DFV-FR-43, DFV-FR-44, DFV-FR-51 splits, joins, and pastes across row boundaries", async () => {
    // The three edits the browser cannot make on its own, because each row is
    // its own editing host: Enter splitting a line, Backspace at the head of a
    // line joining it to the one above, and a paste carrying newlines. Each is
    // one splice of the WHOLE target (DFV-FR-44).
    wireBackend({ revisions: revs("one\nTWO\nthree\n", FILE) });
    const { sessions } = renderDiff(target("notes.md"));
    await awaitDiff();

    // Enter at the caret: the row becomes two rows and the file grows a line.
    const row = targetCell("two");
    row.focus();
    setCaret(row, 1);
    fireEvent.keyDown(row, { key: "Enter" });
    expect(sessions.get("notes.md")?.buffer).toBe("one\nt\nwo\nthree\n");

    // Backspace at offset 0 joins into the line above — and joins it with what
    // that line CURRENTLY holds, not with what it held when the tab mounted.
    // This is the regression that silently discarded an earlier edit.
    await waitFor(() => expect(targetCell("t")).toBeTruthy());
    typeInto(targetCell("t"), "T-EDITED");
    const second = targetCell("wo");
    second.focus();
    setCaret(second, 0);
    fireEvent.keyDown(second, { key: "Backspace" });
    expect(sessions.get("notes.md")?.buffer).toBe("one\nT-EDITEDwo\nthree\n");

    // At the first line there is nothing above to join into, so the keystroke
    // is left to the platform and splices nothing.
    const first = targetCell("one");
    first.focus();
    setCaret(first, 0);
    fireEvent.keyDown(first, { key: "Backspace" });
    expect(sessions.get("notes.md")?.buffer).toBe("one\nT-EDITEDwo\nthree\n");

    // A paste carrying newlines splices at the caret; a single-line paste is
    // ordinary text input and is left to the platform.
    const third = targetCell("three");
    third.focus();
    setCaret(third, 0);
    fireEvent.paste(third, {
      clipboardData: { getData: () => "alpha\nbeta" },
    });
    expect(sessions.get("notes.md")?.buffer).toBe(
      "one\nT-EDITEDwo\nalpha\nbetathree\n",
    );
  });

  it("DFV-FR-52, DFV-FR-25, DFV-FR-43 traverses the artifact's own history from either route", async () => {
    // DFV-FR-50 / EDT-FR-22: one history for the artifact, reached by the
    // accelerator and by the native Edit menu's Undo/Redo, which arrives as a
    // `beforeinput`.
    wireBackend({ revisions: revs("one\nTWO\nthree\n", FILE) });
    const { container, sessions } = renderDiff(target("notes.md"));
    await awaitDiff();

    const session = sessions.get("notes.md")!;
    const seal = () => sealBurst(session.history);
    typeInto(targetCell("two"), "first");
    seal();
    typeInto(targetCell("first"), "second");
    seal();

    const root = container.querySelector(".diff-view__target")!;
    fireEvent.keyDown(root, { key: "z", metaKey: true });
    expect(session.buffer).toBe("one\nfirst\nthree\n");

    fireEvent(
      root,
      new (window as unknown as { InputEvent: typeof InputEvent }).InputEvent(
        "beforeinput",
        { inputType: "historyRedo", bubbles: true, cancelable: true },
      ),
    );
    expect(session.buffer).toBe("one\nsecond\nthree\n");

    // DFV-FR-53: inert while the external-change modal is unresolved, like
    // every other edit operation on the tab.
    act(() => {
      sessions.update("notes.md", { pending: "other", conflict: true });
    });
    fireEvent.keyDown(root, { key: "z", metaKey: true });
    expect(session.buffer).toBe("one\nsecond\nthree\n");
  });

  it("GTC-FR-17, EDT-FR-40 keeps one line-ending convention through an edit", async () => {
    wireBackend({
      revisions: revs("one\r\nTWO\r\nthree\r\n", "one\r\ntwo\r\nthree\r\n"),
    });
    const { sessions } = renderDiff(target("notes.md"));
    await awaitDiff();

    // The comparison reads the two revisions with terminators normalised, so
    // the convention itself is not rendered as a change.
    expect(diffLine("one")).toHaveAttribute("data-kind", "context");
    typeInto(targetCell("two"), "two edited");
    // And the bytes the write would carry hold one convention throughout,
    // rather than the edited line losing its `\r` while every other keeps it.
    expect(sessions.get("notes.md")?.buffer).toBe(
      "one\r\ntwo edited\r\nthree\r\n",
    );
  });

  it("DFV-FR-07, DFV-FR-08 keeps the toolbar's controls whatever the target holds", async () => {
    // DFV-FR-50: the Editor's formatting toolbar and its Find band are NOT
    // reproduced here — this row is about how the comparison reads.
    wireBackend({ revisions: revs("one\nTWO\nthree\n", FILE) });
    const { container } = renderDiff(target("notes.md"));
    await awaitDiff();

    expect(container.querySelector(".editor__toolbar")).toBeNull();
    expect(container.querySelector(".find-panel")).toBeNull();
    expect(
      container.querySelectorAll('[role="radiogroup"]'),
    ).toHaveLength(2);
  });
});
